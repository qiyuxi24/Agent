@echo off
REM ============================================================
REM Votek App Icon Generator (Windows)
REM ------------------------------------------------------------
REM WHAT THIS DOES
REM   1. Runs the pixel-bear generator (Node) to write:
REM        agent-desktop\src-tauri\icons\icon.svg
REM   2. Runs "npx tauri icon" to regenerate EVERY platform icon
REM      (ico, icns, png, Windows Store, iOS, Android) in place,
REM      replacing all previous icon files.
REM
REM HOW TO USE
REM   - Double-click this .bat, or run it from a terminal.
REM   - To change pixel granularity, edit GRID below.
REM       Lower GRID  = bigger pixels / more retro (e.g. 32)
REM       Higher GRID = smoother / less pixelated (e.g. 128)
REM       64 is the default "slightly pixelated" look.
REM   - You can also pass granularity as an argument:
REM       gen-icon.bat 32
REM
REM REQUIREMENTS
REM   - Node.js 22.x on PATH
REM   - npm / npx available
REM   - This .bat finds repo paths by itself (no cd needed)
REM ============================================================

REM ---- Granularity (pixel grid resolution) ----
set GRID=64
if not "%~1"=="" set GRID=%~1

REM repo root = parent of this scripts\ folder
set "ROOT=%~dp0.."

echo [Votek] Generating pixel bear icon (grid=%GRID%)...
node "%~dp0gen-bear-icon.mjs" %GRID%
if errorlevel 1 (
  echo [Votek] ERROR: icon generator failed.
  pause
  exit /b 1
)

echo [Votek] Regenerating all platform icons (replaces every icon file)...
cd /d "%ROOT%"
call npx tauri icon src-tauri\icons\icon.svg
if errorlevel 1 (
  echo [Votek] ERROR: tauri icon failed.
  pause
  exit /b 1
)

echo [Votek] Done. All app icon files have been replaced.
pause
