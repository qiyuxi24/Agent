# code-server 下载/设置脚本
# 从 GitHub Releases 下载 package.tar.gz，解压并安装依赖
# 使用方式：powershell -ExecutionPolicy Bypass -File .\scripts\download-code-server.ps1

param(
    [string]$Version = "4.127.0"
)

$ErrorActionPreference = "Stop"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = Split-Path -Parent $scriptDir
$targetDir = Join-Path $projectRoot "agent-desktop\src-tauri\binaries"
$releaseDir = Join-Path $targetDir "code-server\release"
$entryJs = Join-Path $releaseDir "out\node\entry.js"

Write-Host "=== code-server 下载/设置脚本 ===" -ForegroundColor Cyan
Write-Host "版本: v$Version"
Write-Host "目标: $releaseDir"

# 检查是否已存在
if (Test-Path $entryJs) {
    $confirm = Read-Host "code-server v$Version 已存在，是否重新下载？(y/N)"
    if ($confirm -ne "y" -and $confirm -ne "Y") {
        Write-Host "已跳过。" -ForegroundColor Green
        exit 0
    }
    Remove-Item $releaseDir -Recurse -Force -ErrorAction SilentlyContinue
}

# 创建目录
New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null

# 下载 package.tar.gz
$tarball = Join-Path $targetDir "package.tar.gz"
$url = "https://github.com/coder/code-server/releases/download/v$Version/package.tar.gz"

Write-Host "下载 package.tar.gz ($Version)... 约 54MB" -ForegroundColor Yellow

try {
    # 尝试多种下载方式
    # 1. curl（默认）
    $downloaded = $false
    try {
        Write-Host "  尝试 curl..." -ForegroundColor Gray
        curl.exe -k -L --connect-timeout 30 --max-time 600 --retry 2 `
            -o $tarball $url 2>&1 | Out-Null
        if ((Get-Item $tarball).Length -gt 1000000) {
            $downloaded = $true
        }
    } catch {
        Write-Host "  curl 失败" -ForegroundColor Gray
    }

    # 2. Invoke-WebRequest
    if (-not $downloaded) {
        try {
            Write-Host "  尝试 Invoke-WebRequest..." -ForegroundColor Gray
            Invoke-WebRequest -Uri $url -OutFile $tarball `
                -SkipCertificateCheck -TimeoutSec 600 -UseBasicParsing
            if ((Get-Item $tarball).Length -gt 1000000) {
                $downloaded = $true
            }
        } catch {
            Write-Host "  Invoke-WebRequest 失败: $_" -ForegroundColor Gray
        }
    }

    if (-not $downloaded) {
        Write-Host "所有下载方式均失败！请手动下载：" -ForegroundColor Red
        Write-Host "  $url" -ForegroundColor Yellow
        Write-Host "  保存为: $tarball" -ForegroundColor Yellow
        Write-Host "  然后重新运行本脚本。"
        exit 1
    }

    $size = [math]::Round((Get-Item $tarball).Length / 1MB, 1)
    Write-Host "  下载完成: ${size}MB" -ForegroundColor Green
} catch {
    Write-Host "下载失败: $_" -ForegroundColor Red
    exit 1
}

# 解压
Write-Host "解压 package.tar.gz..." -ForegroundColor Yellow
try {
    Push-Location $releaseDir
    tar -xzf $tarball --strip-components=1
    Pop-Location
    Write-Host "  解压完成" -ForegroundColor Green
} catch {
    Write-Host "tar 解压失败，尝试用 7z..." -ForegroundColor Yellow
    # 用 7z 作为备选
    try {
        Push-Location $releaseDir
        & 7z x $tarball -y | Out-Null
        Pop-Location
        Write-Host "  7z 解压完成" -ForegroundColor Green
    } catch {
        Write-Host "解压失败: $_" -ForegroundColor Red
        Write-Host "请手动解压 $tarball 到 $releaseDir"
        exit 1
    }
}

# 清理 tarball
Remove-Item $tarball -Force -ErrorAction SilentlyContinue

# 安装依赖
Write-Host "安装 npm 依赖（--production）..." -ForegroundColor Yellow
try {
    Push-Location $releaseDir
    $npmResult = npm install --production --ignore-scripts 2>&1
    Pop-Location
    if ($LASTEXITCODE -eq 0) {
        Write-Host "  npm install 完成" -ForegroundColor Green
    } else {
        Write-Host "  npm install 警告（可能部分依赖未安装）: $npmResult" -ForegroundColor Yellow
    }
} catch {
    Write-Host "npm install 失败: $_" -ForegroundColor Red
    Write-Host "请手动在 $releaseDir 运行: npm install --production --ignore-scripts"
}

# 验证
if (Test-Path $entryJs) {
    Write-Host ""
    Write-Host "=== 设置完成 ===" -ForegroundColor Green
    Write-Host "Code Server v$Version 已就绪！"
    Write-Host "入口文件: $entryJs"
    
    # 测试运行
    $version = & node $entryJs --version 2>&1
    Write-Host "版本验证: $version" -ForegroundColor Cyan
} else {
    Write-Host ""
    Write-Host "设置失败：入口文件未找到！" -ForegroundColor Red
    Write-Host "预期路径: $entryJs"
    exit 1
}
