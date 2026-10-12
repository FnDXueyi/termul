//! Process hygiene shared by every background spawn in the crate.
//!
//! - [`hide_console`] applies Windows `CREATE_NO_WINDOW` so short-lived helper
//!   processes (git, codex/claude status probes, npm, ACP terminal commands)
//!   never flash a console window from the GUI app. It is a no-op elsewhere.
//! - [`KillOnCloseJob`] (Windows only) wraps a Job Object configured with
//!   `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`: terminating the job, or closing its
//!   last handle (including when Termul itself exits), kills the assigned
//!   process and every descendant it spawned. Windows has no process-group
//!   kill, so this is the only reliable tree-kill for wrapper launchers and
//!   agents that spawn their own workers.
//!
//! Accepted trade-off of the job: on Windows *every* process the assigned
//! process starts is in the job and dies with it — including a browser or
//! editor an agent (or a terminal command) opens when that app was not
//! already running, and Node `detached: true` spawns, which do not leave the
//! job. This matches the unix process-group kill. The job sets
//! `JOB_OBJECT_LIMIT_BREAKAWAY_OK`, so the only escape hatch is a descendant
//! that explicitly creates its child with `CREATE_BREAKAWAY_FROM_JOB`; without
//! that limit such a `CreateProcess` call would fail with `ACCESS_DENIED`
//! instead. The same policy applies to ConPTY terminals and ACP agents: a
//! process that deliberately asks to outlive its parent is allowed to.

/// Windows `CREATE_NO_WINDOW` process creation flag.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A command builder whose spawned process can be kept off-screen.
pub(crate) trait ConsoleCommand {
    /// Apply `CREATE_NO_WINDOW` on Windows; no-op elsewhere.
    fn hide_console_window(&mut self) -> &mut Self;
}

impl ConsoleCommand for std::process::Command {
    #[cfg(windows)]
    fn hide_console_window(&mut self) -> &mut Self {
        use std::os::windows::process::CommandExt as _;
        self.creation_flags(CREATE_NO_WINDOW)
    }

    #[cfg(not(windows))]
    fn hide_console_window(&mut self) -> &mut Self {
        self
    }
}

impl ConsoleCommand for tokio::process::Command {
    #[cfg(windows)]
    fn hide_console_window(&mut self) -> &mut Self {
        self.creation_flags(CREATE_NO_WINDOW)
    }

    #[cfg(not(windows))]
    fn hide_console_window(&mut self) -> &mut Self {
        self
    }
}

/// Keep a background process from opening a console window on Windows.
///
/// Sets the creation flags to `CREATE_NO_WINDOW` (replacing any flags set
/// earlier), so call it before any other `creation_flags` on the same command.
/// No-op on other platforms. Do not use for spawns that intentionally inherit
/// the caller's terminal (interactive logins).
pub(crate) fn hide_console<C: ConsoleCommand + ?Sized>(command: &mut C) -> &mut C {
    command.hide_console_window()
}

#[cfg(windows)]
pub(crate) use job::KillOnCloseJob;

#[cfg(windows)]
mod job {
    use std::os::windows::io::RawHandle;

    use winapi::um::handleapi::CloseHandle;
    use winapi::um::jobapi2::{
        AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject, TerminateJobObject,
    };
    use winapi::um::winnt::{
        JobObjectExtendedLimitInformation, HANDLE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_BREAKAWAY_OK, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    /// An owned kill-on-close Job Object with one process assigned to it.
    ///
    /// Dropping it closes the handle, which makes the OS terminate every
    /// process still in the job (the assigned process and its descendants).
    #[derive(Debug)]
    pub(crate) struct KillOnCloseJob {
        handle: HANDLE,
    }

    // SAFETY: a job object handle is a kernel handle with no thread affinity;
    // the Win32 job APIs used here are thread-safe.
    unsafe impl Send for KillOnCloseJob {}
    // SAFETY: see `Send` — `terminate` only calls the thread-safe
    // `TerminateJobObject` on the immutable handle.
    unsafe impl Sync for KillOnCloseJob {}

    impl KillOnCloseJob {
        /// Create a kill-on-close job and assign `process` to it. `label`
        /// names the owner in logs (e.g. `"ConPTY"`, `"ACP agent"`).
        ///
        /// Returns `None` (after one `warn` log naming the failing step) when
        /// the job cannot be created, configured, or assigned; the caller keeps
        /// the process running without tree-kill protection. Nested jobs are
        /// supported on Windows 8+, so assignment works even when the process
        /// already inherited a job (e.g. a CI runner or terminal job).
        pub(crate) fn new_assigned(process: RawHandle, label: &str) -> Option<Self> {
            // SAFETY: plain Win32 calls on handles we own (`job`) or that the
            // caller guarantees refer to a live process (`process`). An invalid
            // process handle makes `AssignProcessToJobObject` fail, which is
            // handled below; no memory is shared with the OS beyond `info`,
            // which outlives the call.
            unsafe {
                let job = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
                if job.is_null() {
                    log::warn!(
                        "[{label}] CreateJobObject failed, process tree-kill disabled: {}",
                        std::io::Error::last_os_error()
                    );
                    return None;
                }
                // Own the handle immediately so every early return closes it.
                let job = Self { handle: job };

                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                // BREAKAWAY_OK: a descendant that explicitly asks for
                // CREATE_BREAKAWAY_FROM_JOB escapes instead of failing to start.
                info.BasicLimitInformation.LimitFlags =
                    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_BREAKAWAY_OK;
                if SetInformationJobObject(
                    job.handle,
                    JobObjectExtendedLimitInformation,
                    std::ptr::addr_of_mut!(info).cast(),
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                ) == 0
                {
                    log::warn!(
                        "[{label}] SetInformationJobObject failed, process tree-kill disabled: {}",
                        std::io::Error::last_os_error()
                    );
                    return None;
                }

                if AssignProcessToJobObject(job.handle, process as HANDLE) == 0 {
                    log::warn!(
                        "[{label}] AssignProcessToJobObject failed, process tree-kill disabled: {}",
                        std::io::Error::last_os_error()
                    );
                    return None;
                }

                Some(job)
            }
        }

        /// Terminate every process in the job (the whole tree). Safe to call
        /// repeatedly and after the processes already exited.
        pub(crate) fn terminate(&self) -> std::io::Result<()> {
            // SAFETY: `self.handle` is a valid job handle until `Drop`.
            if unsafe { TerminateJobObject(self.handle, 1) } == 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        }

        /// Give up ownership of the raw job handle. The caller becomes
        /// responsible for closing it (closing it kills the job's processes).
        pub(crate) fn into_raw(self) -> HANDLE {
            let handle = self.handle;
            std::mem::forget(self);
            handle
        }
    }

    impl Drop for KillOnCloseJob {
        fn drop(&mut self) {
            // SAFETY: we own the handle and close it exactly once.
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::{hide_console, KillOnCloseJob};

    /// Prints the child's console window handle (0 = no visible console).
    const PRINT_CONSOLE_WINDOW: &str = "$t = Add-Type -Name W -Namespace P -PassThru \
        -MemberDefinition '[DllImport(\"kernel32.dll\")] public static extern IntPtr GetConsoleWindow();'; \
        [Console]::Out.Write($t::GetConsoleWindow().ToInt64())";

    fn powershell_args() -> [&'static str; 4] {
        [
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            PRINT_CONSOLE_WINDOW,
        ]
    }

    #[test]
    fn hidden_std_command_has_no_console_window() {
        let mut command = std::process::Command::new("powershell");
        hide_console(&mut command).args(powershell_args());
        let output = command.output().expect("spawn powershell");
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "0");
    }

    #[tokio::test]
    async fn hidden_tokio_command_has_no_console_window() {
        let mut command = tokio::process::Command::new("powershell");
        hide_console(&mut command).args(powershell_args());
        let output = command.output().await.expect("spawn powershell");
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "0");
    }

    #[test]
    fn job_assignment_failure_degrades_to_none() {
        // An invalid process handle makes assignment fail; the caller keeps
        // running the process without tree-kill instead of erroring.
        assert!(KillOnCloseJob::new_assigned(std::ptr::null_mut(), "test").is_none());
    }
}
