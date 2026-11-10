# PowerShell script to build MBL Media Server WiX MSI installer
param(
    [string]$Version = "1.3.6",
    [string]$OutputDir = "staging",
    [string]$BinDir = "target\release"
)

$ErrorActionPreference = "Stop"

Write-Host "=== Building MBL Media Server MSI Installer v$Version ==="

# Check WiX Toolset
$wixCandidates = @(
    "C:\Program Files (x86)\WiX Toolset v3.11\bin",
    "C:\Program Files\WiX Toolset v3.11\bin",
    "$env:WIX\bin"
)

foreach ($cand in $wixCandidates) {
    if (Test-Path "$cand\candle.exe") {
        $env:PATH = "$cand;$env:PATH"
        break
    }
}

if (-not (Get-Command candle.exe -ErrorAction SilentlyContinue)) {
    Write-Warning "WiX Toolset (candle.exe) not found on PATH. Please install WiX v3.11."
    exit 1
}

# Ensure staging directory
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

foreach ($binary in @("mbl-server.exe", "mbl-tv.exe", "mbl-worker.exe", "mbl-node.exe")) {
    $source = Join-Path $BinDir $binary
    if (-not (Test-Path $source)) {
        throw "Required release binary not found: $source"
    }
    Copy-Item -Path $source -Destination $OutputDir -Force
}

# Ensure FFmpeg and FFprobe binaries are present in staging for out-of-the-box scanning & transcoding
$ffmpegExe = Join-Path $OutputDir "ffmpeg.exe"
$ffprobeExe = Join-Path $OutputDir "ffprobe.exe"
if (-not (Test-Path $ffmpegExe) -or -not (Test-Path $ffprobeExe)) {
    $binFfmpeg = Join-Path $BinDir "ffmpeg.exe"
    $binFfprobe = Join-Path $BinDir "ffprobe.exe"
    if ((Test-Path $binFfmpeg) -and (Test-Path $binFfprobe)) {
        Copy-Item -Path $binFfmpeg -Destination $OutputDir -Force
        Copy-Item -Path $binFfprobe -Destination $OutputDir -Force
    } else {
        Write-Host "Downloading Gyan.FFmpeg release essentials for Windows..."
        $zipPath = Join-Path $OutputDir "ffmpeg-essentials.zip"
        $extractDir = Join-Path $OutputDir "ffmpeg_tmp"
        Invoke-WebRequest -Uri "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip" -OutFile $zipPath
        Expand-Archive -Path $zipPath -DestinationPath $extractDir -Force
        $foundFfmpeg = Get-ChildItem -Path $extractDir -Recurse -Filter "ffmpeg.exe" | Select-Object -First 1
        if ($foundFfmpeg) {
            $srcDir = $foundFfmpeg.DirectoryName
            Copy-Item -Path (Join-Path $srcDir "ffmpeg.exe") -Destination $OutputDir -Force
            Copy-Item -Path (Join-Path $srcDir "ffprobe.exe") -Destination $OutputDir -Force
            Write-Host "Staged ffmpeg.exe and ffprobe.exe into $OutputDir"
        } else {
            throw "Failed to extract ffmpeg.exe from downloaded Gyan.FFmpeg archive"
        }
        Remove-Item -Path $zipPath -Force -ErrorAction SilentlyContinue
        Remove-Item -Path $extractDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# Compile with candle.exe
$wxsPath = "packaging\windows\mbl.wxs"
$wixObj = "$OutputDir\mbl.wixobj"
$msiOut = "$OutputDir\mbl-$Version-windows-x86_64.msi"

Write-Host "Compiling $wxsPath to $wixObj..."
candle.exe -arch x64 -ext WixFirewallExtension -ext WixUIExtension -dVersion="$Version" -out "$wixObj" "$wxsPath"
if ($LASTEXITCODE -ne 0) {
    throw "WiX compilation failed with exit code $LASTEXITCODE"
}

Write-Host "Linking $wixObj into $msiOut..."
light.exe -ext WixFirewallExtension -ext WixUIExtension -cultures:en-us -sval -b "$OutputDir" "$wixObj" -out "$msiOut"
if ($LASTEXITCODE -ne 0) {
    throw "WiX linking failed with exit code $LASTEXITCODE"
}

Write-Host "Successfully generated MSI installer: $msiOut"
