# Installing on Debian & Ubuntu Linux

## APT Repository Installation

```bash
# 1. Add MBL signing key
curl -fsSL https://pkg.mbl.media/gpg.key | sudo gpg --dearmor -o /etc/apt/keyrings/mbl.gpg

# 2. Add repository source
echo "deb [signed-by=/etc/apt/keyrings/mbl.gpg] https://pkg.mbl.media/deb stable main" | sudo tee /etc/apt/sources.list.d/mbl.list

# 3. Install MBL Server
sudo apt update
sudo apt install mbl-server
```

## Manual `.deb` Package Installation

Download the latest release package matching your architecture:

```bash
# For x86_64:
sudo dpkg -i mbl-server_1.0.0_amd64.deb

# For ARM64 (Raspberry Pi / Odroid):
sudo dpkg -i mbl-server_1.0.0_arm64.deb
```

## Systemd Service Management

The installer registers and enables the `mbl.service` systemd unit running under the dedicated unprivileged system user `mbl`:

```bash
# Check service status
sudo systemctl status mbl

# View real-time logs
sudo journalctl -u mbl -f

# Configuration file location:
/etc/mbl/config.toml

# Application data & SQLite database:
/var/lib/mbl/
```

## Media Storage & Folder Permissions

The `mbl` service account needs read and search access to your media folders. If you keep media inside your personal home directory (e.g. `/home/username/Downloads` or `/home/username/Media`):

```bash
# Allow mbl to browse into your home directory:
sudo setfacl -m u:mbl:rx /home/$USER

# Grant recursive read access to your media directory and future files:
sudo setfacl -R -m u:mbl:rx /home/$USER/Media
sudo setfacl -R -d -m u:mbl:rx /home/$USER/Media
```

See [Troubleshooting](../troubleshooting.md#4-library-folder-access--permissions-path-is-not-readable) for more details.
