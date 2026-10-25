#!/bin/sh
set -e

PUID=${PUID:-1000}
PGID=${PGID:-1000}

# Create or reuse group
if ! getent group mbl >/dev/null 2>&1; then
    groupadd -g "$PGID" mbl 2>/dev/null || groupadd mbl
fi

# Create or reuse user
if ! getent passwd mbl >/dev/null 2>&1; then
    useradd -u "$PUID" -g "$PGID" -d /var/lib/mbl -s /usr/sbin/nologin -M mbl 2>/dev/null || useradd -g "$PGID" -d /var/lib/mbl -s /usr/sbin/nologin -M mbl
fi

# Add mbl to video and render groups for GPU / hardware transcode access (/dev/dri)
for grp in video render; do
    if getent group "$grp" >/dev/null 2>&1; then
        usermod -a -G "$grp" mbl 2>/dev/null || true
    fi
done

# Ensure directories exist and ownership is correct
mkdir -p /var/lib/mbl /etc/mbl
chown -R "$PUID":"$PGID" /var/lib/mbl /etc/mbl 2>/dev/null || true

# Execute command via gosu if started as root
if [ "$(id -u)" = "0" ]; then
    exec gosu "$PUID":"$PGID" "$@"
else
    exec "$@"
fi
