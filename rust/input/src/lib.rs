use std::collections::HashSet;
use std::io::Write;
use std::process::{Command, Stdio};

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
    Ydotool,
    Wtype,
    Xdotool,
}

impl InputTool {
    pub fn name(self) -> &'static str {
        match self {
            Self::Ydotool => "ydotool",
            Self::Wtype => "wtype",
            Self::Xdotool => "xdotool",
        }
    }

    pub fn backend_kind(self) -> BackendKind {
        match self {
            Self::Ydotool => BackendKind::Uinput,
            Self::Wtype => BackendKind::Wayland,
            Self::Xdotool => BackendKind::X11,
        }
    }

    fn supports(self, action: &InputAction) -> bool {
        matches!(
            (self, action),
            (Self::Ydotool, InputAction::MoveMouse(_))
                | (Self::Ydotool, InputAction::Click { .. })
                | (Self::Ydotool, InputAction::TypeText(_))
                | (Self::Wtype, InputAction::TypeText(_))
                | (Self::Xdotool, InputAction::MoveMouse(_))
                | (Self::Xdotool, InputAction::Click { .. })
                | (Self::Xdotool, InputAction::TypeText(_))
                | (Self::Xdotool, InputAction::Hotkey(_))
        )
    }

    fn is_available(self, environment: &InputEnvironment) -> bool {
        match self {
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

pub fn type_text(text: impl Into<String>) -> Result<InputExecutionMetadata> {
    CommandInputBackend.execute_with_metadata(InputAction::TypeText(text.into()))
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
            candidates.push(InputTool::Ydotool);
            candidates.push(InputTool::Wtype);
            candidates.push(InputTool::Xdotool);
        }
        SessionType::X11 => {
            candidates.push(InputTool::Xdotool);
            candidates.push(InputTool::Ydotool);
            candidates.push(InputTool::Wtype);
        }
        SessionType::Unknown => {
            candidates.push(InputTool::Ydotool);
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
        (InputTool::Ydotool, InputAction::MoveMouse(position)) => ydotool_mousemove(*position),
        (InputTool::Ydotool, InputAction::Click { position, button }) => {
            ydotool_mousemove(*position)?;
            run_command(
                "ydotool",
                ["click", "--delay", "0", ydotool_button(*button)],
            )
        }
        (InputTool::Ydotool, InputAction::TypeText(text)) => {
            run_command_with_stdin("ydotool", ["type", "--delay", "0", "--file", "-"], text)
        }
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
        (InputTool::Xdotool, InputAction::TypeText(text)) => {
            run_command("xdotool", ["type", "--delay", "0", "--", text])
        }
        (InputTool::Xdotool, InputAction::Hotkey(keys)) => {
            if keys.is_empty() {
                return Err(PeekabooXError::new("hotkey must contain at least one key"));
            }
            run_command("xdotool", ["key", &keys.join("+")])
        }
        (_, InputAction::Hotkey(_)) => Err(PeekabooXError::new(
            "hotkeys are only implemented for xdotool backend",
        )),
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
    fn selects_ydotool_for_wayland_click_when_uinput_is_accessible() {
        let environment = environment(SessionType::Wayland, ["ydotool", "wtype"], true);
        let action = InputAction::Click {
            position: Point::new(10, 20),
            button: MouseButton::Left,
        };

        let backend = candidate_backends(&environment, &action).remove(0);

        assert_eq!(backend.tool, InputTool::Ydotool);
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
