use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

use crate::storage::VpnConfig;
use rust_i18n::t;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VpnStatus {

    NotConfigured,

    Disconnected,

    Connecting,

    Connected,

    Failed,
}

pub struct VpnHandle {
    child: Child,
    passfile: PathBuf,

    logfile: PathBuf,
    job: Option<usize>,
}

impl Drop for VpnHandle {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        close_job(self.job);
        if !self.passfile.as_os_str().is_empty() {
            let _ = fs::remove_file(&self.passfile);
        }
        if !self.logfile.as_os_str().is_empty() {
            let _ = fs::remove_file(&self.logfile);
        }
    }
}

impl VpnHandle {

    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    pub fn exit_detail(&mut self) -> String {
        build_detail(&read_log(&self.logfile))
    }
}

pub struct VpnManager;

impl VpnManager {

    pub fn openvpn_path(cfg: &VpnConfig) -> PathBuf {
        if let Some(p) = &cfg.openvpn_path {
            let p = PathBuf::from(p);
            if p.is_file() {
                return p;
            }
        }
        crate::storage::launcher_dir()
            .join("tools")
            .join("openvpn")
            .join("openvpn.exe")
    }

    pub fn connect_blocking(
        cfg: &VpnConfig,
        passphrase: Option<&str>,
        timeout: Duration,
    ) -> Result<VpnHandle, String> {
        let exe = Self::openvpn_path(cfg);
        if !exe.is_file() {
            return Err(t!("vpn.openvpn_exe_not_found", exe = exe.display()).to_string());
        }
        let config = match &cfg.config_path {
            Some(c) => PathBuf::from(c),
            None => return Err(t!("vpn.vpn_not_configured").to_string()),
        };
        if !config.is_file() {
            return Err(t!("vpn.vpn_config_file_does_not", config = config.display()).to_string());
        }

        let passfile = passphrase
            .filter(|s| !s.is_empty())
            .map(|p| {
                let tmp =
                    std::env::temp_dir().join(format!("dnet-vpn-{}.tmp", std::process::id()));
                let mut f = fs::File::create(&tmp)
                    .map_err(|e| t!("vpn.failed_to_create_temporary_password", error = e).to_string())?;
                writeln!(f, "{p}").map_err(|e| t!("vpn.failed_to_write_temporary_password", error = e).to_string())?;
                writeln!(f, "{p}").map_err(|e| t!("vpn.failed_to_write_temporary_password", error = e).to_string())?;
                Ok::<_, String>(tmp)
            })
            .transpose()?;

        let has_wintun = exe
            .parent()
            .map(|d| d.join("wintun.dll"))
            .map(|p| p.is_file())
            .unwrap_or(false);
        let mut drivers: Vec<Option<&str>> = if has_wintun {
            vec![Some("wintun"), Some("tap-windows6")]
        } else {
            vec![Some("tap-windows6"), Some("wintun")]
        };
        drivers.push(None);

        let mut last_err = t!("vpn.connection_failed").to_string();
        for driver in &drivers {
            match Self::try_connect_once(&exe, &config, &passfile, *driver, timeout) {
                Ok(handle) => {

                    return Ok(handle);
                }
                Err(e) => {
                    last_err = e;
                    let lo = last_err.to_lowercase();

                    let is_driver_issue = driver.is_some()
                        && (lo.contains("wintun")
                            || lo.contains("tap")
                            || lo.contains("driver")
                            || lo.contains("unrecognized option")
                            || lo.contains("options error"));
                    if !is_driver_issue {
                        break;
                    }
                }
            }
        }
        if let Some(pf) = &passfile {
            let _ = fs::remove_file(pf);
        }
        Err(last_err)
    }

    fn try_connect_once(
        exe: &PathBuf,
        config: &PathBuf,
        passfile: &Option<PathBuf>,
        driver: Option<&str>,
        timeout: Duration,
    ) -> Result<VpnHandle, String> {

        let logfile = std::env::temp_dir().join(format!(
            "dnet-vpn-{}-{}.log",
            std::process::id(),
            driver.unwrap_or("default")
        ));

        let _ = fs::remove_file(&logfile);
        let mut cmd = Command::new(exe);
        cmd.arg("--config")
            .arg(config)
            .arg("--suppress-timestamps")
            .arg("--verb")
            .arg("3")
            .arg("--log")
            .arg(&logfile);
        if let Some(d) = driver {
            cmd.arg("--windows-driver").arg(d);
        }

        if needs_keepalive(config) {
            cmd.arg("--ping").arg("10").arg("--ping-restart").arg("60");
        }
        if let Some(pf) = passfile {

            cmd.arg("--askpass").arg(pf);
            cmd.arg("--auth-user-pass").arg(pf);
        }

        cmd.stdout(Stdio::null()).stderr(Stdio::null());

        #[cfg(windows)]
        cmd.creation_flags(crate::win_util::CREATE_NO_WINDOW);

        let mut child = cmd
            .spawn()
            .map_err(|e| t!("vpn.failed_to_start_openvpn", exe = exe.display(), error = e).to_string())?;

        let job = attach_job(&child);

        let deadline = Instant::now() + timeout;
        let marker = "initialization sequence completed";
        let driver_label = driver.map_or_else(|| t!("vpn.default").to_string(), str::to_string);

        let mut log_lines: Vec<String> = Vec::new();
        let mut log_offset: u64 = 0;
        loop {

            if let Ok(Some(_)) = child.try_wait() {
                std::thread::sleep(Duration::from_millis(200));
                read_log_incremental(&logfile, &mut log_offset, &mut log_lines);
                let detail = build_detail(&log_lines);
                let _ = child.wait();
                close_job(job);
                let _ = fs::remove_file(&logfile);
                return Err(append_wintun_hint(&t!("vpn.connection_failed_with_driver", driver_label = driver_label, detail = detail).to_string()));
            }

            {
                read_log_incremental(&logfile, &mut log_offset, &mut log_lines);
                if log_lines.iter().any(|l| l.to_lowercase().contains(marker)) {

                    std::thread::sleep(Duration::from_millis(300));
                    if let Ok(Some(_)) = child.try_wait() {
                        read_log_incremental(&logfile, &mut log_offset, &mut log_lines);
                        let detail = build_detail(&log_lines);
                        let _ = child.wait();
                        close_job(job);
                        let _ = fs::remove_file(&logfile);
                        return Err(append_wintun_hint(&t!("vpn.connection_failed_with_driver", driver_label = driver_label, detail = detail).to_string()));
                    }

                    return Ok(VpnHandle {
                        child,
                        passfile: passfile.clone().unwrap_or_else(|| PathBuf::new()),
                        logfile,
                        job,
                    });
                }
            }

            if Instant::now() >= deadline {
                read_log_incremental(&logfile, &mut log_offset, &mut log_lines);
                let detail = build_detail(&log_lines);
                let _ = child.kill();
                let _ = child.wait();
                close_job(job);
                let _ = fs::remove_file(&logfile);
                return Err(append_wintun_hint(&t!("vpn.connection_timed_out", timeout = timeout.as_secs(), driver_label = driver_label, detail = detail).to_string()));
            }

            std::thread::sleep(Duration::from_millis(150));
        }
    }
}

fn needs_keepalive(config: &PathBuf) -> bool {
    let text = match fs::read_to_string(config) {
        Ok(s) => s,
        Err(_) => return true,
    };
    let mut inline = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('<') {

            inline = !l.starts_with("</");
            continue;
        }
        if inline || l.starts_with('#') || l.starts_with(';') {
            continue;
        }
        let first = l.split_whitespace().next().unwrap_or("");
        if matches!(first, "ping" | "ping-restart" | "ping-exit" | "keepalive" | "inactive") {
            return false;
        }
    }
    true
}

fn read_log(path: &PathBuf) -> Vec<String> {
    match fs::read_to_string(path) {
        Ok(s) => s.lines().map(|l| l.to_string()).collect(),
        Err(_) => Vec::new(),
    }
}

fn read_log_incremental(path: &PathBuf, offset: &mut u64, lines: &mut Vec<String>) {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(meta) = fs::metadata(path) else {
        return;
    };
    let len = meta.len();
    if len <= *offset {
        return;
    }
    let Ok(mut f) = fs::File::open(path) else {
        return;
    };
    if f.seek(SeekFrom::Start(*offset)).is_err() {
        return;
    }
    let mut buf = String::new();
    if f.read_to_string(&mut buf).is_err() {
        return;
    }

    let Some(end) = buf.rfind('\n') else {
        return;
    };
    *offset += end as u64 + 1;
    lines.extend(buf[..=end].lines().map(|l| l.to_string()));
}

#[cfg(windows)]
pub(crate) fn attach_job(child: &Child) -> Option<usize> {
    use std::os::windows::io::AsRawHandle;
    unsafe {
        let job = CreateJobObjectW(None, None).ok()?;
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
        .is_err()
        {
            let _ = CloseHandle(job);
            return None;
        }
        let ph = HANDLE(child.as_raw_handle() as *mut core::ffi::c_void);
        if AssignProcessToJobObject(job, ph).is_err() {
            let _ = CloseHandle(job);
            return None;
        }

        Some(job.0 as usize)
    }
}

#[cfg(windows)]
pub(crate) fn close_job(job: Option<usize>) {
    if let Some(j) = job {
        unsafe {
            let _ = CloseHandle(HANDLE(j as *mut core::ffi::c_void));
        }
    }
}

fn build_detail(log: &[String]) -> String {
    extract_error_line(log)
        .or_else(|| log.iter().rev().find(|l| !l.trim().is_empty()).cloned())
        .unwrap_or_else(|| t!("vpn.openvpn_did_not_output_clear").to_string())
        .trim()
        .to_string()
}

fn extract_error_line(log: &[String]) -> Option<String> {
    let keywords = [
        "auth_failed",
        "auth failed",
        "tls error",
        "tls handshake",
        "expired",
        "fatal",
        "options error",
        "error",
        "wintun",
        "failed to",
        "cannot",
        "timed out",
        "could not",
        "no such",
        "unable to",
    ];
    for l in log.iter().rev() {
        let lo = l.to_lowercase();
        if keywords.iter().any(|k| lo.contains(k)) {
            return Some(l.trim().to_string());
        }
    }
    None
}

fn append_wintun_hint(msg: &str) -> String {
    if msg.to_lowercase().contains("wintun") {
        format!("{msg}\n{}", t!("vpn.make_sure_wintun_dll"))
    } else {
        msg.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn extract_error_line_takes_last_match() {

        let l = log(&[
            "Mon Jan 1 00:00:00 2024 OpenVPN 2.6 x86_64",
            "TLS Error: TLS key negotiation failed",
            "Mon Jan 1 00:00:01 2024 SIGTERM received",
        ]);
        assert_eq!(
            extract_error_line(&l).as_deref(),
            Some("TLS Error: TLS key negotiation failed")
        );

        assert_eq!(extract_error_line(&log(&["ok", "still ok"])), None);
    }

    #[test]
    fn build_detail_falls_back_to_last_line() {
        let l = log(&["first", "", "last line"]);
        assert_eq!(build_detail(&l), "last line");

        assert!(!build_detail(&[]).trim().is_empty());
    }

    #[test]
    fn wintun_hint_appended_only_for_wintun_errors() {
        let with = append_wintun_hint("Cannot open Wintun adapter");
        assert!(with.starts_with("Cannot open Wintun adapter"));
        assert!(with.contains("wintun.dll"), "a wintun error should get the hint: {with}");

        assert_eq!(append_wintun_hint("AUTH_FAILED"), "AUTH_FAILED");
    }
}
