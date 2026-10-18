# Browser conformance runner

Drives the desktop browser automation host on a real webview and reports one
pass or fail per capability.

## How it runs

`main.rs` is a test target without the libtest harness, because the event loop
must own the main thread on macOS. It:

1. serves the pages in `../browser_fixtures/pages` from an OS-chosen loopback
   port that is never the dev server port;
2. starts a Tauri app with one main window;
3. installs the real `DesktopBrowserHost` and stands in for the renderer
   (`adapter.rs`): it opens the webview when the host asks for an agent tab.
   Loopback fixtures never raise a consent ticket, so nobody answers one;
4. runs each case in `cases.rs` through `dispatch`, the same entry point the
   host MCP parent uses.

The runner reaches the host only through `termul_manager_lib::conformance_support`
and `adapter.rs`.

It skips unless `TERMUL_BROWSER_CONFORMANCE` is set, so plain `cargo test`
stays headless. `TERMUL_BROWSER_CONFORMANCE_OUT` sets where the JSON summary
goes.

## Expected results

`expected/<os>.json` lists the capabilities that fail on that OS today with
their wire error code. The run fails when a result differs:

- a failure that is not listed;
- a listed capability that now passes (delete its entry);
- a listed capability that fails with another code (update the entry).

`known-test-failures/<os>.txt` plays the same role for failing Rust tests, but
only unlisted failures fail the run.

## Running locally

```sh
cd src-tauri
TERMUL_BROWSER_CONFORMANCE=1 cargo test --test browser_conformance -- --nocapture
```

On Linux wrap the command in `xvfb-run -a` when no display is available. On
Windows build the test executable with `tests/browser_conformance/build-windows-tests.ps1`
first, which installs the Common-Controls manifest the executable needs.
