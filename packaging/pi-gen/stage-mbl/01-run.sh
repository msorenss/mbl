#!/usr/bin/env bash
set -euo pipefail

# 1. Install TCP BBR + fq congestion control
install -m 644 files/99-mbl-bbr.conf "${ROOTFS_DIR}/etc/sysctl.d/99-mbl-bbr.conf"

# 2. Configure volatile journald to minimize SD-card writes
mkdir -p "${ROOTFS_DIR}/etc/systemd/journald.conf.d"
install -m 644 files/journald-mbl.conf "${ROOTFS_DIR}/etc/systemd/journald.conf.d/mbl.conf"

# 3. Install MBL APT repository source list (M5.12)
install -m 644 ../../apt/mbl.list "${ROOTFS_DIR}/etc/apt/sources.list.d/mbl.list"

# 4. Install mbl-tv appliance systemd unit (optional boot into mbl-tv)
install -m 644 files/mbl-tv.service "${ROOTFS_DIR}/lib/systemd/system/mbl-tv.service"

# 5. Configure hostname to `mbl` so Avahi advertises `http://mbl.local:8420` on first boot
echo "mbl" > "${ROOTFS_DIR}/etc/hostname"
sed -i 's/raspberrypi/mbl/g' "${ROOTFS_DIR}/etc/hosts" || true

on_chroot << 'EOF'
systemctl enable avahi-daemon
systemctl enable mbl.service || true
# log2ram configuration if installed
if [ -f /etc/log2ram.conf ]; then
    sed -i 's/^SIZE=.*/SIZE=128M/' /etc/log2ram.conf
fi
EOF
