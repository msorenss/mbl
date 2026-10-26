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
