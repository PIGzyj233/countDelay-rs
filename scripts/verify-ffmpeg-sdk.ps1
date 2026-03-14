param(
  [string]$SdkRoot,
  [switch]$AsJson
)

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not $SdkRoot) {
  $SdkRoot = Join-Path $repoRoot 'third_party\ffmpeg\windows-x86_64'
}

$requiredHeaders = @(
  'libavcodec/avcodec.h',
  'libavformat/avformat.h',
  'libavutil/avutil.h',
  'libswscale/swscale.h'
)

$requiredLibs = @(
  'avcodec.lib',
  'avdevice.lib',
  'avfilter.lib',
  'avformat.lib',
  'avutil.lib',
  'swresample.lib',
  'swscale.lib'
)

$requiredDlls = @(
  'avcodec-62.dll',
  'avdevice-62.dll',
  'avfilter-11.dll',
  'avformat-62.dll',
  'avutil-60.dll',
  'swresample-6.dll',
  'swscale-9.dll'
)

function New-Issue {
  param(
    [string]$Severity,
    [string]$Code,
    [string]$Message
  )

  [pscustomobject]@{
    severity = $Severity
    code = $Code
    message = $Message
  }
}

$issues = New-Object System.Collections.Generic.List[object]
$includeDir = Join-Path $SdkRoot 'include'
$libDir = Join-Path $SdkRoot 'lib'
$binDir = Join-Path $SdkRoot 'bin'

foreach ($directory in @($includeDir, $libDir, $binDir)) {
  if (-not (Test-Path $directory -PathType Container)) {
    $issues.Add((New-Issue 'error' 'missing-directory' "required directory is missing: $directory"))
  }
}

foreach ($header in $requiredHeaders) {
  $path = Join-Path $includeDir $header
  if (-not (Test-Path $path -PathType Leaf)) {
    $issues.Add((New-Issue 'error' 'missing-header' "required header is missing: $path"))
  }
}

foreach ($lib in $requiredLibs) {
  $path = Join-Path $libDir $lib
  if (-not (Test-Path $path -PathType Leaf)) {
    $issues.Add((New-Issue 'error' 'missing-import-lib' "required import library is missing: $path"))
  }
}

foreach ($dll in $requiredDlls) {
  $path = Join-Path $binDir $dll
  if (-not (Test-Path $path -PathType Leaf)) {
    $issues.Add((New-Issue 'error' 'missing-runtime-dll' "required runtime DLL is missing: $path"))
  }
}

$versionLine = $null
$builtWith = $null
$configurationFlags = @()
$ffmpegExe = Join-Path $binDir 'ffmpeg.exe'

if (Test-Path $ffmpegExe -PathType Leaf) {
  $versionOutput = & $ffmpegExe -version 2>$null
  if ($LASTEXITCODE -eq 0 -and $versionOutput) {
    $versionLine = $versionOutput | Select-Object -First 1
    $builtWith = $versionOutput | Where-Object { $_ -like 'built with *' } | Select-Object -First 1
    $configurationLine = $versionOutput | Where-Object { $_ -like 'configuration: *' } | Select-Object -First 1
    if ($configurationLine) {
      $configurationFlags = $configurationLine.Substring('configuration: '.Length).Split(' ', [System.StringSplitOptions]::RemoveEmptyEntries)
    }
  }
}

if ($configurationFlags -contains '--enable-gpl') {
  $issues.Add((New-Issue 'warning' 'gpl-build' 'FFmpeg bundle enables GPL code; replace it before product distribution if the app is meant to stay LGPL-compatible'))
}

if ($configurationFlags -contains '--enable-nonfree') {
  $issues.Add((New-Issue 'warning' 'nonfree-build' 'FFmpeg bundle enables nonfree code; replace it before product distribution'))
}

if ($builtWith -and $builtWith.ToLowerInvariant().Contains('gcc')) {
  $issues.Add((New-Issue 'warning' 'gcc-built-bundle' 'FFmpeg bundle reports a GCC build; verify the shipped .lib files link cleanly with x86_64-pc-windows-msvc'))
}

$libclangPath = $null
$candidates = New-Object System.Collections.Generic.List[string]
if ($env:LIBCLANG_PATH) {
  if ([System.IO.Path]::GetFileName($env:LIBCLANG_PATH).ToLowerInvariant() -eq 'libclang.dll') {
    $candidates.Add($env:LIBCLANG_PATH)
  } else {
    $candidates.Add((Join-Path $env:LIBCLANG_PATH 'libclang.dll'))
  }
}
if ($env:ProgramFiles) {
  $candidates.Add((Join-Path $env:ProgramFiles 'LLVM\bin\libclang.dll'))
}
$programFilesX86 = [Environment]::GetEnvironmentVariable('ProgramFiles(x86)')
if ($programFilesX86) {
  $candidates.Add((Join-Path $programFilesX86 'LLVM\bin\libclang.dll'))
}

foreach ($candidate in $candidates) {
  if (-not $libclangPath -and (Test-Path $candidate -PathType Leaf)) {
    $libclangPath = $candidate
  }
}

if (-not $libclangPath) {
  $issues.Add((New-Issue 'warning' 'missing-libclang' 'libclang.dll was not found in LIBCLANG_PATH or common Windows LLVM locations; ffmpeg-sys-next will not build on this machine until LLVM is available'))
}

$report = [pscustomobject]@{
  root = $SdkRoot
  include_dir = $includeDir
  lib_dir = $libDir
  bin_dir = $binDir
  version_line = $versionLine
  built_with = $builtWith
  configuration_flags = $configurationFlags
  libclang_path = $libclangPath
  issues = $issues
}

if ($AsJson) {
  $report | ConvertTo-Json -Depth 8
} else {
  $report
}

if ($issues | Where-Object { $_.severity -eq 'error' }) {
  exit 1
}
