//! Suspend the FFmpeg child when the HLS cache is sufficiently far ahead.

use std::collections::HashMap;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

use windows_sys::Win32::Foundation::{
    ERROR_INVALID_PARAMETER, ERROR_NO_MORE_FILES, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::Threading::{
    GetProcessIdOfThread, OpenThread, ResumeThread, SuspendThread,
    THREAD_QUERY_LIMITED_INFORMATION, THREAD_SUSPEND_RESUME,
};

#[derive(Default)]
pub(crate) struct SuspendedThreads(HashMap<u32, OwnedHandle>);

impl SuspendedThreads {
    #[allow(unsafe_code)]
    pub(crate) fn suspend(pid: u32) -> io::Result<Self> {
        let mut suspended = Self::default();
        // Check again for threads the child started while the first snapshot was
        // being processed. Dropping the guard rolls back a partial suspension.
        for _ in 0..3 {
            let mut added = false;
            for id in thread_ids(pid)? {
                if suspended.0.contains_key(&id) {
                    continue;
                }
                // SAFETY: Open a thread handle with only the rights needed below.
                let handle = unsafe {
                    OpenThread(THREAD_SUSPEND_RESUME | THREAD_QUERY_LIMITED_INFORMATION, 0, id)
                };
                if handle.is_null() {
                    let error = io::Error::last_os_error();
                    if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
                        continue; // The thread can exit after the snapshot.
                    }
                    return Err(error);
                }
                // SAFETY: OpenThread returned a new, valid handle. Check the
                // owner again before suspending in case Windows reused the ID.
                let thread = unsafe { OwnedHandle::from_raw_handle(handle) };
                if unsafe { GetProcessIdOfThread(thread.as_raw_handle()) } != pid {
                    continue;
                }
                if unsafe { SuspendThread(thread.as_raw_handle()) } == u32::MAX {
                    return Err(io::Error::last_os_error());
                }
                suspended.0.insert(id, thread);
                added = true;
            }
            if !added && !suspended.0.is_empty() {
                return Ok(suspended);
            }
        }
        Err(io::Error::other("could not suspend all FFmpeg threads"))
    }
}

impl Drop for SuspendedThreads {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        for thread in self.0.values() {
            // SAFETY: This handle owns one successful suspension of this thread.
            // A thread that exited in the meantime no longer needs resuming.
            unsafe { ResumeThread(thread.as_raw_handle()) };
        }
    }
}

#[allow(unsafe_code)]
fn thread_ids(pid: u32) -> io::Result<Vec<u32>> {
    // SAFETY: This snapshot enumerates threads and requires no process handle.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: The snapshot is a new, valid handle and the entry has the size
    // required by Thread32First/Next. The handle remains open while iterating.
    let snapshot = unsafe { OwnedHandle::from_raw_handle(snapshot) };
    let mut entry =
        THREADENTRY32 { dwSize: size_of::<THREADENTRY32>() as u32, ..Default::default() };
    let mut found = unsafe { Thread32First(snapshot.as_raw_handle(), &mut entry) };
    let mut ids = Vec::new();
    while found != 0 {
        if entry.th32OwnerProcessID == pid {
            ids.push(entry.th32ThreadID);
        }
        entry.dwSize = size_of::<THREADENTRY32>() as u32;
        found = unsafe { Thread32Next(snapshot.as_raw_handle(), &mut entry) };
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() != Some(ERROR_NO_MORE_FILES as i32) {
        return Err(error);
    }
    Ok(ids)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::SuspendedThreads;
    use std::time::Duration;

    #[tokio::test]
    async fn windows_child_stops_producing_output_until_resumed() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("output.txt");
        let mut child = tokio::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "while ($true) { [IO.File]::AppendAllText($env:MBL_TEST_OUTPUT, 'x'); Start-Sleep -Milliseconds 25 }",
            ])
            .env("MBL_TEST_OUTPUT", &output)
            .kill_on_drop(true)
            .spawn()
            .unwrap();

        let output = &output;
        let wait_for_output = |previous| async move {
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    if std::fs::metadata(output).is_ok_and(|file| file.len() > previous) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            })
            .await
            .unwrap();
        };
        wait_for_output(0).await;
        let guard = SuspendedThreads::suspend(child.id().unwrap()).unwrap();
        // An I/O operation already handed to the OS may finish after suspension.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let paused_len = std::fs::metadata(output).unwrap().len();
        tokio::time::sleep(Duration::from_millis(250)).await;
        assert_eq!(std::fs::metadata(output).unwrap().len(), paused_len);
        drop(guard);
        wait_for_output(paused_len).await;
        child.kill().await.unwrap();
    }
}
