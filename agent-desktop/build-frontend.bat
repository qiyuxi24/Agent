@echo off
setlocal enabledelayedexpansion

cd /d "%~dp0"
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

if not exist "node_modules\" (
    echo [INFO] Installing npm dependencies...
    call npm install
)

echo.
echo ========================================
echo   Build Frontend Only (no Rust compile)
echo ========================================
echo.

call npm run build

if %ERRORLEVEL% NEQ 0 (
    echo.
    echo [ERROR] Frontend build failed
    pause
    exit /b 1
)

echo.
echo ========================================
echo   Frontend built successfully!
echo   Output: dist\
echo.
echo   To apply changes:
echo     1. Copy dist\ to the exe directory
echo     2. Or just restart the app (dev mode auto-reloads)
echo ========================================
echo.

:: 尝试自动复制到 src-tauri\target\release\ (如果存在)
set "RELEASE_DIR=src-tauri\target\release"
if exist "%RELEASE_DIR%\agent-desktop.exe" (
    echo [INFO] Copying dist\ to release folder...
    xcopy /E /Y /Q dist\ "%RELEASE_DIR%\dist\" >nul
    echo [OK] dist\ copied to %RELEASE_DIR%\dist\
)

pause
