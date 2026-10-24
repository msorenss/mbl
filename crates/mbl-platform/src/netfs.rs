//! Network filesystem detection. Change notifications are unreliable on network shares, so
//! libraries on them are polled instead of watched.

use std::path::Path;

/// True when `path` lives on a network filesystem (NFS, SMB/CIFS, sshfs, …).
pub fn is_network_path(path: &Path) -> bool {
    imp::is_network_path(path)
}

#[cfg(target_os = "linux")]
mod imp {
    use std::path::Path;

    const NETWORK_FS: &[&str] = &[
        "nfs",
        "nfs4",
        "cifs",
        "smb3",
        "smbfs",
        "smb2",
        "9p",
        "ceph",
        "glusterfs",
        "afs",
        "fuse.sshfs",
        "fuse.rclone",
        "fuse.s3fs",
        "fuse.davfs2",
        "davfs",
    ];

    pub fn is_network_path(path: &Path) -> bool {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
        let Ok(mounts) = std::fs::read_to_string("/proc/self/mounts") else { return false };
        mount_fstype(&mounts, &path).is_some_and(|fs| NETWORK_FS.contains(&fs.as_str()))
    }

    /// Filesystem type of the longest mount point containing `path`.
    pub(super) fn mount_fstype(mounts: &str, path: &Path) -> Option<String> {
        mounts
            .lines()
            .filter_map(|line| {
                let mut f = line.split_whitespace();
                let (_dev, mount, fstype) = (f.next()?, f.next()?, f.next()?);
                Some((unescape(mount), fstype.to_owned()))
            })
            .filter(|(mount, _)| path.starts_with(mount))
            .max_by_key(|(mount, _)| mount.len())
            .map(|(_, fstype)| fstype)
    }

    /// `/proc/mounts` escapes space, tab, newline and backslash as `\ooo` octal.
    fn unescape(s: &str) -> String {
        let bytes = s.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\\'
                && i + 3 < bytes.len()
                && let Some(v) = std::str::from_utf8(&bytes[i + 1..i + 4])
                    .ok()
                    .and_then(|o| u8::from_str_radix(o, 8).ok())
            {
                out.push(v);
                i += 4;
                continue;
            }
            out.push(bytes[i]);
            i += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        const MOUNTS: &str = "\
/dev/mmcblk0p2 / ext4 rw,noatime 0 0
nas:/export/media /mnt/nas nfs4 rw 0 0
//nas/tv\\040shows /mnt/tv\\040shows cifs rw 0 0
/dev/sda1 /mnt/nas/local ext4 rw 0 0
";

        #[test]
        fn picks_longest_mount() {
            let fs = |p: &str| mount_fstype(MOUNTS, Path::new(p));
            assert_eq!(fs("/home/pi/Movies").as_deref(), Some("ext4"));
            assert_eq!(fs("/mnt/nas/Movies").as_deref(), Some("nfs4"));
            assert_eq!(fs("/mnt/nas/local/x").as_deref(), Some("ext4"));
            assert_eq!(fs("/mnt/tv shows/Severance").as_deref(), Some("cifs"));
            assert_eq!(fs("/mnt/nasty").as_deref(), Some("ext4"));
        }
    }
}

#[cfg(windows)]
mod imp {
    #![allow(unsafe_code)]

    use std::os::windows::ffi::OsStrExt;
    use std::path::{Component, Path, Prefix};

    const DRIVE_REMOTE: u32 = 4;

    pub fn is_network_path(path: &Path) -> bool {
        match path.components().next() {
            Some(Component::Prefix(p)) => match p.kind() {
                Prefix::UNC(..) | Prefix::VerbatimUNC(..) => true,
                Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                    let root: Vec<u16> = std::ffi::OsStr::new(&format!("{}:\\", letter as char))
                        .encode_wide()
                        .chain(Some(0))
                        .collect();
                    // SAFETY: `root` is a NUL-terminated UTF-16 string that outlives the call.
                    let kind = unsafe {
                        windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(root.as_ptr())
                    };
                    kind == DRIVE_REMOTE
                }
                _ => false,
            },
            _ => false,
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod imp {
    pub fn is_network_path(_path: &std::path::Path) -> bool {
        false
    }
}
