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
echo   (Rust + Frontend + Code Server)
echo ========================================
echo.

:: 检查/下载 code-server（~212MB，随应用打包）
set "CS_DIR=src-tauri\binaries\code-server\release"
set "CS_ENTRY=%CS_DIR%\out\node\entry.js"
if not exist "%CS_ENTRY%" (
    echo [INFO] Code Server not found, downloading...
    echo [INFO] This is a one-time download (~54MB compressed, ~212MB extracted)
    node "..\scripts\download-code-server.mjs"
    if %ERRORLEVEL% NEQ 0 (
        echo [WARN] Code Server download failed. Please run manually:
        echo        npm run download:code-server
        echo [WARN] Continuing build without Code Server...
    )
) else (
    echo [INFO] Code Server found at %CS_DIR%
)

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
