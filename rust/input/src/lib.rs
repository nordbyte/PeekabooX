use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::Duration;

use peekaboox_core::{BackendKind, PeekabooXError, Point, Result};

pub const EMERGENCY_STOP_HOTKEY_LABEL: &str = "CTRL+ALT+ESC";
pub const LINUX_EV_KEY: u16 = 0x01;
pub const LINUX_KEY_ESC: u16 = 1;
pub const LINUX_KEY_LEFTCTRL: u16 = 29;
pub const LINUX_KEY_LEFTALT: u16 = 56;
pub const LINUX_KEY_RIGHTCTRL: u16 = 97;
pub const LINUX_KEY_RIGHTALT: u16 = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputAction {
    MoveMouse(Point),
    Click {
        position: Point,
        button: MouseButton,
    },
    Drag {
        from: Point,
        to: Point,
        button: MouseButton,
        duration_ms: u64,
    },
    TypeText(String),
    Hotkey(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

pub trait InputBackend {
    fn execute(&self, action: InputAction) -> Result<()>;
    fn emergency_stop(&self) -> Result<()>;
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct EmergencyHotkeyState {
    ctrl_pressed: bool,
    alt_pressed: bool,
    esc_pressed: bool,
}

impl EmergencyHotkeyState {
    pub fn update_linux_key_event(&mut self, event_type: u16, key_code: u16, value: i32) -> bool {
        if event_type != LINUX_EV_KEY {
            return false;
        }

        let pressed = value != 0;
        match key_code {
            LINUX_KEY_LEFTCTRL | LINUX_KEY_RIGHTCTRL => self.ctrl_pressed = pressed,
            LINUX_KEY_LEFTALT | LINUX_KEY_RIGHTALT => self.alt_pressed = pressed,
            LINUX_KEY_ESC => self.esc_pressed = pressed,
            _ => return false,
        }

        pressed && self.ctrl_pressed && self.alt_pressed && self.esc_pressed
    }
}

#[derive(Debug, Default)]
pub struct UnimplementedInputBackend;

impl InputBackend for UnimplementedInputBackend {
    fn execute(&self, _action: InputAction) -> Result<()> {
        Err(PeekabooXError::new(
            "input injection backend is unavailable in this environment",
        ))
    }

    fn emergency_stop(&self) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionType {
    Wayland,
    X11,
    Unknown,
}

impl SessionType {
    fn from_value(value: Option<&str>) -> Self {
        match value.unwrap_or_default().to_ascii_lowercase().as_str() {
            "wayland" => Self::Wayland,
            "x11" => Self::X11,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputEnvironment {
    pub session_type: SessionType,
    pub current_desktop: Option<String>,
    pub commands: HashSet<String>,
    pub uinput_accessible: bool,
}

impl InputEnvironment {
    pub fn detect() -> Self {
        let command_names = ["ydotool", "wtype", "xdotool"];

        Self {
            session_type: SessionType::from_value(
                std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
            ),
            current_desktop: std::env::var("XDG_CURRENT_DESKTOP").ok(),
            commands: command_names
                .into_iter()
                .filter(|command| command_exists(command))
                .map(str::to_owned)
                .collect(),
            uinput_accessible: std::fs::OpenOptions::new()
                .write(true)
                .open("/dev/uinput")
                .is_ok(),
        }
    }

    fn has_command(&self, command: &str) -> bool {
        self.commands.contains(command)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputTool {
    Uinput,
    Ydotool,
    Wtype,
    Xdotool,
}

impl InputTool {
    pub fn name(self) -> &'static str {
        match self {
            Self::Uinput => "uinput",
            Self::Ydotool => "ydotool",
            Self::Wtype => "wtype",
            Self::Xdotool => "xdotool",
        }
    }

    pub fn backend_kind(self) -> BackendKind {
        match self {
            Self::Uinput => BackendKind::Uinput,
            Self::Ydotool => BackendKind::Uinput,
            Self::Wtype => BackendKind::Wayland,
            Self::Xdotool => BackendKind::X11,
        }
    }

    fn supports(self, action: &InputAction) -> bool {
        matches!(
            (self, action),
            (Self::Uinput, InputAction::MoveMouse(_))
                | (Self::Uinput, InputAction::Click { .. })
                | (Self::Uinput, InputAction::Drag { .. })
                | (Self::Ydotool, InputAction::MoveMouse(_))
                | (Self::Ydotool, InputAction::Click { .. })
                | (Self::Ydotool, InputAction::TypeText(_))
                | (Self::Ydotool, InputAction::Hotkey(_))
                | (Self::Wtype, InputAction::TypeText(_))
                | (Self::Xdotool, InputAction::MoveMouse(_))
                | (Self::Xdotool, InputAction::Click { .. })
                | (Self::Xdotool, InputAction::Drag { .. })
                | (Self::Xdotool, InputAction::TypeText(_))
                | (Self::Xdotool, InputAction::Hotkey(_))
        )
    }

    fn is_available(self, environment: &InputEnvironment) -> bool {
        match self {
            Self::Uinput => environment.uinput_accessible,
            Self::Ydotool => environment.has_command("ydotool") && environment.uinput_accessible,
            Self::Wtype => environment.has_command("wtype"),
            Self::Xdotool => environment.has_command("xdotool"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedInputBackend {
    pub tool: InputTool,
    pub session_type: SessionType,
}

impl DetectedInputBackend {
    pub fn name(&self) -> &'static str {
        self.tool.name()
    }

    pub fn backend_kind(&self) -> BackendKind {
        self.tool.backend_kind()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputExecutionMetadata {
    pub backend_name: String,
    pub backend_kind: BackendKind,
    pub action: InputAction,
}

#[derive(Debug, Default)]
pub struct CommandInputBackend;

impl CommandInputBackend {
    pub fn detect_backend_for(&self, action: &InputAction) -> Result<DetectedInputBackend> {
        let environment = InputEnvironment::detect();
        candidate_backends(&environment, action)
            .into_iter()
            .next()
            .ok_or_else(|| missing_backend_error(&environment, action))
    }

    pub fn execute_with_metadata(&self, action: InputAction) -> Result<InputExecutionMetadata> {
        let environment = InputEnvironment::detect();
        let candidates = candidate_backends(&environment, &action);

        if candidates.is_empty() {
            return Err(missing_backend_error(&environment, &action));
        }

        let mut errors = Vec::new();

        for backend in candidates {
            match run_input_tool(backend.tool, &action) {
                Ok(()) => {
                    return Ok(InputExecutionMetadata {
                        backend_name: backend.name().to_owned(),
                        backend_kind: backend.backend_kind(),
                        action,
                    });
                }
                Err(error) => {
                    let _ = release_modifiers();
                    errors.push(format!("{}: {}", backend.name(), error.message()));
                }
            }
        }

        Err(PeekabooXError::new(format!(
            "all input backends failed: {}",
            errors.join("; ")
        )))
    }
}

impl InputBackend for CommandInputBackend {
    fn execute(&self, action: InputAction) -> Result<()> {
        self.execute_with_metadata(action).map(|_| ())
    }

    fn emergency_stop(&self) -> Result<()> {
        release_modifiers()
    }
}

pub fn click(position: Point, button: MouseButton) -> Result<InputExecutionMetadata> {
    CommandInputBackend.execute_with_metadata(InputAction::Click { position, button })
}

pub fn move_mouse(position: Point) -> Result<InputExecutionMetadata> {
    CommandInputBackend.execute_with_metadata(InputAction::MoveMouse(position))
}

pub fn drag(
    from: Point,
    to: Point,
    button: MouseButton,
    duration_ms: u64,
) -> Result<InputExecutionMetadata> {
    CommandInputBackend.execute_with_metadata(InputAction::Drag {
        from,
        to,
        button,
        duration_ms,
    })
}

pub fn type_text(text: impl Into<String>) -> Result<InputExecutionMetadata> {
    CommandInputBackend.execute_with_metadata(InputAction::TypeText(text.into()))
}

pub fn hotkey(keys: Vec<String>) -> Result<InputExecutionMetadata> {
    CommandInputBackend.execute_with_metadata(InputAction::Hotkey(keys))
}

pub fn emergency_stop() -> Result<()> {
    CommandInputBackend.emergency_stop()
}

pub fn candidate_backends(
    environment: &InputEnvironment,
    action: &InputAction,
) -> Vec<DetectedInputBackend> {
    let mut candidates = Vec::new();

    match environment.session_type {
        SessionType::Wayland => {
            candidates.push(InputTool::Uinput);
            candidates.push(InputTool::Ydotool);
            candidates.push(InputTool::Wtype);
            candidates.push(InputTool::Xdotool);
        }
        SessionType::X11 => {
            candidates.push(InputTool::Xdotool);
            candidates.push(InputTool::Uinput);
            candidates.push(InputTool::Ydotool);
            candidates.push(InputTool::Wtype);
        }
        SessionType::Unknown => {
            candidates.push(InputTool::Ydotool);
            candidates.push(InputTool::Uinput);
            candidates.push(InputTool::Xdotool);
            candidates.push(InputTool::Wtype);
        }
    }

    candidates
        .into_iter()
        .filter_map(|tool| {
            if tool.is_available(environment) && tool.supports(action) {
                Some(DetectedInputBackend {
                    tool,
                    session_type: environment.session_type,
                })
            } else {
                None
            }
        })
        .collect()
}

fn run_input_tool(tool: InputTool, action: &InputAction) -> Result<()> {
    match (tool, action) {
        (InputTool::Uinput, InputAction::MoveMouse(position)) => uinput_move_mouse(*position),
        (InputTool::Uinput, InputAction::Click { position, button }) => {
            uinput_click(*position, *button)
        }
        (
            InputTool::Uinput,
            InputAction::Drag {
                from,
                to,
                button,
                duration_ms,
            },
        ) => uinput_drag(*from, *to, *button, *duration_ms),
        (InputTool::Ydotool, InputAction::MoveMouse(position)) => ydotool_mousemove(*position),
        (InputTool::Ydotool, InputAction::Click { position, button }) => {
            ydotool_mousemove(*position)?;
            run_command(
                "ydotool",
                ["click", "--delay", "0", ydotool_button(*button)],
            )
        }
        (InputTool::Ydotool, InputAction::TypeText(text)) => run_command_with_stdin(
            "ydotool",
            ["type", "--delay", "120", "--key-delay", "45", "--file", "-"],
            text,
        ),
        (InputTool::Ydotool, InputAction::Hotkey(keys)) => ydotool_hotkey(keys),
        (InputTool::Wtype, InputAction::TypeText(text)) => run_command("wtype", ["--", text]),
        (InputTool::Xdotool, InputAction::MoveMouse(position)) => run_command(
            "xdotool",
            [
                "mousemove",
                &position.x.to_string(),
                &position.y.to_string(),
            ],
        ),
        (InputTool::Xdotool, InputAction::Click { position, button }) => run_command(
            "xdotool",
            [
                "mousemove",
                &position.x.to_string(),
                &position.y.to_string(),
                "click",
                xdotool_button(*button),
            ],
        ),
        (
            InputTool::Xdotool,
            InputAction::Drag {
                from,
                to,
                button,
                duration_ms,
            },
        ) => xdotool_drag(*from, *to, *button, *duration_ms),
        (InputTool::Xdotool, InputAction::TypeText(text)) => {
            run_command("xdotool", ["type", "--delay", "0", "--", text])
        }
        (InputTool::Xdotool, InputAction::Hotkey(keys)) => xdotool_hotkey(keys),
        _ => Err(PeekabooXError::new(format!(
            "{} does not support action {:?}",
            tool.name(),
            action
        ))),
    }
}

fn ydotool_mousemove(position: Point) -> Result<()> {
    run_command(
        "ydotool",
        [
            "mousemove",
            "--delay",
            "0",
            &position.x.to_string(),
            &position.y.to_string(),
        ],
    )
}

fn ydotool_hotkey(keys: &[String]) -> Result<()> {
    let sequence = hotkey_sequence(keys)?;
    run_command(
        "ydotool",
        ["key", "--delay", "100", "--key-delay", "60", &sequence],
    )
}

fn ydotool_button(button: MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "1",
        MouseButton::Right => "2",
        MouseButton::Middle => "3",
    }
}

fn xdotool_button(button: MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "1",
        MouseButton::Middle => "2",
        MouseButton::Right => "3",
    }
}

fn xdotool_drag(from: Point, to: Point, button: MouseButton, duration_ms: u64) -> Result<()> {
    let button = xdotool_button(button);

    run_command(
        "xdotool",
        [
            "mousemove",
            "--sync",
            &from.x.to_string(),
            &from.y.to_string(),
            "mousedown",
            button,
        ],
    )?;

    let steps = drag_steps(duration_ms, from, to);
    let sleep_per_step = if steps == 0 {
        0
    } else {
        duration_ms / u64::from(steps)
    };

    for step in 1..=steps {
        let next = interpolate_point(from, to, step, steps);
        if let Err(error) = run_command(
            "xdotool",
            [
                "mousemove",
                "--sync",
                &next.x.to_string(),
                &next.y.to_string(),
            ],
        ) {
            let _ = run_command("xdotool", ["mouseup", button]);
            return Err(error);
        }

        if sleep_per_step > 0 {
            sleep(Duration::from_millis(sleep_per_step));
        }
    }

    run_command("xdotool", ["mouseup", button])
}

fn uinput_move_mouse(position: Point) -> Result<()> {
    let (screen_width, screen_height) = detect_screen_size().ok_or_else(|| {
        PeekabooXError::new("uinput move requires a detectable screen size from xrandr or xdpyinfo")
    })?;
    let mut device = UinputPointer::create(screen_width, screen_height)?;
    device.move_to(position)
}

fn uinput_click(position: Point, button: MouseButton) -> Result<()> {
    let (screen_width, screen_height) = detect_screen_size().ok_or_else(|| {
        PeekabooXError::new(
            "uinput click requires a detectable screen size from xrandr or xdpyinfo",
        )
    })?;
    let mut device = UinputPointer::create(screen_width, screen_height)?;
    device.move_to(position)?;
    device.set_button(button, true)?;
    sleep(Duration::from_millis(40));
    device.set_button(button, false)
}

fn uinput_drag(from: Point, to: Point, button: MouseButton, duration_ms: u64) -> Result<()> {
    let (screen_width, screen_height) = detect_screen_size().ok_or_else(|| {
        PeekabooXError::new("uinput drag requires a detectable screen size from xrandr or xdpyinfo")
    })?;
    let mut device = UinputPointer::create(screen_width, screen_height)?;

    device.move_to(from)?;
    device.set_button(button, true)?;

    let steps = drag_steps(duration_ms, from, to);
    let sleep_per_step = if steps == 0 {
        0
    } else {
        duration_ms / u64::from(steps)
    };

    for step in 1..=steps {
        let next = interpolate_point(from, to, step, steps);
        if let Err(error) = device.move_to(next) {
            let _ = device.set_button(button, false);
            return Err(error);
        }

        if sleep_per_step > 0 {
            sleep(Duration::from_millis(sleep_per_step));
        }
    }

    device.set_button(button, false)
}

fn drag_steps(duration_ms: u64, from: Point, to: Point) -> u32 {
    if from == to {
        return 1;
    }

    let distance = (from.x.abs_diff(to.x).max(from.y.abs_diff(to.y)) / 32).max(1);
    let timing = (duration_ms / 16).clamp(1, 120);
    distance.max(timing as u32).min(120)
}

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_ABS: u16 = 0x03;
const SYN_REPORT: u16 = 0x00;
const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;
const BTN_LEFT: u16 = 0x110;
const BTN_RIGHT: u16 = 0x111;
const BTN_MIDDLE: u16 = 0x112;
const BTN_TOOL_MOUSE: u16 = 0x146;
const BUS_USB: u16 = 0x03;
const UINPUT_IOCTL_BASE: u8 = b'U';
const IOC_NRBITS: libc::c_ulong = 8;
const IOC_TYPEBITS: libc::c_ulong = 8;
const IOC_SIZEBITS: libc::c_ulong = 14;
const IOC_NRSHIFT: libc::c_ulong = 0;
const IOC_TYPESHIFT: libc::c_ulong = IOC_NRSHIFT + IOC_NRBITS;
const IOC_SIZESHIFT: libc::c_ulong = IOC_TYPESHIFT + IOC_TYPEBITS;
const IOC_DIRSHIFT: libc::c_ulong = IOC_SIZESHIFT + IOC_SIZEBITS;
const IOC_NONE: libc::c_ulong = 0;
const IOC_WRITE: libc::c_ulong = 1;
const UI_DEV_CREATE: libc::c_ulong = ioctl_none(UINPUT_IOCTL_BASE, 1);
const UI_DEV_DESTROY: libc::c_ulong = ioctl_none(UINPUT_IOCTL_BASE, 2);
const UI_SET_EVBIT: libc::c_ulong = ioctl_write::<libc::c_int>(UINPUT_IOCTL_BASE, 100);
const UI_SET_KEYBIT: libc::c_ulong = ioctl_write::<libc::c_int>(UINPUT_IOCTL_BASE, 101);
const UI_SET_ABSBIT: libc::c_ulong = ioctl_write::<libc::c_int>(UINPUT_IOCTL_BASE, 103);

const fn ioctl_none(kind: u8, number: u8) -> libc::c_ulong {
    ioctl(IOC_NONE, kind, number, 0)
}

const fn ioctl_write<T>(kind: u8, number: u8) -> libc::c_ulong {
    ioctl(
        IOC_WRITE,
        kind,
        number,
        std::mem::size_of::<T>() as libc::c_ulong,
    )
}

const fn ioctl(
    direction: libc::c_ulong,
    kind: u8,
    number: u8,
    size: libc::c_ulong,
) -> libc::c_ulong {
    (direction << IOC_DIRSHIFT)
        | ((kind as libc::c_ulong) << IOC_TYPESHIFT)
        | ((number as libc::c_ulong) << IOC_NRSHIFT)
        | (size << IOC_SIZESHIFT)
}

struct UinputPointer {
    file: File,
    screen_width: i32,
    screen_height: i32,
    tool_active: bool,
}

impl UinputPointer {
    fn create(screen_width: i32, screen_height: i32) -> Result<Self> {
        let mut file = OpenOptions::new().write(true).open("/dev/uinput")?;
        let fd = file.as_raw_fd();

        ioctl_set(fd, UI_SET_EVBIT, EV_SYN)?;
        ioctl_set(fd, UI_SET_EVBIT, EV_KEY)?;
        ioctl_set(fd, UI_SET_EVBIT, EV_ABS)?;
        ioctl_set(fd, UI_SET_KEYBIT, BTN_LEFT)?;
        ioctl_set(fd, UI_SET_KEYBIT, BTN_RIGHT)?;
        ioctl_set(fd, UI_SET_KEYBIT, BTN_MIDDLE)?;
        ioctl_set(fd, UI_SET_KEYBIT, BTN_TOOL_MOUSE)?;
        ioctl_set(fd, UI_SET_ABSBIT, ABS_X)?;
        ioctl_set(fd, UI_SET_ABSBIT, ABS_Y)?;

        let mut device: libc::uinput_user_dev = unsafe { std::mem::zeroed() };
        copy_c_name(&mut device.name, "peekaboox-uinput-pointer");
        device.id.bustype = BUS_USB;
        device.id.vendor = 0x5042;
        device.id.product = 0x5849;
        device.id.version = 1;
        device.absmin[usize::from(ABS_X)] = 0;
        device.absmax[usize::from(ABS_X)] = screen_width.saturating_sub(1);
        device.absmin[usize::from(ABS_Y)] = 0;
        device.absmax[usize::from(ABS_Y)] = screen_height.saturating_sub(1);

        write_struct(&mut file, &device)?;
        ioctl_no_arg(fd, UI_DEV_CREATE)?;
        sleep(Duration::from_millis(100));

        Ok(Self {
            file,
            screen_width,
            screen_height,
            tool_active: false,
        })
    }

    fn move_to(&mut self, point: Point) -> Result<()> {
        if !self.tool_active {
            self.write_event(EV_KEY, BTN_TOOL_MOUSE, 1)?;
            self.tool_active = true;
        }

        self.write_event(EV_ABS, ABS_X, clamp_to_range(point.x, self.screen_width))?;
        self.write_event(EV_ABS, ABS_Y, clamp_to_range(point.y, self.screen_height))?;
        self.synchronize()
    }

    fn set_button(&mut self, button: MouseButton, pressed: bool) -> Result<()> {
        self.write_event(EV_KEY, uinput_button(button), if pressed { 1 } else { 0 })?;
        self.synchronize()
    }

    fn write_event(&mut self, event_type: u16, code: u16, value: i32) -> Result<()> {
        let event = libc::input_event {
            time: unsafe { std::mem::zeroed() },
            type_: event_type,
            code,
            value,
        };

        write_struct(&mut self.file, &event)?;
        Ok(())
    }

    fn synchronize(&mut self) -> Result<()> {
        self.write_event(EV_SYN, SYN_REPORT, 0)
    }
}

impl Drop for UinputPointer {
    fn drop(&mut self) {
        if self.tool_active {
            let _ = self.write_event(EV_KEY, BTN_TOOL_MOUSE, 0);
            let _ = self.synchronize();
        }
        let _ = ioctl_no_arg(self.file.as_raw_fd(), UI_DEV_DESTROY);
    }
}

fn uinput_button(button: MouseButton) -> u16 {
    match button {
        MouseButton::Left => BTN_LEFT,
        MouseButton::Middle => BTN_MIDDLE,
        MouseButton::Right => BTN_RIGHT,
    }
}

fn clamp_to_range(value: i32, upper_bound: i32) -> i32 {
    value.clamp(0, upper_bound.saturating_sub(1).max(0))
}

fn copy_c_name(target: &mut [libc::c_char], name: &str) {
    for (target, byte) in target.iter_mut().zip(name.bytes()) {
        *target = byte as libc::c_char;
    }
}

fn write_struct<T>(file: &mut File, value: &T) -> std::io::Result<()> {
    let bytes = unsafe {
        std::slice::from_raw_parts((value as *const T).cast::<u8>(), std::mem::size_of::<T>())
    };
    file.write_all(bytes)
}

fn ioctl_set(fd: libc::c_int, request: libc::c_ulong, value: u16) -> Result<()> {
    let result = unsafe { libc::ioctl(fd, request, libc::c_int::from(value)) };
    if result < 0 {
        return Err(PeekabooXError::new(
            std::io::Error::last_os_error().to_string(),
        ));
    }

    Ok(())
}

fn ioctl_no_arg(fd: libc::c_int, request: libc::c_ulong) -> Result<()> {
    let result = unsafe { libc::ioctl(fd, request) };
    if result < 0 {
        return Err(PeekabooXError::new(
            std::io::Error::last_os_error().to_string(),
        ));
    }

    Ok(())
}

fn detect_screen_size() -> Option<(i32, i32)> {
    screen_size_from_xrandr().or_else(screen_size_from_xdpyinfo)
}

fn screen_size_from_xrandr() -> Option<(i32, i32)> {
    let output = Command::new("xrandr").arg("--current").output().ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let Some((_, rest)) = line.split_once("current ") else {
            continue;
        };
        let mut tokens = rest.split_whitespace();
        let width = parse_screen_dimension(tokens.next()?)?;
        if tokens.next()? != "x" {
            continue;
        }
        let height = parse_screen_dimension(tokens.next()?)?;
        return valid_screen_size(width, height);
    }

    None
}

fn screen_size_from_xdpyinfo() -> Option<(i32, i32)> {
    let output = Command::new("xdpyinfo").output().ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if !line.contains("dimensions:") {
            continue;
        }
        let dimensions = line.split_whitespace().find(|token| {
            token.contains('x')
                && token
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_ascii_digit())
        })?;
        let (width, height) = dimensions.split_once('x')?;
        return valid_screen_size(width.parse().ok()?, parse_screen_dimension(height)?);
    }

    None
}

fn parse_screen_dimension(value: &str) -> Option<i32> {
    value
        .trim_end_matches(',')
        .trim_end_matches("px")
        .parse::<i32>()
        .ok()
}

fn valid_screen_size(width: i32, height: i32) -> Option<(i32, i32)> {
    (width > 0 && height > 0).then_some((width, height))
}

fn interpolate_point(from: Point, to: Point, step: u32, steps: u32) -> Point {
    Point::new(
        from.x + (((to.x - from.x) as i64 * i64::from(step)) / i64::from(steps)) as i32,
        from.y + (((to.y - from.y) as i64 * i64::from(step)) / i64::from(steps)) as i32,
    )
}

fn xdotool_hotkey(keys: &[String]) -> Result<()> {
    let sequence = hotkey_sequence(keys)?;
    run_command("xdotool", ["key", "--delay", "60", &sequence])
}

fn hotkey_sequence(keys: &[String]) -> Result<String> {
    if keys.is_empty() {
        return Err(PeekabooXError::new("hotkey must contain at least one key"));
    }

    if keys.iter().any(|key| key.trim().is_empty()) {
        return Err(PeekabooXError::new("hotkey keys must not be empty"));
    }

    Ok(keys
        .iter()
        .map(|key| key.trim())
        .collect::<Vec<_>>()
        .join("+"))
}

fn release_modifiers() -> Result<()> {
    let environment = InputEnvironment::detect();

    if environment.has_command("xdotool") {
        return run_command(
            "xdotool",
            [
                "keyup",
                "Control_L",
                "Control_R",
                "Shift_L",
                "Shift_R",
                "Alt_L",
                "Alt_R",
                "Super_L",
                "Super_R",
            ],
        );
    }

    Ok(())
}

fn missing_backend_error(environment: &InputEnvironment, action: &InputAction) -> PeekabooXError {
    PeekabooXError::new(format!(
        "no supported input backend found for {:?} in {:?}; install ydotool with /dev/uinput access for Wayland/global control, wtype for Wayland text input, or xdotool for X11",
        action, environment.session_type
    ))
}

fn run_command<const N: usize>(program: &str, args: [&str; N]) -> Result<()> {
    let output = Command::new(program).args(args).output()?;

    if output.status.success() {
        return Ok(());
    }

    Err(PeekabooXError::new(format!(
        "{program} failed with status {}; stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

fn run_command_with_stdin<const N: usize>(
    program: &str,
    args: [&str; N],
    stdin: &str,
) -> Result<()> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut child_stdin = child
        .stdin
        .take()
        .ok_or_else(|| PeekabooXError::new(format!("failed to open stdin for {program}")))?;
    child_stdin.write_all(stdin.as_bytes())?;
    drop(child_stdin);

    let output = child.wait_with_output()?;

    if output.status.success() {
        return Ok(());
    }

    Err(PeekabooXError::new(format!(
        "{program} failed with status {}; stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

fn command_exists(command: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };

    std::env::split_paths(&paths).any(|path| path.join(command).is_file())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{
        InputAction, InputBackend, InputEnvironment, InputTool, MouseButton, SessionType,
        UnimplementedInputBackend, candidate_backends,
    };
    use peekaboox_core::Point;

    #[test]
    fn emergency_stop_is_available_before_real_backend_exists() {
        let backend = UnimplementedInputBackend;

        assert!(backend.emergency_stop().is_ok());
    }

    #[test]
    fn unimplemented_execute_returns_typed_error() {
        let backend = UnimplementedInputBackend;
        let error = backend
            .execute(InputAction::Click {
                position: Point::new(1, 2),
                button: MouseButton::Left,
            })
            .unwrap_err();

        assert!(error.message().contains("input injection"));
    }

    #[test]
    fn selects_uinput_for_wayland_click_when_uinput_is_accessible() {
        let environment = environment(SessionType::Wayland, ["ydotool", "wtype"], true);
        let action = InputAction::Click {
            position: Point::new(10, 20),
            button: MouseButton::Left,
        };

        let backend = candidate_backends(&environment, &action).remove(0);

        assert_eq!(backend.tool, InputTool::Uinput);
    }

    #[test]
    fn selects_wtype_for_wayland_typing_without_uinput() {
        let environment = environment(SessionType::Wayland, ["ydotool", "wtype"], false);
        let action = InputAction::TypeText("hello".to_owned());

        let backend = candidate_backends(&environment, &action).remove(0);

        assert_eq!(backend.tool, InputTool::Wtype);
    }

    #[test]
    fn selects_xdotool_first_on_x11_clicks() {
        let environment = environment(SessionType::X11, ["ydotool", "xdotool"], true);
        let action = InputAction::Click {
            position: Point::new(10, 20),
            button: MouseButton::Left,
        };

        let backend = candidate_backends(&environment, &action).remove(0);

        assert_eq!(backend.tool, InputTool::Xdotool);
    }

    #[test]
    fn click_has_no_backend_when_only_wtype_exists() {
        let environment = environment(SessionType::Wayland, ["wtype"], false);
        let action = InputAction::Click {
            position: Point::new(10, 20),
            button: MouseButton::Left,
        };

        assert!(candidate_backends(&environment, &action).is_empty());
    }

    #[test]
    fn selects_xdotool_for_hotkeys() {
        let environment = environment(SessionType::X11, ["xdotool", "ydotool"], true);
        let action = InputAction::Hotkey(vec![
            "ctrl".to_owned(),
            "alt".to_owned(),
            "Escape".to_owned(),
        ]);

        let backend = candidate_backends(&environment, &action).remove(0);

        assert_eq!(backend.tool, InputTool::Xdotool);
    }

    #[test]
    fn selects_ydotool_for_wayland_hotkeys_when_uinput_is_accessible() {
        let environment = environment(SessionType::Wayland, ["ydotool", "xdotool"], true);
        let action = InputAction::Hotkey(vec!["ctrl".to_owned(), "s".to_owned()]);

        let backend = candidate_backends(&environment, &action).remove(0);

        assert_eq!(backend.tool, InputTool::Ydotool);
    }

    #[test]
    fn selects_xdotool_for_drags() {
        let environment = environment(SessionType::X11, ["xdotool", "ydotool"], true);
        let action = InputAction::Drag {
            from: Point::new(10, 20),
            to: Point::new(30, 40),
            button: MouseButton::Left,
            duration_ms: 150,
        };

        let backend = candidate_backends(&environment, &action).remove(0);

        assert_eq!(backend.tool, InputTool::Xdotool);
    }

    #[test]
    fn selects_uinput_for_wayland_drags() {
        let environment = environment(SessionType::Wayland, ["ydotool", "wtype"], true);
        let action = InputAction::Drag {
            from: Point::new(10, 20),
            to: Point::new(30, 40),
            button: MouseButton::Left,
            duration_ms: 150,
        };

        let backend = candidate_backends(&environment, &action).remove(0);

        assert_eq!(backend.tool, InputTool::Uinput);
    }

    #[test]
    fn drag_has_no_wayland_backend_without_xdotool() {
        let environment = environment(SessionType::Wayland, ["ydotool", "wtype"], false);
        let action = InputAction::Drag {
            from: Point::new(10, 20),
            to: Point::new(30, 40),
            button: MouseButton::Left,
            duration_ms: 150,
        };

        assert!(candidate_backends(&environment, &action).is_empty());
    }

    #[test]
    fn parses_screen_dimensions_from_xrandr_tokens() {
        assert_eq!(super::parse_screen_dimension("1200,"), Some(1200));
        assert_eq!(super::parse_screen_dimension("1920"), Some(1920));
    }

    #[test]
    fn clamps_uinput_coordinates_to_screen_bounds() {
        assert_eq!(super::clamp_to_range(-10, 1200), 0);
        assert_eq!(super::clamp_to_range(360, 1200), 360);
        assert_eq!(super::clamp_to_range(1400, 1200), 1199);
    }

    #[test]
    fn hotkey_sequence_joins_trimmed_keys() {
        let sequence = super::hotkey_sequence(&[" ctrl ".to_owned(), "s".to_owned()]).unwrap();

        assert_eq!(sequence, "ctrl+s");
    }

    #[test]
    fn drag_steps_are_never_zero() {
        let steps = super::drag_steps(0, Point::new(10, 20), Point::new(10, 20));

        assert_eq!(steps, 1);
    }

    #[test]
    fn emergency_hotkey_state_triggers_on_ctrl_alt_escape() {
        let mut state = super::EmergencyHotkeyState::default();

        assert!(!state.update_linux_key_event(super::LINUX_EV_KEY, super::LINUX_KEY_LEFTCTRL, 1));
        assert!(!state.update_linux_key_event(super::LINUX_EV_KEY, super::LINUX_KEY_LEFTALT, 1));
        assert!(state.update_linux_key_event(super::LINUX_EV_KEY, super::LINUX_KEY_ESC, 1));
    }

    #[test]
    fn emergency_hotkey_state_resets_released_modifiers() {
        let mut state = super::EmergencyHotkeyState::default();

        state.update_linux_key_event(super::LINUX_EV_KEY, super::LINUX_KEY_LEFTCTRL, 1);
        state.update_linux_key_event(super::LINUX_EV_KEY, super::LINUX_KEY_LEFTALT, 1);
        state.update_linux_key_event(super::LINUX_EV_KEY, super::LINUX_KEY_LEFTCTRL, 0);

        assert!(!state.update_linux_key_event(super::LINUX_EV_KEY, super::LINUX_KEY_ESC, 1));
    }

    fn environment<const N: usize>(
        session_type: SessionType,
        commands: [&str; N],
        uinput_accessible: bool,
    ) -> InputEnvironment {
        InputEnvironment {
            session_type,
            current_desktop: None,
            commands: commands
                .into_iter()
                .map(str::to_owned)
                .collect::<HashSet<_>>(),
            uinput_accessible,
        }
    }
}
