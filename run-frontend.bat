@echo off
REM Sol Beast - Start Frontend (Windows)

setlocal

cd frontend

echo Starting Sol Beast Frontend...
echo.
echo http://localhost:3000
echo.
echo Press Ctrl+C to stop
echo.

call npm run dev

pause
