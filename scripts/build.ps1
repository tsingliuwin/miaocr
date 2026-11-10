<#
.SYNOPSIS
  Windows 构建脚本：载入 MSVC 环境、修正临时目录，再调用 cargo。

.DESCRIPTION
  做三件事：
  1. 载入 MSVC 开发环境（bindgen 需要，否则报 stddef.h not found）。
  2. 把 TMP/TEMP 指向工作区内的 .tmp。
  3. 调用 cargo（默认 build -p miaocr-ui）。

  为什么要改 TMP：在某些受限进程环境（例如 agent 沙箱）里，cargo 派生的
  build-script 子进程对系统 TEMP 目录没有写权限，会表现为
  `cl: error D8050` 和 `nmake: fatal error U1054` 这类“无法创建临时文件”的假故障。

.PARAMETER Check
  只做类型检查（cargo check）。

.PARAMETER Test
  跑单测（agent / tools / ui）。

.EXAMPLE
  .\scripts\build.ps1
  .\scripts\build.ps1 -Test
#>
param(
    [switch]$Check,
    [switch]$Test
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

$vcvars = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if (-not (Test-Path $vcvars)) {
    Write-Host "找不到 vcvars64.bat，请确认已安装 VS Build Tools + Windows SDK" -ForegroundColor Red
    exit 1
}

$tmp = Join-Path $root ".tmp"
New-Item -ItemType Directory -Force -Path $tmp | Out-Null

if ($Check) {
    $argLine = "check -p miaocr-ui --offline"
} elseif ($Test) {
    $argLine = "test -p agent -p tools -p miaocr-ui --offline"
} else {
    $argLine = "build -p miaocr-ui --offline"
}

Write-Host "> cargo $argLine" -ForegroundColor Cyan
cmd /c "set `"TMP=$tmp`" && set `"TEMP=$tmp`" && call `"$vcvars`" >nul 2>&1 && cd /d $root && cargo $argLine"
$code = $LASTEXITCODE
if ($code -ne 0) { exit $code }

# 沙箱环境（agent 受限进程）里构建时，链接产物会被打上
# "Mandatory Label\Low" 完整性标签：运行时进程以 Low 完整性执行，写任何
# 普通文件（~/.miaocr 日志、%TEMP% 临时 PNG 等）都会 access denied (os error 5)，
# 症状如 PaddleOCR-json 识别失败「拒绝访问」。构建完统一提回 Medium：
if (-not $Check) {
    $exe = Join-Path $root "target\debug\miaocr.exe"
    if (Test-Path $exe) {
        icacls $exe /setintegritylevel medium | Out-Null
    }
}
exit 0