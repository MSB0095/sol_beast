@echo off
REM Sol Beast - Windows Deployment Script
REM Complete setup and deployment for Windows

setlocal enabledelayedexpansion

REM Colors (Windows 10+)
set "BLUE=[94m"
set "GREEN=[92m"
set "YELLOW=[93m"
set "RED=[91m"
set "RESET=[0m"

set FRONTEND_PORT=3000
set BACKEND_PORT=8080

echo %BLUE%================================%RESET%
echo %BLUE%Sol Beast - Windows Deployment%RESET%
echo %BLUE%================================%RESET%
echo.

REM Check requirements
echo Checking requirements...

where node >nul 2>nul
if %ERRORLEVEL% NEQ 0 (
    echo %RED%X Node.js not found%RESET%
    echo Please install Node.js from https://nodejs.org/
    pause
    exit /b 1
)
echo %GREEN%✓ Node.js found%RESET%

where cargo >nul 2>nul
if %ERRORLEVEL% NEQ 0 (
    echo %RED%X Rust/Cargo not found%RESET%
    echo Please install Rust from https://rustup.rs/
    pause
    exit /b 1
)
echo %GREEN%✓ Rust/Cargo found%RESET%
echo.

REM Setup frontend
echo Setting up frontend...
if not exist "frontend" (
    echo %RED%X Frontend directory not found%RESET%
    pause
    exit /b 1
)

cd frontend
if not exist "node_modules" (
    call npm install
)
cd ..
echo %GREEN%✓ Frontend setup complete%RESET%
echo.

REM Build backend
echo Building backend...
call cargo build --release
if %ERRORLEVEL% NEQ 0 (
    echo %RED%X Backend build failed%RESET%
    pause
    exit /b 1
)
echo %GREEN%✓ Backend build complete%RESET%
echo.

REM Display startup info
echo %BLUE%================================%RESET%
echo %BLUE%Startup Information%RESET%
echo %BLUE%================================%RESET%
echo.
echo %GREEN%Frontend URL:%RESET% http://localhost:%FRONTEND_PORT%
echo %GREEN%Backend URL:%RESET% http://localhost:%BACKEND_PORT%
echo %GREEN%API Base:%RESET% http://localhost:%BACKEND_PORT%/api
echo.
echo %YELLOW%Next Steps:%RESET%
echo 1. Open two Command Prompt windows
echo 2. In first window, run: run-backend.bat
echo 3. In second window, run: run-frontend.bat
echo 4. Open http://localhost:%FRONTEND_PORT% in browser
echo.
pause
