use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::detect::IdentifyResult;
use crate::games::serverbox::StaticIp;

/// A game the launcher started, while it runs.
#[derive(Clone)]
pub struct GameRuntime {
    pub(super) index: usize,
    pub(super) child: Arc<Mutex<std::process::Child>>,
    /// The card reader or amdaemon started with the game.
    pub(super) companions: Arc<Mutex<Vec<std::process::Child>>>,
    /// Programs to end by name when the game exits.
    pub(super) kill_on_exit: Vec<String>,
    /// The ServerBox address added to the adapter for this session.
    pub(super) static_ip: Option<StaticIp>,
}

impl GameRuntime {
    /// Ends the programs that can outlive the processes the launcher started.
    pub(super) fn taskkill_images(&self) {
        for image in &self.kill_on_exit {
            let _ = std::process::Command::new("taskkill")
                .args(["/f", "/im", image])
                .creation_flags(crate::win_util::CREATE_NO_WINDOW)
                .spawn();
        }
    }
}

#[derive(Clone)]
pub struct ServerBoxRuntime {
    pub(super) index: usize,
    pub(super) child: Arc<Mutex<std::process::Child>>,
}

pub(super) struct PendingAdd {
    pub(super) path: PathBuf,
    pub(super) ir: IdentifyResult,
}

pub(super) struct CardBaseCache {

    pub(super) result: Result<image::RgbaImage, String>,
}

#[derive(Clone)]
pub(super) struct CardImage {
    pub(super) bytes: Arc<[u8]>,
    pub(super) width: f64,
    pub(super) height: f64,
}
