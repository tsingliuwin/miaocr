# MiaoCR Release Packaging Script
# Usage: powershell -ExecutionPolicy Bypass -File scripts\package.ps1

param(
    [string]$OutputDir = "dist"
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $ProjectRoot

# Ensure cmake is on PATH (winget installs to this location)
$cmakePath = "C:\Program Files\CMake\bin"
if ((Test-Path $cmakePath) -and ($env:PATH -notlike "*CMake*")) {
    $env:PATH = $cmakePath + ";" + $env:PATH
}

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

# miaocr.exe is always required
Write-Host "[3/4] Copying files..." -ForegroundColor Yellow
$exe = Join-Path $ReleaseDir "miaocr.exe"
if (-not (Test-Path $exe)) {
    Write-Host "  ERROR: miaocr.exe not found!" -ForegroundColor Red
    exit 1
}
Copy-Item $exe $DistPath
$exeSize = [math]::Round((Get-Item $exe).Length / 1MB, 2)
Write-Host "  miaocr.exe - $exeSize MB" -ForegroundColor Green

# MNN.dll: only needed when using prebuilt MNN (not build-from-source)
$mnnDll = Join-Path $ReleaseDir "MNN.dll"
if (Test-Path $mnnDll) {
    $mnnInfo = Get-Item $mnnDll
    if ($mnnInfo.Length -gt 0) {
        Copy-Item $mnnDll $DistPath
        $mnnSize = [math]::Round($mnnInfo.Length / 1MB, 2)
        Write-Host "  MNN.dll - $mnnSize MB (dynamic link mode)" -ForegroundColor Yellow
    } else {
        Write-Host "  MNN.dll is 0 bytes, skipping (static link mode)" -ForegroundColor DarkGray
    }
} else {
    Write-Host "  MNN.dll not found - static link mode, single exe!" -ForegroundColor Green
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
