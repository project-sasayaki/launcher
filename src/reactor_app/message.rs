use std::collections::HashMap;
use std::path::PathBuf;

use windows_reactor::{ContentDialogResult, WindowSize};

use crate::cards::CardScan;
use crate::db;
use crate::detect::IdentifyResult;
use crate::icf;
use crate::netinfo::AdapterInfo;
use crate::rawinput;
use crate::server::ServerState;
use crate::update::{TargetStatus, UpdateTarget};

use super::state::{GameRuntime, ServerBoxRuntime};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowAction {
    Launch,
    LaunchServerBox,
    Cards,
    AddServerBox,
    RemoveServerBox,
    AddAmdaemon,
    RemoveAmdaemon,
    AddGameExe,
    RemoveGameExe,
    /// Open the game's own settings page.
    GameSettings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompanionKind {
    ServerBox,
    Amdaemon,
    GameExe,
}

#[derive(Clone)]
pub enum Message {
    NavChanged(Option<String>),
    PaneOpenChanged(bool),
    AddClicked,
    FilePicked(Option<PathBuf>),
    Identified(Box<IdentifyPayload>),

    IcfIdentified(Box<IcfPayload>),
    ServerPolled(ServerState),
    CardsScanned(u32, Box<CardScan>),
    CardClicked(usize),
    WindowResized(WindowSize),
    CardSlideTick(f64),
    BackToGames,

    GameRowDropToggled(usize),

    GameRowHovered(usize),

    GameRowUnhovered(usize),

    GameRowDropTick(u32, f64),

    RowMenu {
        index: usize,
        action: RowAction,
    },

    RowTestModeToggled(usize, bool),

    InputReverseToggled(usize, bool),

    FfbExpanderToggled,

    FfbAnimTick(u32, f64),

    FfbEnableToggled(bool),

    FfbParamChanged(usize, f64),

    CompanionPicked {
        kind: CompanionKind,
        index: usize,
        path: Option<PathBuf>,
    },

    CompanionIdentified {
        kind: CompanionKind,
        index: usize,
        path: PathBuf,
        result: Result<IdentifyResult, String>,
    },
    NamingTextChanged(String),

    GameRemoveClicked(usize),
    DialogClosed(ContentDialogResult),
    DpiScaleChanged(f64),

    CardComposeFailed(String),

    CardsLoadTick(u32),

    AimeCardCreate,

    AimeCardCreated(u32, Result<(), String>),

    BlankCardSelect,

    GfxPatchToggled(usize, bool),

    RefreshAdapters,
    AdaptersScanned(Box<Vec<AdapterInfo>>),
    AdapterSelected(Option<usize>),
    IpSelected(Option<usize>),

    NetworkSimulateToggled(bool),

    ServerHostChanged(String),

    RoomNickChanged(String),

    ServerboxIpChanged(String),

    HostIpChanged(String),

    StaticIpRemoved(Result<(), String>),

    LogLevelSelected(Option<usize>),

    LangSelected(Option<usize>),

    GameRegionSelected(usize, Option<usize>),
    KeychipChanged(String),
    SkipReaderInitToggled(bool),
    AutoInsertCardToggled(bool),

    SeatSelected(Option<usize>),

    SeatMasterToggled(bool),

    SaveDebounceTick(u32),

    CoinExpanderToggled,

    CoinAnimTick(u32, f64),
    FreeplayToggled(bool),

    CoinValueChanged(usize, Option<f64>),

    GraphicsDefaultToggled(bool),
    GraphicsBorderlessToggled(bool),
    GraphicsResolutionSelected(Option<usize>),
    GraphicsRenderScaleToggled(bool),
    GraphicsRenderScaleLevelSelected(Option<usize>),
    GraphicsMsaaToggled(bool),
    GraphicsMsaaLevelSelected(Option<usize>),
    GraphicsAfToggled(bool),
    GraphicsAfLevelSelected(Option<usize>),

    InputKindSelected(Option<usize>),
    InputDevicesScanned(Box<Vec<rawinput::GameDevice>>),
    InputRefreshDevices,

    InputDeviceSelected(usize, Option<usize>),

    InputDeviceDiagnosticsOpen,

    InputCaptureStart(usize),

    InputCaptured(u32, usize, u32),

    InputCapturedAxis(u32, usize, String),

    InputCapturedXi(u32, usize, String),

    InputCaptureTimeout(u32),

    /// A tab of the game's settings page was chosen, by its title.
    SettingsTabSelected(Option<String>),
    /// A WMMT3 setting was given a value (`None` when its control was emptied, or its
    /// dialog cancelled).
    Wmmt3SettingChanged(&'static str, Option<String>),
    /// Choose the folder or file of a WMMT3 path setting in a dialog, by its key.
    Wmmt3BrowsePath(&'static str),
    /// Capture a WMMT3 button binding from the device.
    Wmmt3CaptureStart(&'static str),
    /// A WMMT3 button binding was captured: epoch, setting key, value.
    Wmmt3Captured(u32, &'static str, String),
    /// The capture got an input KittenTools cannot use: epoch, text key of the reason.
    Wmmt3CaptureUnusable(u32, &'static str),
    Wmmt3DevicesScanned(Box<Vec<rawinput::GameDevice>>),

    InputCaptureTick(u32),

    InputRatioChanged(f64),

    InputRatioReset,

    InputShifterTypeSelected(Option<usize>),

    InputDualStickToggled(bool),

    InputLinearSteeringToggled(bool),

    InputLeftDeadzoneChanged(f64),

    InputRightDeadzoneChanged(f64),

    VpnConfigPicked(Option<PathBuf>),

    VpnImportClicked,

    VpnPassphraseChanged(String),

    VpnConnectClicked,

    VpnDisconnectClicked,

    VpnConnectResult(Option<String>),

    VpnDied(Option<String>),

    RoomFilterChanged(Option<usize>),

    RoomCreateGameChanged(Option<usize>),

    RoomCreateResult(Result<String, String>),

    RoomJoinResult(Result<crate::roomdir::JoinInfo, String>),

    RoomLeaveClicked,

    RoomCloseClicked,

    RoomLaunchGame,

    RoomLaunchServerBox,

    RoomFinished(Result<(), String>),

    RoomTick,

    RoomOpenCreateDialog,

    RoomCreateNameChanged(String),

    RoomCreatePasswordChanged(String),

    RoomOpenJoinDialog,

    RoomNickPromptReady,

    RoomRefreshList,

    RoomJoinDirect(String),

    RoomJoinIdChanged(String),

    RoomJoinPasswordChanged(String),

    RoomListRefreshed(Result<Vec<crate::roomdir::RoomSummary>, String>, bool),

    RoomResumeResult(Result<Option<crate::roomdir::MyRoom>, String>),

    CheckUpdatesNow,

    AutoCheckTick,

    UpdatesChecked(Result<HashMap<UpdateTarget, TargetStatus>, String>),

    UpdateLoader(UpdateTarget),

    CancelUpdate(UpdateTarget),

    UpdateAllClicked,

    UpdatePoll,

    UpdateFinished(UpdateTarget, Result<(), String>),

    AutoUpdateToggled(bool),

    LaunchDone {
        index: usize,
        result: Result<(), String>,

        goto: Option<(String, Option<usize>)>,
    },

    GameLaunched { runtime: GameRuntime },

    GamePollTick,

    ServerBoxLaunched {
        result: Result<(), String>,

        runtime: Option<ServerBoxRuntime>,

        goto: Option<(String, Option<usize>)>,
    },

    LaunchStop(usize),

    ServerBoxStop(usize),
}

#[derive(Clone)]
pub struct IdentifyPayload {
    pub(super) path: PathBuf,
    pub(super) result: Result<IdentifyResult, String>,
}

#[derive(Clone)]
pub struct IcfPayload {
    pub(super) path: PathBuf,
    pub(super) result: Result<(icf::IcfInfo, db::DbMatch), String>,
}

#[derive(Clone, Debug)]
pub(super) enum PendingRoomAction {
    Create { name: String, game_idx: Option<usize>, password: String },
    Join { room_id: String, password: String },
}

pub(super) enum Dialog {
    None,
    Naming {
        name: String,
    },

    ConfirmRemove {
        index: usize,
        name: String,
    },

    InputCapture {
        /// Text key of the label of the binding being captured.
        label_key: &'static str,
    },

    DeviceDiagnostics {
        text: String,
    },

    LaunchFailed {
        message: String,

        goto: Option<(String, Option<usize>)>,
    },

    UpdateAvailable { detail: String },

    RoomCreate {
        name: String,

        game: Option<usize>,
        password: String,
    },

    RoomJoin {
        room_id: String,
        password: String,
    },

    RoomNickPrompt {
        pending: PendingRoomAction,
    },

    Error {
        message: String,
    },
}
