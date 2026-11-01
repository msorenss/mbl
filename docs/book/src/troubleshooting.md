# Troubleshooting & Logs

## Viewing Server Logs

MBL utilizes structured JSON and ANSI color logging powered by `tracing`.

```bash
# On Linux with systemd:
sudo journalctl -u mbl -f

# Filter for errors or warnings only:
sudo journalctl -u mbl -p err..warning

# View persistent daily rotating log files:
cat /var/lib/mbl/logs/mbl.log
```

## Common Diagnostic Scenarios

### 1. Transcoding Fails or Falls Back to Software
- Check hardware encoder permissions (`ls -la /dev/dri` on Linux). Ensure user `mbl` is a member of the `render` or `video` group:
  ```bash
  sudo usermod -aG render,video mbl
  ```
- Run the built-in self-test:
  ```bash
  mbl-server hw test
  ```

### 2. Live TV Tuner Not Found
- Verify HDHomeRun UDP broadcast isn't blocked by host firewall:
  ```bash
  sudo ufw allow 65001/udp
  ```
- Check HDHomeRun web interface in browser (`http://<tuner-ip>/lineup.json`).

### 3. Remote Access / Tailscale Issues
- Verify Tailscale daemon socket permissions:
  ```bash
  ls -la /var/run/tailscale/tailscaled.sock
  ```
- Run reachability test via MCP tool `run_reachability_test` to distinguish between CGNAT, Double NAT, or direct public routing.

### 4. Library Folder Access & Permissions ("path is not readable")

MBL runs as a dedicated, unprivileged system account (`mbl` on Linux, `NT AUTHORITY\NetworkService` on Windows) for security. When adding media libraries located inside a user's home folder or external drive, the service account must have read and execute permissions on both the target folder and its parent directories.

#### Linux (Debian, Ubuntu, Raspberry Pi OS)

If storing media in your personal user directory (e.g., `/home/username/Media` or `/home/username/Downloads`):

1. **Allow `mbl` to traverse the parent home directory:**
   ```bash
   # Gives the mbl user read & execute permission to browse through your home directory:
   sudo setfacl -m u:mbl:rx /home/username
   ```

2. **Grant recursive read & execute permissions to your media directory:**
   ```bash
   # Grant access to current files:
   sudo setfacl -R -m u:mbl:rx /home/username/Media

   # Set default ACL so newly created or downloaded files automatically inherit access:
   sudo setfacl -R -d -m u:mbl:rx /home/username/Media
   ```

3. **External Disks & NAS Mounts (`/mnt` or `/media`):**
   Ensure NTFS or ext4 mount options include read permissions for non-root users:
   ```bash
   # In /etc/fstab for NTFS drives, use:
   UUID=... /media/external ntfs-3g defaults,uid=1000,gid=1000,umask=022 0 0
   ```

#### Windows

On Windows, the `MBLService` runs under the `NetworkService` account. Personal user folders (e.g. `C:\Users\<User>\Videos`) restrict access by default.

1. **Grant read permissions via PowerShell (Admin):**
   ```powershell
   icacls "C:\Users\username\Videos" /grant "NETWORK SERVICE:(OI)(CI)RX"
   ```
2. **Dedicated Drives:**
   Placing media on a secondary drive (e.g. `D:\Media` or `E:\Movies`) usually grants read access to all system services automatically without modifying ACLs.
