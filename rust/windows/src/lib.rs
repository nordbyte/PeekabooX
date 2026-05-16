use std::collections::HashSet;
use std::process::Command;
use std::time::Duration;

use dbus::Path;
use dbus::arg::{PropMap, RefArg, Variant};
use dbus::blocking::Connection;
use peekaboox_core::{BackendKind, PeekabooXError, Rect, Result, WindowInfo, WindowState};
use regex::{Regex, RegexBuilder};

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
        let command_names = ["xdotool", "xprop"];

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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum WindowBackendSelection {
    #[default]
    Auto,
    GnomeShellIntrospect,
    AtSpi,
    Xdotool,
}

impl WindowBackendSelection {
    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::GnomeShellIntrospect => "gnome",
            Self::AtSpi => "at-spi",
            Self::Xdotool => "xdotool",
        }
    }

    pub fn from_name(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "gnome" | "gnome-shell" | "gnome-shell-introspect" => Some(Self::GnomeShellIntrospect),
            "at-spi" | "atspi" => Some(Self::AtSpi),
            "xdotool" | "x11" => Some(Self::Xdotool),
            _ => None,
        }
    }

    fn tool(self) -> Option<WindowTool> {
        match self {
            Self::Auto => None,
            Self::GnomeShellIntrospect => Some(WindowTool::GnomeShellIntrospect),
            Self::AtSpi => Some(WindowTool::AtSpi),
            Self::Xdotool => Some(WindowTool::Xdotool),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum WindowSort {
    #[default]
    Backend,
    Focused,
    Title,
    App,
    Area,
    Id,
    State,
}

impl WindowSort {
    pub fn name(self) -> &'static str {
        match self {
            Self::Backend => "backend",
            Self::Focused => "focused",
            Self::Title => "title",
            Self::App => "app",
            Self::Area => "area",
            Self::Id => "id",
            Self::State => "state",
        }
    }

    pub fn from_name(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "backend" | "backend-order" => Some(Self::Backend),
            "focused" | "focus" => Some(Self::Focused),
            "title" => Some(Self::Title),
            "app" | "app-id" => Some(Self::App),
            "area" | "size" => Some(Self::Area),
            "id" => Some(Self::Id),
            "state" => Some(Self::State),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowQuery {
    pub id: Option<String>,
    pub app: Option<String>,
    pub title: Option<String>,
    pub title_regex: Option<String>,
    pub focused_only: bool,
    pub limit: Option<usize>,
    pub sort: WindowSort,
    pub backend: WindowBackendSelection,
    pub diagnose: bool,
}

impl Default for WindowQuery {
    fn default() -> Self {
        Self {
            id: None,
            app: None,
            title: None,
            title_regex: None,
            focused_only: false,
            limit: None,
            sort: WindowSort::Backend,
            backend: WindowBackendSelection::Auto,
            diagnose: false,
        }
    }
}

impl WindowQuery {
    fn has_filters(&self) -> bool {
        self.id.is_some()
            || self.app.is_some()
            || self.title.is_some()
            || self.title_regex.is_some()
            || self.focused_only
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
    pub backend_reports: Vec<WindowBackendReport>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowBackendReport {
    pub backend_name: String,
    pub backend_kind: BackendKind,
    pub raw_window_count: usize,
    pub matched_window_count: usize,
    pub selected: bool,
    pub error: Option<String>,
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
        self.list_windows_with_query(WindowQuery::default())
    }

    pub fn list_windows_with_query(&self, query: WindowQuery) -> Result<WindowListMetadata> {
        let environment = WindowEnvironment::detect();
        let candidates = candidate_backends_for_query(&environment, query.backend);

        if candidates.is_empty() {
            return Err(missing_backend_error(&environment));
        }

        let mut warnings = Vec::new();
        let mut reports = Vec::new();
        let mut empty_success: Option<WindowListMetadata> = None;
        let last_candidate_index = candidates.len().saturating_sub(1);

        for (index, backend) in candidates.into_iter().enumerate() {
            match list_windows_with_tool(backend.tool) {
                Ok(windows) => {
                    let raw_window_count = windows.len();
                    let matched_windows = apply_window_query(windows, &query)?;
                    let matched_window_count = matched_windows.len();
                    let should_select = matched_window_count > 0
                        || query.backend != WindowBackendSelection::Auto
                        || index == last_candidate_index
                        || (!query.has_filters() && raw_window_count > 0);

                    reports.push(WindowBackendReport {
                        backend_name: backend.name().to_owned(),
                        backend_kind: backend.backend_kind(),
                        raw_window_count,
                        matched_window_count,
                        selected: should_select,
                        error: None,
                    });

                    if should_select {
                        return Ok(WindowListMetadata {
                            backend_name: backend.name().to_owned(),
                            backend_kind: backend.backend_kind(),
                            windows: matched_windows,
                            warnings,
                            backend_reports: reports,
                        });
                    }

                    if raw_window_count == 0 {
                        warnings.push(format!(
                            "{} returned no windows; trying next backend",
                            backend.name()
                        ));
                    } else if query.has_filters() && matched_window_count == 0 {
                        warnings.push(format!(
                            "{} returned {raw_window_count} windows but none matched the query; trying next backend",
                            backend.name()
                        ));
                    }

                    if empty_success.is_none() {
                        empty_success = Some(WindowListMetadata {
                            backend_name: backend.name().to_owned(),
                            backend_kind: backend.backend_kind(),
                            windows: Vec::new(),
                            warnings: warnings.clone(),
                            backend_reports: reports.clone(),
                        });
                    }
                }
                Err(error) => {
                    let message = error.message().to_owned();
                    warnings.push(format!("{}: {message}", backend.name()));
                    reports.push(WindowBackendReport {
                        backend_name: backend.name().to_owned(),
                        backend_kind: backend.backend_kind(),
                        raw_window_count: 0,
                        matched_window_count: 0,
                        selected: false,
                        error: Some(message),
                    });
                }
            }
        }

        if let Some(mut metadata) = empty_success {
            metadata.warnings = warnings;
            metadata.backend_reports = reports;
            if let Some(report) = metadata
                .backend_reports
                .iter_mut()
                .find(|report| report.error.is_none())
            {
                report.selected = true;
            }
            return Ok(metadata);
        }

        Err(PeekabooXError::new(format!(
            "all window enumeration backends failed: {}",
            warnings.join("; ")
        )))
    }
}

fn apply_window_query(
    mut windows: Vec<WindowInfo>,
    query: &WindowQuery,
) -> Result<Vec<WindowInfo>> {
    let title_regex = query
        .title_regex
        .as_deref()
        .map(compile_case_insensitive_regex)
        .transpose()?;

    windows.retain(|window| window_matches_query(window, query, title_regex.as_ref()));
    sort_windows(&mut windows, query.sort);

    if let Some(limit) = query.limit {
        windows.truncate(limit);
    }

    Ok(windows)
}

fn window_matches_query(
    window: &WindowInfo,
    query: &WindowQuery,
    title_regex: Option<&Regex>,
) -> bool {
    if query.id.as_deref().is_some_and(|id| window.id != id) {
        return false;
    }

    if query.focused_only && !window.focused {
        return false;
    }

    if query
        .app
        .as_deref()
        .is_some_and(|app| !window_matches_app(window, app))
    {
        return false;
    }

    if query
        .title
        .as_deref()
        .is_some_and(|title| !text_contains_case_insensitive(&window.title, title))
    {
        return false;
    }

    if title_regex.is_some_and(|regex| !regex.is_match(&window.title)) {
        return false;
    }

    true
}

fn window_matches_app(window: &WindowInfo, app: &str) -> bool {
    window
        .app_id
        .as_deref()
        .is_some_and(|app_id| text_contains_case_insensitive(app_id, app))
        || text_contains_case_insensitive(&window.title, app)
}

fn text_contains_case_insensitive(value: &str, needle: &str) -> bool {
    value
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

fn compile_case_insensitive_regex(pattern: &str) -> Result<Regex> {
    RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .map_err(|error| PeekabooXError::new(format!("invalid title regex: {error}")))
}

fn sort_windows(windows: &mut [WindowInfo], sort: WindowSort) {
    match sort {
        WindowSort::Backend => {}
        WindowSort::Focused => windows.sort_by(|left, right| {
            right
                .focused
                .cmp(&left.focused)
                .then_with(|| normalized_title(left).cmp(&normalized_title(right)))
                .then_with(|| left.id.cmp(&right.id))
        }),
        WindowSort::Title => windows.sort_by(|left, right| {
            normalized_title(left)
                .cmp(&normalized_title(right))
                .then_with(|| left.id.cmp(&right.id))
        }),
        WindowSort::App => windows.sort_by(|left, right| {
            normalized_app(left)
                .cmp(&normalized_app(right))
                .then_with(|| normalized_title(left).cmp(&normalized_title(right)))
                .then_with(|| left.id.cmp(&right.id))
        }),
        WindowSort::Area => windows.sort_by(|left, right| {
            window_area(right)
                .cmp(&window_area(left))
                .then_with(|| normalized_title(left).cmp(&normalized_title(right)))
                .then_with(|| left.id.cmp(&right.id))
        }),
        WindowSort::Id => windows.sort_by(|left, right| left.id.cmp(&right.id)),
        WindowSort::State => windows.sort_by(|left, right| {
            format!("{:?}", left.state)
                .cmp(&format!("{:?}", right.state))
                .then_with(|| normalized_title(left).cmp(&normalized_title(right)))
                .then_with(|| left.id.cmp(&right.id))
        }),
    }
}

fn normalized_title(window: &WindowInfo) -> String {
    window.title.to_ascii_lowercase()
}

fn normalized_app(window: &WindowInfo) -> String {
    window
        .app_id
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn window_area(window: &WindowInfo) -> u64 {
    u64::from(window.bounds.width) * u64::from(window.bounds.height)
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

pub fn focus_window(window_id: &str) -> Result<()> {
    CommandWindowBackend.focus_window(window_id)
}

pub fn list_windows_with_query(query: WindowQuery) -> Result<WindowListMetadata> {
    CommandWindowBackend.list_windows_with_query(query)
}

pub fn candidate_backends(environment: &WindowEnvironment) -> Vec<DetectedWindowBackend> {
    candidate_backends_for_query(environment, WindowBackendSelection::Auto)
}

pub fn candidate_backends_for_query(
    environment: &WindowEnvironment,
    backend: WindowBackendSelection,
) -> Vec<DetectedWindowBackend> {
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
        .filter(|tool| backend.tool().is_none_or(|selected| *tool == selected))
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

    let (x, y, width, height) = atspi_extents_if_component(connection, object_ref).ok()?;
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

fn atspi_extents_if_component(
    connection: &Connection,
    object_ref: &AtSpiRef,
) -> Result<(i32, i32, i32, i32)> {
    if atspi_has_component_interface(connection, object_ref).unwrap_or(true) {
        atspi_extents(connection, object_ref)
    } else {
        Err(PeekabooXError::new(
            "AT-SPI object does not expose Component",
        ))
    }
}

fn atspi_has_component_interface(connection: &Connection, object_ref: &AtSpiRef) -> Result<bool> {
    let proxy = connection.with_proxy(
        object_ref.0.as_str(),
        object_ref.1.clone(),
        Duration::from_secs(2),
    );
    let (interfaces,): (Vec<String>,) = proxy
        .method_call("org.a11y.atspi.Accessible", "GetInterfaces", ())
        .map_err(|error| {
            PeekabooXError::new(format!("AT-SPI interfaces lookup failed: {error}"))
        })?;

    Ok(interfaces_include_component(&interfaces))
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

fn interfaces_include_component(interfaces: &[String]) -> bool {
    interfaces
        .iter()
        .any(|interface| interface == "org.a11y.atspi.Component" || interface == "Component")
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
    let fullscreen =
        variant_bool_any(properties, &["is-fullscreen", "fullscreen"]).unwrap_or(false);
    let maximized = variant_bool_any(
        properties,
        &[
            "is-maximized",
            "maximized",
            "maximized-horizontally",
            "maximized-vertically",
        ],
    )
    .unwrap_or(false);

    Some(WindowInfo {
        id: id.to_string(),
        title,
        app_id: variant_string(properties, "app-id")
            .or_else(|| variant_string(properties, "gtk-application-id"))
            .or_else(|| variant_string(properties, "wm-class")),
        bounds: Rect::new(x, y, width, height),
        focused,
        state: if hidden {
            WindowState::Minimized
        } else if fullscreen {
            WindowState::Fullscreen
        } else if maximized {
            WindowState::Maximized
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
        app_id: xdotool_window_app_id(id),
        bounds,
        focused: active_window_id.is_some_and(|active_id| active_id == id),
        state: xdotool_window_state(id).unwrap_or(WindowState::Normal),
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

fn xdotool_window_app_id(id: &str) -> Option<String> {
    if !command_exists("xprop") {
        return None;
    }

    run_command_capture("xprop", ["-id", id, "WM_CLASS"])
        .ok()
        .and_then(|output| parse_xprop_wm_class(&output))
}

fn xdotool_window_state(id: &str) -> Option<WindowState> {
    if !command_exists("xprop") {
        return None;
    }

    run_command_capture("xprop", ["-id", id, "_NET_WM_STATE"])
        .ok()
        .and_then(|output| parse_xprop_window_state(&output))
}

pub fn parse_xprop_wm_class(output: &str) -> Option<String> {
    let quoted_values: Vec<&str> = output.split('"').skip(1).step_by(2).collect();
    quoted_values
        .last()
        .or_else(|| quoted_values.first())
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub fn parse_xprop_window_state(output: &str) -> Option<WindowState> {
    if output.contains("_NET_WM_STATE_FULLSCREEN") {
        return Some(WindowState::Fullscreen);
    }

    if output.contains("_NET_WM_STATE_HIDDEN") {
        return Some(WindowState::Minimized);
    }

    if output.contains("_NET_WM_STATE_MAXIMIZED_HORZ")
        || output.contains("_NET_WM_STATE_MAXIMIZED_VERT")
    {
        return Some(WindowState::Maximized);
    }

    if output.contains("_NET_WM_STATE") {
        return Some(WindowState::Normal);
    }

    None
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

fn variant_bool_any(properties: &PropMap, keys: &[&str]) -> Option<bool> {
    keys.iter().find_map(|key| variant_bool(properties, key))
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
    match value.to_ascii_lowercase().as_str() {
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
        WindowEnvironment, WindowQuery, WindowSort, WindowTool, apply_window_query,
        atspi_state_contains, candidate_backends, parse_xdotool_geometry, parse_xprop_window_state,
        parse_xprop_wm_class, should_ignore_xdotool_window,
    };
    use crate::SessionType;
    use peekaboox_core::{Rect, WindowInfo, WindowState};

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

    #[test]
    fn filters_sorts_and_limits_windows() {
        let windows = vec![
            window("2", "Terminal", Some("org.gnome.Terminal"), false, 100, 100),
            window(
                "1",
                "Calculator",
                Some("org.gnome.Calculator"),
                true,
                300,
                200,
            ),
        ];
        let query = WindowQuery {
            app: Some("calculator".to_owned()),
            focused_only: true,
            limit: Some(1),
            sort: WindowSort::Area,
            ..WindowQuery::default()
        };

        let result = apply_window_query(windows, &query).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "1");
    }

    #[test]
    fn filters_windows_by_case_insensitive_title_regex() {
        let windows = vec![window("1", "GNOME Calculator", None, false, 300, 200)];
        let query = WindowQuery {
            title_regex: Some("gnome calc.*".to_owned()),
            ..WindowQuery::default()
        };

        let result = apply_window_query(windows, &query).unwrap();

        assert_eq!(result[0].title, "GNOME Calculator");
    }

    #[test]
    fn parses_xprop_wm_class() {
        assert_eq!(
            parse_xprop_wm_class("WM_CLASS(STRING) = \"gnome-calculator\", \"Gnome-calculator\""),
            Some("Gnome-calculator".to_owned())
        );
    }

    #[test]
    fn parses_xprop_window_state() {
        assert_eq!(
            parse_xprop_window_state("_NET_WM_STATE(ATOM) = _NET_WM_STATE_FULLSCREEN"),
            Some(WindowState::Fullscreen)
        );
        assert_eq!(
            parse_xprop_window_state(
                "_NET_WM_STATE(ATOM) = _NET_WM_STATE_MAXIMIZED_VERT, _NET_WM_STATE_MAXIMIZED_HORZ"
            ),
            Some(WindowState::Maximized)
        );
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

    fn window(
        id: &str,
        title: &str,
        app_id: Option<&str>,
        focused: bool,
        width: u32,
        height: u32,
    ) -> WindowInfo {
        WindowInfo {
            id: id.to_owned(),
            title: title.to_owned(),
            app_id: app_id.map(str::to_owned),
            bounds: Rect::new(0, 0, width, height),
            focused,
            state: WindowState::Normal,
        }
    }
}
