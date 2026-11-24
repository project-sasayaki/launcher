#![allow(unsafe_op_in_unsafe_fn)]

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use windows::{
    Win32::{
        Devices::DeviceAndDriverInstallation::{
            CM_Get_Device_Interface_PropertyW, CM_Get_DevNode_PropertyW, CM_Get_Parent,
            CM_Locate_DevNodeW, CM_LOCATE_DEVNODE_NORMAL, CR_SUCCESS,
        },
        Devices::HumanInterfaceDevice::{
            HIDP_BUTTON_CAPS, HIDP_CAPS, HIDP_VALUE_CAPS, HidD_GetProductString,
            HidP_GetButtonCaps, HidP_GetCaps, HidP_GetUsageValue, HidP_GetUsages,
            HidP_GetValueCaps, HidP_Input, PHIDP_PREPARSED_DATA,
        },
        Devices::Properties::{
            DEVPKEY_NAME, DEVPKEY_Device_DriverInfPath, DEVPKEY_Device_InstanceId, DEVPROP_TYPE_STRING, DEVPROPTYPE,
        },
        Foundation::{CloseHandle, DEVPROPKEY, HANDLE, HWND, LPARAM, LRESULT, WPARAM},
        Storage::FileSystem::{
            CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
            OPEN_EXISTING,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::Input::{
            GetRawInputData, GetRawInputDeviceInfoW, GetRawInputDeviceList, RAWINPUT,
            RAWINPUTDEVICE, RAWINPUTDEVICELIST, RAWINPUTHEADER, RID_INPUT, RIDEV_INPUTSINK,
            RIDI_DEVICEINFO, RIDI_DEVICENAME, RIDI_PREPARSEDDATA, RIM_TYPEHID,
            RegisterRawInputDevices,
        },
        UI::Input::XboxController::{
            XInputGetCapabilities, XInputGetState, XINPUT_CAPABILITIES, XINPUT_FLAG_GAMEPAD,
            XINPUT_STATE, XUSER_MAX_COUNT,
        },
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DispatchMessageW, MSG, PEEK_MESSAGE_REMOVE_TYPE,
            PeekMessageW, RegisterClassW, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE,
            WM_INPUT, WNDCLASSW,
        },
    },
    core::{w, PCWSTR},
};

use rust_i18n::t;

const HID_USAGE_PAGE_GENERIC: u16 = 0x01;
const HID_USAGE_JOYSTICK: u16 = 0x04;
const HID_USAGE_GAMEPAD: u16 = 0x05;
const HID_USAGE_PAGE_BUTTON: u16 = 0x09;

const HID_USAGE_HAT: u16 = 0x39;

pub const DPAD_UP: u32 = 1;
pub const DPAD_RIGHT: u32 = 2;
pub const DPAD_DOWN: u32 = 3;
pub const DPAD_LEFT: u32 = 4;

fn hat_to_dpad(v: u32) -> Option<u32> {
    match v {
        0 => Some(DPAD_UP),
        2 => Some(DPAD_RIGHT),
        4 => Some(DPAD_DOWN),
        6 => Some(DPAD_LEFT),
        _ => None,
    }
}

#[derive(Clone, Debug)]
pub struct GameDevice {

    pub path: String,

    pub label: String,

    pub name: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NameSource {

    HidProduct,

    SiblingInterface,

    DevNode,

    ParentDevNode,

    PathFallback,
}

impl NameSource {
    /// Text key of the name of this source, as the diagnostics report shows it.
    fn label_key(self) -> &'static str {
        match self {
            NameSource::HidProduct => "rawinput.source_hid_product",
            NameSource::SiblingInterface => "rawinput.source_sibling_interface",
            NameSource::DevNode => "rawinput.source_dev_node",
            NameSource::ParentDevNode => "rawinput.source_parent_dev_node",
            NameSource::PathFallback => "rawinput.source_path_fallback",
        }
    }
}

static LAST_REPORT: OnceLock<Mutex<String>> = OnceLock::new();

pub fn device_diagnostics_text() -> String {
    LAST_REPORT
        .get()
        .and_then(|m| m.lock().ok().map(|g| g.clone()))
        .unwrap_or_default()
}

fn set_diagnostics(text: String) {
    let m = LAST_REPORT.get_or_init(|| Mutex::new(String::new()));
    if let Ok(mut g) = m.lock() {
        *g = text;
    }
}

#[derive(Clone, Debug)]
pub struct CapturedButton {

    pub button: u32,

    pub is_dpad: bool,
}

#[derive(Clone, Debug)]
pub struct CapturedAxis {
    /// The HID usage's name (X, Y, …, Slider, Dial, Wheel).
    pub name: String,
    /// The axis DirectInput makes of it, by the name KittenTools reads it by (X, Y, Z, RX,
    /// RY, RZ, U, V); `None` when DirectInput puts it in none of those eight.
    pub dinput: Option<&'static str>,
}

/// The axes of `DIJOYSTATE2` KittenTools reads, by its names for them (`idas_di.c`): the six
/// fixed axes, then the two sliders.
pub const DINPUT_AXES: [&str; 8] = ["X", "Y", "Z", "RX", "RY", "RZ", "U", "V"];

const HID_USAGE_PAGE_SIMULATION: u16 = 0x02;

/// The DirectInput axis of each value of a device, given as `(usage page, first usage, last
/// usage)` in the order `HidP_GetValueCaps` lists them; the result names each value's first
/// usage.
///
/// This is Windows' rule as Wine's DirectInput reproduces it (`dlls/dinput/joystick_hid.c`):
/// X to Rz, and the simulation controls that stand for them, keep their own axis; a second
/// value for an axis already taken, Slider, Dial and every other value go to the sliders, in
/// the order of the list. The hat switch is a POV, not an axis.
fn dinput_axis_names(values: &[(u16, u16, u16)]) -> Vec<Option<&'static str>> {
    let mut taken = [false; 6];
    let mut sliders = 0;
    let mut names = Vec::with_capacity(values.len());
    for &(page, first, last) in values {
        let mut first_name = None;
        for usage in first..=last {
            // The axis the usage stands for: 0 to 5 are X to Rz.
            let fixed = match (page, usage) {
                (HID_USAGE_PAGE_GENERIC, HID_USAGE_HAT) => continue,
                (HID_USAGE_PAGE_GENERIC, 0x30..=0x35) => Some((usage - 0x30) as usize),
                (HID_USAGE_PAGE_SIMULATION, 0xC8) => Some(0), // steering
                (HID_USAGE_PAGE_SIMULATION, 0xC4) => Some(1), // accelerator
                (HID_USAGE_PAGE_GENERIC, 0x38) | (HID_USAGE_PAGE_SIMULATION, 0xBB) => Some(2), // wheel, throttle
                (HID_USAGE_PAGE_SIMULATION, 0xBA | 0xC5) => Some(5), // rudder, brake
                _ => None,
            };
            // Its place in DIJOYSTATE2's axes: its own, or else the next slider (6, 7, …).
            let instance = match fixed {
                Some(axis) if !taken[axis] => {
                    taken[axis] = true;
                    axis
                }
                _ => {
                    sliders += 1;
                    5 + sliders
                }
            };
            if usage == first {
                first_name = DINPUT_AXES.get(instance).copied();
            }
        }
        names.push(first_name);
    }
    names
}

/// The usages a value capability covers, from its first to its last.
fn usage_range(cap: &HIDP_VALUE_CAPS) -> (u16, u16) {
    // SAFETY: `IsRange` tells which member of the union the HID parser filled in.
    unsafe {
        if cap.IsRange {
            (cap.Anonymous.Range.UsageMin, cap.Anonymous.Range.UsageMax)
        } else {
            let usage = cap.Anonymous.NotRange.Usage;
            (usage, usage)
        }
    }
}

static CAPTURE_SLOT: OnceLock<Mutex<Option<CapturedButton>>> = OnceLock::new();
static PREV_STATE: OnceLock<Mutex<HashMap<isize, Vec<u16>>>> = OnceLock::new();

static HAT_STATE: OnceLock<Mutex<HashMap<isize, u32>>> = OnceLock::new();
static AXIS_SLOT: OnceLock<Mutex<Option<CapturedAxis>>> = OnceLock::new();

static AXIS_STATE: OnceLock<Mutex<HashMap<isize, Vec<i32>>>> = OnceLock::new();
static CLASS_REGISTERED: OnceLock<()> = OnceLock::new();

fn capture_slot() -> &'static Mutex<Option<CapturedButton>> {
    CAPTURE_SLOT.get_or_init(|| Mutex::new(None))
}

fn prev_state() -> &'static Mutex<HashMap<isize, Vec<u16>>> {
    PREV_STATE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn hat_state() -> &'static Mutex<HashMap<isize, u32>> {
    HAT_STATE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn axis_slot() -> &'static Mutex<Option<CapturedAxis>> {
    AXIS_SLOT.get_or_init(|| Mutex::new(None))
}

fn axis_state() -> &'static Mutex<HashMap<isize, Vec<i32>>> {
    AXIS_STATE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn axis_usage_name(usage: u16) -> String {
    let s = match usage {
        0x30 => "X",
        0x31 => "Y",
        0x32 => "Z",
        0x33 => "RX",
        0x34 => "RY",
        0x35 => "RZ",
        0x36 => "Slider",
        0x37 => "Dial",
        0x38 => "Wheel",
        _ => return format!("Axis{:03X}", usage),
    };
    s.to_string()
}

fn axis_threshold(cap: &HIDP_VALUE_CAPS) -> i32 {
    let range = (cap.LogicalMax as i64 - cap.LogicalMin as i64).unsigned_abs() as i32;
    if range <= 0 {
        256
    } else {
        (range / 32).max(16)
    }
}

fn label_from_path(path: &str) -> String {
    let lower = path.to_lowercase();
    if let Some((_, rest)) = lower.split_once("hid#") {
        rest.split('#').next().unwrap_or(rest).to_uppercase()
    } else {
        t!("rawinput.unknown_device").to_string()
    }
}

unsafe fn hid_product_name(path: &str) -> Option<String> {
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let handle = CreateFileW(
        windows::core::PCWSTR(wide.as_ptr()),
        0,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        None,
        OPEN_EXISTING,
        FILE_FLAGS_AND_ATTRIBUTES(0),
        None,
    )
    .ok()?;
    let mut buf = [0u16; 128];
    let ok = HidD_GetProductString(handle, buf.as_mut_ptr() as *mut _, buf.len() as u32);
    let _ = CloseHandle(handle);
    if !ok {
        return None;
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let name = String::from_utf16_lossy(&buf[..end]);
    let name = name.trim_matches(|c: char| c.is_whitespace() || c.is_control()).to_string();
    (!name.is_empty()).then_some(name)
}

fn group_key(path: &str) -> String {
    let lower = path.to_lowercase();
    let seg = match lower.split_once("hid#") {
        Some((_, rest)) => rest.split('#').next().unwrap_or(rest),
        None => return String::new(),
    };
    seg.split("&mi_").next().unwrap_or(seg).to_string()
}

fn is_top_level_collection(path: &str) -> bool {
    !path.to_lowercase().contains("&mi_")
}

/// The INF files of Windows' own class drivers. Windows names a device they drive after
/// its class ("HID-compliant game controller", "USB Composite Device", … in the display
/// language), so that name does not tell which product it is.
const GENERIC_DRIVER_INFS: [&str; 7] =
    ["input.inf", "hidserv.inf", "usb.inf", "usbhub.inf", "usbhub3.inf", "keyboard.inf", "msmouse.inf"];

fn is_generic_driver_inf(inf: &str) -> bool {
    GENERIC_DRIVER_INFS.iter().any(|generic| inf.eq_ignore_ascii_case(generic))
}

/// The name Windows shows for a device node, unless it is only the name of a device class.
unsafe fn devnode_product_name(devnode: u32) -> Option<String> {
    let inf = devnode_string_property(devnode, &DEVPKEY_Device_DriverInfPath).unwrap_or_default();
    if is_generic_driver_inf(&inf) {
        return None;
    }
    devnode_string_property(devnode, &DEVPKEY_NAME)
}

fn string_from_utf16_bytes(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    String::from_utf16_lossy(&units[..end]).trim().to_string()
}

unsafe fn devnode_string_property(devnode: u32, key: *const DEVPROPKEY) -> Option<String> {
    let mut ptype = DEVPROPTYPE(0);
    let mut size = 0u32;
    let cr = CM_Get_DevNode_PropertyW(devnode, key, &mut ptype, None, &mut size, 0);
    if cr != CR_SUCCESS || size == 0 {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    let cr = CM_Get_DevNode_PropertyW(devnode, key, &mut ptype, Some(buf.as_mut_ptr()), &mut size, 0);
    if cr != CR_SUCCESS || ptype != DEVPROP_TYPE_STRING {
        return None;
    }
    let name = string_from_utf16_bytes(&buf);
    (!name.is_empty()).then_some(name)
}

unsafe fn pnp_device_name(path: &str) -> Option<(String, NameSource)> {
    let wide = crate::win_util::to_wide_null(path);
    let mut ptype = DEVPROPTYPE(0);
    let mut size = 0u32;

    let cr = CM_Get_Device_Interface_PropertyW(
        PCWSTR(wide.as_ptr()),
        &DEVPKEY_Device_InstanceId,
        &mut ptype,
        None,
        &mut size,
        0,
    );
    if cr != CR_SUCCESS || size == 0 {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    let cr = CM_Get_Device_Interface_PropertyW(
        PCWSTR(wide.as_ptr()),
        &DEVPKEY_Device_InstanceId,
        &mut ptype,
        Some(buf.as_mut_ptr()),
        &mut size,
        0,
    );
    if cr != CR_SUCCESS {
        return None;
    }
    let instance_id = string_from_utf16_bytes(&buf);
    if instance_id.is_empty() {
        return None;
    }
    let id_wide = crate::win_util::to_wide_null(&instance_id);
    let mut devnode = 0u32;
    if CM_Locate_DevNodeW(&mut devnode, PCWSTR(id_wide.as_ptr()), CM_LOCATE_DEVNODE_NORMAL)
        != CR_SUCCESS
    {
        return None;
    }
    if let Some(n) = devnode_product_name(devnode) {
        return Some((n, NameSource::DevNode));
    }
    let mut parent = devnode;
    for _ in 0..2 {
        let mut next = 0u32;
        if CM_Get_Parent(&mut next, parent, 0) != CR_SUCCESS || next == 0 {
            break;
        }
        parent = next;
        if let Some(n) = devnode_product_name(parent) {
            return Some((n, NameSource::ParentDevNode));
        }
    }
    None
}

unsafe fn resolve_device_name(path: &str, group_rep: Option<&String>) -> (String, NameSource) {
    if let Some(n) = hid_product_name(path) {
        return (n, NameSource::HidProduct);
    }
    if let Some(n) = group_rep {
        return (n.clone(), NameSource::SiblingInterface);
    }
    if let Some((n, src)) = pnp_device_name(path) {
        return (n, src);
    }
    (label_from_path(path), NameSource::PathFallback)
}

pub fn list_input_devices(kind: Option<&str>) -> Vec<GameDevice> {
    match kind {
        Some(k) if k.eq_ignore_ascii_case("xinput") => list_xinput_devices(),
        Some(_) => list_game_devices(),
        None => Vec::new(),
    }
}

fn list_xinput_devices() -> Vec<GameDevice> {
    let mut devices = Vec::new();
    for index in 0..XUSER_MAX_COUNT {
        let mut caps: XINPUT_CAPABILITIES = unsafe { std::mem::zeroed() };

        if unsafe { XInputGetCapabilities(index, XINPUT_FLAG_GAMEPAD, &mut caps) } != 0 {
            continue;
        }
        let label = t!("rawinput.gamepad", value = index + 1).to_string();
        devices.push(GameDevice {
            path: index.to_string(),

            name: label.clone(),
            label,
        });
    }
    devices
}

#[derive(Clone, Debug)]
pub struct CapturedXiButton {

    pub name: String,
}

fn xi_button_name(mask: u16) -> Option<&'static str> {
    const DPAD_UP: u16 = 0x0001;
    const DPAD_DOWN: u16 = 0x0002;
    const DPAD_LEFT: u16 = 0x0004;
    const DPAD_RIGHT: u16 = 0x0008;
    const START: u16 = 0x0010;
    const BACK: u16 = 0x0020;
    const LEFT_THUMB: u16 = 0x0040;
    const RIGHT_THUMB: u16 = 0x0080;
    const LEFT_SHOULDER: u16 = 0x0100;
    const RIGHT_SHOULDER: u16 = 0x0200;
    const A: u16 = 0x1000;
    const B: u16 = 0x2000;
    const X: u16 = 0x4000;
    const Y: u16 = 0x8000;
    Some(match mask {
        DPAD_UP => "DPAD_UP",
        DPAD_DOWN => "DPAD_DOWN",
        DPAD_LEFT => "DPAD_LEFT",
        DPAD_RIGHT => "DPAD_RIGHT",
        START => "START",
        BACK => "BACK",
        LEFT_THUMB => "LTHUMB",
        RIGHT_THUMB => "RTHUMB",
        LEFT_SHOULDER => "LB",
        RIGHT_SHOULDER => "RB",
        A => "A",
        B => "B",
        X => "X",
        Y => "Y",
        _ => return None,
    })
}

pub fn capture_xi_button(pad_index: u32, timeout_ms: u32) -> Option<CapturedXiButton> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms as u64);
    let mut prev_buttons: u16 = 0;
    let mut prev_lt = false;
    let mut prev_rt = false;
    loop {
        let mut state: XINPUT_STATE = unsafe { std::mem::zeroed() };

        if unsafe { XInputGetState(pad_index, &mut state) } == 0 {

            let w = state.Gamepad.wButtons.0;
            let newly = w & !prev_buttons;
            if newly != 0 {

                for bit in 0..16u32 {
                    let m = 1u16 << bit;
                    if newly & m != 0 {
                        if let Some(name) = xi_button_name(m) {
                            return Some(CapturedXiButton { name: name.to_string() });
                        }
                    }
                }
            }
            prev_buttons = w;
            let lt = state.Gamepad.bLeftTrigger > 0;
            let rt = state.Gamepad.bRightTrigger > 0;
            if lt && !prev_lt {
                return Some(CapturedXiButton {
                    name: "LT".to_string(),
                });
            }
            if rt && !prev_rt {
                return Some(CapturedXiButton {
                    name: "RT".to_string(),
                });
            }
            prev_lt = lt;
            prev_rt = rt;
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub fn list_game_devices() -> Vec<GameDevice> {
    unsafe { list_game_devices_impl() }
}

unsafe fn list_game_devices_impl() -> Vec<GameDevice> {
    let mut count = 0u32;
    let flags = std::mem::size_of::<RAWINPUTDEVICELIST>() as u32;

    if GetRawInputDeviceList(None, &mut count, flags) != 0 || count == 0 {
        return Vec::new();
    }
    let mut list = vec![
        RAWINPUTDEVICELIST {
            hDevice: HANDLE::default(),
            dwType: RIM_TYPEHID,
        };
        count as usize
    ];
    if GetRawInputDeviceList(Some(list.as_mut_ptr()), &mut count, flags) == u32::MAX {
        return Vec::new();
    }

    let mut paths: Vec<String> = Vec::new();
    for entry in list.iter().take(count as usize) {
        if entry.dwType != RIM_TYPEHID {
            continue;
        }
        let Some(path) = device_info_string(entry.hDevice, RIDI_DEVICENAME) else {
            continue;
        };

        let mut info_bytes = vec![0u8; 32];
        let mut size = info_bytes.len() as u32;
        let n = GetRawInputDeviceInfoW(
            Some(entry.hDevice),
            RIDI_DEVICEINFO,
            Some(info_bytes.as_mut_ptr() as *mut _),
            &mut size,
        );
        if n == u32::MAX {
            continue;
        }

        let usage = &info_bytes[20..24];
        let usage_page = u16::from_le_bytes([usage[0], usage[1]]);
        let usage = u16::from_le_bytes([usage[2], usage[3]]);
        if usage_page != HID_USAGE_PAGE_GENERIC
            || (usage != HID_USAGE_JOYSTICK && usage != HID_USAGE_GAMEPAD)
        {
            continue;
        }
        paths.push(path);
    }

    let hid_names: Vec<Option<String>> = paths.iter().map(|p| hid_product_name(p)).collect();

    let mut group_rep: HashMap<String, String> = HashMap::new();
    for (p, n) in paths.iter().zip(hid_names.iter()) {
        let Some(n) = n else { continue };
        let key = group_key(p);
        if key.is_empty() {
            continue;
        }
        match group_rep.get(&key) {
            None => {
                group_rep.insert(key, n.clone());
            }

            Some(_) if is_top_level_collection(p) => {
                group_rep.insert(key, n.clone());
            }
            Some(_) => {}
        }
    }

    let mut report = format!("{}\n", t!("rawinput.diagnostics_title"));
    let mut devices: Vec<GameDevice> = Vec::new();
    let mut fallback_paths: Vec<String> = Vec::new();
    for p in &paths {
        let (name, source) = resolve_device_name(p, group_rep.get(&group_key(p)));
        let line = t!("rawinput.diagnostics_device", path = p, source = t!(source.label_key()), name = name);
        report.push_str(&format!("  {line}\n"));

        if source == NameSource::PathFallback {
            fallback_paths.push(p.clone());
        }
        devices.push(GameDevice {
            path: p.clone(),
            label: name.clone(),
            name,
        });
    }

    let mut counts: HashMap<String, usize> = HashMap::new();
    for d in &devices {
        *counts.entry(d.name.clone()).or_default() += 1;
    }
    for d in devices.iter_mut() {
        if counts.get(&d.name).copied().unwrap_or(0) > 1 {
            d.label = t!("rawinput.label_with_path", name = d.name, path = label_from_path(&d.path)).to_string();
        }

        if fallback_paths.iter().any(|p| *p == d.path) {
            d.label = t!("rawinput.label_product_name_unavailable", label = d.label).to_string();
        }
    }
    for d in &devices {
        let line = t!("rawinput.diagnostics_list_item", label = d.label, name = d.name);
        report.push_str(&format!("  {line}\n"));
    }
    eprintln!("{report}");
    set_diagnostics(report);
    devices
}

unsafe fn device_info_string(
    hdevice: HANDLE,
    command: windows::Win32::UI::Input::RAW_INPUT_DEVICE_INFO_COMMAND,
) -> Option<String> {
    let mut size = 0u32;
    if GetRawInputDeviceInfoW(Some(hdevice), command, None, &mut size) == u32::MAX || size == 0 {
        return None;
    }
    let mut buf = vec![0u16; size as usize];
    let n = GetRawInputDeviceInfoW(
        Some(hdevice),
        command,
        Some(buf.as_mut_ptr() as *mut _),
        &mut size,
    );
    if n == u32::MAX {
        return None;
    }
    buf.truncate(n as usize);
    Some(String::from_utf16_lossy(&buf))
}

enum CaptureKind {
    Button,
    Axis,
}

pub fn capture_button(timeout_ms: u32) -> Option<CapturedButton> {
    if unsafe { capture_impl(Duration::from_millis(timeout_ms as u64), CaptureKind::Button) } {
        capture_slot().lock().unwrap().take()
    } else {
        None
    }
}

pub fn capture_axis(timeout_ms: u32) -> Option<CapturedAxis> {
    if unsafe { capture_impl(Duration::from_millis(timeout_ms as u64), CaptureKind::Axis) } {
        axis_slot().lock().unwrap().take()
    } else {
        None
    }
}

unsafe fn capture_impl(timeout: Duration, kind: CaptureKind) -> bool {
    *capture_slot().lock().unwrap() = None;
    *axis_slot().lock().unwrap() = None;
    prev_state().lock().unwrap().clear();
    hat_state().lock().unwrap().clear();
    axis_state().lock().unwrap().clear();

    let hinstance = match GetModuleHandleW(None).ok() {
        Some(h) => h,
        None => return false,
    };

    if CLASS_REGISTERED.set(()).is_ok() {
        let wc = WNDCLASSW {
            lpfnWndProc: Some(capture_wndproc),
            hInstance: hinstance.into(),
            lpszClassName: w!("DnetRawInputCapture"),
            ..Default::default()
        };
        RegisterClassW(&wc);
    }

    let hwnd = match CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("DnetRawInputCapture"),
        w!("Dnet input capture"),
        WINDOW_STYLE(0),
        0,
        0,
        0,
        0,
        None,
        None,
        Some(hinstance.into()),
        None,
    )
    .ok()
    {
        Some(h) => h,
        None => return false,
    };

    let devices = [
        RAWINPUTDEVICE {
            usUsagePage: HID_USAGE_PAGE_GENERIC,
            usUsage: HID_USAGE_JOYSTICK,
            dwFlags: RIDEV_INPUTSINK,
            hwndTarget: hwnd,
        },
        RAWINPUTDEVICE {
            usUsagePage: HID_USAGE_PAGE_GENERIC,
            usUsage: HID_USAGE_GAMEPAD,
            dwFlags: RIDEV_INPUTSINK,
            hwndTarget: hwnd,
        },
    ];

    if RegisterRawInputDevices(&devices, std::mem::size_of::<RAWINPUTDEVICE>() as u32).is_err() {
        return false;
    }

    let deadline = Instant::now() + timeout;
    let mut msg = MSG::default();
    loop {

        while PeekMessageW(
            &mut msg,
            Some(hwnd),
            0,
            0,
            PEEK_MESSAGE_REMOVE_TYPE(1),
        )
        .as_bool()
        {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let filled = match kind {
            CaptureKind::Button => capture_slot().lock().unwrap().is_some(),
            CaptureKind::Axis => axis_slot().lock().unwrap().is_some(),
        };
        if filled {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

unsafe extern "system" fn capture_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_INPUT {
        handle_raw_input(lparam);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

unsafe fn handle_raw_input(lparam: LPARAM) {
    let hraw = windows::Win32::UI::Input::HRAWINPUT(lparam.0 as *mut _);
    let header_size = std::mem::size_of::<RAWINPUTHEADER>() as u32;
    let mut size = 0u32;
    if GetRawInputData(hraw, RID_INPUT, None, &mut size, header_size) == u32::MAX || size == 0 {
        return;
    }
    let mut buf = vec![0u8; size as usize];
    if GetRawInputData(
        hraw,
        RID_INPUT,
        Some(buf.as_mut_ptr() as *mut _),
        &mut size,
        header_size,
    ) == u32::MAX
    {
        return;
    }
    let raw = &*(buf.as_ptr() as *const RAWINPUT);
    if raw.data.hid.dwCount == 0 {
        return;
    }
    let hdevice = raw.header.hDevice;

    let report_len = raw.data.hid.dwSizeHid as usize;
    let report_off = std::mem::offset_of!(RAWINPUT, data) + 8;

    let mut psize = 0u32;
    if GetRawInputDeviceInfoW(Some(hdevice), RIDI_PREPARSEDDATA, None, &mut psize) == u32::MAX
        || psize == 0
    {
        return;
    }
    let mut pbuf = vec![0u8; psize as usize];
    if GetRawInputDeviceInfoW(
        Some(hdevice),
        RIDI_PREPARSEDDATA,
        Some(pbuf.as_mut_ptr() as *mut _),
        &mut psize,
    ) == u32::MAX
    {
        return;
    }
    let preparsed = PHIDP_PREPARSED_DATA(pbuf.as_mut_ptr() as isize);

    let mut caps: HIDP_CAPS = std::mem::zeroed();
    if HidP_GetCaps(preparsed, &mut caps).is_err() {
        return;
    }
    let mut bcaps =
        vec![std::mem::zeroed::<HIDP_BUTTON_CAPS>(); caps.NumberInputButtonCaps as usize];
    let mut bcount = caps.NumberInputButtonCaps;
    if bcount > 0 && HidP_GetButtonCaps(HidP_Input, bcaps.as_mut_ptr(), &mut bcount, preparsed).is_err() {
        return;
    }

    let report_end = report_off + report_len;
    if report_end > buf.len() {
        return;
    }
    let report = &mut buf[report_off..report_end];

    let mut usages: Vec<u16> = Vec::new();
    for cap in bcaps.iter() {
        if cap.UsagePage != HID_USAGE_PAGE_BUTTON {
            continue;
        }
        let mut list = vec![0u16; 128];
        let mut ulen = list.len() as u32;
        if HidP_GetUsages(
            HidP_Input,
            HID_USAGE_PAGE_BUTTON,
            Some(cap.LinkCollection),
            list.as_mut_ptr(),
            &mut ulen,
            preparsed,
            report,
        )
        .is_err()
        {
            continue;
        }
        list.truncate(ulen as usize);
        for u in list {
            if !usages.contains(&u) {
                usages.push(u);
            }
        }
    }

    let key = hdevice.0 as isize;
    let newly = {
        let mut prev = prev_state().lock().unwrap();
        let old = prev.entry(key).or_default();
        let newly: Vec<u16> = usages
            .iter()
            .filter(|u| !old.contains(u))
            .copied()
            .collect();
        *old = usages;
        newly
    };
    if !newly.is_empty() {
        let mut slot = capture_slot().lock().unwrap();
        if slot.is_none() {
            *slot = Some(CapturedButton {
                button: newly[0] as u32,
                is_dpad: false,
            });
        }
    }

    let mut vcaps = vec![std::mem::zeroed::<HIDP_VALUE_CAPS>(); caps.NumberInputValueCaps as usize];
    let mut vcount = caps.NumberInputValueCaps;
    if HidP_GetValueCaps(HidP_Input, vcaps.as_mut_ptr(), &mut vcount, preparsed).is_err() {
        return;
    }
    let vcaps = &vcaps[..vcount as usize];

    let mut hat: Option<u32> = None;
    for cap in vcaps.iter() {
        if cap.UsagePage != HID_USAGE_PAGE_GENERIC {
            continue;
        }
        let (min, max) = usage_range(cap);
        if !(min..=max).contains(&HID_USAGE_HAT) {
            continue;
        }
        let mut value: u32 = 0;
        if HidP_GetUsageValue(
            HidP_Input,
            HID_USAGE_PAGE_GENERIC,
            Some(cap.LinkCollection),
            HID_USAGE_HAT,
            &mut value,
            preparsed,
            report,
        )
        .is_ok()
        {
            hat = Some(value);
            break;
        }
    }
    if let Some(v) = hat {

        let changed = hat_state().lock().unwrap().insert(key, v) != Some(v);
        if let Some(dir) = hat_to_dpad(v) {
            if changed {
                let mut slot = capture_slot().lock().unwrap();
                if slot.is_none() {
                    *slot = Some(CapturedButton {
                        button: dir,
                        is_dpad: true,
                    });
                }
            }
        }
    }

    let mut axis_prev = axis_state().lock().unwrap();
    let axis_vec = axis_prev.entry(key).or_default();
    let values: Vec<(u16, u16, u16)> = vcaps
        .iter()
        .map(|cap| {
            let (min, max) = usage_range(cap);
            (cap.UsagePage, min, max)
        })
        .collect();
    let dinput_names = dinput_axis_names(&values);

    let mut axis_index: u32 = 0;
    for (cap_index, cap) in vcaps.iter().enumerate() {
        let (page, min, max) = values[cap_index];
        if page != HID_USAGE_PAGE_GENERIC {
            continue;
        }
        if (min..=max).contains(&HID_USAGE_HAT) {

            continue;
        }
        let usage = min;
        let idx = axis_index as usize;
        axis_index += 1;
        let mut value: u32 = 0;
        if HidP_GetUsageValue(
            HidP_Input,
            HID_USAGE_PAGE_GENERIC,
            Some(cap.LinkCollection),
            usage,
            &mut value,
            preparsed,
            report,
        )
        .is_err()
        {
            continue;
        }
        let v = value as i32;
        let threshold = axis_threshold(cap);

        let moved = match axis_vec.get(idx).copied() {
            Some(p) => (v - p).abs() >= threshold,
            None => false,
        };
        if moved {
            let mut slot = axis_slot().lock().unwrap();
            if slot.is_none() {
                *slot = Some(CapturedAxis {
                    name: axis_usage_name(usage),
                    dinput: dinput_names[cap_index],
                });
            }
        }

        if idx >= axis_vec.len() {
            axis_vec.resize(idx + 1, v);
        }
        axis_vec[idx] = v;
    }
}

#[cfg(test)]
mod tests {
    use super::{dinput_axis_names, group_key, is_generic_driver_inf, is_top_level_collection};
    use super::{HID_USAGE_PAGE_GENERIC as GENERIC, HID_USAGE_PAGE_SIMULATION as SIMULATION};

    #[test]
    fn fixed_axes_keep_their_names_wherever_they_are() {
        // Rz listed before X, and the hat switch in between.
        let values = [(GENERIC, 0x35, 0x35), (GENERIC, 0x39, 0x39), (GENERIC, 0x30, 0x30), (GENERIC, 0x31, 0x31)];
        assert_eq!(dinput_axis_names(&values), vec![Some("RZ"), None, Some("X"), Some("Y")]);
    }

    #[test]
    fn sliders_and_dials_take_u_then_v_in_list_order() {
        let values = [(GENERIC, 0x37, 0x37), (GENERIC, 0x30, 0x30), (GENERIC, 0x36, 0x36), (GENERIC, 0x36, 0x36)];
        assert_eq!(dinput_axis_names(&values), vec![Some("U"), Some("X"), Some("V"), None]);
    }

    #[test]
    fn a_second_value_for_a_taken_axis_becomes_a_slider() {
        // The wheel usage stands for Z, which the Z value took first.
        let values = [(GENERIC, 0x32, 0x32), (GENERIC, 0x38, 0x38)];
        assert_eq!(dinput_axis_names(&values), vec![Some("Z"), Some("U")]);
    }

    #[test]
    fn simulation_controls_stand_for_axes() {
        let values = [(SIMULATION, 0xC8, 0xC8), (SIMULATION, 0xC4, 0xC4), (SIMULATION, 0xC5, 0xC5)];
        assert_eq!(dinput_axis_names(&values), vec![Some("X"), Some("Y"), Some("RZ")]);
    }

    #[test]
    fn a_range_is_named_by_its_first_usage_and_takes_every_slot() {
        // X to Z in one value, then a slider: the range takes X, Y and Z.
        let values = [(GENERIC, 0x30, 0x32), (GENERIC, 0x36, 0x36)];
        assert_eq!(dinput_axis_names(&values), vec![Some("X"), Some("U")]);
    }

    #[test]
    fn same_device_shares_group_key() {
        assert_eq!(
            group_key(r"\\?\HID#VID_044F&PID_B696&MI_00#7&1a2b3c"),
            "vid_044f&pid_b696"
        );
        assert_eq!(
            group_key(r"\\?\HID#VID_044F&PID_B696&MI_01#7&1a2b3c"),
            "vid_044f&pid_b696"
        );
        assert_eq!(group_key(r"\\?\HID#VID_044F&PID_B696#7&1a2b3c"), "vid_044f&pid_b696");
    }

    #[test]
    fn top_level_collection_has_no_interface_suffix() {
        assert!(is_top_level_collection(r"\\?\HID#VID_044F&PID_B696#7&1a2b3c"));
        assert!(!is_top_level_collection(r"\\?\HID#VID_044F&PID_B696&MI_01#7&1a2b3c"));
    }

    #[test]
    fn generic_driver_names_are_rejected() {
        assert!(is_generic_driver_inf("input.inf"));
        assert!(is_generic_driver_inf("usb.inf"));
        assert!(is_generic_driver_inf("Input.INF"));
        assert!(!is_generic_driver_inf("oem42.inf"));
        assert!(!is_generic_driver_inf(""));
    }
}
