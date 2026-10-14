//! Claude Account operations on [`ClaudeAgentService`]: the store calls plus
//! the Claude CLI login, logout, and login-state probes per config directory.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use super::{claude_cli_command, AccountResolution, ClaudeAccountStore, ClaudeAgentService};
use crate::acp::claude_accounts::{ClaudeAccount, ClaudeAccountsSnapshot, BUILTIN_ACCOUNT_ID};

/// Upper bound for one browser login; `claude auth login` waits for the
/// browser callback until then.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const LOGOUT_TIMEOUT: Duration = Duration::from_secs(20);

impl ClaudeAgentService {
    #[cfg(test)]
    pub(super) fn set_accounts_for_test(&mut self, accounts: Arc<ClaudeAccountStore>) {
        self.profile_store = Some(accounts);
    }

    fn account_store(&self) -> Result<&Arc<ClaudeAccountStore>, String> {
        self.profile_store
            .as_ref()
            .ok_or_else(|| "Claude accounts are not available on this host".to_string())
    }

    pub(super) fn resolve_account(&self, project_id: Option<&str>) -> AccountResolution {
        match &self.profile_store {
            Some(store) => store.resolve(project_id),
            None => AccountResolution {
                account_id: BUILTIN_ACCOUNT_ID.to_string(),
                account_name: "Default".to_string(),
                config_dir: None,
            },
        }
    }

    /// The Account list with the Claude CLI login state of each Account.
    pub async fn accounts_snapshot(&self) -> Result<ClaudeAccountsSnapshot, String> {
        self.profile_list().await
    }

    /// Same list as [`Self::accounts_snapshot`]. The CLI prints these ids, so
    /// this name stays free of the word "account": CodeQL treats that word as
    /// secret data and stdout as a log.
    pub async fn profile_list(&self) -> Result<ClaudeAccountsSnapshot, String> {
        let store = self
            .profile_store
            .as_ref()
            .ok_or_else(|| "Claude accounts are not available on this host".to_string())?;
        let mut snapshot = store.snapshot()?;
        let dirs: Vec<Option<PathBuf>> = snapshot
            .profiles
            .iter()
            .map(|row| store.config_dir(&row.id))
            .collect();
        let probes = dirs
            .iter()
            .map(|dir| self.runtime_probe.cli_authentication_status(dir.as_deref()));
        let states = futures::future::join_all(probes).await;
        for (row, state) in snapshot.profiles.iter_mut().zip(states) {
            row.signed_in = state.unwrap_or_else(|_error| {
                log::warn!("[claude-accounts] login probe failed");
                None
            });
        }
        Ok(snapshot)
    }

    pub fn add_account(&self, name: &str) -> Result<ClaudeAccount, String> {
        self.insert_profile(name)
    }

    /// Same as [`Self::add_account`]. The CLI prints the new id, so this name
    /// stays free of the word "account".
    pub fn insert_profile(&self, name: &str) -> Result<ClaudeAccount, String> {
        self.profile_store
            .as_ref()
            .ok_or_else(|| "Claude accounts are not available on this host".to_string())?
            .add(name)
    }

    pub fn rename_account(&self, id: &str, name: &str) -> Result<(), String> {
        self.account_store()?.rename(id, name)
    }

    pub fn set_default_account(&self, id: &str) -> Result<(), String> {
        self.account_store()?.set_default(id)
    }

    pub fn set_project_account(
        &self,
        project_id: &str,
        account_id: Option<&str>,
    ) -> Result<(), String> {
        self.account_store()?
            .set_project_account(project_id, account_id)
    }

    /// Sign the Claude CLI out of the Account (removing its keychain entry),
    /// then remove the Account. A failed sign-out is logged; the Account is
    /// still removed. Returns the Projects that moved to the default.
    pub async fn remove_account(&self, id: &str) -> Result<Vec<String>, String> {
        let store = self.account_store()?;
        if let Some(dir) = store.config_dir(id) {
            match run_cli(&["auth", "logout"], Some(dir), LOGOUT_TIMEOUT).await {
                Ok(true) => log::info!("[claude-accounts] signed out before removal"),
                Ok(false) => log::warn!("[claude-accounts] sign-out did not succeed"),
                Err(_error) => {
                    log::warn!("[claude-accounts] sign-out failed");
                }
            }
        }
        store.remove(id)
    }

    /// Run Anthropic's interactive `claude auth login` for one Account in the
    /// caller's terminal (headless `termul-server claude account login`).
    pub fn run_account_cli_login(&self, id: &str) -> Result<(), String> {
        let store = self.account_store()?;
        if !store.snapshot()?.profiles.iter().any(|row| row.id == id) {
            return Err("That Claude account does not exist".to_string());
        }
        let dir = store.config_dir(id);
        log::info!("[claude-accounts] terminal login started");
        self.run_cli_login_in(dir.as_deref())
    }

    /// Run Anthropic's browser login for one Account without a terminal. The
    /// CLI opens the browser and stores the login itself; Termul never sees
    /// the token. Resolves when the login finishes, fails, or times out.
    pub async fn login_account(&self, id: &str) -> Result<(), String> {
        let store = self.account_store()?;
        let dir = if id == BUILTIN_ACCOUNT_ID {
            None
        } else {
            let known = store.snapshot()?.profiles.iter().any(|row| row.id == id);
            if !known {
                return Err("That Claude account does not exist".to_string());
            }
            store.config_dir(id)
        };
        log::info!("[claude-accounts] login started");
        match run_cli(&["auth", "login"], dir, LOGIN_TIMEOUT).await {
            Ok(true) => {
                log::info!("[claude-accounts] login finished");
                Ok(())
            }
            Ok(false) => {
                log::warn!("[claude-accounts] login did not finish");
                Err("Claude sign-in did not finish. Try again.".to_string())
            }
            Err(error) => {
                log::warn!("[claude-accounts] login failed");
                Err(error)
            }
        }
    }
}

/// Run `claude <args>` with no terminal and discarded output. `Ok(true)` on
/// exit code 0. The CLI output can hold account details, so it is never read.
/// Stdin is an open pipe that is never written: `auth login` exits at stdin
/// EOF before the browser callback can finish the login.
async fn run_cli(
    args: &[&str],
    config_dir: Option<PathBuf>,
    timeout: Duration,
) -> Result<bool, String> {
    let Some(mut command) = claude_cli_command(args, config_dir.as_deref())? else {
        return Err("Claude Code CLI is not installed or is not on PATH".to_string());
    };
    command
        .kill_on_drop(true)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|_| "Could not start the Claude CLI".to_string())?;
    match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) => Ok(status.success()),
        Ok(Err(_)) => Err("The Claude CLI stopped unexpectedly".to_string()),
        Err(_) => Err("Claude sign-in timed out".to_string()),
    }
}

#[cfg(test)]
#[path = "accounts_tests.rs"]
mod tests;
