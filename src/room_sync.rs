use std::collections::HashMap;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::awl::{HOST_GAME_IP, ROOM_SYNC_PORT};

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

const POLL_INTERVAL: Duration = Duration::from_millis(200);

const CONNECT_RETRY: Duration = Duration::from_secs(1);

const STATUS_KEEPALIVE: Duration = Duration::from_secs(30);

const STATUS_STALE: Duration = Duration::from_secs(75);

const READ_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncMember {
    pub peer_id: String,
    pub ip: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nick: Option<String>,
    #[serde(default)]
    pub playing: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncTable {
    pub version: u64,
    pub host_peer_id: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_nick: Option<String>,
    #[serde(default)]
    pub host_playing: bool,
    pub members: Vec<SyncMember>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum UpFrame {
    Hello {
        peer_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        nick: Option<String>,
        #[serde(default)]
        playing: bool,
    },
    Status {
        peer_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        nick: Option<String>,
        #[serde(default)]
        playing: bool,
    },
}

#[derive(Deserialize)]
struct DownTag {
    #[serde(rename = "type")]
    kind: String,
}

const CLOSE_FRAME: &str = "{\"type\":\"close\"}\n";

struct HostState {
    stop: AtomicBool,

    closing: AtomicBool,

    version: AtomicU64,

    base: Mutex<Option<(String, Vec<(String, String)>, bool)>>,

    host_nick: Mutex<Option<String>>,

    statuses: Mutex<HashMap<String, (bool, Instant)>>,

    nicks: Mutex<HashMap<String, String>>,

    last: Mutex<Option<SyncTable>>,
}

fn host_rebuild(state: &HostState) {
    let Some((host_peer_id, members, host_playing)) = lock(&state.base).clone() else {
        return;
    };
    let host_nick = lock(&state.host_nick).clone();
    let now = Instant::now();
    let list: Vec<SyncMember> = {
        let statuses = lock(&state.statuses);
        let nicks = lock(&state.nicks);
        members
            .iter()
            .map(|(peer_id, ip)| SyncMember {
                playing: statuses
                    .get(peer_id)
                    .map(|(p, seen)| *p && now.duration_since(*seen) < STATUS_STALE)
                    .unwrap_or(false),
                nick: nicks.get(peer_id).cloned(),
                peer_id: peer_id.clone(),
                ip: ip.clone(),
            })
            .collect()
    };
    let mut last = lock(&state.last);
    let same = last
        .as_ref()
        .map(|t| {
            t.host_peer_id == host_peer_id
                && t.host_playing == host_playing
                && t.host_nick == host_nick
                && t.members.len() == list.len()
                && t.members
                    .iter()
                    .zip(list.iter())
                    .all(|(a, b)| {
                        a.peer_id == b.peer_id
                            && a.ip == b.ip
                            && a.nick == b.nick
                            && a.playing == b.playing
                    })
        })
        .unwrap_or(false);
    if same {
        return;
    }
    let version = state.version.load(Ordering::SeqCst) + 1;
    state.version.store(version, Ordering::SeqCst);
    *last = Some(SyncTable {
        version,
        host_peer_id,
        host_nick,
        host_playing,
        members: list,
    });
}

fn host_handle_up(state: &HostState, frame: UpFrame) {
    let (peer_id, playing, nick) = match frame {
        UpFrame::Hello { peer_id, nick, playing } | UpFrame::Status { peer_id, nick, playing } => {
            (peer_id, playing, nick)
        }
    };
    let now = Instant::now();
    state
        .statuses
        .lock()
        .unwrap()
        .insert(peer_id.clone(), (playing, now));
    if let Some(n) = nick {
        if !n.is_empty() {
            lock(&state.nicks).insert(peer_id, n);
        }
    }
    host_rebuild(state);
}

fn host_conn_loop(state: Arc<HostState>, stream: TcpStream) {
    let _ = stream.set_nodelay(true);
    if stream.set_read_timeout(Some(READ_TIMEOUT)).is_err() {
        return;
    }
    let Ok(reader_handle) = stream.try_clone() else { return };
    let mut reader = BufReader::new(reader_handle);
    let mut writer = stream;
    let mut sent_version = 0u64;
    let mut line = String::new();
    loop {
        if state.stop.load(Ordering::SeqCst) {
            return;
        }

        if state.closing.load(Ordering::SeqCst) {
            let _ = writer.write_all(CLOSE_FRAME.as_bytes()).and_then(|_| writer.flush());
            return;
        }
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => return,
            Ok(_) => {
                match serde_json::from_str::<UpFrame>(line.trim()) {
                    Ok(frame) => host_handle_up(&state, frame),
                    Err(_) => {}
                }
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(_) => return,
        }

        let v = state.version.load(Ordering::SeqCst);
        if v != sent_version {
            let snap = lock(&state.last).clone();
            if let Some(t) = snap {
                let mut buf = serde_json::to_string(&t).unwrap_or_default();
                buf.push('\n');
                if writer.write_all(buf.as_bytes()).and_then(|_| writer.flush()).is_err() {
                    return;
                }
                sent_version = v;
            }
        }
    }
}

fn host_accept_loop(state: Arc<HostState>) {
    let mut listener: Option<TcpListener> = None;
    while !state.stop.load(Ordering::SeqCst) {
        if listener.is_none() {
            match TcpListener::bind((HOST_GAME_IP, ROOM_SYNC_PORT)) {
                Ok(l) => {
                    let _ = l.set_nonblocking(true);
                    listener = Some(l);
                }
                Err(_) => {
                    std::thread::sleep(Duration::from_secs(1));
                    continue;
                }
            }
        }
        match listener.as_ref().unwrap().accept() {
            Ok((stream, _)) => {
                let st = state.clone();
                let _ = std::thread::Builder::new()
                    .name("room-sync-host-conn".into())
                    .spawn(move || host_conn_loop(st, stream));
            }
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => std::thread::sleep(POLL_INTERVAL),
            Err(_) => std::thread::sleep(POLL_INTERVAL),
        }
    }
}

pub struct HostSync {
    state: Arc<HostState>,
}

impl HostSync {
    pub fn start() -> HostSync {
        let state = Arc::new(HostState {
            stop: AtomicBool::new(false),
            closing: AtomicBool::new(false),
            version: AtomicU64::new(0),
            base: Mutex::new(None),
            host_nick: Mutex::new(None),
            statuses: Mutex::new(HashMap::new()),
            nicks: Mutex::new(HashMap::new()),
            last: Mutex::new(None),
        });
        let s = state.clone();
        let _ = std::thread::Builder::new()
            .name("room-sync-host".into())
            .spawn(move || host_accept_loop(s));
        HostSync { state }
    }

    pub fn publish(
        &self,
        host_peer_id: &str,
        members: &[(String, String)],
        host_playing: bool,
        host_nick: Option<&str>,
    ) {
        *lock(&self.state.base) =
            Some((host_peer_id.to_string(), members.to_vec(), host_playing));
        *lock(&self.state.host_nick) =
            host_nick.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
        host_rebuild(&self.state);
    }

    pub fn member_playing(&self) -> HashMap<String, bool> {
        let now = Instant::now();
        self.state
            .statuses
            .lock()
            .unwrap()
            .iter()
            .map(|(k, (p, seen))| (k.clone(), *p && now.duration_since(*seen) < STATUS_STALE))
            .collect()
    }

    pub fn member_nick(&self) -> HashMap<String, String> {
        lock(&self.state.nicks).clone()
    }

    pub fn notify_close(&self) {
        self.state.closing.store(true, Ordering::SeqCst);
    }
}

impl Drop for HostSync {
    fn drop(&mut self) {
        self.state.stop.store(true, Ordering::SeqCst);
    }
}

struct MemberState {
    stop: AtomicBool,
    connected: AtomicBool,

    closed: AtomicBool,

    peer_id: String,

    nick: Mutex<String>,

    inbox: Mutex<Option<SyncTable>>,

    outbox: Mutex<Option<bool>>,
}

fn up_frame_json(kind: &str, peer_id: &str, nick: &str, playing: bool) -> String {
    let nick_field = if nick.is_empty() {
        String::new()
    } else {
        let mut n = String::from(",\"nick\":");
        n.push_str(&serde_json::to_string(nick).unwrap_or_else(|_| "\"\"".into()));
        n
    };
    format!(
        "{{\"type\":\"{kind}\",\"peer_id\":{},\"playing\":{playing}{nick_field}}}",
        serde_json::to_string(peer_id).unwrap_or_else(|_| "\"\"".into())
    )
}

fn member_session(state: &Arc<MemberState>, stream: TcpStream) {
    let _ = stream.set_nodelay(true);
    if stream.set_read_timeout(Some(READ_TIMEOUT)).is_err() {
        return;
    }
    let Ok(writer_handle) = stream.try_clone() else { return };
    let Ok(hello_writer) = stream.try_clone() else { return };
    let mut writer = hello_writer;

    let hello_nick = lock(&state.nick).clone();
    let hello_playing = lock(&state.outbox).take().unwrap_or(false);
    let hello = up_frame_json("hello", &state.peer_id, &hello_nick, hello_playing);
    if writer
        .write_all(hello.as_bytes())
        .and_then(|_| writer.write_all(b"\n"))
        .and_then(|_| writer.flush())
        .is_err()
    {
        return;
    }
    state.connected.store(true, Ordering::SeqCst);

    let wstate = state.clone();
    let writer_thread = std::thread::Builder::new()
        .name("room-sync-member-w".into())
        .spawn(move || {
            let mut writer = writer_handle;
            let mut last_sent = (hello_playing, Instant::now());
            while !wstate.stop.load(Ordering::SeqCst) && !wstate.closed.load(Ordering::SeqCst) {
                let pending = lock(&wstate.outbox).take();
                let due = last_sent.1.elapsed() >= STATUS_KEEPALIVE;
                if pending.is_some() || due {
                    let playing = pending.unwrap_or(last_sent.0);
                    let nick = lock(&wstate.nick).clone();
                    let frame = up_frame_json("status", &wstate.peer_id, &nick, playing);
                    if writer
                        .write_all(frame.as_bytes())
                        .and_then(|_| writer.write_all(b"\n"))
                        .and_then(|_| writer.flush())
                        .is_err()
                    {
                        return;
                    }
                    last_sent = (playing, Instant::now());
                }
                std::thread::sleep(POLL_INTERVAL);
            }
        });

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        if state.stop.load(Ordering::SeqCst) || state.closed.load(Ordering::SeqCst) {
            break;
        }
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim();

                if serde_json::from_str::<DownTag>(trimmed)
                    .map(|t| t.kind == "close")
                    .unwrap_or(false)
                {
                    state.closed.store(true, Ordering::SeqCst);
                    break;
                }
                if let Ok(table) = serde_json::from_str::<SyncTable>(trimmed) {
                    *lock(&state.inbox) = Some(table);
                }
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(_) => break,
        }
    }
    state.connected.store(false, Ordering::SeqCst);
    if let Ok(handle) = writer_thread {
        let _ = handle.join();
    }
}

fn member_connect_loop(state: Arc<MemberState>) {
    while !state.stop.load(Ordering::SeqCst) && !state.closed.load(Ordering::SeqCst) {
        match TcpStream::connect((HOST_GAME_IP, ROOM_SYNC_PORT)) {
            Ok(stream) => member_session(&state, stream),
            Err(_) => {
                state.connected.store(false, Ordering::SeqCst);
            }
        }

        let deadline = Instant::now() + CONNECT_RETRY;
        while Instant::now() < deadline
            && !state.stop.load(Ordering::SeqCst)
            && !state.closed.load(Ordering::SeqCst)
        {
            std::thread::sleep(POLL_INTERVAL);
        }
    }
}

pub struct MemberSync {
    state: Arc<MemberState>,
}

impl MemberSync {
    pub fn start(my_peer_id: String, my_nick: String) -> MemberSync {
        let state = Arc::new(MemberState {
            stop: AtomicBool::new(false),
            connected: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            peer_id: my_peer_id,
            nick: Mutex::new(my_nick),
            inbox: Mutex::new(None),
            outbox: Mutex::new(None),
        });
        let s = state.clone();
        let _ = std::thread::Builder::new()
            .name("room-sync-member".into())
            .spawn(move || member_connect_loop(s));
        MemberSync { state }
    }

    pub fn take_snapshot(&self) -> Option<SyncTable> {
        lock(&self.state.inbox).take()
    }

    pub fn is_closed(&self) -> bool {
        self.state.closed.load(Ordering::SeqCst)
    }

    pub fn send_status(&self, playing: bool) {
        *lock(&self.state.outbox) = Some(playing);
    }
}

impl Drop for MemberSync {
    fn drop(&mut self) {
        self.state.stop.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_frame_json_with_and_without_nick() {
        let with = up_frame_json("hello", "peer1", "玩家", true);
        assert_eq!(with, "{\"type\":\"hello\",\"peer_id\":\"peer1\",\"playing\":true,\"nick\":\"玩家\"}");
        let without = up_frame_json("status", "peer1", "", false);
        assert_eq!(without, "{\"type\":\"status\",\"peer_id\":\"peer1\",\"playing\":false}");

        let f: UpFrame = serde_json::from_str(&with).unwrap();
        match f {
            UpFrame::Hello { peer_id, nick, playing } => {
                assert_eq!(peer_id, "peer1");
                assert_eq!(nick.as_deref(), Some("玩家"));
                assert!(playing);
            }
            _ => panic!("should parse as hello"),
        }
    }

    #[test]
    fn up_frame_without_nick_is_backward_compatible() {

        let f: UpFrame =
            serde_json::from_str("{\"type\":\"status\",\"peer_id\":\"p\",\"playing\":true}").unwrap();
        match f {
            UpFrame::Status { peer_id, nick, playing } => {
                assert_eq!(peer_id, "p");
                assert_eq!(nick, None);
                assert!(playing);
            }
            _ => panic!("should parse as status"),
        }
    }

    #[test]
    fn host_rebuild_aggregates_nick_and_host_nick() {
        let state = Arc::new(HostState {
            stop: AtomicBool::new(false),
            closing: AtomicBool::new(false),
            version: AtomicU64::new(0),
            base: Mutex::new(Some(("host".into(), vec![("m1".into(), "172.10.1.103".into())], false))),
            host_nick: Mutex::new(Some("房主昵称".into())),
            statuses: Mutex::new(HashMap::new()),
            nicks: Mutex::new(HashMap::from([("m1".into(), "成员昵称".into())])),
            last: Mutex::new(None),
        });
        host_rebuild(&state);
        let t = lock(&state.last).clone().unwrap();
        assert_eq!(t.host_nick.as_deref(), Some("房主昵称"));
        assert_eq!(t.members[0].nick.as_deref(), Some("成员昵称"));
    }

    #[test]
    fn table_and_close_frames_roundtrip() {

        let t = SyncTable {
            version: 3,
            host_peer_id: "host".into(),
            host_nick: Some("H".into()),
            host_playing: false,
            members: vec![SyncMember {
                peer_id: "m".into(),
                ip: "172.10.1.103".into(),
                nick: Some("M".into()),
                playing: true,
            }],
        };
        let json = serde_json::to_string(&t).unwrap();
        let back: SyncTable = serde_json::from_str(&json).unwrap();
        assert_eq!(back.host_nick.as_deref(), Some("H"));
        assert_eq!(back.members[0].nick.as_deref(), Some("M"));

        let legacy: SyncTable = serde_json::from_str(
            "{\"version\":1,\"host_peer_id\":\"h\",\"members\":[{\"peer_id\":\"m\",\"ip\":\"i\"}]}",
        )
        .unwrap();
        assert_eq!(legacy.host_nick, None);
        assert_eq!(legacy.members[0].nick, None);

        let tag: DownTag = serde_json::from_str("{\"type\":\"close\"}").unwrap();
        assert_eq!(tag.kind, "close");
    }
}
