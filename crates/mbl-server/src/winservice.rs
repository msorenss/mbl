//! Windows Service Control Manager (SCM) integration and service lifecycle commands.

use std::future::Future;
use std::path::Path;

use camino::Utf8Path;
use clap::Subcommand;

use crate::{CliOverrides, Config};

/// Windows Service internal identifier.
pub const SERVICE_NAME: &str = "MblServer";

/// Windows Service display name in `services.msc`.
pub const SERVICE_DISPLAY_NAME: &str = "MBL Media Server";

/// Windows Service description in `services.msc`.
pub const SERVICE_DESCRIPTION: &str =
    "High-performance, lightweight personal media server (http://localhost:8420)";

/// Subcommands for managing the Windows background service (`mbl-server service <action>`).
#[derive(Debug, Clone, Subcommand)]
pub enum ServiceAction {
    /// Register and start MBL Media Server as an automatic Windows background service.
    Install,
    /// Stop and remove the MBL Media Server Windows background service.
    Uninstall,
    /// Start the registered MBL Media Server Windows service.
    Start,
    /// Stop the running MBL Media Server Windows service.
    Stop,
    /// Run under the Windows Service Control Manager dispatcher (equivalent to `--service`).
    Run,
}

/// Format the `binPath=` value for `sc.exe create`, quoting the executable path and appending `--service`
/// plus any explicit CLI overrides.
pub fn format_service_bin_path(
    exe: &Path,
    config_file: Option<&Utf8Path>,
    data_dir: Option<&Utf8Path>,
    port: Option<u16>,
) -> String {
    let mut cmd = format!("\"{}\" --service", exe.display());
    if let Some(cfg) = config_file {
        cmd.push_str(&format!(" --config \"{cfg}\""));
    }
    if let Some(data) = data_dir {
        cmd.push_str(&format!(" --data-dir \"{data}\""));
    }
    if let Some(p) = port {
        cmd.push_str(&format!(" --port {p}"));
    }
    cmd
}

/// Build the argument list for `sc.exe create MblServer ...`.
pub fn sc_create_args(bin_path: &str) -> Vec<String> {
    vec![
        "create".into(),
        SERVICE_NAME.into(),
        "binPath=".into(),
        bin_path.into(),
        "start=".into(),
        "auto".into(),
        "DisplayName=".into(),
        SERVICE_DISPLAY_NAME.into(),
    ]
}

/// Build the argument list for `sc.exe failure MblServer ...` (auto-restart after 5s on crash).
pub fn sc_failure_args() -> Vec<String> {
    vec![
        "failure".into(),
        SERVICE_NAME.into(),
        "reset=".into(),
        "86400".into(),
        "actions=".into(),
        "restart/5000/restart/5000/restart/5000".into(),
    ]
}

/// Encode a Rust string as a null-terminated UTF-16 vector for Win32 wide APIs.
pub fn encode_utf16_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Run the MBL server on a multi-threaded Tokio runtime until `shutdown` resolves.
pub fn run_server_blocking<F>(overrides: CliOverrides, shutdown: F) -> anyhow::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let config = Config::load(&overrides)?;
    let paths = config.paths();
    let (_log_guard, recent_logs) = crate::logging::init(&config.log, &paths.logs)?;
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        commit = env!("MBL_GIT_COMMIT"),
        build_time = env!("MBL_BUILD_TIMESTAMP"),
        data = %paths.data,
        "starting MBL"
    );

    tokio::runtime::Builder::new_multi_thread().enable_all().build()?.block_on(async {
        let stopping = tokio_util::sync::CancellationToken::new();
        let stop_signal = stopping.clone();
        tokio::spawn(async move {
            shutdown.await;
            stop_signal.cancel();
        });
        while !stopping.is_cancelled() {
            let restart = std::sync::Arc::new(tokio::sync::Notify::new());
            let server = crate::app::build_runtime(
                config.clone(),
                recent_logs.clone(),
                Some(restart.clone()),
            )
            .await?;
            let stop = stopping.clone();
            server
                .serve(async move {
                    tokio::select! {
                        () = stop.cancelled() => {},
                        () = restart.notified() => {},
                    }
                })
                .await?;
            if !stopping.is_cancelled() {
                tracing::info!("restarting MBL");
            }
        }
        Ok(())
    })
}

/// Run the server under the Windows Service Control Manager when `as_service` is true on Windows,
/// or in standard interactive console mode otherwise.
pub fn run_server(overrides: CliOverrides, as_service: bool) -> anyhow::Result<()> {
    #[cfg(windows)]
    if as_service {
        return win_scm::run_dispatcher(overrides);
    }
    let _ = as_service;
    run_server_blocking(overrides, crate::shutdown::signal())
}

/// Execute a `mbl-server service <action>` command.
pub fn handle_service_command(
    action: &ServiceAction,
    overrides: CliOverrides,
) -> anyhow::Result<()> {
    match action {
        ServiceAction::Run => run_server(overrides, true),
        ServiceAction::Install => install_service(&overrides),
        ServiceAction::Uninstall => uninstall_service(),
        ServiceAction::Start => start_service(),
        ServiceAction::Stop => stop_service(),
    }
}

#[cfg(windows)]
fn write_line(msg: &str) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{msg}");
}

#[cfg(windows)]
fn install_service(overrides: &CliOverrides) -> anyhow::Result<()> {
    use std::process::Command;

    let exe = std::env::current_exe()?;
    let bin_path = format_service_bin_path(
        &exe,
        overrides.config_file.as_deref(),
        overrides.data_dir.as_deref(),
        overrides.port,
    );

    // Ensure default ProgramData directory exists and is accessible.
    let data_dir = overrides.data_dir.clone().unwrap_or_else(mbl_platform::dirs::default_data_dir);
    let _ = std::fs::create_dir_all(&data_dir);
    if let Some(parent) = data_dir.parent() {
        // Grant BUILTIN\Users (*S-1-5-32-545) Modify rights so both service and console runs can access data.
        let _ = Command::new("icacls.exe")
            .args([parent.as_str(), "/grant", "*S-1-5-32-545:(OI)(CI)M", "/T", "/C", "/Q"])
            .output();
    }

    // Stop and remove any previous service registration so upgrades succeed cleanly.
    win_scm::remove_registration(true)?;

    let create_out = Command::new("sc.exe").args(sc_create_args(&bin_path)).output()?;
    if !create_out.status.success() {
        let stderr = String::from_utf8_lossy(&create_out.stderr);
        let stdout = String::from_utf8_lossy(&create_out.stdout);
        anyhow::bail!("Failed to register Windows service {SERVICE_NAME}: {stdout} {stderr}");
    }

    let _ =
        Command::new("sc.exe").args(["description", SERVICE_NAME, SERVICE_DESCRIPTION]).output();
    let _ = Command::new("sc.exe").args(sc_failure_args()).output();

    write_line(&format!("Registered Windows service '{SERVICE_DISPLAY_NAME}' ({SERVICE_NAME})."));

    let start_out = Command::new("sc.exe").args(["start", SERVICE_NAME]).output()?;
    if start_out.status.success() {
        write_line(&format!(
            "Started service '{SERVICE_DISPLAY_NAME}'. Open http://localhost:8420 in your browser."
        ));
    } else {
        let stdout = String::from_utf8_lossy(&start_out.stdout);
        write_line(&format!(
            "Service '{SERVICE_DISPLAY_NAME}' registered (start output: {}).",
            stdout.trim()
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn install_service(_overrides: &CliOverrides) -> anyhow::Result<()> {
    anyhow::bail!(
        "Windows service installation is only supported on Windows. On Linux, use `sudo systemctl enable --now mbl`."
    )
}

#[cfg(windows)]
fn uninstall_service() -> anyhow::Result<()> {
    win_scm::remove_registration(false)?;
    write_line(&format!("Windows service '{SERVICE_NAME}' is removed or pending deletion."));
    Ok(())
}

#[cfg(not(windows))]
fn uninstall_service() -> anyhow::Result<()> {
    anyhow::bail!(
        "Windows service uninstallation is only supported on Windows. On Linux, use `sudo systemctl disable --now mbl`."
    )
}

#[cfg(windows)]
fn start_service() -> anyhow::Result<()> {
    use std::process::Command;

    let out = Command::new("sc.exe").args(["start", SERVICE_NAME]).output()?;
    if !out.status.success() {
        let stdout = String::from_utf8_lossy(&out.stdout);
        anyhow::bail!("Failed to start service {SERVICE_NAME}: {}", stdout.trim());
    }
    write_line(&format!("Started Windows service '{SERVICE_DISPLAY_NAME}'."));
    Ok(())
}

#[cfg(not(windows))]
fn start_service() -> anyhow::Result<()> {
    anyhow::bail!("Windows service control is only supported on Windows.")
}

#[cfg(windows)]
fn stop_service() -> anyhow::Result<()> {
    use std::process::Command;

    let out = Command::new("sc.exe").args(["stop", SERVICE_NAME]).output()?;
    if !out.status.success() {
        let stdout = String::from_utf8_lossy(&out.stdout);
        anyhow::bail!("Failed to stop service {SERVICE_NAME}: {}", stdout.trim());
    }
    write_line(&format!("Stopped Windows service '{SERVICE_DISPLAY_NAME}'."));
    Ok(())
}

#[cfg(not(windows))]
fn stop_service() -> anyhow::Result<()> {
    anyhow::bail!("Windows service control is only supported on Windows.")
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod win_scm {
    use std::ffi::c_void;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
    use std::time::{Duration, Instant};

    use anyhow::Context;
    use windows_sys::Win32::Foundation::{
        ERROR_SERVICE_CANNOT_ACCEPT_CTRL, ERROR_SERVICE_DOES_NOT_EXIST,
        ERROR_SERVICE_MARKED_FOR_DELETE, ERROR_SERVICE_NOT_ACTIVE,
    };
    use windows_sys::Win32::System::Services::{
        CloseServiceHandle, ControlService, DeleteService, OpenSCManagerW, OpenServiceW,
        QueryServiceStatus, SC_HANDLE, SC_MANAGER_CONNECT, SERVICE_QUERY_STATUS, SERVICE_STATUS,
        SERVICE_STOP,
    };

    use crate::CliOverrides;

    use super::{SERVICE_NAME, encode_utf16_null, run_server_blocking};

    const DELETE_ACCESS: u32 = 0x0001_0000;
    const SERVICE_TRANSITION_TIMEOUT: Duration = Duration::from_secs(45);

    struct ScHandle(SC_HANDLE);

    impl Drop for ScHandle {
        fn drop(&mut self) {
            // SAFETY: The wrapper is only constructed from a successful SCM open call.
            unsafe { CloseServiceHandle(self.0) };
        }
    }

    fn open_service(manager: &ScHandle, name: &[u16], access: u32) -> std::io::Result<ScHandle> {
        // SAFETY: The manager is live and `name` is the NUL-terminated service name.
        let handle = unsafe { OpenServiceW(manager.0, name.as_ptr(), access) };
        if handle.is_null() { Err(std::io::Error::last_os_error()) } else { Ok(ScHandle(handle)) }
    }

    fn is_service_error(error: &std::io::Error, code: u32) -> bool {
        error.raw_os_error() == Some(code as i32)
    }

    fn stop_before_delete(service: &ScHandle, deadline: Instant) -> anyhow::Result<()> {
        loop {
            let mut status = SERVICE_STATUS::default();
            // SAFETY: The service handle and writable output buffer are valid for both calls.
            if unsafe { QueryServiceStatus(service.0, &mut status) } == 0 {
                return Err(std::io::Error::last_os_error()).context("Could not query MblServer");
            }
            if status.dwCurrentState == SERVICE_STOPPED {
                return Ok(());
            }
            if status.dwCurrentState != SERVICE_STOP_PENDING
                && unsafe { ControlService(service.0, SERVICE_CONTROL_STOP, &mut status) } == 0
            {
                let error = std::io::Error::last_os_error();
                if !is_service_error(&error, ERROR_SERVICE_NOT_ACTIVE)
                    && !is_service_error(&error, ERROR_SERVICE_CANNOT_ACCEPT_CTRL)
                {
                    return Err(error).context("Could not stop MblServer");
                }
            }
            anyhow::ensure!(Instant::now() < deadline, "Timed out waiting for MblServer to stop");
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    pub(super) fn remove_registration(wait_for_removal: bool) -> anyhow::Result<()> {
        // SAFETY: Null machine and database names select the local default SCM database.
        let manager =
            unsafe { OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT) };
        if manager.is_null() {
            return Err(std::io::Error::last_os_error())
                .context("Could not open Windows Service Control Manager");
        }
        let manager = ScHandle(manager);
        let name = encode_utf16_null(SERVICE_NAME);
        let deadline = Instant::now() + SERVICE_TRANSITION_TIMEOUT;
        match open_service(&manager, &name, SERVICE_QUERY_STATUS | SERVICE_STOP | DELETE_ACCESS) {
            Ok(service) => {
                stop_before_delete(&service, deadline)?;
                // SAFETY: The live handle was opened with DELETE access.
                if unsafe { DeleteService(service.0) } == 0 {
                    let error = std::io::Error::last_os_error();
                    if !is_service_error(&error, ERROR_SERVICE_MARKED_FOR_DELETE) {
                        return Err(error).context("Could not delete MblServer");
                    }
                }
            }
            Err(error) if is_service_error(&error, ERROR_SERVICE_DOES_NOT_EXIST) => return Ok(()),
            Err(error) if is_service_error(&error, ERROR_SERVICE_MARKED_FOR_DELETE) => {}
            Err(error) => return Err(error).context("Could not open MblServer for removal"),
        }
        if wait_for_removal {
            loop {
                match open_service(&manager, &name, SERVICE_QUERY_STATUS) {
                    Err(error) if is_service_error(&error, ERROR_SERVICE_DOES_NOT_EXIST) => break,
                    Err(error) if !is_service_error(&error, ERROR_SERVICE_MARKED_FOR_DELETE) => {
                        return Err(error).context("Could not verify MblServer was removed");
                    }
                    _ => {}
                }
                anyhow::ensure!(
                    Instant::now() < deadline,
                    "Timed out waiting for MblServer to be removed"
                );
                std::thread::sleep(Duration::from_millis(250));
            }
        }
        Ok(())
    }

    const SERVICE_WIN32_OWN_PROCESS: u32 = 0x0000_0010;
    const SERVICE_STOPPED: u32 = 0x0000_0001;
    const SERVICE_START_PENDING: u32 = 0x0000_0002;
    const SERVICE_STOP_PENDING: u32 = 0x0000_0003;
    const SERVICE_RUNNING: u32 = 0x0000_0004;

    const SERVICE_ACCEPT_STOP: u32 = 0x0000_0001;
    const SERVICE_ACCEPT_SHUTDOWN: u32 = 0x0000_0004;

    const SERVICE_CONTROL_STOP: u32 = 0x0000_0001;
    const SERVICE_CONTROL_INTERROGATE: u32 = 0x0000_0004;
    const SERVICE_CONTROL_SHUTDOWN: u32 = 0x0000_0005;

    const NO_ERROR: u32 = 0;
    const ERROR_CALL_NOT_IMPLEMENTED: u32 = 120;
    const ERROR_FAILED_SERVICE_CONTROLLER_CONNECT: i32 = 1063;
    const ERROR_SERVICE_SPECIFIC_ERROR: u32 = 1066;

    type ServiceStatusHandle = isize;
    type HandlerFunctionEx =
        Option<unsafe extern "system" fn(u32, u32, *mut c_void, *mut c_void) -> u32>;

    #[repr(C)]
    struct ServiceTableEntryW {
        lp_service_name: *const u16,
        lp_service_proc: Option<unsafe extern "system" fn(u32, *mut *mut u16)>,
    }

    #[repr(C)]
    struct ServiceStatus {
        dw_service_type: u32,
        dw_current_state: u32,
        dw_controls_accepted: u32,
        dw_win32_exit_code: u32,
        dw_service_specific_exit_code: u32,
        dw_check_point: u32,
        dw_wait_hint: u32,
    }

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn StartServiceCtrlDispatcherW(lp_service_start_table: *const ServiceTableEntryW) -> i32;
        fn RegisterServiceCtrlHandlerExW(
            lp_service_name: *const u16,
            lp_handler_proc: HandlerFunctionEx,
            lp_context: *mut c_void,
        ) -> ServiceStatusHandle;
        fn SetServiceStatus(
            h_service_status: ServiceStatusHandle,
            lp_service_status: *mut ServiceStatus,
        ) -> i32;
    }

    static OVERRIDES: OnceLock<CliOverrides> = OnceLock::new();
    static STATUS_HANDLE: AtomicIsize = AtomicIsize::new(0);
    static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);
    static STOP_NOTIFY: OnceLock<tokio::sync::Notify> = OnceLock::new();

    fn report_status(state: u32, controls_accepted: u32, exit_code: u32, wait_hint: u32) {
        let handle = STATUS_HANDLE.load(Ordering::SeqCst);
        if handle == 0 {
            return;
        }
        let mut status = ServiceStatus {
            dw_service_type: SERVICE_WIN32_OWN_PROCESS,
            dw_current_state: state,
            dw_controls_accepted: controls_accepted,
            dw_win32_exit_code: if exit_code == 0 { 0 } else { ERROR_SERVICE_SPECIFIC_ERROR },
            dw_service_specific_exit_code: exit_code,
            dw_check_point: if matches!(state, SERVICE_START_PENDING | SERVICE_STOP_PENDING) {
                1
            } else {
                0
            },
            dw_wait_hint: wait_hint,
        };
        // SAFETY: `handle` is a valid `SERVICE_STATUS_HANDLE` from `RegisterServiceCtrlHandlerExW`.
        unsafe {
            let _ = SetServiceStatus(handle, &raw mut status);
        }
    }

    unsafe extern "system" fn service_ctrl_handler(
        control: u32,
        _event_type: u32,
        _event_data: *mut c_void,
        _context: *mut c_void,
    ) -> u32 {
        match control {
            SERVICE_CONTROL_STOP | SERVICE_CONTROL_SHUTDOWN => {
                report_status(SERVICE_STOP_PENDING, 0, 0, 10_000);
                STOP_REQUESTED.store(true, Ordering::SeqCst);
                if let Some(notify) = STOP_NOTIFY.get() {
                    notify.notify_waiters();
                }
                NO_ERROR
            }
            SERVICE_CONTROL_INTERROGATE => NO_ERROR,
            _ => ERROR_CALL_NOT_IMPLEMENTED,
        }
    }

    unsafe extern "system" fn service_main(_argc: u32, _argv: *mut *mut u16) {
        let name = encode_utf16_null(SERVICE_NAME);
        // SAFETY: `name` is null-terminated UTF-16 and `service_ctrl_handler` matches `LPHANDLER_FUNCTION_EX`.
        let handle = unsafe {
            RegisterServiceCtrlHandlerExW(
                name.as_ptr(),
                Some(service_ctrl_handler),
                std::ptr::null_mut(),
            )
        };
        if handle == 0 {
            return;
        }
        STATUS_HANDLE.store(handle, Ordering::SeqCst);
        let _ = STOP_NOTIFY.set(tokio::sync::Notify::new());

        report_status(SERVICE_START_PENDING, 0, 0, 10_000);
        // Immediately transition to RUNNING so SCM and MSI installers never hang waiting on initialization.
        report_status(SERVICE_RUNNING, SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN, 0, 0);

        let overrides = OVERRIDES.get().cloned().unwrap_or_default();
        let res = run_server_blocking(overrides, async {
            if STOP_REQUESTED.load(Ordering::SeqCst) {
                return;
            }
            if let Some(notify) = STOP_NOTIFY.get() {
                let notified = notify.notified();
                if !STOP_REQUESTED.load(Ordering::SeqCst) {
                    notified.await;
                }
            } else {
                crate::shutdown::signal().await;
            }
        });

        let exit_code = if res.is_ok() { 0 } else { 1 };
        report_status(SERVICE_STOPPED, 0, exit_code, 0);
    }

    pub fn run_dispatcher(overrides: CliOverrides) -> anyhow::Result<()> {
        let _ = OVERRIDES.set(overrides.clone());
        let name = encode_utf16_null(SERVICE_NAME);
        let table = [
            ServiceTableEntryW {
                lp_service_name: name.as_ptr(),
                lp_service_proc: Some(service_main),
            },
            ServiceTableEntryW { lp_service_name: std::ptr::null(), lp_service_proc: None },
        ];
        // SAFETY: `table` is a null-terminated 2-entry array valid for the lifetime of the call.
        let ok = unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) };
        if ok == 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(ERROR_FAILED_SERVICE_CONTROLLER_CONNECT) {
                return run_server_blocking(overrides, crate::shutdown::signal());
            }
            anyhow::bail!("StartServiceCtrlDispatcherW failed: {err}");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn formats_service_bin_path_with_quotes_and_overrides() {
        let exe = PathBuf::from(r"C:\Program Files\MBL\bin\mbl-server.exe");
        let basic = format_service_bin_path(&exe, None, None, None);
        assert_eq!(basic, r#""C:\Program Files\MBL\bin\mbl-server.exe" --service"#);

        let with_opts = format_service_bin_path(
            &exe,
            Some(Utf8Path::new(r"D:\MBL\mbl.toml")),
            Some(Utf8Path::new(r"D:\MBL\data")),
            Some(9090),
        );
        assert_eq!(
            with_opts,
            r#""C:\Program Files\MBL\bin\mbl-server.exe" --service --config "D:\MBL\mbl.toml" --data-dir "D:\MBL\data" --port 9090"#
        );
    }

    #[test]
    fn builds_sc_create_and_failure_arguments() {
        let create = sc_create_args(r#""C:\MBL\bin\mbl-server.exe" --service"#);
        assert_eq!(
            create,
            vec![
                "create",
                "MblServer",
                "binPath=",
                r#""C:\MBL\bin\mbl-server.exe" --service"#,
                "start=",
                "auto",
                "DisplayName=",
                "MBL Media Server",
            ]
        );

        let failure = sc_failure_args();
        assert_eq!(
            failure,
            vec![
                "failure",
                "MblServer",
                "reset=",
                "86400",
                "actions=",
                "restart/5000/restart/5000/restart/5000",
            ]
        );
    }

    #[test]
    fn encodes_null_terminated_utf16() {
        let encoded = encode_utf16_null("MblServer");
        assert_eq!(encoded.last().copied(), Some(0));
        assert_eq!(encoded.len(), "MblServer".len() + 1);
    }

    #[test]
    fn wix_installer_exposes_directory_selection_and_optional_service_feature() {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .unwrap_or_else(|| Path::new("."));
        let wxs = std::fs::read_to_string(repo_root.join("packaging/windows/mbl.wxs")).unwrap();
        let ps1 =
            std::fs::read_to_string(repo_root.join("packaging/windows/build-msi.ps1")).unwrap();
        let rtf = std::fs::read_to_string(repo_root.join("packaging/windows/License.rtf")).unwrap();

        // 1. License RTF must exist for LicenseAgreementDlg
        assert!(rtf.starts_with("{\\rtf1"));

        // 2. Directory chooser (WIXUI_INSTALLDIR + InstallDirDlg + ConfigurableDirectory)
        assert!(wxs.contains(r#"Property Id="WIXUI_INSTALLDIR" Value="INSTALLFOLDER""#));
        assert!(wxs.contains(r#"ConfigurableDirectory="INSTALLFOLDER""#));
        assert!(wxs.contains(r#"DialogRef Id="InstallDirDlg""#));
        assert!(wxs.contains(r#"DialogRef Id="CustomizeDlg""#));

        // 3. Optional Windows Background Service feature
        assert!(wxs.contains(r#"Feature Id="ServiceFeature""#));
        assert!(wxs.contains(r#"CustomAction Id="InstallMblService""#));
        assert!(wxs.contains(r#"ExeCommand="service install""#));
        assert!(wxs.contains(r#"CustomAction Id="UninstallMblService""#));
        assert!(wxs.contains(r#"ExeCommand="service uninstall""#));

        // 4. Wait for the old process before replacing files, but never wait for service startup.
        assert!(!wxs.contains(r#"Start="install""#));
        assert!(wxs.contains(r#"Wait="yes""#));

        // 5. build-msi.ps1 must link WixUIExtension
        assert!(ps1.contains("-ext WixUIExtension"));
    }
}
