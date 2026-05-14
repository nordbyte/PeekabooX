use std::collections::HashSet;
use std::process::Command;
use std::time::Duration;

use dbus::Path;
use dbus::arg::{PropMap, RefArg, Variant};
use dbus::blocking::Connection;
use peekaboox_core::{BackendKind, PeekabooXError, Rect, Result, WindowInfo, WindowState};

pub trait WindowBackend {
    fn list_windows(&self) -> Result<Vec<WindowInfo>>;
    fn active_window(&self) -> Result<Option<WindowInfo>>;
    fn focus_window(&self, window_id: &str) -> Result<()>;
}

#[derive(Debug, Default)]
pub struct UnimplementedWindowBackend;

impl WindowBackend for UnimplementedWindowBackend {
    fn list_windows(&self) -> Result<Vec<WindowInfo>> {
        Err(PeekabooXError::new(
            "window enumeration backend is unavailable in this environment",
        ))
    }

    fn active_window(&self) -> Result<Option<WindowInfo>> {
        Err(PeekabooXError::new(
            "active window backend is unavailable in this environment",
        ))
    }

    fn focus_window(&self, _window_id: &str) -> Result<()> {
        Err(PeekabooXError::new(
            "window focus backend is unavailable in this environment",
        ))
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
pub struct WindowEnvironment {
    pub session_type: SessionType,
    pub current_desktop: Option<String>,
    pub commands: HashSet<String>,
}

impl WindowEnvironment {
    pub fn detect() -> Self {
        let command_names = ["xdotool"];

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
        }
    }

    fn has_command(&self, command: &str) -> bool {
        self.commands.contains(command)
    }

    fn is_gnome(&self) -> bool {
        self.current_desktop
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .contains("gnome")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowTool {
    GnomeShellIntrospect,
    AtSpi,
    Xdotool,
}

impl WindowTool {
    pub fn name(self) -> &'static str {
        match self {
            Self::GnomeShellIntrospect => "gnome-shell-introspect",
            Self::AtSpi => "at-spi",
            Self::Xdotool => "xdotool",
        }
    }

    pub fn backend_kind(self) -> BackendKind {
        match self {
            Self::GnomeShellIntrospect => BackendKind::Wayland,
            Self::AtSpi => BackendKind::AtSpi,
            Self::Xdotool => BackendKind::X11,
        }
    }

    fn is_available(self, environment: &WindowEnvironment) -> bool {
        match self {
            Self::GnomeShellIntrospect => environment.is_gnome(),
            Self::AtSpi => true,
            Self::Xdotool => environment.has_command("xdotool"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedWindowBackend {
    pub tool: WindowTool,
    pub session_type: SessionType,
}

impl DetectedWindowBackend {
    pub fn name(&self) -> &'static str {
        self.tool.name()
    }

    pub fn backend_kind(&self) -> BackendKind {
        self.tool.backend_kind()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowListMetadata {
    pub backend_name: String,
    pub backend_kind: BackendKind,
    pub windows: Vec<WindowInfo>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Default)]
pub struct CommandWindowBackend;

impl CommandWindowBackend {
    pub fn detect_backend(&self) -> Result<DetectedWindowBackend> {
        let environment = WindowEnvironment::detect();
        candidate_backends(&environment)
            .into_iter()
            .next()
            .ok_or_else(|| missing_backend_error(&environment))
    }

    pub fn list_windows_with_metadata(&self) -> Result<WindowListMetadata> {
        let environment = WindowEnvironment::detect();
        let candidates = candidate_backends(&environment);

        if candidates.is_empty() {
            return Err(missing_backend_error(&environment));
        }

        let mut warnings = Vec::new();

        for backend in candidates {
            match list_windows_with_tool(backend.tool) {
                Ok(windows) => {
                    return Ok(WindowListMetadata {
                        backend_name: backend.name().to_owned(),
                        backend_kind: backend.backend_kind(),
                        windows,
                        warnings,
                    });
                }
                Err(error) => warnings.push(format!("{}: {}", backend.name(), error.message())),
            }
        }

        Err(PeekabooXError::new(format!(
            "all window enumeration backends failed: {}",
            warnings.join("; ")
        )))
    }
}

impl WindowBackend for CommandWindowBackend {
    fn list_windows(&self) -> Result<Vec<WindowInfo>> {
        self.list_windows_with_metadata()
            .map(|metadata| metadata.windows)
    }

    fn active_window(&self) -> Result<Option<WindowInfo>> {
        Ok(self
            .list_windows_with_metadata()?
            .windows
            .into_iter()
            .find(|window| window.focused))
    }

    fn focus_window(&self, window_id: &str) -> Result<()> {
        run_command("xdotool", ["windowactivate", window_id])
    }
}

pub fn list_windows() -> Result<WindowListMetadata> {
    CommandWindowBackend.list_windows_with_metadata()
}

pub fn candidate_backends(environment: &WindowEnvironment) -> Vec<DetectedWindowBackend> {
    let mut candidates = Vec::new();

    match environment.session_type {
        SessionType::Wayland => {
            candidates.push(WindowTool::GnomeShellIntrospect);
            candidates.push(WindowTool::AtSpi);
            candidates.push(WindowTool::Xdotool);
        }
        SessionType::X11 => {
            candidates.push(WindowTool::Xdotool);
            candidates.push(WindowTool::AtSpi);
            candidates.push(WindowTool::GnomeShellIntrospect);
        }
        SessionType::Unknown => {
            candidates.push(WindowTool::GnomeShellIntrospect);
            candidates.push(WindowTool::AtSpi);
            candidates.push(WindowTool::Xdotool);
        }
    }

    candidates
        .into_iter()
        .filter_map(|tool| {
            if tool.is_available(environment) {
                Some(DetectedWindowBackend {
                    tool,
                    session_type: environment.session_type,
                })
            } else {
                None
            }
        })
        .collect()
}

fn list_windows_with_tool(tool: WindowTool) -> Result<Vec<WindowInfo>> {
    match tool {
        WindowTool::GnomeShellIntrospect => list_gnome_shell_windows(),
        WindowTool::AtSpi => list_atspi_windows(),
        WindowTool::Xdotool => list_xdotool_windows(),
    }
}

fn list_gnome_shell_windows() -> Result<Vec<WindowInfo>> {
    let connection = Connection::new_session().map_err(|error| {
        PeekabooXError::new(format!("failed to connect to session bus: {error}"))
    })?;
    let proxy = connection.with_proxy(
        "org.gnome.Shell.Introspect",
        "/org/gnome/Shell/Introspect",
        Duration::from_secs(3),
    );

    let (windows,): (Vec<(u64, PropMap)>,) = proxy
        .method_call("org.gnome.Shell.Introspect", "GetWindows", ())
        .map_err(|error| {
            PeekabooXError::new(format!("GNOME Shell Introspect GetWindows failed: {error}"))
        })?;

    Ok(windows
        .into_iter()
        .filter_map(|(id, properties)| window_from_gnome_properties(id, &properties))
        .collect())
}

type AtSpiRef = (String, Path<'static>);

fn list_atspi_windows() -> Result<Vec<WindowInfo>> {
    let session = Connection::new_session().map_err(|error| {
        PeekabooXError::new(format!("failed to connect to session bus: {error}"))
    })?;
    let bus_proxy = session.with_proxy("org.a11y.Bus", "/org/a11y/bus", Duration::from_secs(3));
    let (address,): (String,) = bus_proxy
        .method_call("org.a11y.Bus", "GetAddress", ())
        .map_err(|error| PeekabooXError::new(format!("AT-SPI bus lookup failed: {error}")))?;

    let atspi = Connection::new_address(&address).map_err(|error| {
        PeekabooXError::new(format!("failed to connect to AT-SPI bus: {error}"))
    })?;
    let root_proxy = atspi.with_proxy(
        "org.a11y.atspi.Registry",
        "/org/a11y/atspi/accessible/root",
        Duration::from_secs(3),
    );
    let (applications,): (Vec<AtSpiRef>,) = root_proxy
        .method_call("org.a11y.atspi.Accessible", "GetChildren", ())
        .map_err(|error| PeekabooXError::new(format!("AT-SPI root enumeration failed: {error}")))?;

    let mut windows = Vec::new();

    for application in applications {
        let app_name = atspi_accessible_name(&atspi, &application).ok();
        let children = atspi_children(&atspi, &application).unwrap_or_default();

        for child in children {
            if let Some(window) = atspi_window_info(&atspi, &child, app_name.as_deref()) {
                windows.push(window);
            }
        }
    }

    windows.sort_by(|left, right| left.title.cmp(&right.title));
    windows.dedup_by(|left, right| left.id == right.id);

    Ok(windows)
}

fn atspi_window_info(
    connection: &Connection,
    object_ref: &AtSpiRef,
    app_name: Option<&str>,
) -> Option<WindowInfo> {
    let title = atspi_accessible_name(connection, object_ref).ok()?;
    let title = title.trim().to_owned();
    if title.is_empty() {
        return None;
    }

    let role = atspi_role_name(connection, object_ref).unwrap_or_default();
    if !is_window_like_atspi_role(&role) {
        return None;
    }

    let (x, y, width, height) = atspi_extents(connection, object_ref).ok()?;
    if width <= 0 || height <= 0 {
        return None;
    }
    let states = atspi_state_set(connection, object_ref).unwrap_or_default();

    Some(WindowInfo {
        id: format!("{}{}", object_ref.0, object_ref.1),
        title,
        app_id: app_name.map(str::to_owned),
        bounds: Rect::new(
            x,
            y,
            u32::try_from(width).ok()?,
            u32::try_from(height).ok()?,
        ),
        focused: atspi_state_contains(&states, 1) || atspi_state_contains(&states, 12),
        state: WindowState::Unknown,
    })
}

fn atspi_children(connection: &Connection, object_ref: &AtSpiRef) -> Result<Vec<AtSpiRef>> {
    let proxy = connection.with_proxy(
        object_ref.0.as_str(),
        object_ref.1.clone(),
        Duration::from_secs(2),
    );
    let (children,): (Vec<AtSpiRef>,) = proxy
        .method_call("org.a11y.atspi.Accessible", "GetChildren", ())
        .map_err(|error| PeekabooXError::new(format!("AT-SPI GetChildren failed: {error}")))?;

    Ok(children)
}

fn atspi_accessible_name(connection: &Connection, object_ref: &AtSpiRef) -> Result<String> {
    let proxy = connection.with_proxy(
        object_ref.0.as_str(),
        object_ref.1.clone(),
        Duration::from_secs(2),
    );
    let (value,): (Variant<Box<dyn RefArg>>,) = proxy
        .method_call(
            "org.freedesktop.DBus.Properties",
            "Get",
            ("org.a11y.atspi.Accessible", "Name"),
        )
        .map_err(|error| PeekabooXError::new(format!("AT-SPI Name lookup failed: {error}")))?;

    value
        .0
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| PeekabooXError::new("AT-SPI Name property was not a string"))
}

fn atspi_role_name(connection: &Connection, object_ref: &AtSpiRef) -> Result<String> {
    let proxy = connection.with_proxy(
        object_ref.0.as_str(),
        object_ref.1.clone(),
        Duration::from_secs(2),
    );
    let (role,): (String,) = proxy
        .method_call("org.a11y.atspi.Accessible", "GetRoleName", ())
        .map_err(|error| PeekabooXError::new(format!("AT-SPI role lookup failed: {error}")))?;

    Ok(role)
}

fn atspi_extents(connection: &Connection, object_ref: &AtSpiRef) -> Result<(i32, i32, i32, i32)> {
    let proxy = connection.with_proxy(
        object_ref.0.as_str(),
        object_ref.1.clone(),
        Duration::from_secs(2),
    );
    let ((x, y, width, height),): ((i32, i32, i32, i32),) = proxy
        .method_call("org.a11y.atspi.Component", "GetExtents", (0_u32,))
        .map_err(|error| PeekabooXError::new(format!("AT-SPI extents lookup failed: {error}")))?;

    Ok((x, y, width, height))
}

fn atspi_state_set(connection: &Connection, object_ref: &AtSpiRef) -> Result<Vec<u32>> {
    let proxy = connection.with_proxy(
        object_ref.0.as_str(),
        object_ref.1.clone(),
        Duration::from_secs(2),
    );
    let (states,): (Vec<u32>,) = proxy
        .method_call("org.a11y.atspi.Accessible", "GetState", ())
        .map_err(|error| PeekabooXError::new(format!("AT-SPI state lookup failed: {error}")))?;

    Ok(states)
}

fn atspi_state_contains(states: &[u32], bit: usize) -> bool {
    let Some(value) = states.get(bit / 32) else {
        return false;
    };

    value & (1_u32 << (bit % 32)) != 0
}

fn is_window_like_atspi_role(role: &str) -> bool {
    matches!(
        role,
        "frame" | "window" | "dialog" | "application" | "alert"
    )
}

fn window_from_gnome_properties(id: u64, properties: &PropMap) -> Option<WindowInfo> {
    let title = variant_string(properties, "title")?;
    let title = title.trim().to_owned();
    if title.is_empty() {
        return None;
    }

    let x = variant_i32(properties, "x").unwrap_or_default();
    let y = variant_i32(properties, "y").unwrap_or_default();
    let width = variant_u32(properties, "width").unwrap_or_default();
    let height = variant_u32(properties, "height").unwrap_or_default();
    let focused = variant_bool(properties, "has-focus").unwrap_or(false);
    let hidden = variant_bool(properties, "is-hidden").unwrap_or(false);

    Some(WindowInfo {
        id: id.to_string(),
        title,
        app_id: variant_string(properties, "app-id")
            .or_else(|| variant_string(properties, "wm-class")),
        bounds: Rect::new(x, y, width, height),
        focused,
        state: if hidden {
            WindowState::Minimized
        } else {
            WindowState::Normal
        },
    })
}

fn list_xdotool_windows() -> Result<Vec<WindowInfo>> {
    let ids_output =
        run_command_capture_allowing_empty("xdotool", ["search", "--onlyvisible", "--name", "."])?;
    let active_window_id = run_command_capture("xdotool", ["getactivewindow"])
        .ok()
        .map(|id| id.trim().to_owned());

    let mut windows = Vec::new();

    for id in ids_output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Some(window) = xdotool_window_info(id, active_window_id.as_deref())? else {
            continue;
        };
        windows.push(window);
    }

    Ok(windows)
}

fn xdotool_window_info(id: &str, active_window_id: Option<&str>) -> Result<Option<WindowInfo>> {
    let title = run_command_capture("xdotool", ["getwindowname", id])
        .unwrap_or_default()
        .trim()
        .to_owned();

    if should_ignore_xdotool_window(&title) {
        return Ok(None);
    }

    let geometry_output = run_command_capture("xdotool", ["getwindowgeometry", id])?;
    let bounds = parse_xdotool_geometry(&geometry_output).unwrap_or(Rect::new(0, 0, 0, 0));

    Ok(Some(WindowInfo {
        id: id.to_owned(),
        title,
        app_id: None,
        bounds,
        focused: active_window_id.is_some_and(|active_id| active_id == id),
        state: WindowState::Normal,
    }))
}

pub fn parse_xdotool_geometry(output: &str) -> Option<Rect> {
    let mut x = None;
    let mut y = None;
    let mut width = None;
    let mut height = None;

    for line in output.lines().map(str::trim) {
        if let Some(position) = line.strip_prefix("Position: ") {
            let coordinates = position.split_once(" (screen:")?.0;
            let (parsed_x, parsed_y) = coordinates.split_once(',')?;
            x = parsed_x.parse::<i32>().ok();
            y = parsed_y.parse::<i32>().ok();
        }

        if let Some(geometry) = line.strip_prefix("Geometry: ") {
            let (parsed_width, parsed_height) = geometry.split_once('x')?;
            width = parsed_width.parse::<u32>().ok();
            height = parsed_height.parse::<u32>().ok();
        }
    }

    Some(Rect::new(x?, y?, width?, height?))
}

fn should_ignore_xdotool_window(title: &str) -> bool {
    let normalized = title.trim().to_ascii_lowercase();
    normalized.is_empty() || normalized == "mutter guard window"
}

fn variant_string(properties: &PropMap, key: &str) -> Option<String> {
    properties.get(key)?.0.as_str().map(str::to_owned)
}

fn variant_bool(properties: &PropMap, key: &str) -> Option<bool> {
    let value = properties.get(key)?;
    value.0.as_i64().map(|number| number != 0).or_else(|| {
        value
            .0
            .as_u64()
            .map(|number| number != 0)
            .or_else(|| value.0.as_str().and_then(parse_bool))
    })
}

fn variant_i32(properties: &PropMap, key: &str) -> Option<i32> {
    variant_i64(properties, key).and_then(|value| i32::try_from(value).ok())
}

fn variant_u32(properties: &PropMap, key: &str) -> Option<u32> {
    properties
        .get(key)?
        .0
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .or_else(|| variant_i64(properties, key).and_then(|value| u32::try_from(value).ok()))
}

fn variant_i64(properties: &PropMap, key: &str) -> Option<i64> {
    properties.get(key)?.0.as_i64()
}

fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn missing_backend_error(environment: &WindowEnvironment) -> PeekabooXError {
    PeekabooXError::new(format!(
        "no supported window enumeration backend found for {:?}; GNOME Wayland requires org.gnome.Shell.Introspect permission, X11/XWayland requires xdotool",
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

fn run_command_capture<const N: usize>(program: &str, args: [&str; N]) -> Result<String> {
    let output = Command::new(program).args(args).output()?;

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).to_string());
    }

    Err(PeekabooXError::new(format!(
        "{program} failed with status {}; stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

fn run_command_capture_allowing_empty<const N: usize>(
    program: &str,
    args: [&str; N],
) -> Result<String> {
    let output = Command::new(program).args(args).output()?;

    if output.status.success() || output.stdout.is_empty() {
        return Ok(String::from_utf8_lossy(&output.stdout).to_string());
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
        WindowEnvironment, WindowTool, atspi_state_contains, candidate_backends,
        parse_xdotool_geometry, should_ignore_xdotool_window,
    };
    use crate::SessionType;
    use peekaboox_core::Rect;

    #[test]
    fn selects_gnome_introspect_first_on_gnome_wayland() {
        let environment = environment(SessionType::Wayland, Some("ubuntu:GNOME"), []);

        let backend = candidate_backends(&environment).remove(0);

        assert_eq!(backend.tool, WindowTool::GnomeShellIntrospect);
    }

    #[test]
    fn selects_xdotool_on_x11() {
        let environment = environment(SessionType::X11, None, ["xdotool"]);

        let backend = candidate_backends(&environment).remove(0);

        assert_eq!(backend.tool, WindowTool::Xdotool);
    }

    #[test]
    fn parses_xdotool_geometry() {
        let geometry = "Window 6291466\n  Position: 12,34 (screen: 0)\n  Geometry: 800x600\n";

        assert_eq!(
            parse_xdotool_geometry(geometry),
            Some(Rect::new(12, 34, 800, 600))
        );
    }

    #[test]
    fn filters_mutter_guard_window() {
        assert!(should_ignore_xdotool_window("mutter guard window"));
        assert!(should_ignore_xdotool_window(" "));
        assert!(!should_ignore_xdotool_window("Terminal"));
    }

    #[test]
    fn reads_atspi_state_bitsets() {
        assert!(atspi_state_contains(&[0b10], 1));
        assert!(atspi_state_contains(&[0, 0b1], 32));
        assert!(!atspi_state_contains(&[0b10], 12));
    }

    fn environment<const N: usize>(
        session_type: SessionType,
        current_desktop: Option<&str>,
        commands: [&str; N],
    ) -> WindowEnvironment {
        WindowEnvironment {
            session_type,
            current_desktop: current_desktop.map(str::to_owned),
            commands: commands
                .into_iter()
                .map(str::to_owned)
                .collect::<HashSet<_>>(),
        }
    }
}
