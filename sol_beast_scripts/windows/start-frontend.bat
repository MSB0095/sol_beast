@echo off
REM Moved to sol_beast_scripts/windows/start-frontend.bat
setlocal
REM Determine project root
set SCRIPT_DIR=%~dp0
set ROOT_DIR=%SCRIPT_DIR%\..\..
REM Detect frontend directory name
if exist "%ROOT_DIR%sol_beast_frontend" (
    set FRONTEND_DIR=%ROOT_DIR%sol_beast_frontend
) else (
    set FRONTEND_DIR=%ROOT_DIR%frontend
)

cd /d "%FRONTEND_DIR%"

echo Starting Sol Beast Frontend...
echo.
echo http://localhost:3000
echo.
echo Press Ctrl+C to stop
echo.

call npm run dev

pause

@echo off
REM Wrapper: Start Sol Beast Frontend (Windows)
call "%~dp0\..\..\run-frontend.bat" %*
