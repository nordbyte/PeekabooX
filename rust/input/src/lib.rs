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
    PasteText {
        text: String,
        preserve_clipboard: bool,
    },
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
        let command_names = [
            "ydotool", "wtype", "xdotool", "wl-copy", "wl-paste", "xclip", "xsel",
        ];

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
    WlClipboard,
    XclipClipboard,
    XselClipboard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputToolSelection {
    #[default]
    Auto,
    Uinput,
    Ydotool,
    Wtype,
    Xdotool,
}

impl InputToolSelection {
    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Uinput => "uinput",
            Self::Ydotool => "ydotool",
            Self::Wtype => "wtype",
            Self::Xdotool => "xdotool",
        }
    }

    fn tool(self) -> Option<InputTool> {
        match self {
            Self::Auto => None,
            Self::Uinput => Some(InputTool::Uinput),
            Self::Ydotool => Some(InputTool::Ydotool),
            Self::Wtype => Some(InputTool::Wtype),
            Self::Xdotool => Some(InputTool::Xdotool),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MoveBoundsPolicy {
    #[default]
    Allow,
    Clamp,
    Fail,
}

impl MoveBoundsPolicy {
    pub fn name(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Clamp => "clamp",
            Self::Fail => "fail",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MoveMouseOptions {
    pub duration_ms: u64,
    pub steps: Option<u32>,
    pub bounds_policy: MoveBoundsPolicy,
    pub backend: InputToolSelection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClickMouseOptions {
    pub bounds_policy: MoveBoundsPolicy,
    pub backend: InputToolSelection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TypeTextOptions {
    pub typing_speed_chars_per_second: Option<u32>,
    pub delay_ms: Option<u64>,
    pub key_delay_ms: Option<u64>,
    pub backend: InputToolSelection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragMouseOptions {
    pub duration_ms: u64,
    pub steps: Option<u32>,
    pub bounds_policy: MoveBoundsPolicy,
    pub backend: InputToolSelection,
}

impl Default for DragMouseOptions {
    fn default() -> Self {
        Self {
            duration_ms: 250,
            steps: None,
            bounds_policy: MoveBoundsPolicy::Allow,
            backend: InputToolSelection::Auto,
        }
    }
}

impl InputTool {
    pub fn name(self) -> &'static str {
        match self {
            Self::Uinput => "uinput",
            Self::Ydotool => "ydotool",
            Self::Wtype => "wtype",
            Self::Xdotool => "xdotool",
            Self::WlClipboard => "wl-copy+hotkey",
            Self::XclipClipboard => "xclip+hotkey",
            Self::XselClipboard => "xsel+hotkey",
        }
    }

    pub fn backend_kind(self) -> BackendKind {
        match self {
            Self::Uinput => BackendKind::Uinput,
            Self::Ydotool => BackendKind::Uinput,
            Self::Wtype => BackendKind::Wayland,
            Self::Xdotool => BackendKind::X11,
            Self::WlClipboard => BackendKind::Wayland,
            Self::XclipClipboard | Self::XselClipboard => BackendKind::X11,
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
                | (Self::WlClipboard, InputAction::PasteText { .. })
                | (Self::XclipClipboard, InputAction::PasteText { .. })
                | (Self::XselClipboard, InputAction::PasteText { .. })
        )
    }

    fn is_available(self, environment: &InputEnvironment) -> bool {
        match self {
            Self::Uinput => environment.uinput_accessible,
            Self::Ydotool => environment.has_command("ydotool") && environment.uinput_accessible,
            Self::Wtype => environment.has_command("wtype"),
            Self::Xdotool => environment.has_command("xdotool"),
            Self::WlClipboard => {
                environment.has_command("wl-copy")
                    && !paste_hotkey_candidates(environment).is_empty()
            }
            Self::XclipClipboard => {
                environment.has_command("xclip") && !paste_hotkey_candidates(environment).is_empty()
            }
            Self::XselClipboard => {
                environment.has_command("xsel") && !paste_hotkey_candidates(environment).is_empty()
            }
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
        self.detect_backend_for_with_selection(action, InputToolSelection::Auto)
    }

    pub fn detect_backend_for_with_selection(
        &self,
        action: &InputAction,
        selection: InputToolSelection,
    ) -> Result<DetectedInputBackend> {
        let environment = InputEnvironment::detect();
        candidate_backends_with_selection(&environment, action, selection)
            .into_iter()
            .next()
            .ok_or_else(|| missing_backend_error_for_selection(&environment, action, selection))
    }

    pub fn execute_with_metadata(&self, action: InputAction) -> Result<InputExecutionMetadata> {
        self.execute_with_metadata_with_selection(action, InputToolSelection::Auto)
    }

    pub fn execute_with_metadata_with_selection(
        &self,
        action: InputAction,
        selection: InputToolSelection,
    ) -> Result<InputExecutionMetadata> {
        let environment = InputEnvironment::detect();
        let candidates = candidate_backends_with_selection(&environment, &action, selection);

        if candidates.is_empty() {
            return Err(missing_backend_error_for_selection(
                &environment,
                &action,
                selection,
            ));
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

    pub fn move_mouse_with_options(
        &self,
        position: Point,
        options: MoveMouseOptions,
    ) -> Result<InputExecutionMetadata> {
        let position = apply_move_bounds_policy(position, options.bounds_policy)?;
        let action = InputAction::MoveMouse(position);

        if options.duration_ms == 0 && options.steps.unwrap_or(1) <= 1 {
            return self.execute_with_metadata_with_selection(action, options.backend);
        }

        let backend = self.detect_backend_for_with_selection(&action, options.backend)?;
        let start = current_mouse_position()?;
        smooth_move_mouse(backend.tool, start, position, options)?;
        Ok(InputExecutionMetadata {
            backend_name: backend.name().to_owned(),
            backend_kind: backend.backend_kind(),
            action,
        })
    }

    pub fn click_with_options(
        &self,
        position: Point,
        button: MouseButton,
        options: ClickMouseOptions,
    ) -> Result<InputExecutionMetadata> {
        let position = apply_move_bounds_policy(position, options.bounds_policy)?;
        self.execute_with_metadata_with_selection(
            InputAction::Click { position, button },
            options.backend,
        )
    }

    pub fn drag_with_options(
        &self,
        from: Point,
        to: Point,
        button: MouseButton,
        options: DragMouseOptions,
    ) -> Result<InputExecutionMetadata> {
        let from = apply_move_bounds_policy(from, options.bounds_policy)?;
        let to = apply_move_bounds_policy(to, options.bounds_policy)?;
        let action = InputAction::Drag {
            from,
            to,
            button,
            duration_ms: options.duration_ms,
        };
        let environment = InputEnvironment::detect();
        let candidates = candidate_backends_with_selection(&environment, &action, options.backend);

        if candidates.is_empty() {
            return Err(missing_backend_error_for_selection(
                &environment,
                &action,
                options.backend,
            ));
        }

        let mut errors = Vec::new();
        for backend in candidates {
            match run_drag_tool(backend.tool, from, to, button, options) {
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

    pub fn type_text_with_options(
        &self,
        text: String,
        options: TypeTextOptions,
    ) -> Result<InputExecutionMetadata> {
        validate_type_text_options(options)?;
        let action = InputAction::TypeText(text.clone());
        let environment = InputEnvironment::detect();
        let candidates = candidate_backends_with_selection(&environment, &action, options.backend);

        if candidates.is_empty() {
            return Err(missing_backend_error_for_selection(
                &environment,
                &action,
                options.backend,
            ));
        }

        let mut errors = Vec::new();
        for backend in candidates {
            match run_type_text_tool(backend.tool, &text, options) {
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
    click_with_options(position, button, ClickMouseOptions::default())
}

pub fn click_with_options(
    position: Point,
    button: MouseButton,
    options: ClickMouseOptions,
) -> Result<InputExecutionMetadata> {
    CommandInputBackend.click_with_options(position, button, options)
}

pub fn move_mouse(position: Point) -> Result<InputExecutionMetadata> {
    move_mouse_with_options(position, MoveMouseOptions::default())
}

pub fn move_mouse_with_options(
    position: Point,
    options: MoveMouseOptions,
) -> Result<InputExecutionMetadata> {
    CommandInputBackend.move_mouse_with_options(position, options)
}

pub fn current_mouse_position() -> Result<Point> {
    let environment = InputEnvironment::detect();
    if !environment.has_command("xdotool") {
        return Err(PeekabooXError::new(
            "cursor position query requires xdotool",
        ));
    }

    let output = run_command_capture_stdout("xdotool", ["getmouselocation", "--shell"])?;
    parse_xdotool_mouse_location(&output)
        .ok_or_else(|| PeekabooXError::new("failed to parse xdotool cursor position output"))
}

pub fn screen_size() -> Option<(i32, i32)> {
    detect_screen_size()
}

pub fn resolve_move_position(position: Point, bounds_policy: MoveBoundsPolicy) -> Result<Point> {
    apply_move_bounds_policy(position, bounds_policy)
}

pub fn drag(
    from: Point,
    to: Point,
    button: MouseButton,
    duration_ms: u64,
) -> Result<InputExecutionMetadata> {
    drag_with_options(
        from,
        to,
        button,
        DragMouseOptions {
            duration_ms,
            ..DragMouseOptions::default()
        },
    )
}

pub fn drag_with_options(
    from: Point,
    to: Point,
    button: MouseButton,
    options: DragMouseOptions,
) -> Result<InputExecutionMetadata> {
    CommandInputBackend.drag_with_options(from, to, button, options)
}

pub fn type_text(text: impl Into<String>) -> Result<InputExecutionMetadata> {
    type_text_with_options(text, TypeTextOptions::default())
}

pub fn type_text_with_options(
    text: impl Into<String>,
    options: TypeTextOptions,
) -> Result<InputExecutionMetadata> {
    CommandInputBackend.type_text_with_options(text.into(), options)
}

pub fn paste_text(text: impl Into<String>) -> Result<InputExecutionMetadata> {
    paste_text_with_options(text, false)
}

pub fn paste_text_with_options(
    text: impl Into<String>,
    preserve_clipboard: bool,
) -> Result<InputExecutionMetadata> {
    CommandInputBackend.execute_with_metadata(InputAction::PasteText {
        text: text.into(),
        preserve_clipboard,
    })
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
            if matches!(action, InputAction::PasteText { .. }) {
                candidates.push(InputTool::WlClipboard);
                candidates.push(InputTool::XclipClipboard);
                candidates.push(InputTool::XselClipboard);
            } else {
                candidates.push(InputTool::Uinput);
                if matches!(action, InputAction::TypeText(_)) {
                    candidates.push(InputTool::Wtype);
                    candidates.push(InputTool::Ydotool);
                } else {
                    candidates.push(InputTool::Ydotool);
                    candidates.push(InputTool::Wtype);
                }
                candidates.push(InputTool::Xdotool);
            }
        }
        SessionType::X11 => {
            if matches!(action, InputAction::PasteText { .. }) {
                candidates.push(InputTool::XclipClipboard);
                candidates.push(InputTool::XselClipboard);
                candidates.push(InputTool::WlClipboard);
            } else {
                candidates.push(InputTool::Xdotool);
                candidates.push(InputTool::Uinput);
                candidates.push(InputTool::Ydotool);
                candidates.push(InputTool::Wtype);
            }
        }
        SessionType::Unknown => {
            if matches!(action, InputAction::PasteText { .. }) {
                candidates.push(InputTool::WlClipboard);
                candidates.push(InputTool::XclipClipboard);
                candidates.push(InputTool::XselClipboard);
            } else {
                candidates.push(InputTool::Ydotool);
                candidates.push(InputTool::Uinput);
                candidates.push(InputTool::Xdotool);
                candidates.push(InputTool::Wtype);
            }
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

pub fn candidate_backends_with_selection(
    environment: &InputEnvironment,
    action: &InputAction,
    selection: InputToolSelection,
) -> Vec<DetectedInputBackend> {
    let candidates = candidate_backends(environment, action);
    let Some(selected_tool) = selection.tool() else {
        return candidates;
    };

    candidates
        .into_iter()
        .filter(|backend| backend.tool == selected_tool)
        .collect()
}

fn run_input_tool(tool: InputTool, action: &InputAction) -> Result<()> {
    match (tool, action) {
        (InputTool::Uinput, InputAction::MoveMouse(position))
        | (InputTool::Ydotool, InputAction::MoveMouse(position))
        | (InputTool::Xdotool, InputAction::MoveMouse(position)) => {
            run_move_mouse_tool(tool, *position)
        }
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
        ) => uinput_drag(
            *from,
            *to,
            *button,
            DragMouseOptions {
                duration_ms: *duration_ms,
                ..DragMouseOptions::default()
            },
        ),
        (InputTool::Ydotool, InputAction::Click { position, button }) => {
            ydotool_mousemove(*position)?;
            run_command(
                "ydotool",
                ["click", "--delay", "0", ydotool_button(*button)],
            )
        }
        (InputTool::Ydotool, InputAction::TypeText(text)) => {
            run_type_text_tool(tool, text, TypeTextOptions::default())
        }
        (InputTool::Ydotool, InputAction::Hotkey(keys)) => ydotool_hotkey(keys),
        (InputTool::Wtype, InputAction::TypeText(text)) => {
            run_type_text_tool(tool, text, TypeTextOptions::default())
        }
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
        ) => xdotool_drag(
            *from,
            *to,
            *button,
            DragMouseOptions {
                duration_ms: *duration_ms,
                ..DragMouseOptions::default()
            },
        ),
        (InputTool::Xdotool, InputAction::TypeText(text)) => {
            run_type_text_tool(tool, text, TypeTextOptions::default())
        }
        (InputTool::Xdotool, InputAction::Hotkey(keys)) => xdotool_hotkey(keys),
        (
            InputTool::WlClipboard,
            InputAction::PasteText {
                text,
                preserve_clipboard,
            },
        )
        | (
            InputTool::XclipClipboard,
            InputAction::PasteText {
                text,
                preserve_clipboard,
            },
        )
        | (
            InputTool::XselClipboard,
            InputAction::PasteText {
                text,
                preserve_clipboard,
            },
        ) => clipboard_paste(tool, text, *preserve_clipboard),
        _ => Err(PeekabooXError::new(format!(
            "{} does not support action {:?}",
            tool.name(),
            action
        ))),
    }
}

fn run_type_text_tool(tool: InputTool, text: &str, options: TypeTextOptions) -> Result<()> {
    validate_type_text_options(options)?;
    match tool {
        InputTool::Ydotool => {
            run_command_with_stdin_vec("ydotool", type_text_command_args(tool, options), text)
        }
        InputTool::Wtype => {
            run_command_with_stdin_vec("wtype", type_text_command_args(tool, options), text)
        }
        InputTool::Xdotool => {
            if let Some(delay_ms) = options.delay_ms {
                sleep_ms(delay_ms);
            }
            run_command_with_stdin_vec("xdotool", type_text_command_args(tool, options), text)
        }
        _ => Err(PeekabooXError::new(format!(
            "{} does not support text typing",
            tool.name()
        ))),
    }
}

fn type_text_command_args(tool: InputTool, options: TypeTextOptions) -> Vec<String> {
    match tool {
        InputTool::Ydotool => vec![
            "type".to_owned(),
            "--delay".to_owned(),
            type_initial_delay_ms(tool, options).to_string(),
            "--key-delay".to_owned(),
            type_key_delay_ms(tool, options).to_string(),
            "--file".to_owned(),
            "-".to_owned(),
        ],
        InputTool::Wtype => {
            let mut args = Vec::new();
            let initial_delay = type_initial_delay_ms(tool, options);
            if initial_delay > 0 {
                args.push("-s".to_owned());
                args.push(initial_delay.to_string());
            }
            let key_delay = type_key_delay_ms(tool, options);
            if key_delay > 0 {
                args.push("-d".to_owned());
                args.push(key_delay.to_string());
            }
            args.push("-".to_owned());
            args
        }
        InputTool::Xdotool => vec![
            "type".to_owned(),
            "--delay".to_owned(),
            type_key_delay_ms(tool, options).to_string(),
            "--file".to_owned(),
            "-".to_owned(),
        ],
        _ => Vec::new(),
    }
}

fn type_initial_delay_ms(tool: InputTool, options: TypeTextOptions) -> u64 {
    options.delay_ms.unwrap_or(match tool {
        InputTool::Ydotool => 120,
        _ => 0,
    })
}

fn type_key_delay_ms(tool: InputTool, options: TypeTextOptions) -> u64 {
    if let Some(delay_ms) = options.key_delay_ms {
        return delay_ms;
    }
    if let Some(chars_per_second) = options.typing_speed_chars_per_second {
        return typing_speed_to_key_delay_ms(chars_per_second);
    }
    match tool {
        InputTool::Ydotool => 45,
        _ => 0,
    }
}

fn typing_speed_to_key_delay_ms(chars_per_second: u32) -> u64 {
    if chars_per_second == 0 {
        return 0;
    }
    1000_u64.div_ceil(u64::from(chars_per_second)).max(1)
}

fn validate_type_text_options(options: TypeTextOptions) -> Result<()> {
    if options.typing_speed_chars_per_second == Some(0) {
        return Err(PeekabooXError::new(
            "typing_speed_chars_per_second must be greater than zero",
        ));
    }
    if options.typing_speed_chars_per_second.is_some() && options.key_delay_ms.is_some() {
        return Err(PeekabooXError::new(
            "typing_speed_chars_per_second cannot be combined with key_delay_ms",
        ));
    }
    if options.backend == InputToolSelection::Uinput {
        return Err(PeekabooXError::new(
            "type backend must be auto, wtype, ydotool, or xdotool",
        ));
    }
    Ok(())
}

fn sleep_ms(milliseconds: u64) {
    if milliseconds > 0 {
        std::thread::sleep(std::time::Duration::from_millis(milliseconds));
    }
}

fn run_drag_tool(
    tool: InputTool,
    from: Point,
    to: Point,
    button: MouseButton,
    options: DragMouseOptions,
) -> Result<()> {
    match tool {
        InputTool::Uinput => uinput_drag(from, to, button, options),
        InputTool::Xdotool => xdotool_drag(from, to, button, options),
        _ => Err(PeekabooXError::new(format!(
            "{} does not support pointer drags",
            tool.name()
        ))),
    }
}

fn run_move_mouse_tool(tool: InputTool, position: Point) -> Result<()> {
    match tool {
        InputTool::Uinput => uinput_move_mouse(position),
        InputTool::Ydotool => ydotool_mousemove(position),
        InputTool::Xdotool => run_command(
            "xdotool",
            [
                "mousemove",
                &position.x.to_string(),
                &position.y.to_string(),
            ],
        ),
        _ => Err(PeekabooXError::new(format!(
            "{} does not support pointer movement",
            tool.name()
        ))),
    }
}

fn smooth_move_mouse(
    tool: InputTool,
    from: Point,
    to: Point,
    options: MoveMouseOptions,
) -> Result<()> {
    if tool == InputTool::Uinput {
        return smooth_uinput_move_mouse(from, to, options);
    }

    let steps = move_steps(options.duration_ms, from, to, options.steps)?;
    let sleep_per_step = if options.duration_ms == 0 {
        0
    } else {
        options.duration_ms / u64::from(steps)
    };

    for step in 1..=steps {
        let next = interpolate_point(from, to, step, steps);
        run_move_mouse_tool(tool, next)?;

        if sleep_per_step > 0 && step < steps {
            sleep(Duration::from_millis(sleep_per_step));
        }
    }

    Ok(())
}

fn smooth_uinput_move_mouse(from: Point, to: Point, options: MoveMouseOptions) -> Result<()> {
    let (screen_width, screen_height) = detect_screen_size().ok_or_else(|| {
        PeekabooXError::new(
            "uinput smooth move requires a detectable screen size from xrandr or xdpyinfo",
        )
    })?;
    let mut device = UinputPointer::create(screen_width, screen_height)?;
    let steps = move_steps(options.duration_ms, from, to, options.steps)?;
    let sleep_per_step = if options.duration_ms == 0 {
        0
    } else {
        options.duration_ms / u64::from(steps)
    };

    for step in 1..=steps {
        let next = interpolate_point(from, to, step, steps);
        device.move_to(next)?;

        if sleep_per_step > 0 && step < steps {
            sleep(Duration::from_millis(sleep_per_step));
        }
    }

    Ok(())
}

fn move_steps(
    duration_ms: u64,
    from: Point,
    to: Point,
    requested_steps: Option<u32>,
) -> Result<u32> {
    if let Some(0) = requested_steps {
        return Err(PeekabooXError::new("move steps must be greater than zero"));
    }

    Ok(requested_steps.unwrap_or_else(|| drag_steps(duration_ms, from, to)))
}

fn apply_move_bounds_policy(position: Point, policy: MoveBoundsPolicy) -> Result<Point> {
    match policy {
        MoveBoundsPolicy::Allow => Ok(position),
        MoveBoundsPolicy::Clamp => {
            let (width, height) = detect_screen_size().ok_or_else(|| {
                PeekabooXError::new(
                    "move --clamp requires a detectable screen size from xrandr or xdpyinfo",
                )
            })?;
            Ok(Point::new(
                clamp_to_range(position.x, width),
                clamp_to_range(position.y, height),
            ))
        }
        MoveBoundsPolicy::Fail => {
            let (width, height) = detect_screen_size().ok_or_else(|| {
                PeekabooXError::new(
                    "move --fail-out-of-bounds requires a detectable screen size from xrandr or xdpyinfo",
                )
            })?;
            if position.x < 0 || position.y < 0 || position.x >= width || position.y >= height {
                return Err(PeekabooXError::new(format!(
                    "move target {},{} is outside screen bounds 0,0,{}x{}",
                    position.x, position.y, width, height
                )));
            }
            Ok(position)
        }
    }
}

fn clipboard_paste(tool: InputTool, text: &str, preserve_clipboard: bool) -> Result<()> {
    let previous_clipboard = if preserve_clipboard {
        Some(read_clipboard_text(tool)?)
    } else {
        None
    };

    match tool {
        InputTool::WlClipboard | InputTool::XclipClipboard | InputTool::XselClipboard => {
            set_clipboard_text(tool, text)?
        }
        _ => {
            return Err(PeekabooXError::new(format!(
                "{} is not a clipboard paste backend",
                tool.name()
            )));
        }
    }

    sleep(Duration::from_millis(80));
    let paste_result = send_paste_hotkey();
    if let Some(previous_text) = previous_clipboard {
        sleep(Duration::from_millis(120));
        if let Err(restore_error) = set_clipboard_text(tool, &previous_text)
            && paste_result.is_ok()
        {
            return Err(restore_error);
        }
    }
    paste_result
}

fn set_clipboard_text(tool: InputTool, text: &str) -> Result<()> {
    match tool {
        InputTool::WlClipboard => run_clipboard_command_with_stdin("wl-copy", [], text),
        InputTool::XclipClipboard => {
            run_clipboard_command_with_stdin("xclip", ["-selection", "clipboard"], text)
        }
        InputTool::XselClipboard => {
            run_clipboard_command_with_stdin("xsel", ["--clipboard", "--input"], text)
        }
        _ => Err(PeekabooXError::new(format!(
            "{} is not a clipboard backend",
            tool.name()
        ))),
    }
}

fn read_clipboard_text(tool: InputTool) -> Result<String> {
    match tool {
        InputTool::WlClipboard => run_command_capture_stdout("wl-paste", ["--no-newline"]),
        InputTool::XclipClipboard => {
            run_command_capture_stdout("xclip", ["-selection", "clipboard", "-out"])
        }
        InputTool::XselClipboard => run_command_capture_stdout("xsel", ["--clipboard", "--output"]),
        _ => Err(PeekabooXError::new(format!(
            "{} is not a clipboard backend",
            tool.name()
        ))),
    }
}

fn run_clipboard_command_with_stdin<const N: usize>(
    program: &str,
    args: [&str; N],
    stdin: &str,
) -> Result<()> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let mut child_stdin = child
        .stdin
        .take()
        .ok_or_else(|| PeekabooXError::new(format!("failed to open stdin for {program}")))?;
    child_stdin.write_all(stdin.as_bytes())?;
    drop(child_stdin);

    // Clipboard owners often keep running or fork after stdin closes; readiness
    // is enough here because the next step sends ctrl+v to the focused window.
    for _ in 0..20 {
        if let Some(status) = child.try_wait()? {
            if status.success() {
                return Ok(());
            }

            return Err(PeekabooXError::new(format!(
                "{program} failed with status {status}"
            )));
        }

        sleep(Duration::from_millis(25));
    }

    Ok(())
}

fn run_command_capture_stdout<const N: usize>(program: &str, args: [&str; N]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }

    Err(PeekabooXError::new(format!(
        "{program} failed with status {}; stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    )))
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

fn xdotool_drag(
    from: Point,
    to: Point,
    button: MouseButton,
    options: DragMouseOptions,
) -> Result<()> {
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

    let steps = move_steps(options.duration_ms, from, to, options.steps)?;
    let sleep_per_step = if steps == 0 {
        0
    } else {
        options.duration_ms / u64::from(steps)
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

fn uinput_drag(
    from: Point,
    to: Point,
    button: MouseButton,
    options: DragMouseOptions,
) -> Result<()> {
    let (screen_width, screen_height) = detect_screen_size().ok_or_else(|| {
        PeekabooXError::new("uinput drag requires a detectable screen size from xrandr or xdpyinfo")
    })?;
    let mut device = UinputPointer::create(screen_width, screen_height)?;

    device.move_to(from)?;
    device.set_button(button, true)?;

    let steps = move_steps(options.duration_ms, from, to, options.steps)?;
    let sleep_per_step = if steps == 0 {
        0
    } else {
        options.duration_ms / u64::from(steps)
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

fn parse_xdotool_mouse_location(output: &str) -> Option<Point> {
    let mut x = None;
    let mut y = None;

    for line in output.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "X" => x = value.trim().parse::<i32>().ok(),
            "Y" => y = value.trim().parse::<i32>().ok(),
            _ => {}
        }
    }

    Some(Point::new(x?, y?))
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

fn send_paste_hotkey() -> Result<()> {
    let environment = InputEnvironment::detect();
    let keys = vec!["ctrl".to_owned(), "v".to_owned()];
    let mut errors = Vec::new();

    for tool in paste_hotkey_candidates(&environment) {
        let result = match tool {
            InputTool::Ydotool => ydotool_hotkey(&keys),
            InputTool::Xdotool => xdotool_hotkey(&keys),
            _ => continue,
        };

        match result {
            Ok(()) => return Ok(()),
            Err(error) => errors.push(format!("{}: {}", tool.name(), error.message())),
        }
    }

    if errors.is_empty() {
        return Err(PeekabooXError::new(
            "paste requires ydotool with /dev/uinput access or xdotool to press ctrl+v",
        ));
    }

    Err(PeekabooXError::new(format!(
        "all paste hotkey backends failed: {}",
        errors.join("; ")
    )))
}

fn paste_hotkey_candidates(environment: &InputEnvironment) -> Vec<InputTool> {
    let preferred = match environment.session_type {
        SessionType::Wayland => [InputTool::Ydotool, InputTool::Xdotool],
        SessionType::X11 => [InputTool::Xdotool, InputTool::Ydotool],
        SessionType::Unknown => [InputTool::Ydotool, InputTool::Xdotool],
    };

    let action = InputAction::Hotkey(vec!["ctrl".to_owned(), "v".to_owned()]);
    preferred
        .into_iter()
        .filter(|tool| tool.is_available(environment) && tool.supports(&action))
        .collect()
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
        "no supported input backend found for {:?} in {:?}; install ydotool with /dev/uinput access for Wayland/global control, wtype for Wayland text input, xdotool for X11, or wl-copy/xclip/xsel for clipboard paste",
        action, environment.session_type
    ))
}

fn missing_backend_error_for_selection(
    environment: &InputEnvironment,
    action: &InputAction,
    selection: InputToolSelection,
) -> PeekabooXError {
    if selection == InputToolSelection::Auto {
        return missing_backend_error(environment, action);
    }

    PeekabooXError::new(format!(
        "selected input backend {} is unavailable or does not support {:?} in {:?}",
        selection.name(),
        action,
        environment.session_type
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

fn run_command_with_stdin_vec(program: &str, args: Vec<String>, stdin: &str) -> Result<()> {
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
        InputAction, InputBackend, InputEnvironment, InputTool, InputToolSelection, MouseButton,
        SessionType, TypeTextOptions, UnimplementedInputBackend, candidate_backends,
        candidate_backends_with_selection,
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
    fn selects_wtype_for_wayland_typing_before_ydotool() {
        let environment = environment(SessionType::Wayland, ["ydotool", "wtype"], true);
        let action = InputAction::TypeText("/tmp/peekaboox-output.txt".to_owned());

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
    fn selects_wl_clipboard_for_wayland_paste_when_hotkey_is_available() {
        let environment = environment(SessionType::Wayland, ["wl-copy", "ydotool"], true);
        let action = InputAction::PasteText {
            text: "/tmp/peekaboox-output.txt".to_owned(),
            preserve_clipboard: false,
        };

        let backend = candidate_backends(&environment, &action).remove(0);

        assert_eq!(backend.tool, InputTool::WlClipboard);
    }

    #[test]
    fn selects_xclip_for_x11_paste_when_hotkey_is_available() {
        let environment = environment(SessionType::X11, ["xclip", "xdotool"], false);
        let action = InputAction::PasteText {
            text: "/tmp/peekaboox-output.txt".to_owned(),
            preserve_clipboard: false,
        };

        let backend = candidate_backends(&environment, &action).remove(0);

        assert_eq!(backend.tool, InputTool::XclipClipboard);
    }

    #[test]
    fn paste_has_no_backend_without_clipboard_tool() {
        let environment = environment(SessionType::Wayland, ["ydotool"], true);
        let action = InputAction::PasteText {
            text: "/tmp/peekaboox-output.txt".to_owned(),
            preserve_clipboard: false,
        };

        assert!(candidate_backends(&environment, &action).is_empty());
    }

    #[test]
    fn paste_has_no_backend_without_hotkey_tool() {
        let environment = environment(SessionType::Wayland, ["wl-copy"], false);
        let action = InputAction::PasteText {
            text: "/tmp/peekaboox-output.txt".to_owned(),
            preserve_clipboard: false,
        };

        assert!(candidate_backends(&environment, &action).is_empty());
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
    fn backend_selection_filters_candidate_backends() {
        let environment = environment(SessionType::X11, ["xdotool", "ydotool"], true);
        let action = InputAction::MoveMouse(Point::new(10, 20));

        let backend =
            candidate_backends_with_selection(&environment, &action, InputToolSelection::Ydotool)
                .remove(0);

        assert_eq!(backend.tool, InputTool::Ydotool);
    }

    #[test]
    fn type_backend_selection_accepts_wtype() {
        let environment = environment(SessionType::Wayland, ["wtype", "ydotool"], true);
        let action = InputAction::TypeText("hello".to_owned());

        let backend =
            candidate_backends_with_selection(&environment, &action, InputToolSelection::Wtype)
                .remove(0);

        assert_eq!(backend.tool, InputTool::Wtype);
    }

    #[test]
    fn type_text_options_map_speed_to_backend_delays() {
        let options = TypeTextOptions {
            typing_speed_chars_per_second: Some(20),
            delay_ms: Some(10),
            key_delay_ms: None,
            backend: InputToolSelection::Wtype,
        };

        assert_eq!(
            super::type_text_command_args(InputTool::Wtype, options),
            ["-s", "10", "-d", "50", "-"].map(str::to_owned).to_vec()
        );
        assert_eq!(
            super::type_text_command_args(InputTool::Xdotool, options),
            ["type", "--delay", "50", "--file", "-"]
                .map(str::to_owned)
                .to_vec()
        );
        assert_eq!(
            super::type_text_command_args(InputTool::Ydotool, options),
            vec![
                "type".to_owned(),
                "--delay".to_owned(),
                "10".to_owned(),
                "--key-delay".to_owned(),
                "50".to_owned(),
                "--file".to_owned(),
                "-".to_owned(),
            ]
        );
    }

    #[test]
    fn type_text_options_reject_ambiguous_speed_and_key_delay() {
        let options = TypeTextOptions {
            typing_speed_chars_per_second: Some(20),
            delay_ms: None,
            key_delay_ms: Some(50),
            backend: InputToolSelection::Auto,
        };

        let error = super::validate_type_text_options(options).unwrap_err();

        assert!(
            error
                .message()
                .contains("cannot be combined with key_delay_ms")
        );
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
    fn parses_xdotool_mouse_location_output() {
        let point =
            super::parse_xdotool_mouse_location("X=11\nY=22\nSCREEN=0\nWINDOW=1\n").unwrap();

        assert_eq!(point, Point::new(11, 22));
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
    fn move_steps_reject_zero_requested_steps() {
        let error =
            super::move_steps(100, Point::new(0, 0), Point::new(10, 10), Some(0)).unwrap_err();

        assert!(error.message().contains("greater than zero"));
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
