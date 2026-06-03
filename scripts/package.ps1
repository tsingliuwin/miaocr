# MiaoCR Release Packaging Script
# Usage: powershell -ExecutionPolicy Bypass -File scripts\package.ps1

param(
    [string]$OutputDir = "dist"
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $ProjectRoot

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  MiaoCR Release Packager" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# 1. Build release
Write-Host "[1/4] Building release..." -ForegroundColor Yellow
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Host "  Build FAILED!" -ForegroundColor Red
    exit 1
}
Write-Host "  Build OK" -ForegroundColor Green

# 2. Prepare output directory
$DistPath = Join-Path $ProjectRoot $OutputDir
Write-Host "[2/4] Preparing output: $DistPath" -ForegroundColor Yellow
if (Test-Path $DistPath) {
    Remove-Item -Recurse -Force $DistPath
}
New-Item -ItemType Directory -Path $DistPath -Force | Out-Null
Write-Host "  Directory ready" -ForegroundColor Green

# 3. Copy required files
$ReleaseDir = Join-Path $ProjectRoot "target\release"
$RequiredFiles = @("miaocr.exe", "MNN.dll")

Write-Host "[3/4] Copying files..." -ForegroundColor Yellow
foreach ($fname in $RequiredFiles) {
    $src = Join-Path $ReleaseDir $fname
    if (-not (Test-Path $src)) {
        Write-Host "  ERROR: $fname not found!" -ForegroundColor Red
        exit 1
    }
    $fileInfo = Get-Item $src
    if ($fileInfo.Length -eq 0) {
        Write-Host "  ERROR: $fname is 0 bytes!" -ForegroundColor Red
        exit 1
    }
    Copy-Item $src $DistPath
    $sizeMB = [math]::Round($fileInfo.Length / 1MB, 2)
    Write-Host "  $fname - $sizeMB MB" -ForegroundColor Green
}

# 4. Summary
Write-Host "[4/4] Done!" -ForegroundColor Yellow
Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Release files:" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Get-ChildItem $DistPath | ForEach-Object {
    $sizeMB = [math]::Round($_.Length / 1MB, 2)
    Write-Host "  $($_.Name) - $sizeMB MB" -ForegroundColor White
}
$totalSize = [math]::Round((Get-ChildItem $DistPath | Measure-Object -Property Length -Sum).Sum / 1MB, 2)
Write-Host "  ------------------------" -ForegroundColor DarkGray
Write-Host "  Total: $totalSize MB" -ForegroundColor White
Write-Host ""
Write-Host "Output: $DistPath" -ForegroundColor Green
