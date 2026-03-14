param(
  [string]$Target
)

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$workerManifest = Join-Path $repoRoot 'src-tauri\video-worker\Cargo.toml'
$binariesDir = Join-Path $repoRoot 'src-tauri\binaries'
$sdkBinDir = Join-Path $repoRoot 'third_party\ffmpeg\windows-x86_64\bin'

if (-not $Target) {
  $hostLine = & rustc -vV | Select-String '^host: '
  if (-not $hostLine) {
    throw 'failed to detect Rust host target triple'
  }
  $Target = $hostLine.ToString().Split(':', 2)[1].Trim()
}

& powershell -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'verify-ffmpeg-sdk.ps1') | Out-Host
if ($LASTEXITCODE -ne 0) {
  throw 'FFmpeg SDK preflight failed'
}

New-Item -ItemType Directory -Force -Path $binariesDir | Out-Null

$buildArgs = @(
  'build',
  '--manifest-path', $workerManifest,
  '--release',
  '--target', $Target
)

& cargo @buildArgs
if ($LASTEXITCODE -ne 0) {
  throw 'video-worker build failed'
}

$exeSuffix = if ($Target -like '*windows*') { '.exe' } else { '' }
$builtSidecar = Join-Path $repoRoot "src-tauri\video-worker\target\$Target\release\video-worker$exeSuffix"
$bundledSidecar = Join-Path $binariesDir "video-worker-$Target$exeSuffix"

if (-not (Test-Path $builtSidecar -PathType Leaf)) {
  throw "built sidecar not found: $builtSidecar"
}

Copy-Item -Path $builtSidecar -Destination $bundledSidecar -Force

foreach ($dll in @(
  'avcodec-62.dll',
  'avdevice-62.dll',
  'avfilter-11.dll',
  'avformat-62.dll',
  'avutil-60.dll',
  'swresample-6.dll',
  'swscale-9.dll'
)) {
  $source = Join-Path $sdkBinDir $dll
  if (Test-Path $source -PathType Leaf) {
    Copy-Item -Path $source -Destination (Join-Path $binariesDir $dll) -Force
  }
}

[pscustomobject]@{
  target = $Target
  sidecar = $bundledSidecar
  binaries_dir = $binariesDir
} | Format-List
