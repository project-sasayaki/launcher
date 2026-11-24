use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use windows::core::PCWSTR;
use windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;
use windows::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
    WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
    WinHttpSetOption, WINHTTP_ACCESS_TYPE_NO_PROXY,
    WINHTTP_ADDREQ_FLAG_ADD, WINHTTP_ADDREQ_FLAG_REPLACE, WINHTTP_FLAG_SECURE, WINHTTP_OPEN_REQUEST_FLAGS,
    WINHTTP_OPTION_SECURITY_FLAGS, SECURITY_FLAG_IGNORE_CERT_CN_INVALID,
    WINHTTP_OPTION_CONNECT_TIMEOUT, WINHTTP_OPTION_RECEIVE_TIMEOUT,
    WINHTTP_OPTION_IPV6_FAST_FALLBACK,
    WINHTTP_OPTION_REDIRECT_POLICY, WINHTTP_OPTION_REDIRECT_POLICY_ALWAYS,
    WINHTTP_QUERY_CONTENT_LENGTH, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
};

use crate::i18n::{self, Lang};
use crate::storage::{self, Settings, launcher_dir};
use rust_i18n::t;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum UpdateTarget {
    Kitten,
    Mice,
    Segatools,

    AsiLoader,

    ShaderFix,

    Awl,

    Launcher,
}

impl UpdateTarget {

    pub fn dir_name(&self) -> &'static str {
        match self {
            UpdateTarget::Kitten => "kitten",
            UpdateTarget::Mice => "mice",
            UpdateTarget::Segatools => "segatools",
            UpdateTarget::AsiLoader => "asi-loader",
            UpdateTarget::ShaderFix => "shaderfix",
            UpdateTarget::Awl => "awl",
            UpdateTarget::Launcher => "launcher",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            UpdateTarget::Kitten => "kittentools",
            UpdateTarget::Mice => "micetools",
            UpdateTarget::Segatools => "segatools",
            UpdateTarget::AsiLoader => "Ultimate-ASI-Loader",
            UpdateTarget::ShaderFix => "shaderfix-idas",
            UpdateTarget::Awl => "awl",
            UpdateTarget::Launcher => "Sasayaki Launcher",
        }
    }

    pub fn default_repos(&self) -> &'static [&'static str] {
        match self {
            UpdateTarget::Kitten => &[
                "https://github.com/project-sasayaki/kittentools",
                "https://gitcode.com/project-sasayaki/kittentools",
            ],
            UpdateTarget::Mice => &[
                "https://github.com/project-sasayaki/mice-inid-release",
                "https://gitcode.com/project-sasayaki/mice-inid-release"
            ],
            UpdateTarget::Segatools => &[
                "https://github.com/project-sasayaki/segatools-inid-release",
                "https://gitcode.com/project-sasayaki/segatools-inid-release"
            ],
            UpdateTarget::AsiLoader => &[
                "https://github.com/ThirteenAG/Ultimate-ASI-Loader",
                "https://gitcode.com/project-sasayaki/Ultimate-ASI-Loader"],
            UpdateTarget::ShaderFix => &["https://gitea.tendokyu.moe/puniru/shaderfix-idas"],

            UpdateTarget::Awl => &[
                "https://github.com/project-sasayaki/awl",
                "https://gitcode.com/project-sasayaki/awl",
            ],

            UpdateTarget::Launcher => &["https://gitcode.com/project-sasayaki/launcher"],
        }
    }

    pub fn has_update_source(&self) -> bool {
        !self.default_repos().is_empty()
    }

    pub fn asset_matches(&self, name_lc: &str) -> bool {
        match self {
            UpdateTarget::Kitten | UpdateTarget::Mice | UpdateTarget::Segatools => {
                name_lc.starts_with(self.dir_name()) && name_lc.ends_with(".zip")
            }
            UpdateTarget::AsiLoader => {
                name_lc.starts_with("ultimate-asi-loader")
                    && name_lc.contains("nopdb")
                    && !name_lc.contains("x64")
            }
            UpdateTarget::ShaderFix => name_lc == "shaderfix.zip",

            UpdateTarget::Awl => name_lc.starts_with("awl") && name_lc.ends_with(".zip"),

            UpdateTarget::Launcher => name_lc.starts_with("launcher") && name_lc.ends_with(".zip"),
        }
    }
}

pub const ALL_TARGETS: [UpdateTarget; 7] = [
    UpdateTarget::Launcher,
    UpdateTarget::Kitten,
    UpdateTarget::Mice,
    UpdateTarget::Segatools,
    UpdateTarget::AsiLoader,
    UpdateTarget::ShaderFix,
    UpdateTarget::Awl,
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UpdateState {
    NotConfigured,
    Checking,
    UpToDate,
    Outdated,
    Error,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UpdatePhase {
    Idle,
    Checking,
    Downloading,
    Extracting,
    Installing,
    Done,
    Error,
}

#[derive(Clone, Debug)]
pub struct TargetStatus {
    pub state: UpdateState,
    pub phase: UpdatePhase,
    pub download_done: u64,
    pub download_total: u64,

    pub remote_version: Option<String>,

    pub url: Option<String>,

    pub sha256: Option<String>,

    pub mirrors: Vec<String>,

    pub detail: Option<String>,
}

impl Default for TargetStatus {
    fn default() -> Self {
        TargetStatus {
            state: UpdateState::NotConfigured,
            phase: UpdatePhase::Idle,
            download_done: 0,
            download_total: 0,
            remote_version: None,
            url: None,
            sha256: None,
            mirrors: Vec::new(),
            detail: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct UpdateStatus {
    pub per_target: HashMap<UpdateTarget, TargetStatus>,
}

impl Default for UpdateStatus {
    fn default() -> Self {
        let mut per_target = HashMap::new();
        for &t in ALL_TARGETS.iter() {
            per_target.insert(t, TargetStatus::default());
        }
        UpdateStatus { per_target }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LoaderRelease {
    pub name: String,
    pub version: String,
    pub url: String,
    #[serde(default)]
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct UpdateIndex {
    pub loaders: Vec<LoaderRelease>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,

    #[serde(default)]
    pub digest: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ReleaseInfo {
    pub tag_name: String,
    #[serde(default)]
    pub assets: Vec<ReleaseAsset>,

    #[serde(default)]
    pub release_status: Option<String>,

    #[serde(default)]
    pub created_at: Option<String>,
}

fn parse_version(s: &str) -> Vec<u64> {
    s.trim_start_matches('v')
        .split(['.', '-', '+'])
        .filter_map(|p| {
            let digits: String = p.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse().ok()
        })
        .collect()
}

pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let pa = parse_version(a);
    let pb = parse_version(b);
    let n = pa.len().max(pb.len());
    for i in 0..n {
        let x = pa.get(i).copied().unwrap_or(0);
        let y = pb.get(i).copied().unwrap_or(0);
        if x != y {
            return x.cmp(&y);
        }
    }
    std::cmp::Ordering::Equal
}

const CACHE_SCHEMA: u32 = 2;

#[derive(Serialize, Deserialize)]
struct CacheFile {
    #[serde(default)]
    schema: u32,
    saved_at: u64,
    index: UpdateIndex,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn load_cache() -> Option<UpdateIndex> {
    let path = storage::data_dir().join("update_cache.json");
    let text = std::fs::read_to_string(&path).ok()?;
    let c: CacheFile = serde_json::from_str(&text).ok()?;
    if c.schema != CACHE_SCHEMA {
        return None;
    }
    if now_secs().saturating_sub(c.saved_at) < 24 * 3600 {
        Some(c.index)
    } else {
        None
    }
}

pub fn save_cache(index: &UpdateIndex) {
    let path = storage::data_dir().join("update_cache.json");
    if let Some(dir) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            eprintln!("failed to create the update cache folder: {e}");
        }
    }
    let mut merged = load_cache().unwrap_or_default();
    for l in &index.loaders {
        if let Some(slot) = merged.loaders.iter_mut().find(|x| x.name == l.name) {
            *slot = l.clone();
        } else {
            merged.loaders.push(l.clone());
        }
    }
    let c = CacheFile {
        schema: CACHE_SCHEMA,
        saved_at: now_secs(),
        index: merged,
    };
    if let Ok(s) = serde_json::to_string_pretty(&c) {
        if let Err(e) = std::fs::write(&path, s) {
            eprintln!("failed to write the update cache: {e}");
        }
    }
}

pub fn last_version_of(settings: &Settings, t: UpdateTarget) -> Option<String> {
    match t {
        UpdateTarget::Kitten => settings.last_kitten_version.clone(),
        UpdateTarget::Mice => settings.last_mice_version.clone(),
        UpdateTarget::Segatools => settings.last_segatools_version.clone(),
        UpdateTarget::AsiLoader => settings.last_asiloader_version.clone(),
        UpdateTarget::ShaderFix => settings.last_shaderfix_version.clone(),
        UpdateTarget::Awl => settings.last_awl_version.clone(),

        UpdateTarget::Launcher => Some(crate::APP_VERSION.to_string()),
    }
}

pub fn set_last_version(settings: &mut Settings, t: UpdateTarget, v: &str) {
    match t {
        UpdateTarget::Kitten => settings.last_kitten_version = Some(v.to_string()),
        UpdateTarget::Mice => settings.last_mice_version = Some(v.to_string()),
        UpdateTarget::Segatools => settings.last_segatools_version = Some(v.to_string()),
        UpdateTarget::AsiLoader => settings.last_asiloader_version = Some(v.to_string()),
        UpdateTarget::ShaderFix => settings.last_shaderfix_version = Some(v.to_string()),
        UpdateTarget::Awl => settings.last_awl_version = Some(v.to_string()),

        UpdateTarget::Launcher => {}
    }
}

pub fn apply_check(shared: &Arc<Mutex<UpdateStatus>>, results: HashMap<UpdateTarget, TargetStatus>) {
    if let Ok(mut s) = shared.lock() {
        for (t, ts) in results {
            if let Some(slot) = s.per_target.get_mut(&t) {
                *slot = ts;
            }
        }
    }
}

pub fn check_one(
    target: UpdateTarget,
    rel: Option<&LoaderRelease>,
    settings: &Settings,
) -> TargetStatus {
    let Some(r) = rel else {
        return TargetStatus {
            state: UpdateState::UpToDate,
            detail: Some(t!("update.no_updates_available").to_string()),
            ..Default::default()
        };
    };
    let local = last_version_of(settings, target);
    let cmp = local
        .as_deref()
        .map(|v| compare_versions(&r.version, v))
        .unwrap_or(std::cmp::Ordering::Greater);
    let state = if cmp == std::cmp::Ordering::Greater {
        UpdateState::Outdated
    } else {
        UpdateState::UpToDate
    };
    let detail = Some(match local {
        Some(v) => format!("{} → {}", v, r.version),
        None => r.version.clone(),
    });
    TargetStatus {
        state,
        phase: UpdatePhase::Idle,
        download_done: 0,
        download_total: 0,
        remote_version: Some(r.version.clone()),
        url: Some(r.url.clone()),
        sha256: r.sha256.clone(),
        mirrors: Vec::new(),
        detail,
    }
}

pub fn check_updates(index: &UpdateIndex, settings: &Settings) -> HashMap<UpdateTarget, TargetStatus> {
    let mut map = HashMap::new();
    for &target in ALL_TARGETS.iter() {

        if !target.has_update_source() {
            map.insert(target, TargetStatus::default());
            continue;
        }
        let remote = index
            .loaders
            .iter()
            .find(|l| l.name == target.dir_name());
        map.insert(target, check_one(target, remote, settings));
    }
    map
}

const HTTP_HEADERS: &str = "User-Agent: sasayaki-launcher/1.0\r\n";

#[link(name = "winhttp")]
unsafe extern "system" {
    #[link_name = "WinHttpAddRequestHeaders"]
    fn raw_add_request_headers(
        request: *mut core::ffi::c_void,
        headers: *const u16,
        headers_len: u32,
        modifiers: u32,
    ) -> i32;
}

#[derive(Default)]
pub struct HttpAbort {
    handles: std::sync::Mutex<Vec<usize>>,
}

impl HttpAbort {
    pub fn new() -> Self {
        Self::default()
    }

    fn register(&self, handle: *mut core::ffi::c_void) {

        self.handles
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(handle as usize);
    }

    pub fn close_all(&self) {
        for h in std::mem::take(&mut *self.handles.lock().unwrap_or_else(|e| e.into_inner())) {
            unsafe {
                let _ = WinHttpCloseHandle(h as *mut core::ffi::c_void);
            }
        }
    }
}

fn parse_url(url: &str) -> Result<(String, u16, String, bool), String> {
    let url = url.trim();
    let (secure, rest) = if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else {
        return Err(t!("update.only_http_https_urls_are").to_string());
    };
    let (hostpart, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match hostpart.rfind(':') {
        Some(i) => (
            hostpart[..i].to_string(),
            hostpart[i + 1..]
                .parse::<u16>()
                .map_err(|_| t!("update.invalid_url_port").to_string())?,
        ),
        None => (hostpart.to_string(), if secure { 443 } else { 80 }),
    };
    if host.is_empty() {
        return Err(t!("update.url_host_is_empty").to_string());
    }
    Ok((host, port, path.to_string(), secure))
}

fn query_number(request: *mut core::ffi::c_void, flag: u32) -> Option<u32> {
    let mut buf: u32 = 0;
    let mut len: u32 = 4;
    let ok = unsafe {
        WinHttpQueryHeaders(
            request,
            flag,
            PCWSTR::null(),
            Some((&mut buf as *mut u32) as *mut core::ffi::c_void),
            &mut len,
            core::ptr::null_mut(),
        )
    }
    .is_ok();
    if ok {
        Some(buf)
    } else {
        None
    }
}

fn with_http<F>(
    url: &str,
    timeout: Duration,
    connect_timeout: Duration,
    abort: Option<&HttpAbort>,
    f: F,
) -> Result<(), String>
where
    F: FnOnce(*mut core::ffi::c_void, Option<u64>) -> Result<(), String>,
{
    let (host, port, path, secure) = parse_url(url)?;

    let connect_host = host.clone();
    unsafe {

        let session = WinHttpOpen(
            PCWSTR::null(),
            WINHTTP_ACCESS_TYPE_NO_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        );

        let ipv6_fb: u32 = 1;
        let ipv6_fb_bytes = ipv6_fb.to_ne_bytes();
        let _ = WinHttpSetOption(
            Some(session),
            WINHTTP_OPTION_IPV6_FAST_FALLBACK,
            Some(&ipv6_fb_bytes),
        );
        if session.is_null() {
            return Err(t!("http.failed_to_initialize").to_string());
        }
        if let Some(a) = abort {
            a.register(session);
        }
        let ct = timeout.as_millis().min(u32::MAX as u128) as u32;
        let ct_bytes = ct.to_ne_bytes();
        let connect_ms = connect_timeout.as_millis().min(u32::MAX as u128) as u32;
        let connect_bytes = connect_ms.to_ne_bytes();
        let _ = WinHttpSetOption(
            Some(session),
            WINHTTP_OPTION_CONNECT_TIMEOUT,
            Some(&connect_bytes),
        );
        let _ = WinHttpSetOption(
            Some(session),
            WINHTTP_OPTION_RECEIVE_TIMEOUT,
            Some(&ct_bytes),
        );
        let host_w = crate::win_util::to_wide_null(&connect_host);
        let connect = WinHttpConnect(session, PCWSTR::from_raw(host_w.as_ptr()), port, 0);
        if connect.is_null() {
            if let Some(a) = abort {
                a.close_all();
            } else {
                let _ = WinHttpCloseHandle(session);
            }
            return Err(t!("update.failed_to_connect_to", host = host).to_string());
        }
        if let Some(a) = abort {
            a.register(connect);
        }
        let path_w = crate::win_util::to_wide_null(&path);
        let verb_w = crate::win_util::to_wide_null("GET");
        let flags = if secure {
            WINHTTP_FLAG_SECURE
        } else {
            WINHTTP_OPEN_REQUEST_FLAGS(0)
        };
        let request = WinHttpOpenRequest(
            connect,
            PCWSTR::from_raw(verb_w.as_ptr()),
            PCWSTR::from_raw(path_w.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            flags,
        );
        if request.is_null() {
            if let Some(a) = abort {
                a.close_all();
            } else {
                let _ = WinHttpCloseHandle(connect);
                let _ = WinHttpCloseHandle(session);
            }
            return Err(t!("http.failed_to_create_request").to_string());
        }
        if let Some(a) = abort {
            a.register(request);
        }

        let policy: u32 = WINHTTP_OPTION_REDIRECT_POLICY_ALWAYS;
        let policy_bytes = policy.to_ne_bytes();
        let _ = WinHttpSetOption(
            Some(request),
            WINHTTP_OPTION_REDIRECT_POLICY,
            Some(&policy_bytes),
        );

        let sec: u32 = SECURITY_FLAG_IGNORE_CERT_CN_INVALID;
        let sec_bytes = sec.to_ne_bytes();
        let _ = WinHttpSetOption(Some(request), WINHTTP_OPTION_SECURITY_FLAGS, Some(&sec_bytes));
        let result = (|| -> Result<(), String> {
            let headers = crate::win_util::to_wide_null(HTTP_HEADERS);
            let ok = raw_add_request_headers(request, headers.as_ptr(), (-1i32) as u32, WINHTTP_ADDREQ_FLAG_ADD);

            let host_hdr = crate::win_util::to_wide_null(&format!("Host: {host}"));
            let _ = raw_add_request_headers(
                request,
                host_hdr.as_ptr(),
                (-1i32) as u32,
                WINHTTP_ADDREQ_FLAG_ADD | WINHTTP_ADDREQ_FLAG_REPLACE,
            );
            if ok == 0 {
                let e = std::io::Error::last_os_error();
                return Err(t!("update.failed_to_add_request_headers", error = e).to_string());
            }
            WinHttpSendRequest(request, None, None, 0, 0, 0).map_err(|e| {
                t!("http.failed_to_send_request", error = e).to_string()
            })?;
            WinHttpReceiveResponse(request, core::ptr::null_mut()).map_err(|e| {
                t!("http.failed_to_receive_response", error = e).to_string()
            })?;
            let status = query_number(request, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER)
                .ok_or_else(|| {
                    t!("update.failed_to_read_status_code").to_string()
                })?;
            if status < 200 || status >= 300 {
                return Err(t!("update.http_status", status = status, url = url).to_string());
            }
            let total = query_number(
                request,
                WINHTTP_QUERY_CONTENT_LENGTH | WINHTTP_QUERY_FLAG_NUMBER,
            )
            .map(|v| v as u64);
            f(request, total)
        })();

        if let Some(a) = abort {
            a.close_all();
        } else {
            let _ = WinHttpCloseHandle(request);
            let _ = WinHttpCloseHandle(connect);
            let _ = WinHttpCloseHandle(session);
        }
        result
    }
}

fn read_body(request: *mut core::ffi::c_void, out: &mut Vec<u8>) -> Result<(), String> {
    loop {
        let mut avail: u32 = 0;
        if unsafe { WinHttpQueryDataAvailable(request, &mut avail) }.is_err() {
            break;
        }
        if avail == 0 {
            break;
        }
        let mut buf = vec![0u8; avail as usize];
        let mut read: u32 = 0;
        if unsafe {
            WinHttpReadData(
                request,
                buf.as_mut_ptr() as *mut core::ffi::c_void,
                avail,
                &mut read,
            )
        }
        .is_err()
        {
            break;
        }
        if read == 0 {
            break;
        }
        buf.truncate(read as usize);
        out.extend_from_slice(&buf);
    }
    Ok(())
}

fn read_body_to_file(
    request: *mut core::ffi::c_void,
    file: &mut std::fs::File,
    total: Option<u64>,
    cancel: &std::sync::atomic::AtomicBool,
    on_progress: &dyn Fn(u64, u64),
) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    let mut written: u64 = 0;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(t!("update.update_cancelled").to_string());
        }
        let mut avail: u32 = 0;
        if unsafe { WinHttpQueryDataAvailable(request, &mut avail) }.is_err() {
            break;
        }
        if avail == 0 {
            break;
        }
        let mut buf = vec![0u8; avail as usize];
        let mut read: u32 = 0;
        if unsafe {
            WinHttpReadData(
                request,
                buf.as_mut_ptr() as *mut core::ffi::c_void,
                avail,
                &mut read,
            )
        }
        .is_err()
        {
            break;
        }
        if read == 0 {
            break;
        }
        buf.truncate(read as usize);
        file.write_all(&buf)
            .map_err(|e| t!("update.write_failed", error = e).to_string())?;
        written += read as u64;
        on_progress(written, total.unwrap_or(0));
    }

    if cancel.load(Ordering::Relaxed) {
        return Err(t!("update.update_cancelled").to_string());
    }
    Ok(())
}

pub fn repo_api_url(src: &str) -> Result<String, String> {
    let err = || {
        t!("update.only_github_gitcode_repo_urls")
        .to_string()
    };
    let url = src.trim();
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .ok_or_else(err)?;
    let (host, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].trim_matches('/')),
        None => (rest, ""),
    };
    let host = host.trim_start_matches("www.");

    let (api_base, suffix) = match host {
        "github.com" => ("https://api.github.com/repos", "/releases/latest"),
        "gitcode.com" => ("https://api.gitcode.com/api/v5/repos", "/releases/latest"),

        "gitea.tendokyu.moe" => ("https://gitea.tendokyu.moe/api/v1/repos", "/releases/latest"),
        _ => return Err(err()),
    };
    if path.is_empty() {
        return Err(err());
    }

    if let Some(idx) = path.find("/releases/tag/") {
        let tag = path[idx + "/releases/tag/".len()..].trim_matches('/');
        let owner_repo = &path[..idx];
        if tag.is_empty() || !is_owner_repo(owner_repo) {
            return Err(err());
        }
        return Ok(format!("{api_base}/{owner_repo}/releases/tags/{tag}"));
    }

    let mut p = path;
    if let Some(s) = p.strip_suffix("/releases/latest") {
        p = s;
    } else if let Some(s) = p.strip_suffix("/releases") {
        p = s;
    }
    if let Some(s) = p.strip_suffix(".git") {
        p = s;
    }
    let p = p.trim_matches('/');
    if !is_owner_repo(p) {
        return Err(err());
    }
    Ok(format!("{api_base}/{p}{suffix}"))
}

fn is_owner_repo(path: &str) -> bool {
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    segs.len() == 2
}

fn release_to_loader_for(
    rel: &ReleaseInfo,
    target: UpdateTarget,
) -> Result<LoaderRelease, String> {
    let tag_version = rel.tag_name.trim_start_matches('v').to_string();
    for asset in &rel.assets {
        let name_lc = asset.name.to_lowercase();
        if target.asset_matches(&name_lc) {
            let loader = target.dir_name();
            return Ok(LoaderRelease {
                name: loader.to_string(),
                version: tag_version.clone(),
                url: asset.browser_download_url.clone(),
                sha256: asset
                    .digest
                    .as_ref()
                    .and_then(|d| d.strip_prefix("sha256:").map(|h| h.to_string())),
            });
        }
    }
    Err(t!("update.no_usable_asset_found_in")
    .to_string())
}

fn parse_releases(text: &str) -> Result<Vec<ReleaseInfo>, String> {
    if let Ok(rel) = serde_json::from_str::<ReleaseInfo>(text) {
        return Ok(vec![rel]);
    }
    let mut rels: Vec<ReleaseInfo> = serde_json::from_str(text).map_err(|e| {
        t!("update.failed_to_parse_release", error = e).to_string()
    })?;
    if rels.is_empty() {
        return Err(t!("update.repo_has_no_usable_release").to_string());
    }
    sort_releases_newest_first(&mut rels);
    Ok(rels)
}

fn sort_releases_newest_first(rels: &mut Vec<ReleaseInfo>) {
    let is_latest = |r: &ReleaseInfo| r.release_status.as_deref() == Some("latest");

    if rels.iter().any(|r| is_latest(r)) {
        rels.sort_by(|a, b| is_latest(b).cmp(&is_latest(a)));
        return;
    }

    if rels.iter().any(|r| r.created_at.is_some()) {
        rels.sort_by(|a, b| {
            b.created_at
                .as_deref()
                .unwrap_or("")
                .cmp(a.created_at.as_deref().unwrap_or(""))
        });
        return;
    }

    rels.reverse();
}

fn fetch_latest_release(
    api_url: &str,
    abort: Option<&HttpAbort>,
) -> Result<Vec<ReleaseInfo>, String> {
    let text = http_get_text(api_url, abort)?;
    parse_releases(&text)
}

fn loader_from_candidates(
    rels: &[ReleaseInfo],
    target: UpdateTarget,
) -> Result<LoaderRelease, String> {
    let mut last_err = t!("update.repo_has_no_usable_release")
    .to_string();
    for rel in rels {
        match release_to_loader_for(rel, target) {
            Ok(l) => return Ok(l),
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
}

fn select_repo(target: UpdateTarget, lang: Lang) -> Option<&'static str> {
    let repos = target.default_repos();
    let pick = |host: &str| repos.iter().find(|r| r.contains(host)).copied();
    // GitCode is the mirror that is reachable from mainland China; everyone else uses GitHub.
    let host = if lang == Lang::Zh { "gitcode.com" } else { "github.com" };
    pick(host).or_else(|| repos.first().copied())
}

const CHECK_DEADLINE: Duration = Duration::from_secs(15);

pub fn fetch_updates_for(
    targets: &[UpdateTarget],
    on_result: &mut dyn FnMut(UpdateTarget, Option<LoaderRelease>),
) {
    use std::sync::mpsc;
    use std::time::Instant;

    let lang = i18n::current();

    let jobs: Vec<(UpdateTarget, String)> = targets
        .iter()
        .filter_map(|&t| select_repo(t, lang).map(|repo| (t, repo.to_string())))
        .collect();

    let (tx, rx) = mpsc::channel::<(UpdateTarget, Option<LoaderRelease>)>();

    let aborts: Vec<std::sync::Arc<HttpAbort>> = (0..jobs.len())
        .map(|_| std::sync::Arc::new(HttpAbort::new()))
        .collect();
    for ((target, repo), abort) in jobs.iter().zip(&aborts) {
        let tx = tx.clone();
        let (target, repo) = (*target, (*repo).clone());
        let abort = abort.clone();
        std::thread::spawn(move || {
            let r = repo_api_url(&repo)
                .and_then(|api| fetch_latest_release(&api, Some(&abort)))
                .and_then(|rels| loader_from_candidates(&rels, target))
                .ok();
            let _ = tx.send((target, r));
        });
    }
    drop(tx);

    let deadline = Instant::now() + CHECK_DEADLINE;
    for _ in 0..jobs.len() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match rx.recv_timeout(remaining) {
            Ok((target, rel)) => on_result(target, rel),
            Err(_) => break,
        }
    }

    for a in &aborts {
        a.close_all();
    }
}

fn http_get_text(
    url: &str,
    abort: Option<&HttpAbort>,
) -> Result<String, String> {
    let mut body = Vec::new();

    with_http(
        url,
        Duration::from_secs(15),
        Duration::from_secs(10),
        abort,
        |request, _total| read_body(request, &mut body),
    )?;
    String::from_utf8(body)
        .map_err(|_| t!("update.response_is_not_valid_utf").to_string())
}

fn http_download_to_file(
    primary_url: &str,
    mirrors: &[String],
    out: &Path,
    cancel: &std::sync::atomic::AtomicBool,
    abort: &HttpAbort,
    on_progress: &dyn Fn(u64, u64),
) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    let tmp = out.with_extension("part");
    let _ = std::fs::remove_file(&tmp);
    // A launcher put in a new folder has no tools\ yet; the first update creates it.
    if let Some(dir) = tmp.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| t!("update.failed_to_create_temp_file", error = e).to_string())?;
    }
    let mut file = std::fs::File::create(&tmp)
        .map_err(|e| t!("update.failed_to_create_temp_file", error = e).to_string())?;
    let urls: Vec<String> = std::iter::once(primary_url.to_string())
        .chain(mirrors.iter().cloned())
        .collect();
    let mut last_err = String::new();
    let mut ok = false;
    for url in &urls {
        if cancel.load(Ordering::Relaxed) {
            last_err = t!("update.update_cancelled").to_string();
            break;
        }

        match with_http(
            url,
            Duration::from_secs(30),
            Duration::from_secs(30),
            Some(abort),
            |request, total| read_body_to_file(request, &mut file, total, cancel, on_progress),
        ) {
            Ok(()) => {
                ok = true;
                break;
            }
            Err(e) => last_err = e,
        }
    }
    if !ok {
        let _ = std::fs::remove_file(&tmp);
        return Err(if cancel.load(Ordering::Relaxed) {
            t!("update.update_cancelled").to_string()
        } else {
            t!("update.download_failed", error = last_err).to_string()
        });
    }
    std::fs::rename(&tmp, out)
        .map_err(|e| t!("update.rename_failed", error = e).to_string())
}

fn sha256_of_file(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    Ok(format!("{:x}", digest))
}

fn strip_top_dir(dir: &Path) -> PathBuf {
    if let Ok(mut entries) = std::fs::read_dir(dir) {
        let first = entries.next().and_then(|e| e.ok());
        if let Some(f) = first {
            if entries.next().is_none() && f.path().is_dir() {
                return f.path();
            }
        }
    }
    dir.to_path_buf()
}

fn move_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    copy_dir(src, dst)?;
    std::fs::remove_dir_all(src)
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

fn extract_zip(
    zip_path: &Path,
    out_dir: &Path,
    cancel: &std::sync::atomic::AtomicBool,
    progress: &dyn Fn(u64, u64),
) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| t!("update.failed_to_open_archive", error = e).to_string())?;
    let total = archive.len();
    let mut done = 0u64;
    for i in 0..total {
        if cancel.load(Ordering::Relaxed) {
            return Err(t!("update.update_cancelled").to_string());
        }
        let mut entry = archive
            .by_index(i)
            .map_err(|e| t!("update.failed_to_read_entry", error = e).to_string())?;
        let name = match entry.enclosed_name() {
            Some(p) => p.to_path_buf(),
            None => continue,
        };
        let out_path = out_dir.join(&name);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let mut outf = std::fs::File::create(&out_path).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut outf).map_err(|e| e.to_string())?;
        }
        done += 1;
        progress(done, total as u64);
    }
    Ok(())
}

pub fn install_zip(
    zip_path: &Path,
    base_dir: &Path,
    target: UpdateTarget,
    cancel: &std::sync::atomic::AtomicBool,
    progress: &dyn Fn(UpdatePhase, u64, u64),
) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    progress(UpdatePhase::Extracting, 0, 0);
    let tmp_extract = base_dir.join(format!(".__update_{}", target.dir_name()));
    let _ = std::fs::remove_dir_all(&tmp_extract);
    std::fs::create_dir_all(&tmp_extract)
        .map_err(|e| t!("update.failed_to_create_temp_dir", error = e).to_string())?;
    extract_zip(zip_path, &tmp_extract, cancel, &|done, total| {
        progress(UpdatePhase::Extracting, done, total)
    })
    .map_err(|e| {
        let _ = std::fs::remove_dir_all(&tmp_extract);
        t!("update.extraction_failed", error = e).to_string()
    })?;
    let _ = std::fs::remove_file(zip_path);

    if cancel.load(Ordering::Relaxed) {
        let _ = std::fs::remove_dir_all(&tmp_extract);
        return Err(t!("update.update_cancelled").to_string());
    }

    let content_root = strip_top_dir(&tmp_extract);
    let target_dir = base_dir.join(target.dir_name());
    let backup = base_dir.join(format!(".{}.old", target.dir_name()));
    let _ = std::fs::remove_dir_all(&backup);
    if target_dir.exists() {
        std::fs::rename(&target_dir, &backup).map_err(|e| {
            let _ = std::fs::remove_dir_all(&tmp_extract);
            t!("update.backup_failed", error = e).to_string()
        })?;
    }

    progress(UpdatePhase::Installing, 0, 0);
    move_dir(&content_root, &target_dir).map_err(|e| {

        let _ = std::fs::remove_dir_all(&target_dir);
        if backup.exists() {
            let _ = std::fs::rename(&backup, &target_dir);
        }
        let _ = std::fs::remove_dir_all(&tmp_extract);
        t!("update.installation_failed", error = e).to_string()
    })?;

    let _ = std::fs::remove_dir_all(&backup);
    let _ = std::fs::remove_dir_all(&tmp_extract);
    progress(UpdatePhase::Done, 1, 1);
    Ok(())
}

pub fn install_launcher_zip(
    zip_path: &Path,
    cancel: &std::sync::atomic::AtomicBool,
    progress: &dyn Fn(UpdatePhase, u64, u64),
) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    let base = launcher_dir();
    progress(UpdatePhase::Extracting, 0, 0);
    let tmp_extract = base.join(".__update_launcher");
    let _ = std::fs::remove_dir_all(&tmp_extract);
    std::fs::create_dir_all(&tmp_extract)
        .map_err(|e| t!("update.failed_to_create_temp_dir", error = e).to_string())?;
    extract_zip(zip_path, &tmp_extract, cancel, &|done, total| {
        progress(UpdatePhase::Extracting, done, total)
    })
    .map_err(|e| {
        let _ = std::fs::remove_dir_all(&tmp_extract);
        t!("update.extraction_failed", error = e).to_string()
    })?;
    let _ = std::fs::remove_file(zip_path);

    if cancel.load(Ordering::Relaxed) {
        let _ = std::fs::remove_dir_all(&tmp_extract);
        return Err(t!("update.update_cancelled").to_string());
    }

    let content_root = strip_top_dir(&tmp_extract);
    let new_exe = content_root.join("launcher.exe");
    if !new_exe.is_file() {
        let _ = std::fs::remove_dir_all(&tmp_extract);
        return Err(t!("update.launcher_exe_not_found_in")
        .to_string());
    }

    progress(UpdatePhase::Installing, 0, 0);
    let parked_dir = tmp_extract.join(".old");
    let mut parked: Vec<(PathBuf, PathBuf)> = Vec::new();

    fn merge_copy(
        src: &Path,
        dst: &Path,
        park: &Path,
        top: bool,
        parked: &mut Vec<(PathBuf, PathBuf)>,
    ) -> Result<(), String> {
        std::fs::create_dir_all(dst).map_err(|e| t!("update.installation_failed", error = e).to_string())?;
        for entry in std::fs::read_dir(src)
            .map_err(|e| t!("update.installation_failed", error = e).to_string())?
            .flatten()
        {
            let name = entry.file_name();
            let lname = name.to_string_lossy().to_lowercase();
            if top && (lname == "launcher.exe" || lname == "data" || lname == ".old") {
                continue;
            }
            let s = entry.path();
            let d = dst.join(&name);
            let p = park.join(&name);
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                merge_copy(&s, &d, &p, false, parked)?;
            } else {
                if d.exists() {
                    std::fs::create_dir_all(park)
                        .map_err(|e| t!("update.installation_failed", error = e).to_string())?;
                    std::fs::rename(&d, &p)
                        .map_err(|e| t!("update.installation_failed", error = e).to_string())?;
                    parked.push((p, d.clone()));
                }
                std::fs::copy(&s, &d)
                    .map_err(|e| t!("update.installation_failed", error = e).to_string())?;
            }
        }
        Ok(())
    }

    if let Err(e) = merge_copy(&content_root, &base, &parked_dir, true, &mut parked) {
        for (p, d) in &parked {
            let _ = std::fs::rename(p, d);
        }
        let _ = std::fs::remove_dir_all(&tmp_extract);
        return Err(e);
    }

    let current_exe = std::env::current_exe().map_err(|e| {
        let _ = std::fs::remove_dir_all(&tmp_extract);
        t!("update.failed_to_resolve_own_path", error = e).to_string()
    })?;
    let park_exe = parked_dir.join("launcher.exe");
    std::fs::create_dir_all(&parked_dir)
        .map_err(|e| t!("update.installation_failed", error = e).to_string())?;
    std::fs::rename(&current_exe, &park_exe).map_err(|e| {
        for (p, d) in &parked {
            let _ = std::fs::rename(p, d);
        }
        let _ = std::fs::remove_dir_all(&tmp_extract);
        t!("update.backup_failed", error = e).to_string()
    })?;
    if let Err(e) = std::fs::rename(&new_exe, base.join("launcher.exe")) {
        for (p, d) in &parked {
            let _ = std::fs::rename(p, d);
        }
        let _ = std::fs::rename(&park_exe, &current_exe);
        let _ = std::fs::remove_dir_all(&tmp_extract);
        return Err(t!("update.installation_failed", error = e).to_string());
    }

    let _ = std::fs::remove_dir_all(&tmp_extract);

    progress(UpdatePhase::Done, 1, 1);

    let new_process = base.join("launcher.exe");
    let self_pid = std::process::id().to_string();
    let spawned = {
        use std::os::windows::process::CommandExt;
        let mut cmd = std::process::Command::new(&new_process);
        cmd.current_dir(&base);
        cmd.arg("--self-restart").arg(&self_pid);
        cmd.creation_flags(0x0100_0000);
        match cmd.spawn() {
            Ok(child) => {
                unsafe {
                    let _ = AllowSetForegroundWindow(child.id());
                }
                true
            }
            Err(_) => {
                let mut cmd = std::process::Command::new(&new_process);
                cmd.current_dir(&base);
                cmd.arg("--self-restart").arg(&self_pid);
                match cmd.spawn() {
                    Ok(child) => {
                        unsafe {
                            let _ = AllowSetForegroundWindow(child.id());
                        }
                        true
                    }
                    Err(_) => false,
                }
            }
        }
    };
    if !spawned {
        return Err(t!("update.failed_to_launch_new_version").to_string());
    }

    std::process::exit(0)
}

pub fn download_and_install(
    target: UpdateTarget,
    primary_url: &str,
    mirrors: &[String],
    sha256: Option<&str>,
    base_dir: &Path,
    cancel: &std::sync::atomic::AtomicBool,
    abort: &HttpAbort,
    progress: &dyn Fn(UpdatePhase, u64, u64),
) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    progress(UpdatePhase::Downloading, 0, 0);

    let tmp_extract = base_dir.join(format!(".__update_{}", target.dir_name()));
    let zip_path = base_dir.join(format!(".__update_{}.zip", target.dir_name()));

    let r = http_download_to_file(
        primary_url,
        mirrors,
        &zip_path,
        cancel,
        abort,
        &|done, total| progress(UpdatePhase::Downloading, done, total),
    );
    if let Err(e) = r {
        let _ = std::fs::remove_dir_all(&tmp_extract);
        let _ = std::fs::remove_file(&zip_path);
        return Err(e);
    }
    if cancel.load(Ordering::Relaxed) {
        let _ = std::fs::remove_dir_all(&tmp_extract);
        let _ = std::fs::remove_file(&zip_path);
        return Err(t!("update.update_cancelled").to_string());
    }

    if let Some(expected) = sha256 {
        let actual = sha256_of_file(&zip_path)
            .map_err(|e| t!("update.checksum_computation_failed", error = e).to_string())?;
        if actual.to_lowercase() != expected.to_lowercase() {
            let _ = std::fs::remove_dir_all(&tmp_extract);
            let _ = std::fs::remove_file(&zip_path);
            return Err(t!("update.sha256_verification_failed").to_string());
        }
    }

    let r = match target {
        UpdateTarget::Launcher => install_launcher_zip(&zip_path, cancel, progress),
        _ => install_zip(&zip_path, base_dir, target, cancel, progress),
    };
    if r.is_err() {
        let _ = std::fs::remove_file(&zip_path);
    }
    r
}

pub fn tools_dir() -> PathBuf {
    launcher_dir().join("tools")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare() {
        assert_eq!(compare_versions("1.9", "1.10"), std::cmp::Ordering::Less);
        assert_eq!(compare_versions("1.10", "1.9"), std::cmp::Ordering::Greater);
        assert_eq!(compare_versions("1.4", "1.4.0"), std::cmp::Ordering::Equal);
        assert_eq!(compare_versions("v1.4.2", "1.4"), std::cmp::Ordering::Greater);
        assert_eq!(compare_versions("2.0.1", "2.0.0"), std::cmp::Ordering::Greater);
    }

    #[test]
    fn check_updates_matches_loaders() {
        let index = UpdateIndex {
            loaders: vec![LoaderRelease {
                name: "kitten".into(),
                version: "1.5.0".into(),
                url: "https://x/k.zip".into(),
                sha256: None,
            }],
        };

        let mut settings = Settings::default();
        let r = check_updates(&index, &settings);
        assert_eq!(
            r[&UpdateTarget::Kitten].state,
            UpdateState::Outdated
        );

        assert_eq!(r[&UpdateTarget::Mice].state, UpdateState::UpToDate);
        assert_eq!(r[&UpdateTarget::Mice].detail.as_deref(), Some(&*t!("update.no_updates_available")));

        settings.last_kitten_version = Some("1.5.0".into());
        let r2 = check_updates(&index, &settings);
        assert_eq!(r2[&UpdateTarget::Kitten].state, UpdateState::UpToDate);

        settings.last_kitten_version = Some("1.4.0".into());
        let r3 = check_updates(&index, &settings);
        assert_eq!(r3[&UpdateTarget::Kitten].state, UpdateState::Outdated);
    }

    #[test]
    fn launcher_component_has_source() {

        assert!(ALL_TARGETS.contains(&UpdateTarget::Launcher));
        assert!(UpdateTarget::Launcher.has_update_source());
        let r = check_updates(&UpdateIndex::default(), &Settings::default());
        assert_ne!(
            r[&UpdateTarget::Launcher].state,
            UpdateState::NotConfigured
        );

        assert_eq!(
            last_version_of(&Settings::default(), UpdateTarget::Launcher).as_deref(),
            Some(crate::APP_VERSION)
        );
    }

    #[test]
    fn release_version_from_tag() {
        let rel = ReleaseInfo {
            tag_name: "v1.2.3.4".into(),
            assets: vec![ReleaseAsset {
                name: "launcher.zip".into(),
                browser_download_url: "https://x/launcher.zip".into(),
                digest: None,
            }],
            release_status: None,
            created_at: None,
        };
        let lr = release_to_loader_for(&rel, UpdateTarget::Launcher).unwrap();
        assert_eq!(lr.version, "1.2.3.4");
        assert_eq!(lr.name, "launcher");
    }

    #[test]
    fn select_repo_by_language() {

        let zh = i18n::Lang::Zh;
        let en = i18n::Lang::En;
        assert_eq!(
            select_repo(UpdateTarget::Kitten, zh),
            Some("https://gitcode.com/project-sasayaki/kittentools")
        );
        assert_eq!(
            select_repo(UpdateTarget::Kitten, en),
            Some("https://github.com/project-sasayaki/kittentools")
        );

        assert_eq!(
            select_repo(UpdateTarget::ShaderFix, zh),
            Some("https://gitea.tendokyu.moe/puniru/shaderfix-idas")
        );
        assert_eq!(
            select_repo(UpdateTarget::ShaderFix, en),
            Some("https://gitea.tendokyu.moe/puniru/shaderfix-idas")
        );

        assert_eq!(
            select_repo(UpdateTarget::Launcher, zh),
            Some("https://gitcode.com/project-sasayaki/launcher")
        );
        assert_eq!(
            select_repo(UpdateTarget::Launcher, en),
            Some("https://gitcode.com/project-sasayaki/launcher")
        );
    }

    #[test]
    fn repo_api_url_variants() {
        let err_of = |s: &str| repo_api_url(s).unwrap_err();

        assert_eq!(
            repo_api_url("https://github.com/a/b").unwrap(),
            "https://api.github.com/repos/a/b/releases/latest"
        );
        assert_eq!(
            repo_api_url("https://github.com/a/b/").unwrap(),
            "https://api.github.com/repos/a/b/releases/latest"
        );
        assert_eq!(
            repo_api_url("https://www.github.com/a/b.git").unwrap(),
            "https://api.github.com/repos/a/b/releases/latest"
        );
        assert_eq!(
            repo_api_url("https://github.com/a/b/releases").unwrap(),
            "https://api.github.com/repos/a/b/releases/latest"
        );
        assert_eq!(
            repo_api_url("https://github.com/a/b/releases/latest").unwrap(),
            "https://api.github.com/repos/a/b/releases/latest"
        );
        assert_eq!(
            repo_api_url("https://gitcode.com/a/b").unwrap(),
            "https://api.gitcode.com/api/v5/repos/a/b/releases/latest"
        );

        assert_eq!(
            repo_api_url("https://github.com/a/b/releases/tag/v1.4.2").unwrap(),
            "https://api.github.com/repos/a/b/releases/tags/v1.4.2"
        );

        assert!(err_of("https://gitlab.com/a/b").contains("GitHub"));
        assert!(err_of("github.com/a/b").contains("GitHub"));
        assert!(err_of("https://github.com/a").contains("GitHub"));
        assert!(err_of("https://github.com/").contains("GitHub"));
        assert!(err_of("https://github.com/a/b/releases/tag/").contains("GitHub"));
    }

    const GITCODE_LATEST_JSON: &str = r#"{
        "tag_name": "1.0.1.0",
        "created_at": "2026-10-11T10:56:09+08:00",
        "release_status": "latest",
        "assets": [
            {"browser_download_url": "https://x/1.0.1.0.zip", "name": "1.0.1.0.zip", "type": "source"},
            {"browser_download_url": "https://gitcode.com/o/r/releases/download/1.0.1.0/kitten.zip", "name": "kitten.zip", "type": "attach"}
        ]
    }"#;

    const GITCODE_ARRAY_JSON: &str = r#"[
        {
            "tag_name": "1.0.0.0",
            "created_at": "2026-09-18T12:33:59+08:00",
            "release_status": "none",
            "assets": [
                {"browser_download_url": "https://gitcode.com/o/r/releases/download/1.0.0.0/kitten.zip", "name": "kitten.zip", "type": "attach"}
            ]
        },
        {
            "tag_name": "1.0.1.0",
            "created_at": "2026-10-11T10:56:09+08:00",
            "release_status": "latest",
            "assets": [
                {"browser_download_url": "https://gitcode.com/o/r/releases/download/1.0.1.0/kitten.zip", "name": "kitten.zip", "type": "attach"}
            ]
        }
    ]"#;

    #[test]
    fn parse_releases_single_object() {
        let rels = parse_releases(GITCODE_LATEST_JSON).unwrap();
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].tag_name, "1.0.1.0");
    }

    #[test]
    fn parse_releases_picks_newest_from_ascending_array() {

        let rels = parse_releases(GITCODE_ARRAY_JSON).unwrap();
        assert_eq!(rels[0].tag_name, "1.0.1.0");
        assert_eq!(rels[1].tag_name, "1.0.0.0");
    }

    #[test]
    fn parse_releases_falls_back_to_created_at_and_tail() {

        let by_time = r#"[
            {"tag_name": "v1", "created_at": "2026-01-01T00:00:00+08:00", "assets": []},
            {"tag_name": "v3", "created_at": "2026-03-01T00:00:00+08:00", "assets": []},
            {"tag_name": "v2", "created_at": "2026-02-01T00:00:00+08:00", "assets": []}
        ]"#;
        let rels = parse_releases(by_time).unwrap();
        assert_eq!(rels[0].tag_name, "v3");

        let no_time = r#"[
            {"tag_name": "v1", "assets": []},
            {"tag_name": "v2", "assets": []}
        ]"#;
        let rels = parse_releases(no_time).unwrap();
        assert_eq!(rels[0].tag_name, "v2");

        assert!(parse_releases("[]").is_err());
        assert!(parse_releases("not json").is_err());
    }

    #[test]
    fn loader_from_candidates_prefers_newest_with_matching_asset() {
        let rels = parse_releases(GITCODE_ARRAY_JSON).unwrap();
        let l = loader_from_candidates(&rels, UpdateTarget::Kitten).unwrap();
        assert_eq!(l.version, "1.0.1.0");
        assert_eq!(
            l.url,
            "https://gitcode.com/o/r/releases/download/1.0.1.0/kitten.zip"
        );
    }

    #[test]
    fn loader_from_candidates_skips_release_without_asset() {

        let json = r#"[
            {
                "tag_name": "1.0.0.0",
                "assets": [{"browser_download_url": "https://x/kitten.zip", "name": "kitten.zip"}]
            },
            {
                "tag_name": "1.0.1.0",
                "release_status": "latest",
                "assets": [{"browser_download_url": "https://x/1.0.1.0.zip", "name": "1.0.1.0.zip"}]
            }
        ]"#;
        let rels = parse_releases(json).unwrap();
        assert_eq!(rels[0].tag_name, "1.0.1.0");
        let l = loader_from_candidates(&rels, UpdateTarget::Kitten).unwrap();
        assert_eq!(l.version, "1.0.0.0");

        assert!(loader_from_candidates(&rels, UpdateTarget::Mice).is_err());
    }

    #[test]
    fn cache_schema_invalidates_files_written_by_older_builds() {

        let old = format!(
            r#"{{"saved_at": {}, "index": {{"loaders": [{{"name": "kitten", "version": "1.0.0.0", "url": "https://x/k.zip", "sha256": null}}]}}}}"#,
            now_secs()
        );
        let c: CacheFile = serde_json::from_str(&old).unwrap();
        assert_ne!(c.schema, CACHE_SCHEMA);

        let cur = CacheFile {
            schema: CACHE_SCHEMA,
            saved_at: now_secs(),
            index: UpdateIndex::default(),
        };
        let text = serde_json::to_string(&cur).unwrap();
        let back: CacheFile = serde_json::from_str(&text).unwrap();
        assert_eq!(back.schema, CACHE_SCHEMA);
    }

    #[test]
    fn install_zip_swaps_and_rolls_back() {
        let tmp = std::env::temp_dir().join("dnet_update_test");
        let _ = std::fs::remove_dir_all(&tmp);
        let base = tmp.join("tools");
        std::fs::create_dir_all(&base).unwrap();
        let target_dir = base.join("kitten");
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(target_dir.join("old.txt"), b"old").unwrap();

        let zip_path = tmp.join("kitten.zip");
        {
            use std::io::Write;
            let file = std::fs::File::create(&zip_path).unwrap();
            let mut zw = zip::ZipWriter::new(file);
            let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
            zw.start_file("kitten/new.txt", opts).unwrap();
            zw.write_all(b"new").unwrap();
            zw.finish().unwrap();
        }

        let progress = |_: UpdatePhase, _: u64, _: u64| {};
        let cancel = std::sync::atomic::AtomicBool::new(false);
        install_zip(&zip_path, &base, UpdateTarget::Kitten, &cancel, &progress).unwrap();

        assert!(target_dir.join("new.txt").exists());
        assert!(!target_dir.join("old.txt").exists());

        assert!(!base.join(".kitten.old").exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
