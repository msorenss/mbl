# Backups & Disaster Recovery

MBL relies on a clean, single-file SQLite database with write-ahead logging (WAL), making backups fast, consistent, and corruption-free.

## Online Hot Backups (`VACUUM INTO`)

Backups can be performed while the server is actively streaming and scanning without locking readers or writers:

With MCP, the tool `create_backup` makes one. It uses SQLite's `VACUUM INTO`, producing a clean, defragmented snapshot next to the database in `backups/` (for example `/var/lib/mbl/backups`).

## Scheduled maintenance

**Server settings → Database maintenance** runs at a chosen local hour every 1–30 days (on by default, at 04:00 daily):

- `PRAGMA optimize` and a write-ahead log checkpoint; `VACUUM` when a fifth or more of the file is unused or after a cleanup.
- An automatic backup, `backups/mbl-auto-<time>.db`; the chosen number is kept (default 7, 0 makes none). Other backups are never touched.
- Finished and failed jobs older than 30 days are removed.
- Titles whose files have all been missing for the chosen number of days are removed, with empty seasons, shows, albums and artists. This happens automatically only when a number of days is set (7–365; default never). **Preview** shows how many and which, and **Remove gone titles** removes them after a confirmation. Removing a title also removes its watch history, so the limit should be long enough to cover a disk being away.
- A library is left alone when one of its folders cannot be read or is empty (an unplugged disk often leaves an empty mount point), and titles on storage nodes are never removed.

`POST /api/v1/system/maintenance/run` (`dryRun`, `cleanup`) and the MCP tool `run_maintenance` (`dry_run`, then `confirm: true`) do the same.

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
