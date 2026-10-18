@echo off
call "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
set CARGO_TARGET_DIR=E:\open-source\PecutAPP\termul\src-tauri\target-v3-1-1
set CARGO_INCREMENTAL=0
cd /d E:\open-source\PecutAPP\termul\.claude\worktrees\v3-1-1\spike-oq
cargo %*
