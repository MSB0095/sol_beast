@echo off
REM Sol Beast - Start Backend (Windows)

setlocal

echo Starting Sol Beast Backend...
echo.
echo http://localhost:8080
echo.
echo Press Ctrl+C to stop
echo.

set RUST_LOG=info
cargo run --release

pause
