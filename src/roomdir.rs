use serde::Deserialize;
use std::sync::OnceLock;
use windows::core::PCWSTR;

use rust_i18n::t;

static DIR_SESSION: OnceLock<usize> = OnceLock::new();

fn shared_session() -> Result<*mut core::ffi::c_void, String> {
    use windows::Win32::Networking::WinHttp::{
        WinHttpOpen, WinHttpSetOption, WINHTTP_ACCESS_TYPE_NO_PROXY, WINHTTP_OPTION_RECEIVE_TIMEOUT,
    };
    if let Some(&s) = DIR_SESSION.get() {
        return Ok(s as *mut core::ffi::c_void);
    }
    let session = unsafe {
        WinHttpOpen(
            PCWSTR::null(),
            WINHTTP_ACCESS_TYPE_NO_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        )
    };
    if session.is_null() {
        return Err(t!("http.failed_to_initialize").to_string());
    }
    unsafe {
        let timeout_bytes = 5000u32.to_ne_bytes();
        let _ = WinHttpSetOption(Some(session), WINHTTP_OPTION_RECEIVE_TIMEOUT, Some(&timeout_bytes));
    }
    let _ = DIR_SESSION.set(session as usize);
    Ok(session)
}

pub const DEFAULT_DIRECTORY: &str = "https://dnet.arcadehub.xyz/roomapi";

#[derive(Clone, Debug)]
pub struct RoomDirClient {
    base: String,
}

#[derive(Clone, Debug)]
pub enum DirError {

    NotFound,

    Forbidden,

    Full,

    Transport(String),

    Other(String),
}

impl DirError {
    fn from_status(status: u16, body: &str) -> DirError {
        match status {
            404 => DirError::NotFound,
            403 => DirError::Forbidden,
            409 => DirError::Full,
            _ => DirError::Other(format!("HTTP {status}: {body}")),
        }
    }
    pub fn message(&self) -> String {
        match self {
            DirError::NotFound => t!("roomdir.room_not_found_or_expired").to_string(),
            DirError::Forbidden => t!("roomdir.wrong_room_password").to_string(),
            DirError::Full => t!("roomdir.room_is_full").to_string(),
            DirError::Transport(_e) => t!("roomdir.cannot_reach_room_server").to_string(),
            DirError::Other(m) => m.clone(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct RoomSummary {
    pub room_id: String,
    pub name: String,
    pub game: String,
    pub players: u32,
    pub max: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct JoinInfo {
    pub host_peer_id: String,
    pub host_ip: String,
    pub host_ip2: String,
    pub your_ip: String,
    pub game: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MyRoom {
    pub room_id: String,
    pub name: String,
    pub game: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MemberInfo {
    pub peer_id: String,
    pub ip: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MembersInfo {
        pub host_peer_id: String,
    pub members: Vec<MemberInfo>,
}

#[link(name = "winhttp")]
unsafe extern "system" {
    #[link_name = "WinHttpAddRequestHeaders"]
    fn raw_add_request_headers(
        request: *mut core::ffi::c_void,
        headers: *const u16,
        len: u32,
        flags: u32,
    ) -> i32;
}

impl RoomDirClient {
    pub fn new(base: impl Into<String>) -> Self {
        RoomDirClient { base: base.into() }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base.trim_end_matches('/'), path.trim_start_matches('/'))
    }

    fn request(&self, verb: &str, path: &str, body: Option<&str>) -> Result<(u16, String), String> {
        use windows::Win32::Networking::WinHttp::{
            WinHttpCloseHandle, WinHttpConnect, WinHttpOpenRequest,
            WinHttpQueryDataAvailable, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse,
            WinHttpSendRequest, WinHttpSetOption, SECURITY_FLAG_IGNORE_CERT_CN_INVALID,
            WINHTTP_ADDREQ_FLAG_ADD,
            WINHTTP_FLAG_SECURE,
            WINHTTP_OPEN_REQUEST_FLAGS, WINHTTP_OPTION_REDIRECT_POLICY,
            WINHTTP_OPTION_REDIRECT_POLICY_ALWAYS, WINHTTP_OPTION_SECURITY_FLAGS,
            WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
        };
        let url = self.url(path);
        let url_t = url.trim();
        let (secure, rest) = if let Some(r) = url_t.strip_prefix("https://") {
            (true, r)
        } else if let Some(r) = url_t.strip_prefix("http://") {
            (false, r)
        } else {
            return Err(t!("roomdir.directory_url_must_be_http").to_string());
        };
        let (host_part, path_part) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        let (host, port) = match host_part.rfind(':').and_then(|i| host_part[i + 1..].parse::<u16>().ok()) {
            Some(p) => (&host_part[..host_part.rfind(':').unwrap()], p),
            None => (host_part, if secure { 443 } else { 80 }),
        };
        let host = host.to_string();

        unsafe {

            let session = shared_session()?;

            let host_w = crate::win_util::to_wide_null(&host);
            let connect = WinHttpConnect(session, PCWSTR::from_raw(host_w.as_ptr()), port, 0);
            if connect.is_null() {
                return Err(t!("roomdir.cannot_connect_to", host = host).to_string());
            }
            let path_w = crate::win_util::to_wide_null(&path_part);
            let verb_w = crate::win_util::to_wide_null(verb);
            let flags = if secure { WINHTTP_FLAG_SECURE } else { WINHTTP_OPEN_REQUEST_FLAGS(0) };
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
                let _ = WinHttpCloseHandle(connect);
                return Err(t!("http.failed_to_create_request").to_string());
            }
            let policy: u32 = WINHTTP_OPTION_REDIRECT_POLICY_ALWAYS;
            let _ = WinHttpSetOption(
                Some(request),
                WINHTTP_OPTION_REDIRECT_POLICY,
                Some(&policy.to_ne_bytes()),
            );
            let sec: u32 = SECURITY_FLAG_IGNORE_CERT_CN_INVALID;
            let _ = WinHttpSetOption(Some(request), WINHTTP_OPTION_SECURITY_FLAGS, Some(&sec.to_ne_bytes()));

            let result = (|| -> Result<(), String> {

                let hdr = crate::win_util::to_wide_null("Content-Type: application/json\r\n");
                if verb == "POST" {
                    let ok = raw_add_request_headers(request, hdr.as_ptr(), (-1i32) as u32, WINHTTP_ADDREQ_FLAG_ADD);
                    if ok == 0 {
                        return Err(t!("roomdir.failed_to_add_request_headers").to_string());
                    }
                }
                let body_bytes = body.unwrap_or("").as_bytes();
                let optional: Option<*const core::ffi::c_void> = if body_bytes.is_empty() {
                    None
                } else {
                    Some(body_bytes.as_ptr() as *const core::ffi::c_void)
                };
                let body_len = body_bytes.len() as u32;
                WinHttpSendRequest(request, None, optional, body_len, body_len, 0)
                    .map_err(|e| t!("http.failed_to_send_request", error = e).to_string())?;
                WinHttpReceiveResponse(request, core::ptr::null_mut())
                    .map_err(|e| t!("http.failed_to_receive_response", error = e).to_string())?;
                Ok(())
            })();
            if let Err(e) = result {
                let _ = WinHttpCloseHandle(request);
                let _ = WinHttpCloseHandle(connect);
                return Err(e);
            }

            let mut status: u32 = 0;
            let mut len: u32 = 4;
            let _ = WinHttpQueryHeaders(
                request,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                PCWSTR::null(),
                Some((&mut status as *mut u32) as *mut core::ffi::c_void),
                &mut len,
                core::ptr::null_mut(),
            );
            let mut body_bytes: Vec<u8> = Vec::new();
            loop {
                let mut avail: u32 = 0;
                if WinHttpQueryDataAvailable(request, &mut avail).is_err() || avail == 0 {
                    break;
                }
                let start = body_bytes.len();
                body_bytes.resize(start + avail as usize, 0);
                let mut read: u32 = 0;
                if WinHttpReadData(request, body_bytes.as_mut_ptr().add(start) as *mut core::ffi::c_void, avail, &mut read).is_err() || read == 0 {
                    break;
                }
                body_bytes.truncate(start + read as usize);
            }
            let _ = WinHttpCloseHandle(request);
            let _ = WinHttpCloseHandle(connect);
            Ok((status as u16, String::from_utf8_lossy(&body_bytes).to_string()))
        }
    }

    fn parse_json<T: for<'de> Deserialize<'de>>(status: u16, body: &str) -> Result<T, DirError> {
        if !(200..300).contains(&status) {
            return Err(DirError::from_status(status, body));
        }
        serde_json::from_str(body).map_err(|e| DirError::Other(format!("bad json: {e}")))
    }

    pub fn create_room(
        &self,
        name: &str,
        game: &str,
        game_id: &str,
        password: &str,
        host_peer_id: &str,
        keychip: &str,
    ) -> Result<String, String> {
        let body = serde_json::json!({
            "name": name,
            "game": game,
            "game_id": game_id,
            "password": password,
            "host_peer_id": host_peer_id,
            "keychip": keychip,
        });
        let (status, text) = self.request("POST", "/rooms", Some(&body.to_string()))?;
        if !(200..300).contains(&status) {
            return Err(format!("HTTP {status}: {text}"));
        }
        let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("bad json: {e}"))?;
        v.get("room_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("missing room_id: {text}"))
    }

    pub fn list_rooms(&self, game_id: Option<&str>) -> Result<Vec<RoomSummary>, String> {
        let path = match game_id.map(str::trim).filter(|v| !v.is_empty()) {
            Some(id) => format!("/rooms?game_id={id}"),
            None => "/rooms".to_string(),
        };
        let (status, text) = self.request("GET", &path, None)?;
        #[derive(Deserialize)]
        struct Resp {
            rooms: Vec<RoomSummary>,
        }
        let r: Resp = Self::parse_json(status, &text).map_err(|e| e.message())?;
        Ok(r.rooms)
    }

    pub fn join(&self, room_id: &str, password: &str, peer_id: &str) -> Result<JoinInfo, DirError> {
        let body = serde_json::json!({ "peer_id": peer_id, "password": password });
        let (status, text) = self
            .request("POST", &format!("/rooms/{room_id}/join"), Some(&body.to_string()))
            .map_err(DirError::Transport)?;
        Self::parse_json(status, &text)
    }

    pub fn my_room(&self, keychip: &str) -> Result<Option<MyRoom>, String> {
        let (status, text) = self
            .request("GET", &format!("/rooms/mine?keychip={}", keychip), None)
            .map_err(|e| e)?;
        if status == 404 {
            return Ok(None);
        }
        if !(200..300).contains(&status) {
            return Err(format!("HTTP {status}: {text}"));
        }
        let r: MyRoom = serde_json::from_str(&text).map_err(|e| format!("bad json: {e}"))?;
        Ok(Some(r))
    }

    pub fn members(&self, room_id: &str) -> Result<MembersInfo, DirError> {
        let (status, text) = self
            .request("GET", &format!("/rooms/{room_id}/members"), None)
            .map_err(DirError::Transport)?;
        Self::parse_json(status, &text)
    }

    pub fn leave(&self, room_id: &str, peer_id: &str) -> Result<(), String> {
        let body = serde_json::json!({ "peer_id": peer_id });
        let (status, _text) = self.request("POST", &format!("/rooms/{room_id}/leave"), Some(&body.to_string()))?;
        if (200..300).contains(&status) {
            Ok(())
        } else {
            Err(format!("HTTP {status}"))
        }
    }

    pub fn close(
        &self,
        room_id: &str,
        host_peer_id: Option<&str>,
        keychip: &str,
    ) -> Result<(), String> {
        let mut q = format!("/rooms/{room_id}?keychip={}", keychip.trim());
        if let Some(p) = host_peer_id.map(str::trim).filter(|s| !s.is_empty()) {
            q.push_str(&format!("&host_peer_id={p}"));
        }
        let (status, _text) = self.request("DELETE", &q, None)?;
        if (200..300).contains(&status) {
            Ok(())
        } else {
            Err(format!("HTTP {status}"))
        }
    }

    pub fn heartbeat(&self, room_id: &str, keychip: &str) -> Result<(), DirError> {
        let body = serde_json::json!({ "keychip": keychip });
        let (status, text) = self
            .request("POST", &format!("/rooms/{room_id}/heartbeat"), Some(&body.to_string()))
            .map_err(DirError::Transport)?;
        if (200..300).contains(&status) {
            Ok(())
        } else {
            Err(DirError::from_status(status, &text))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_maps_to_dir_error() {

        assert!(matches!(DirError::from_status(404, "x"), DirError::NotFound));
        assert!(matches!(DirError::from_status(403, "x"), DirError::Forbidden));
        assert!(matches!(DirError::from_status(409, "x"), DirError::Full));

        match DirError::from_status(500, "boom") {
            DirError::Other(m) => {
                assert!(m.contains("500"), "the status code should be included: {m}");
                assert!(m.contains("boom"), "the response body should be included: {m}");
            }
            e => panic!("anything but 404/403/409 should be Other, got {e:?}"),
        }
    }

    #[test]
    fn dir_error_message_is_not_empty() {
        for e in [
            DirError::NotFound,
            DirError::Forbidden,
            DirError::Full,
            DirError::Transport("x".to_string()),
            DirError::Other("oops".to_string()),
        ] {
            assert!(!e.message().trim().is_empty(), "{e:?} should have a message");
        }
    }

    #[test]
    fn room_summary_parses_and_requires_fields() {
        let s: RoomSummary = serde_json::from_str(
            r#"{"room_id":"abc","name":"八人房","game":"ID8","game_id":"SBZZ","players":2,"max":4}"#,
        )
        .unwrap();
        assert_eq!(s.room_id, "abc");
        assert_eq!(s.name, "八人房");
        assert_eq!((s.players, s.max), (2, 4));

        assert!(serde_json::from_str::<RoomSummary>(r#"{"room_id":"x"}"#).is_err());
    }

    #[test]
    fn join_info_defaults_optional_name() {

        let j: JoinInfo = serde_json::from_str(
            r#"{"host_peer_id":"p1","host_ip":"10.0.0.1","host_ip2":"10.0.0.2","your_ip":"10.0.0.3","game":"ID8"}"#,
        )
        .unwrap();
        assert_eq!(j.name, "");
        assert_eq!(j.host_ip, "10.0.0.1");
    }

    #[test]
    fn members_info_parses_member_list() {
        let m: MembersInfo = serde_json::from_str(
            r#"{"host_peer_id":"p0","members":[{"peer_id":"p0","ip":"10.0.0.1"},{"peer_id":"p1","ip":"10.0.0.2"}]}"#,
        )
        .unwrap();
        assert_eq!(m.members.len(), 2);
        assert_eq!(m.members[1].peer_id, "p1");

        let empty: MembersInfo =
            serde_json::from_str(r#"{"host_peer_id":"p0","members":[]}"#).unwrap();
        assert!(empty.members.is_empty());
    }
}
