# PowerShell script to build MBL Media Server WiX MSI installer
param(
    [string]$Version = "0.6.0",
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

# Copy binaries into staging
if (Test-Path "$BinDir\mbl-server.exe") {
    Copy-Item "$BinDir\mbl-server.exe" "$OutputDir\"
} else {
    Write-Warning "$BinDir\mbl-server.exe not found! Please build release binaries first."
}

if (Test-Path "$BinDir\mbl-tv.exe") {
    Copy-Item "$BinDir\mbl-tv.exe" "$OutputDir\"
} else {
    # Placeholder if TV client wasn't built for this target
    Set-Content -Path "$OutputDir\mbl-tv.exe" -Value "mbl-tv placeholder"
}

if (Test-Path "$BinDir\mbl-worker.exe") {
    Copy-Item "$BinDir\mbl-worker.exe" "$OutputDir\"
} else {
    # Placeholder until M8 distributed workers
    Set-Content -Path "$OutputDir\mbl-worker.exe" -Value "mbl-worker placeholder"
}

# Compile with candle.exe
$wxsPath = "packaging\windows\mbl.wxs"
$wixObj = "$OutputDir\mbl.wixobj"
$msiOut = "$OutputDir\mbl-$Version-windows-x86_64.msi"

Write-Host "Compiling $wxsPath to $wixObj..."
candle.exe -ext WixFirewallExtension -dVersion="$Version" -out "$wixObj" "$wxsPath"

Write-Host "Linking $wixObj into $msiOut..."
light.exe -ext WixFirewallExtension -sval "$wixObj" -out "$msiOut"

Write-Host "Successfully generated MSI installer: $msiOut"
