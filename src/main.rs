#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::os::windows::ffi::OsStrExt;

use rust_i18n::t;

pub const APP_VERSION: &str = "1.0.0.2";

// Compiles the texts of locales/*.yml into the binary. A key missing from a locale is
// shown in English; i18n::tests checks that no locale misses any.
rust_i18n::i18n!("locales", fallback = "en");

mod awl;
mod cards;
mod db;
mod detect;
mod file_util;
mod filedialog;
mod font_file;
mod games;
mod i18n;
mod icf;
mod ini;
mod loaders;
mod netinfo;
mod rawinput;
mod reactor_app;
mod room_sync;
mod roomdir;
mod server;
mod storage;
mod update;
mod vpn;
mod win_util;

use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Security::{
    GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, SW_SHOWNORMAL};
use windows::core::PCWSTR;

fn is_elevated() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut ret = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut core::ffi::c_void),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret,
        );
        let _ = CloseHandle(token);
        ok.is_ok() && elevation.TokenIsElevated != 0
    }
}

fn relaunch_elevated() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let exe_wide: Vec<u16> = exe
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let verb_wide = crate::win_util::to_wide_null("runas");
    unsafe {
        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOCLOSEPROCESS,
            lpVerb: windows::core::PCWSTR::from_raw(verb_wide.as_ptr()),
            lpFile: windows::core::PCWSTR::from_raw(exe_wide.as_ptr()),
            lpParameters: windows::core::PCWSTR::null(),
            nShow: SW_SHOWNORMAL.0 as i32,
            ..Default::default()
        };
        if ShellExecuteExW(&mut info).is_ok() {

            std::process::exit(0);
        }
    }
    false
}

fn bootstrap_runtime() {
    if let Err(e) = windows_reactor::bootstrap() {

        let msg = t!("main.runtime_init_failed", error = e).to_string();
        let msg_wide = crate::win_util::to_wide_null(&msg);
        let title_wide = crate::win_util::to_wide_null("Sasayaki launcher");
        unsafe {
            MessageBoxW(
                None,
                PCWSTR::from_raw(msg_wide.as_ptr()),
                PCWSTR::from_raw(title_wide.as_ptr()),
                MB_ICONERROR,
            );
        }
        std::process::exit(1);
    }
}

fn run_room_watchdog(parent_pid: u32, room_id: &str, keychip: &str, peer_id: &str, directory: &str) {
    unsafe {
        if let Ok(h) = OpenProcess(PROCESS_SYNCHRONIZE, false, parent_pid) {

            loop {
                match WaitForSingleObject(h, 1000) {
                    WAIT_OBJECT_0 => break,
                    WAIT_TIMEOUT => continue,

                    _ => break,
                }
            }
            let _ = CloseHandle(h);
        }
    }

    let client = crate::roomdir::RoomDirClient::new(directory);
    let peer = if peer_id.is_empty() { None } else { Some(peer_id) };
    for _ in 0..3 {
        match client.close(room_id, peer, keychip) {
            Ok(()) => break,
            Err(_) => std::thread::sleep(std::time::Duration::from_secs(1)),
        }
    }
}

fn cleanup_update_leftovers() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Some(dir) = exe.parent() else {
        return;
    };
    let _ = std::fs::remove_file(dir.join("launcher.exe.old"));
    let _ = std::fs::remove_dir_all(dir.join(".__launcher_backup"));
    let _ = std::fs::remove_dir_all(dir.join(".__update_launcher"));
    let _ = std::fs::remove_dir_all(dir.join("tools").join(".__update_launcher"));
}

fn wait_process_exit(pid: u32) {
    unsafe {
        if let Ok(h) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            for _ in 0..10 {
                match WaitForSingleObject(h, 1000) {
                    WAIT_OBJECT_0 => break,
                    WAIT_TIMEOUT => continue,
                    _ => break,
                }
            }
            let _ = CloseHandle(h);
        }
    }
}

fn main() {

    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--self-restart") {
        if let Some(pid) = args.get(pos + 1).and_then(|p| p.parse::<u32>().ok()) {
            wait_process_exit(pid);
        }
    }
    cleanup_update_leftovers();

    if let Some(pos) = args.iter().position(|a| a == "--room-watchdog") {
        if let Some(p) = args.get(pos + 1..pos + 6) {
            run_room_watchdog(
                p[0].parse().unwrap_or(0),
                &p[1],
                &p[2],
                &p[3],
                &p[4],
            );
            return;
        }
    }

    if !is_elevated() {
        let _ = relaunch_elevated();
    }

    i18n::set_lang(storage::Store::new().load_lang());

    bootstrap_runtime();
    if let Err(e) = windows_reactor::App::run_component::<reactor_app::Launcher>(()) {

        eprintln!("the launcher UI exited with an error: {e}");
        std::process::exit(1);
    }
}
