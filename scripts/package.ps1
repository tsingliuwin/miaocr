# MiaoCR Release Packaging Script
# Usage: powershell -ExecutionPolicy Bypass -File scripts\package.ps1

param(
    [string]$OutputDir = "dist"
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $ProjectRoot

# 与 build.ps1 相同的三件套（缺一不可）：
# 1) vcvars64：bindgen / cl 需要 MSVC 环境，否则 stddef.h not found / D8050
# 2) TMP/TEMP 指向工作区：受限环境里 build-script 子进程写不了系统 TEMP
$vcvars = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if (-not (Test-Path $vcvars)) {
    Write-Host "找不到 vcvars64.bat，请确认已安装 VS Build Tools + Windows SDK" -ForegroundColor Red
    exit 1
}
$tmp = Join-Path $ProjectRoot ".tmp"
New-Item -ItemType Directory -Force -Path $tmp | Out-Null

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
cmd /c "set `"TMP=$tmp`" && set `"TEMP=$tmp`" && call `"$vcvars`" >nul 2>&1 && cargo build --release -p miaocr-ui --offline"
if ($LASTEXITCODE -ne 0) {
    Write-Host "  Build FAILED!" -ForegroundColor Red
    exit 1
}
# 3) 沙箱构建会把产物打成 Low 完整性标签（运行时写文件全 access denied），
#    打包前提回 Medium —— 发布产物绝不能带 Low 标出去。
$releaseExe = Join-Path $ProjectRoot "target\release\miaocr.exe"
if (Test-Path $releaseExe) {
    icacls $releaseExe /setintegritylevel medium | Out-Null
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

# miaocr.exe 是唯一发布的产物（egui 版已移除）
Write-Host "[3/4] Copying files..." -ForegroundColor Yellow
$exeName = "miaocr.exe"
$exe = Join-Path $ReleaseDir $exeName
if (-not (Test-Path $exe)) {
    Write-Host "  ERROR: $exeName not found!" -ForegroundColor Red
    exit 1
}
Copy-Item $exe $DistPath
# 复制是「新建文件」，会继承创建进程（沙箱）的 Low 完整性标签，
# 所以标签修复必须在复制之后对 dist 里的成品再做一次。
icacls (Join-Path $DistPath $exeName) /setintegritylevel medium | Out-Null
$exeSize = [math]::Round((Get-Item $exe).Length / 1MB, 2)
Write-Host "  $exeName - $exeSize MB" -ForegroundColor Green

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
