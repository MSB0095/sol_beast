@echo off
REM Moved to sol_beast_scripts/windows/start-backend.bat
setlocal
set SCRIPT_DIR=%~dp0
set ROOT_DIR=%SCRIPT_DIR%\..\..
cd /d "%ROOT_DIR%"
echo Starting Sol Beast Backend...
echo.
echo http://localhost:8080
echo.
echo Press Ctrl+C to stop
echo.
set RUST_LOG=info
cargo run --release
pause

@echo off
REM Wrapper: Start Sol Beast Backend (Windows)
call "%~dp0\..\..\run-backend.bat" %*
