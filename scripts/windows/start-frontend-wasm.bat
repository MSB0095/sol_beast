@echo off
REM Start the frontend in wasm-only mode (Windows)
setlocal
REM Determine project root relative to script
set SCRIPT_DIR=%~dp0
set ROOT_DIR=%SCRIPT_DIR%\..\..

REM Detect frontend directory name
if exist "%ROOT_DIR%\sol_beast_frontend\package.json" (
    set FRONTEND_DIR=%ROOT_DIR%\sol_beast_frontend
) else (
    set FRONTEND_DIR=%ROOT_DIR%\frontend
)

echo Building WASM module for frontend (output -> %FRONTEND_DIR%\src\wasm)
REM Attempt to use bash for the wasm build script — Windows users with WSL/Git-Bash should have bash available
bash "%ROOT_DIR%\sol_beast_wasm\wasm-pack-build.sh"

echo Starting Sol Beast Frontend (WASM mode)...
echo http://localhost:3000
echo Mode: frontend-wasm

cd /d "%FRONTEND_DIR%"
set VITE_RUNTIME_MODE=frontend-wasm
call npm run dev
