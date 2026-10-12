//! Windows tests for console hiding and [`KillOnCloseJob`].

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
    assert!(KillOnCloseJob::new_assigned(std::ptr::null_mut(), "test", true).is_none());
}
