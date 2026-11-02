# Installing on Windows

MBL provides an official **WiX-based MSI Installer** (`mbl-<version>-windows-x86_64.msi`) as well as a standalone portable `.zip` archive.

## Installation via MSI Wizard

1. Download `mbl-<version>-windows-x86_64.msi` from GitHub Releases.
2. Run the installer (prompts for Administrator elevation).
3. Walk through the interactive wizard:
   - **Destination Folder:** Choose where MBL is installed (defaults to `C:\Program Files\MBL\`, click **Change...** to pick any drive or folder).
   - **Custom Setup & Windows Service:** Choose which components to install:
     - **MBL Media Server:** Core server binary (`bin\mbl-server.exe`), Start Menu shortcuts, and Windows Firewall rule (TCP port 8420).
     - **Windows Background Service (Auto-Start):** Registers and starts `MblServer` as an automatic Windows background service. Uncheck this option if you prefer to start `mbl-server.exe` manually in a console window.
     - **Transcode Worker (`mbl-worker.exe`):** Distributed hardware transcode worker.
     - **Federated Library Node (`mbl-node.exe`):** Remote storage node.
     - **MBL TV 10-foot Client (`mbl-tv.exe`):** Fullscreen TV interface.

## Managing the Windows Service via CLI

You can also install, start, stop, or remove the Windows background service at any time from an Administrator PowerShell prompt:

```powershell
# Register and start MBL as an automatic Windows background service:
.\mbl-server.exe service install

# Stop or start the service:
.\mbl-server.exe service stop
.\mbl-server.exe service start

# Remove the Windows service registration:
.\mbl-server.exe service uninstall
```

## Windows Package Manager (`winget`)

```powershell
winget install msorenss.mbl
```

## Media Storage & Folder Permissions

If your library folders are stored inside a restricted Windows profile folder, ensure the service account has read permissions:

```powershell
# Run PowerShell as Administrator:
icacls "C:\Users\<Username>\Videos" /grant "*S-1-5-18:(OI)(CI)RX" /grant "NETWORK SERVICE:(OI)(CI)RX"
```

Dedicated secondary drives (e.g. `D:\Media`) typically inherit read access for system services automatically.

## Hardware Acceleration on Windows

- **Intel:** QuickSync Video (QSV) via Intel Graphics Driver.
- **NVIDIA:** NVENC / NVDEC via GeForce / RTX Driver.
- **AMD:** Advanced Media Framework (AMF) via Radeon Adrenalin Driver.
- **DirectX 11:** Direct3D11 Video Acceleration (D3D11VA) for native DXVA decoding.
