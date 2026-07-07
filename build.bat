@echo off
setlocal enabledelayedexpansion

set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

cd /d "%~dp0agent-desktop\"
if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] Cannot find agent-desktop directory
    pause
    exit /b 1
)

where node >nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] Node.js not found
    pause
    exit /b 1
)

where cargo >nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] Rust/Cargo not found
    pause
    exit /b 1
)

if not exist "node_modules\" (
    echo [INFO] Installing npm dependencies...
    call npm install
)

echo.
echo ========================================
echo   Agent Desktop - Full Build
echo   (Rust + Frontend)
echo ========================================
echo.

call npm run tauri build

if %ERRORLEVEL% NEQ 0 (
    echo.
    echo [ERROR] Build failed
    pause
    exit /b 1
)

echo.
echo ========================================
echo   Build complete!
echo   Output: src-tauri\target\release\bundle\
echo.
echo   Architecture:
echo   agent-desktop.exe  <-- Rust binary (compiled once)
echo   dist\              <-- Frontend (can rebuild separately)
echo.
echo   To update UI only: run build-frontend.bat
echo ========================================
pause
