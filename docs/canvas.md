# Canvas

The canvas is a built-in design editor. Each project opens one canvas tab backed by a `.figcraft` document in the project directory. The editor is the figcraft web bundle running in an iframe. Termul serves the bundle, persists the document, and gives agents the figcraft tools. There is no sidecar process and no second copy of the document outside the iframe.

This page describes the shipped design. The architecture decisions it follows are the canvas spine decisions AD-17 to AD-31 (kept in the planning artifacts, not in this repository).

## How it fits together

```text
renderer          canvas tab -> CanvasShell iframe /canvas/<id>/?embed=kiosk&ct=...
                  canvas-bridge.ts   postMessage, figcraft control protocol
                  canvas-api.ts      facade -> tauri-canvas-api.ts / web-canvas-api.ts
host              src-tauri/src/canvas/
                    static_server.rs   bundle serving, ct token store, cookie gate, open/close
                    persistence/       mount registry, lease, autosave, conflicts, close sequence
                    pin.rs             the bundle tag this build expects
                  src-tauri/src/web/canvas_api.rs, canvas_persistence_api.rs   web routes
                  src-tauri/src/commands/canvas.rs                             Tauri commands
                  src-tauri/src/acp/host_mcp/canvas*.rs                        agent gateway
bundle            canvas/figcraft.pin -> canvas/bundle/ (embedded, same asset set as the web UI)
```

- **One document authority.** The figcraft engine inside the iframe is the only live copy of a document. Every read or write of document state, including agent tool calls, goes through that iframe.
- **Termul owns the file.** The editor emits `event.dirtyChanged`. The host waits for `canvas.autosave_ms` of quiet, asks the editor for a `host.snapshot`, writes the `.figcraft` bytes atomically, then calls `host.markSaved`. A user save skips the debounce.
- **Same URL shape everywhere.** The desktop app binds a loopback listener on `127.0.0.1:0` and iframes it. `termul-server` and the desktop shared-live host serve the same bundle same-origin under `/canvas/<id>/`. Remote clients only ever reach the canvas through those routes.

## Documents

A canvas document is a `.figcraft` file. Opening the canvas picks the project root's alphabetically first `.figcraft` file, or creates `design.figcraft` on the first save. Other extensions are never opened as a canvas. The file explorer opens a `.figcraft` file as the project's canvas tab.

## Lifecycle

- **Mount once per project.** The first open mounts the iframe. Closing the tab hides the mount (an offscreen 1x1 visible iframe, never `display:none`) so agents can keep working. A mount ends on project close, an explicit close, eviction, or lease loss.
- **Cap and eviction.** At most `canvas.max_mounted` canvases stay mounted (default 2, clamped to 1..16). Opening one more saves and unloads the least recently active one. A failed save blocks the eviction.
- **One lease per document.** Exactly one renderer (a desktop window or a web tab) holds a document's mount. A newer opener takes the lease, and the previous holder unmounts and shows `CANVAS_OPEN_ELSEWHERE` with a take-back action. A dropped connection keeps the lease for `canvas.lease_grace_ms` (default 5000) so a reload does not lose it. An agent call that arrives in that window waits for the holder to come back, up to the call timeout, and answers `CANVAS_CLOSED` once the grace runs out.
- **Close sequence.** Every unmount runs quiesce, save, verify, unmount. If the save fails the user is asked and nothing is discarded silently.
- **Crash recovery.** An `event.crashed`, or `canvas.crash_timeouts` consecutive timed-out calls (default 3), remounts the editor from the last saved bytes.
- **Disk changes.** Termul's file watcher owns detection. A clean editor reloads silently. A dirty editor suppresses autosave and the user chooses keep mine, take disk, or save a copy.

## Settings

The renderer sends these with each canvas open, and the host clamps them.

| Setting | Default | Meaning |
| --- | --- | --- |
| `canvas.autosave_ms` | 1500 | Quiet time before a dirty document is autosaved |
| `canvas.lease_grace_ms` | 5000 | How long a window keeps its lease after its connection drops |
| `canvas.max_mounted` | 2 | Mounted canvases kept at once |
| `canvas.crash_timeouts` | 3 | Consecutive timed-out calls that mark the editor crashed |
| `canvas.call_timeout_ms` | 10000 | Gateway timeout for one agent call (`CANVAS_TIMEOUT`) |

## Access control

The iframe cannot send an `Authorization` header, so the bundle has its own credential, separate from the web auth bearer.

- `open` mints a per-canvas `ct` token (32 hex characters from the OS CSPRNG) and reuses it on repeat opens.
- The first request to `/canvas/<id>/` carries `?ct=<token>`. The response sets a per-canvas cookie `canvas_ct_<canvasId>` scoped to `/canvas/<id>/`, which authorizes the bundle's sub-resources. Tokens are compared in constant time.
- The desktop cookie is `SameSite=None; Secure` (the iframe is a third-party context on a loopback origin). The same-origin web cookie is `HttpOnly; SameSite=Lax`.
- `close` revokes the token, so stale URLs and cookies stop working.
- Neither the token nor the cookie value is ever logged.
- The `/canvas/open`, `/canvas/close` and persistence routes stay behind the web auth bearer. They are write routes: a view-only peer (remote peer without write access, or the desktop shared-live host) is refused with `FORBIDDEN`. Doc paths must lie inside a registered, non-archived project root.

## Agents

Canvas tools live in the per-session host `termul` MCP server (`acp/host_mcp`). The gateway resolves the session's project, forwards each call to the renderer that holds the mount's lease, and answers from the engine. It caches no document or selection state. Tool names are figcraft's own. `save_file` is Termul's own save of the project document. A project with no mount answers `CANVAS_CLOSED`, a closing one `CANVAS_CLOSING`, and a stalled editor `CANVAS_TIMEOUT`. No MCP registry entry is persisted for the canvas, and no `/canvas/mcp` route exists. On load the renderer drops any `canvas-mcp-*` entries an older build persisted.

## Interfaces

| Surface | Desktop (Tauri command) | Web (`termul-server`) |
| --- | --- | --- |
| Open a project canvas | `canvas_open` | `POST /canvas/open` |
| Close, with optional discard | `canvas_close` | `POST /canvas/close` |
| Persistence round trip | `canvas_read_document`, `canvas_document_loaded`, `canvas_dirty_changed`, `canvas_set_visible`, `canvas_write_snapshot`, `canvas_disk_changed`, `canvas_resolve_conflict`, `canvas_crashed` | `POST /canvas/read-document`, `/document-loaded`, `/dirty-changed`, `/set-visible`, `/write-snapshot`, `/disk-changed`, `/resolve-conflict`, `/crashed` |
| The bundle | loopback listener `http://127.0.0.1:<port>/canvas/<id>/` | `GET /canvas/<id>/` |

Both transports share the same Rust service and typed error codes. Every facade method exists in both `tauri-canvas-api.ts` and `web-canvas-api.ts`, and parity tests hold them together.

## The bundle

- **Pin.** `canvas/figcraft.pin` holds the fork's release tag, download URL and sha256. CI fails when it disagrees with the `figcraft-mcp` dependency tag in `src-tauri/Cargo.toml`.
- **Fetch.** `bun scripts/canvas/fetch-figcraft-bundle.mjs` downloads the pinned zip, verifies the checksum and unpacks it to `canvas/bundle/`. `--zip <file>` uses a local zip. The release workflow runs it before the Rust build, and the bundle is embedded alongside the web UI.
- **Dev override.** Set `TERMUL_FIGCRAFT_BUNDLE` to a built bundle directory (a figcraft checkout's `dist-web`) to serve it instead of the embedded one.
- **Missing bundle.** A build without a bundle logs one warning and answers the canvas routes with a typed error. The rest of Termul is unaffected.
- **Fork.** The bundle comes from `gnoviawan/figcraft` (branch `termul`). Its patches are limited to the web host API, the kiosk flag, the brand knob and the theme hook.

## Retired: the OpenPencil daemon

An earlier canvas embedded OpenPencil as a managed `op-host-web-server` sidecar, one daemon per `.op` document. It was replaced by figcraft and removed in E4-1. Gone with it:

- the daemon pool and spawner, the stdout handshake, and the `TERMUL_OP_HOST_SERVER` and `TERMUL_OPENPENCIL_ROOT` binary lookup,
- the HTTP proxy to the daemon, including the root `/pkg`, `/canvaskit` and `/api` relays (those paths are plain SPA paths now),
- `.op` document handling, the `canvas_save`, `canvas_status` and `canvas_bundle_base` commands, and the `/canvas/save`, `/canvas/bundle-base` and `/canvas/mcp` routes.

The `ct` token store and the per-canvas cookie gate survived, and the cookie was renamed from `op_canvas_ct_<id>` to `canvas_ct_<id>`. Nothing needs migrating: a `.op` file is not a canvas document and is ignored.
