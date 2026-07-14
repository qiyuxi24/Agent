@echo off
REM ============================================================
REM  Votek - Production Build (calls unified CLI)
REM ============================================================
cd /d "%~dp0"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
echo.
echo ========================================
echo   Votek - Full Build
echo ========================================
echo.
node scripts\build\index.mjs build
if %ERRORLEVEL% NEQ 0 (
    echo.
    echo [ERROR] Build failed
    pause
    exit /b 1
)
echo.
echo ========================================
echo   Output: agent-desktop\src-tauri\target\release\bundle\
echo.
echo   Quick UI-only update: build-frontend.bat
echo ========================================
pause
