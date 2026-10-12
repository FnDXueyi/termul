//! Server-side generic key-value store for the web client (issue #613).
//!
//! The browser client persists terminal layout, settings, editor state,
//! command history, snapshots, SSH profiles, etc. through `persistenceApi`.
//! On desktop that lands in Tauri's plugin-store; in the standalone web
//! client it previously fell back to a `localStorage` stub — per-browser
//! only, so switching browsers or refreshing in a different browser lost
//! everything. This store gives the server a durable home for that state: a
//! single JSON file written atomically (temp + rename + fsync, the same
//! `atomic_file` machinery `FileProjectRegistry::save_atomic` uses), so the
//! state survives browser switches and server restarts.
//!
//! # Schema (AD-18)
//!
//! On disk the file is `{"schemaVersion":1,"data":{ "key": value, … }}`. A
//! legacy bare-map file (`{ "key": value }`, written before versioning) is
//! migrated on open: the original bytes are first copied to
//! `<file>.pre-v1-<unix-ts>.bak` (a backup failure aborts the open), then the
//! file is rewritten in the envelope. A file whose `schemaVersion` is newer
//! than this build understands is refused with [`WebStoreError::NewerSchema`]
//! and left byte-identical.
//!
//! An object is an envelope only when it has a numeric `schemaVersion` AND a
//! `data` key; any other object is a legacy map, so a user key that happens to
//! be named `schemaVersion` is not misread. Unknown extra top-level keys of a
//! v1 envelope are kept verbatim across rewrites. The 10 MiB write limit is
//! measured on the `data` payload, not the enveloped file, so migrating a
//! legacy map never leaves a store that cannot be written.
//!
//! Exposed to browsers as the `store_read` / `store_write` / `store_delete`
//! WS requests (see `ws.rs`). The client debounces high-frequency writes, so
//! write volume here is low.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use serde::Serialize;
use serde_json::Value;
use tracing::{info, warn};

use crate::acp::atomic_file::{self, NewerSchema};

/// Current on-disk schema version of the web store envelope.
pub const STORE_SCHEMA_VERSION: u32 = 1;

/// Why [`WebStore::open`] refused to open the store file. Both variants are
/// startup-fatal for the host that hit them.
#[derive(Debug)]
pub enum WebStoreError {
    /// The file was written by a newer host (`schemaVersion` above
    /// [`STORE_SCHEMA_VERSION`]); it was not modified.
    NewerSchema(NewerSchema),
    /// A legacy bare-map file could not be backed up before migration, so the
    /// migration was not attempted and the file was not modified.
    MigrationBackup { path: PathBuf, source: io::Error },
}

impl std::fmt::Display for WebStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NewerSchema(error) => write!(f, "web store {error}"),
            Self::MigrationBackup { path, source } => write!(
                f,
                "web store '{}' is a legacy (unversioned) file and could not be backed up \
                 before migration: {source}; refusing to open it, the file was left untouched",
                path.display()
            ),
        }
    }
}

impl std::error::Error for WebStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NewerSchema(error) => Some(error),
            Self::MigrationBackup { source, .. } => Some(source),
        }
    }
}

/// Maximum size of the `data` payload a write may persist.
const MAX_DATA_BYTES: usize = 10 * 1024 * 1024;

/// Top-level keys of the envelope that are not `schemaVersion`/`data`,
/// preserved verbatim.
type ExtraKeys = serde_json::Map<String, Value>;

/// Borrowed envelope for serialization (no clone of the map per write).
#[derive(Serialize)]
struct EnvelopeRef<'a> {
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    data: &'a HashMap<String, Value>,
    #[serde(flatten)]
    extra: &'a ExtraKeys,
}

fn encode(map: &HashMap<String, Value>, extra: &ExtraKeys) -> io::Result<Vec<u8>> {
    serde_json::to_vec(&EnvelopeRef {
        schema_version: STORE_SCHEMA_VERSION,
        data: map,
        extra,
    })
    .map_err(|e| io::Error::other(e.to_string()))
}

/// Bytes the envelope adds around the `data` payload (everything but the
/// payload itself), so the limit can be checked on one serialization.
fn envelope_overhead(extra: &ExtraKeys) -> io::Result<usize> {
    // An empty `data` serializes as `{}` (2 bytes), which the payload replaces.
    Ok(encode(&HashMap::new(), extra)?.len() - 2)
}

/// What a store file's bytes mean to this build.
enum Decoded {
    /// `{"schemaVersion":1,"data":{…}, …unknown keys}`.
    Current(HashMap<String, Value>, ExtraKeys),
    /// A bare `{ "key": value }` map from before versioning.
    Legacy(HashMap<String, Value>),
}

fn decode(path: &Path, bytes: &[u8]) -> Result<Decoded, DecodeFailure> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|e| DecodeFailure::Corrupt(format!("invalid JSON: {e}")))?;
    let Value::Object(mut object) = value else {
        return Err(DecodeFailure::Corrupt(
            "top-level value is not a JSON object".to_string(),
        ));
    };
    let version = object.get("schemaVersion").and_then(Value::as_u64);
    match (version, object.get("data")) {
        // Newer envelope (a `data` key beside a numeric version): refuse,
        // whatever shape `data` has in that future version.
        (Some(found), Some(_)) if found > u64::from(STORE_SCHEMA_VERSION) => {
            Err(DecodeFailure::Newer(NewerSchema {
                path: path.to_path_buf(),
                found: u32::try_from(found).unwrap_or(u32::MAX),
                supported: STORE_SCHEMA_VERSION,
            }))
        }
        (Some(1), Some(Value::Object(_))) => {
            let Some(Value::Object(data)) = object.remove("data") else {
                unreachable!("matched an object `data` above");
            };
            object.remove("schemaVersion");
            Ok(Decoded::Current(data.into_iter().collect(), object))
        }
        // Anything else — including a bare map with a user key named
        // `schemaVersion` — is a legacy map.
        _ => Ok(Decoded::Legacy(object.into_iter().collect())),
    }
}

enum DecodeFailure {
    Newer(NewerSchema),
    Corrupt(String),
}

/// A key → arbitrary-JSON-value map persisted to one JSON file.
///
/// All mutation goes through a single `parking_lot::Mutex`; each write
/// serializes the whole map and replaces the file atomically (see the module
/// doc for the on-disk envelope).
pub struct WebStore {
    path: PathBuf,
    /// Unknown top-level envelope keys, written back unchanged.
    extra: ExtraKeys,
    inner: Mutex<Option<HashMap<String, Value>>>,
}

impl WebStore {
    /// Open (and load) the store at `path`.
    ///
    /// - Missing file → empty store (first run); the envelope appears on the
    ///   first write.
    /// - Current envelope → loaded.
    /// - Legacy bare map → backed up to `<path>.pre-v1-<ts>.bak`, then
    ///   rewritten in the envelope. Backup failure → `Err`, file untouched.
    ///   A failed rewrite only warns (the next write persists the envelope).
    /// - Newer `schemaVersion` → `Err(NewerSchema)`, file untouched.
    /// - Corrupt file → backed up to `<path>.corrupt-<ts>.bak`, then treated
    ///   as empty — the web client degrades to defaults rather than the
    ///   server refusing to start (same corrupt-handling posture as
    ///   `AcpCatalogService` / `AcpInstallService`).
    /// - Other read failure → warn + unavailable store (reads/writes return
    ///   `STORE_UNAVAILABLE`; the file is not touched).
    pub fn open(path: PathBuf) -> Result<Self, WebStoreError> {
        let mut extra = ExtraKeys::new();
        let data = match fs::read(&path) {
            Ok(bytes) => match decode(&path, &bytes) {
                Ok(Decoded::Current(map, unknown)) => {
                    extra = unknown;
                    Some(map)
                }
                Ok(Decoded::Legacy(map)) => Some(Self::migrate_legacy(&path, &bytes, map)?),
                Err(DecodeFailure::Newer(newer)) => {
                    warn!("{}", WebStoreError::NewerSchema(newer.clone()));
                    return Err(WebStoreError::NewerSchema(newer));
                }
                Err(DecodeFailure::Corrupt(reason)) => {
                    match atomic_file::backup_corrupt(&path, &bytes) {
                        Ok(_) => {
                            warn!(
                                "store file '{}' is corrupt ({reason}); backed up and starting with an empty store",
                                path.display()
                            );
                            Some(HashMap::new())
                        }
                        Err(backup_err) => {
                            warn!(
                                "store file '{}' is corrupt ({reason}) and backup failed: {backup_err}; store is unavailable",
                                path.display()
                            );
                            None
                        }
                    }
                }
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                info!(
                    "store file '{}' not found; starting with an empty store",
                    path.display()
                );
                Some(HashMap::new())
            }
            Err(e) => {
                warn!(
                    "could not read store file '{}': {e}; store is unavailable",
                    path.display()
                );
                None
            }
        };
        Ok(Self {
            path,
            extra,
            inner: Mutex::new(data),
        })
    }

    fn migrate_legacy(
        path: &Path,
        original: &[u8],
        map: HashMap<String, Value>,
    ) -> Result<HashMap<String, Value>, WebStoreError> {
        let backup = atomic_file::backup_pre_migration(path, original, STORE_SCHEMA_VERSION)
            .map_err(|source| WebStoreError::MigrationBackup {
                path: path.to_path_buf(),
                source,
            })?;
        info!(
            "store file '{}' is a legacy unversioned map; original backed up to '{}', migrating to schema v{STORE_SCHEMA_VERSION}",
            path.display(),
            backup.display()
        );
        match encode(&map, &ExtraKeys::new()).and_then(|bytes| atomic_file::replace(path, &bytes)) {
            Ok(()) => {}
            Err(e) => warn!(
                "could not rewrite migrated store '{}': {e}; the envelope is written on the next successful write",
                path.display()
            ),
        }
        Ok(map)
    }

    /// The backing file path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read a value, or `None` when the key is absent.
    pub fn read(&self, key: &str) -> Result<Option<Value>, io::Error> {
        let lock = self.inner.lock();
        let map = lock
            .as_ref()
            .ok_or_else(|| io::Error::other("STORE_UNAVAILABLE"))?;
        Ok(map.get(key).cloned())
    }

    /// Write a value, persisting atomically. Returns the IO error on failure.
    pub fn write(&self, key: &str, value: Value, expected: Option<Value>) -> io::Result<bool> {
        let mut lock = self.inner.lock();
        let map = lock
            .as_mut()
            .ok_or_else(|| io::Error::other("STORE_UNAVAILABLE"))?;

        if expected.is_some() && map.get(key) != expected.as_ref() {
            return Ok(false); // CAS failed
        }

        let mut next = map.clone();
        next.insert(key.to_string(), value);

        let bytes = encode(&next, &self.extra)?;
        if bytes.len() - envelope_overhead(&self.extra)? > MAX_DATA_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "store data exceeds 10MB limit",
            ));
        }
        atomic_file::replace(&self.path, &bytes)?;

        *map = next;
        Ok(true)
    }

    /// Delete a key, persisting atomically. Returns whether the key existed.
    pub fn delete(&self, key: &str) -> io::Result<bool> {
        let mut lock = self.inner.lock();
        let map = lock
            .as_mut()
            .ok_or_else(|| io::Error::other("STORE_UNAVAILABLE"))?;

        let mut next = map.clone();
        let removed = next.remove(key).is_some();

        let bytes = encode(&next, &self.extra)?;
        atomic_file::replace(&self.path, &bytes)?;

        *map = next;
        Ok(removed)
    }
}

#[cfg(test)]
mod tests;
