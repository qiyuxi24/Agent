@echo off
REM ============================================================
REM  Votek - Dev Mode
REM ============================================================
cd /d "%~dp0"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

REM Auto-switch Node 22 if nvm-windows is available
where nvm >nul 2>&1 && nvm use 22 >nul 2>&1

echo.
echo ========================================
echo   Votek - Dev Mode
echo   (LLM proxy inside Rust, no extra server)
echo ========================================
echo.
node scripts\build\index.mjs dev
pause
