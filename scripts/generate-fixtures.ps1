#!/usr/bin/env pwsh
# generate-fixtures.ps1
# Generates deterministic test video fixtures for video-worker tests.
# Requires: vendored ffmpeg.exe at third_party/ffmpeg/windows-x86_64/bin/ffmpeg.exe

param(
    [switch]$Force
)

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
if (-not (Test-Path "$RepoRoot/src-tauri")) {
    $RepoRoot = Split-Path -Parent $PSScriptRoot
}
$FfmpegExe = Join-Path $RepoRoot "third_party/ffmpeg/windows-x86_64/bin/ffmpeg.exe"
$FixtureDir = Join-Path $RepoRoot "tests/fixtures/videos"
$ManifestPath = Join-Path $RepoRoot "tests/fixtures/manifest.json"

if (-not (Test-Path $FfmpegExe)) {
    Write-Error "ffmpeg.exe not found at $FfmpegExe"
    exit 1
}

New-Item -ItemType Directory -Force -Path $FixtureDir | Out-Null

function Generate-If-Missing {
    param([string]$Name, [string[]]$Args)
    $OutPath = Join-Path $FixtureDir $Name
    if ((Test-Path $OutPath) -and -not $Force) {
        Write-Host "[skip] $Name already exists (use -Force to regenerate)"
        return
    }
    Write-Host "[generate] $Name ..."
    $AllArgs = @("-y", "-hide_banner", "-loglevel", "error") + $Args + @($OutPath)
    $ErrorActionPreference = "Continue"
    & $FfmpegExe @AllArgs 2>&1 | Out-Null
    $ErrorActionPreference = "Stop"
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Failed to generate $Name"
        exit 1
    }
    Write-Host "[ok] $Name"
}

# --- CFR fixture: 10 frames at 30fps, 64x48, solid color per frame ---
# Uses lavfi testsrc2 which is deterministic.
Generate-If-Missing "cfr_30fps_10frames.mp4" @(
    "-f", "lavfi",
    "-i", "testsrc2=size=64x48:rate=30:duration=0.3334",
    "-c:v", "libx264",
    "-preset", "ultrafast",
    "-pix_fmt", "yuv420p",
    "-an"
)

# --- CFR fixture: 60 frames at 25fps, 128x96 (longer sample for stress) ---
Generate-If-Missing "cfr_25fps_60frames.mp4" @(
    "-f", "lavfi",
    "-i", "testsrc2=size=128x96:rate=25:duration=2.4",
    "-c:v", "libx264",
    "-preset", "ultrafast",
    "-pix_fmt", "yuv420p",
    "-an"
)

# --- VFR fixture: concat segments with different frame rates ---
# Segment 1: 5 frames at 10fps (0.5s), Segment 2: 5 frames at 30fps (~0.167s)
# We achieve VFR by generating two clips and concatenating with a raw timestamp approach.
$VfrTempDir = Join-Path $env:TEMP "countdelay_vfr_$$"
New-Item -ItemType Directory -Force -Path $VfrTempDir | Out-Null

try {
    $Seg1 = Join-Path $VfrTempDir "seg1.mp4"
    $Seg2 = Join-Path $VfrTempDir "seg2.mp4"
    $ConcatList = Join-Path $VfrTempDir "concat.txt"

    $ErrorActionPreference = "Continue"
    & $FfmpegExe -y -hide_banner -loglevel error -f lavfi -i "testsrc2=size=64x48:rate=10:duration=0.5" `
        -c:v libx264 -preset ultrafast -pix_fmt yuv420p -an $Seg1 2>&1 | Out-Null
    & $FfmpegExe -y -hide_banner -loglevel error -f lavfi -i "testsrc2=size=64x48:rate=30:duration=0.1667" `
        -c:v libx264 -preset ultrafast -pix_fmt yuv420p -an $Seg2 2>&1 | Out-Null
    $ErrorActionPreference = "Stop"

    "file '$Seg1'`nfile '$Seg2'" | Out-File -Encoding ascii $ConcatList

    $VfrOut = Join-Path $FixtureDir "vfr_mixed_rate.mp4"
    if ((Test-Path $VfrOut) -and -not $Force) {
        Write-Host "[skip] vfr_mixed_rate.mp4 already exists (use -Force to regenerate)"
    } else {
        Write-Host "[generate] vfr_mixed_rate.mp4 ..."
        $ErrorActionPreference = "Continue"
        & $FfmpegExe -y -hide_banner -loglevel error -f concat -safe 0 -i $ConcatList -c copy $VfrOut 2>&1 | Out-Null
        $ErrorActionPreference = "Stop"
        if ($LASTEXITCODE -ne 0) {
            Write-Error "Failed to generate vfr_mixed_rate.mp4"
            exit 1
        }
        Write-Host "[ok] vfr_mixed_rate.mp4"
    }
} finally {
    Remove-Item -Recurse -Force $VfrTempDir -ErrorAction SilentlyContinue
}

# --- B-frame fixture: video with B-frames for reorder testing ---
Generate-If-Missing "bframes_30fps.mp4" @(
    "-f", "lavfi",
    "-i", "testsrc2=size=64x48:rate=30:duration=1.0",
    "-c:v", "libx264",
    "-preset", "medium",
    "-bf", "2",
    "-g", "12",
    "-pix_fmt", "yuv420p",
    "-an"
)

# --- Build manifest ---
Write-Host ""
Write-Host "Building manifest ..."
$Manifest = @{
    generated_by = "scripts/generate-fixtures.ps1"
    ffmpeg_version = (& $FfmpegExe -version 2>&1 | Where-Object { $_ -is [string] -or $_ -is [System.Management.Automation.ErrorRecord] } | Select-Object -First 1).ToString()
    fixtures = @()
}

Get-ChildItem -Path $FixtureDir -Filter "*.mp4" | ForEach-Object {
    $Hash = (Get-FileHash -Algorithm SHA256 $_.FullName).Hash.ToLower()
    $Manifest.fixtures += @{
        name = $_.Name
        sha256 = $Hash
        size_bytes = $_.Length
    }
}

$Manifest | ConvertTo-Json -Depth 4 | Out-File -Encoding utf8 $ManifestPath
Write-Host "[ok] manifest.json written with $($Manifest.fixtures.Count) fixtures"
Write-Host ""
Write-Host "All fixtures ready."
