//! Available (non-reserved) filesystem space, including a not-yet-created output directory.
use std::{io, path::Path};

pub fn available_bytes(path: &Path) -> io::Result<u64> {
    let mut ancestor = path;
    if ancestor.as_os_str().is_empty() {
        ancestor = Path::new(".");
    }
    while !ancestor.try_exists()? {
        ancestor = ancestor
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No existing storage parent"))?;
        if ancestor.as_os_str().is_empty() {
            ancestor = Path::new(".");
        }
    }
    platform_available(ancestor)
}

#[cfg(unix)]
fn platform_available(path: &Path) -> io::Result<u64> {
    let stat = rustix::fs::statvfs(path)?;
    Ok(stat.f_bavail.saturating_mul(if stat.f_frsize == 0 { stat.f_bsize } else { stat.f_frsize }))
}
#[cfg(windows)]
#[allow(unsafe_code)]
fn platform_available(path: &Path) -> io::Result<u64> {
    use std::os::windows::ffi::OsStrExt;
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut available = 0;
    // SAFETY: the path is NUL-terminated; the live output buffer is writable.
    let ok = unsafe {
        windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
            path.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if ok == 0 { Err(io::Error::last_os_error()) } else { Ok(available) }
}
#[cfg(not(any(unix, windows)))]
fn platform_available(_: &Path) -> io::Result<u64> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "Filesystem space checks are unavailable"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn missing_output_directory_is_checked_without_creating_it() {
        let temp = tempfile::tempdir().expect("temp directory");
        let output = temp.path().join("new/recordings");
        assert!(super::available_bytes(&output).is_ok());
        assert!(!output.exists());
        assert!(
            super::available_bytes(std::path::Path::new("mbl-nonexistent-disk-test/recordings"))
                .is_ok()
        );
    }
}
