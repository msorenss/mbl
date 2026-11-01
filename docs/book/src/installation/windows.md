# Installing on Windows

MBL provides an official **WiX-based MSI Installer** (`mbl-setup-x64.msi`) as well as a standalone portable archive.

## Installation via MSI

1. Download `mbl-setup-x64.msi` from GitHub Releases.
2. Run the installer (requires Administrator elevation).
3. The installer configures:
   - **`mbl.exe`:** Core service binary installed in `C:\Program Files\MBL\`.
   - **Windows Service:** `MBLService` configured to start automatically on system boot under `NT AUTHORITY\NetworkService`.
   - **Windows Firewall Rule:** Inbound TCP port 8420 opened for private network profiles.
   - **System Tray Utility:** `mbl-tray.exe` launched at user logon for quick start/stop, log viewing, and web UI opening.

## Windows Package Manager (`winget`)

```powershell
winget install msorenss.mbl
```

## Media Storage & Folder Permissions

On Windows, `MBLService` runs under `NT AUTHORITY\NetworkService`. If your library folders are stored inside your personal Windows profile (such as `C:\Users\<Username>\Videos` or `Downloads`), grant read permissions to the service:

```powershell
# Run PowerShell as Administrator:
icacls "C:\Users\<Username>\Videos" /grant "NETWORK SERVICE:(OI)(CI)RX"
```

Dedicated secondary drives (e.g. `D:\Media`) typically inherit read access for all system services automatically.

## Hardware Acceleration on Windows

- **Intel:** QuickSync Video (QSV) via Intel Graphics Driver.
- **NVIDIA:** NVENC / NVDEC via GeForce / RTX Driver.
- **AMD:** Advanced Media Framework (AMF) via Radeon Adrenalin Driver.
- **DirectX 11:** Direct3D11 Video Acceleration (D3D11VA) for native DXVA decoding.
