@echo off
setlocal

cd /d "%~dp0agent-desktop\"

echo ========================================
echo   Downloading Code Server (v4.127.0)
echo   Size: ~54MB compressed / ~212MB extracted
echo ========================================
echo.

node "..\scripts\download-code-server.mjs"

if %ERRORLEVEL% NEQ 0 (
    echo.
    echo [ERROR] Code Server download failed.
    echo You can also try: npm run download:code-server
    pause
    exit /b 1
)

echo.
echo [OK] Code Server is ready!
pause
