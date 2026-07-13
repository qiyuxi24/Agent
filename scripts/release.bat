@echo off
setlocal enabledelayedexpansion

:: ============================================================
::  Agent Desktop — 发布脚本
::
::  用法：
::    release.bat 0.2.0          发布指定版本
::    release.bat 0.2.0 --skip    跳过本地构建（只打 tag 推 CI）
::    release.bat 0.2.0 --dry     预览模式，不实际执行
:: ============================================================

set "VERSION=%1"
set "MODE=%2"
if "%MODE%"=="" set "MODE=--build"

:: ==== 参数校验 ====
if "%VERSION%"=="" (
    echo [ERROR] 请提供版本号，如: release.bat 0.2.0
    exit /b 1
)

:: 验证版本号格式 x.y.z
echo %VERSION% | findstr /r "^[0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*$" >nul
if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] 版本号格式错误，应为 x.y.z，如 0.2.0
    exit /b 1
)

:: ==== 路径检查 ====
where node >nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] Node.js 未安装或未加入 PATH
    exit /b 1
)

where cargo >nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] Rust/Cargo 未安装或未加入 PATH
    exit /b 1
)

where git >nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] Git 未安装或未加入 PATH
    exit /b 1
)

:: ==== 确认 git 状态 ====
for /f "tokens=*" %%i in ('git status --porcelain') do set "DIRTY=%%i"
if not "%DIRTY%"=="" (
    echo [WARNING] 工作区有未提交的改动：
    git status --short
    echo.
    set /p "CONTINUE=是否继续？(y/N) "
    if /i not "!CONTINUE!"=="y" exit /b 0
)

:: 确保在 main 分支
for /f "tokens=*" %%i in ('git branch --show-current') do set "BRANCH=%%i"
if not "%BRANCH%"=="main" (
    echo [WARNING] 当前不在 main 分支（当前: %BRANCH%）
    set /p "CONTINUE=是否继续？(y/N) "
    if /i not "!CONTINUE!"=="y" exit /b 0
)

:: ==== 显示发布信息 ====
echo.
echo ========================================
echo   Agent Desktop Release
echo ========================================
echo.
echo   Version:    %VERSION%
echo   Tag:        v%VERSION%
echo   Branch:     %BRANCH%
echo   Mode:       %MODE%
echo.

if "%MODE%"=="--dry" (
    echo [DRY RUN] 预览模式，不会实际执行任何操作。
    exit /b 0
)

:: ==== ① 更新版本号 ====
echo [1/4] 更新版本号到 %VERSION%...

set "CONF=agent-desktop\src-tauri\tauri.conf.json"
if not exist "%CONF%" (
    echo [ERROR] 找不到 %CONF%
    exit /b 1
)

powershell -NoProfile -Command ^
    "$c = Get-Content '%CONF%' -Raw | ConvertFrom-Json; $c.version = '%VERSION%'; $c | ConvertTo-Json -Depth 10 | Set-Content '%CONF%' -Encoding UTF8"

if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] 版本号更新失败
    exit /b 1
)
echo   已更新 tauri.conf.json#version → %VERSION%

:: ==== ② Git 提交 & 打标签 ====
echo [2/4] 提交版本变更并打标签...

git add %CONF%
git commit -m "chore: bump version to v%VERSION%"
if %ERRORLEVEL% NEQ 0 (
    echo [WARNING] git commit 可能无变更或失败，继续...
)

git tag -a "v%VERSION%" -m "Release v%VERSION%"
if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] git tag 失败
    exit /b 1
)
echo   已创建标签 v%VERSION%

:: ==== ③ 本地构建（可选） ====
if "%MODE%"=="--skip" goto :push

echo [3/4] 本地构建安装包...
cd /d "%~dp0..\agent-desktop"

if not exist "node_modules\" (
    echo   安装 npm 依赖...
    call npm install
    if %ERRORLEVEL% NEQ 0 (
        echo [ERROR] npm install 失败
        exit /b 1
    )
)

echo   开始编译（预计 3-8 分钟）...
call npm run tauri build
if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] 构建失败
    cd /d "%~dp0.."
    exit /b 1
)

cd /d "%~dp0.."

:: 复制安装包到 releases/ 目录
if not exist "releases\" mkdir "releases"
set "SRC=agent-desktop\src-tauri\target\release\bundle\nsis\Agent Desktop_%VERSION%_x64-setup.exe"
set "DST=releases\Agent Desktop_%VERSION%_x64-setup.exe"
copy /y "%SRC%" "%DST%" >nul 2>&1
echo   安装包已复制到: %DST%

:push

:: ==== ④ 推送到远程 ====
echo [4/4] 推送到远程仓库...
echo.
echo   即将执行：
echo     git push origin main
echo     git push origin v%VERSION%
echo.
set /p "CONFIRM=确认推送？(y/N) "
if /i not "%CONFIRM%"=="y" (
    echo 已取消。标签和本地改动保留，可手动推送。
    exit /b 0
)

git push origin main
if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] git push 失败
    exit /b 1
)

git push origin v%VERSION%
if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] git push --tags 失败
    exit /b 1
)

:: ==== 完成 ====
echo.
echo ========================================
echo   Release v%VERSION% 完成！
echo ========================================
echo.
echo   已推送到 GitHub。
echo   如果配置了 GitHub Actions，CI 会自动构建并创建 Release。
echo.
echo   GitHub Releases: https://github.com/你的仓库/Agent/releases
echo.
echo   你有两个选择：
echo     A) 等待 CI 自动构建（~8 分钟），然后去 Release 页面审核发布
echo     B) 使用本地已构建好的安装包：
echo        %DST%
echo ========================================
pause
