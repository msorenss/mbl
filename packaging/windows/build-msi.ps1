# PowerShell script to build MBL Media Server WiX MSI installer
param(
    [string]$Version = "1.1.0",
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

# Compile with candle.exe
$wxsPath = "packaging\windows\mbl.wxs"
$wixObj = "$OutputDir\mbl.wixobj"
$msiOut = "$OutputDir\mbl-$Version-windows-x86_64.msi"

Write-Host "Compiling $wxsPath to $wixObj..."
candle.exe -arch x64 -ext WixFirewallExtension -dVersion="$Version" -out "$wixObj" "$wxsPath"
if ($LASTEXITCODE -ne 0) {
    throw "WiX compilation failed with exit code $LASTEXITCODE"
}

Write-Host "Linking $wixObj into $msiOut..."
light.exe -ext WixFirewallExtension -sval -b "$OutputDir" "$wixObj" -out "$msiOut"
if ($LASTEXITCODE -ne 0) {
    throw "WiX linking failed with exit code $LASTEXITCODE"
}

Write-Host "Successfully generated MSI installer: $msiOut"
