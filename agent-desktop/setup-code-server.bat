@echo off
REM ============================================================
REM  Votek - Code Server setup (calls unified CLI)
REM ============================================================
cd /d "%~dp0"
echo ========================================
echo   Preparing Code Server
echo ========================================
echo.
node scripts\build\index.mjs prepare
if %ERRORLEVEL% NEQ 0 (
    echo.
    echo [ERROR] Code Server setup failed.
    echo You can also try: npm run download:code-server
    pause
    exit /b 1
)
echo.
echo [OK] Code Server is ready!
pause
