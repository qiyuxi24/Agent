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
    if %ERRORLEVEL% NEQ 0 (
        echo [ERROR] npm install failed
        pause
        exit /b 1
    )
)

:: 检查 code-server 是否已就绪（IDE 功能需要，约 212MB）
set "CS_ENTRY=src-tauri\binaries\code-server\release\out\node\entry.js"
if not exist "%CS_ENTRY%" (
    echo [INFO] Code Server not found, downloading (one-time, ~212MB)...
    cd /d "%~dp0"
    node scripts\download-code-server.mjs
    cd /d "%~dp0agent-desktop"
    if %ERRORLEVEL% NEQ 0 (
        echo [WARN] Code Server download failed. IDE feature will be unavailable.
        echo [WARN] You can retry later: run setup-code-server.bat
    )
)

echo.
echo ========================================
echo   Agent Desktop - Dev Mode
echo   LLM proxy runs inside Rust (no extra server needed)
echo ========================================
echo.

call npm run tauri dev

pause
