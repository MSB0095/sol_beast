@echo off
REM Sol Beast - Start Frontend (Windows)

setlocal

REM Detect frontend directory name
if exist "sol_beast_frontend" (
	set FRONTEND_DIR=sol_beast_frontend
) else (
	set FRONTEND_DIR=frontend
)

cd %FRONTEND_DIR%

echo Starting Sol Beast Frontend...
echo.
echo http://localhost:3000
echo.
echo Press Ctrl+C to stop
echo.

call npm run dev

pause
