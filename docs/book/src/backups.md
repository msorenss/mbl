# Backups & Disaster Recovery

MBL relies on a clean, single-file SQLite database with write-ahead logging (WAL), making backups fast, consistent, and corruption-free.

## Online Hot Backups (`VACUUM INTO`)

Backups can be performed while the server is actively streaming and scanning without locking readers or writers:

```bash
# Trigger hot backup via MCP:
# Tool: create_backup {}

# Or trigger via REST endpoint (Admin token required):
POST /api/v1/settings/backup
```

This invokes SQLite's native `VACUUM INTO '<backup_path>'`, producing a clean, defragmented snapshot of the primary database.

## Restoring from Backup

To restore an MBL server:

```bash
# 1. Stop the MBL service
sudo systemctl stop mbl

# 2. Replace the database file
sudo cp /path/to/backup.db /var/lib/mbl/mbl.db

# 3. Remove stale WAL and SHM files
sudo rm -f /var/lib/mbl/mbl.db-wal /var/lib/mbl/mbl.db-shm

# 4. Fix permissions and start service
sudo chown mbl:mbl /var/lib/mbl/mbl.db
sudo systemctl start mbl
```
