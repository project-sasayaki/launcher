use std::path::PathBuf;

use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance, CoInitializeEx,
    CoTaskMemFree, CoUninitialize,
};
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::Win32::UI::Shell::{FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH};
use windows::core::PCWSTR;

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn pick_open_file(pairs: &[(&str, &str)], title: &str) -> Option<PathBuf> {
    unsafe {
        let mut filter = Vec::new();
        for (name, pat) in pairs {
            filter.extend(to_wide(name));
            filter.extend(to_wide(pat));
        }
        filter.push(0);

        let mut file_buf = [0u16; 1024];
        let title_w = to_wide(title);
        let mut ofn = OPENFILENAMEW {
            lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
            hwndOwner: windows::Win32::Foundation::HWND::default(),
            lpstrFilter: PCWSTR(filter.as_ptr()),
            lpstrFile: windows::core::PWSTR(file_buf.as_mut_ptr()),
            nMaxFile: file_buf.len() as u32,
            lpstrTitle: PCWSTR(title_w.as_ptr()),
            Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST,
            ..Default::default()
        };
        if GetOpenFileNameW(&mut ofn).as_bool() {
            let len = file_buf.iter().position(|&c| c == 0).unwrap_or(0);
            Some(PathBuf::from(String::from_utf16_lossy(&file_buf[..len])))
        } else {
            None
        }
    }
}

/// Asks for a folder in the Windows folder dialog; `None` when it is cancelled.
///
/// The dialog is a COM object, so the calling thread (a background thread) joins COM for it.
pub fn pick_folder(title: &str) -> Option<PathBuf> {
    unsafe {
        let joined_com = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).is_ok();
        let picked = show_folder_dialog(title);
        if joined_com {
            CoUninitialize();
        }
        picked.ok()
    }
}

/// Shows the folder dialog; an error also when it is cancelled.
unsafe fn show_folder_dialog(title: &str) -> windows::core::Result<PathBuf> {
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
        dialog.SetOptions(dialog.GetOptions()? | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM)?;
        let title_w = to_wide(title);
        dialog.SetTitle(PCWSTR(title_w.as_ptr()))?;
        dialog.Show(None)?;
        let name = dialog.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?;
        let path = name.to_string().map(PathBuf::from);
        CoTaskMemFree(Some(name.0 as *const _));
        Ok(path?)
    }
}
