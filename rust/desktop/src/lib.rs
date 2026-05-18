use std::cmp::{max, min};
use std::collections::HashSet;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use peekaboox_core::{
    CaptureFrame, PeekabooXError, PixelFormat, Point, Rect, Result, UiElement, WindowInfo,
};
use peekaboox_input::MouseButton;
use serde::Deserialize;

const DEFAULT_FOCUS_WAIT_MS: u64 = 1_000;
const DEFAULT_OVERVIEW_WAIT_MS: u64 = 800;
const ACTION_FOCUS_OVERVIEW_WAIT_MS: u64 = 1_000;
static TEMP_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);
const DESKTOP_PROFILE_FILE_SCHEMA_VERSION: &str = "desktop-profile.v1";
const DESKTOP_PROFILE_PATH_ENV: &str = "PEEKABOOX_DESKTOP_PROFILE_PATH";
const TELEGRAM_PROFILE_ID: &str = "telegram";
const TELEGRAM_SEARCH_NAME: &str = "Telegram";
const TELEGRAM_DESKTOP_IDS: &[&str] = &[
    "telegram-desktop",
    "org.telegram.desktop",
    "telegram-desktop_telegram-desktop",
];
const TELEGRAM_ALIASES: &[&str] = &["telegram", "telegram-desktop", "org.telegram.desktop"];
const PAINT_PROFILE_ID: &str = "paint";
const DRAWING_PROFILE_ID: &str = "drawing";
const PINTA_PROFILE_ID: &str = "pinta";
const KOLOURPAINT_PROFILE_ID: &str = "kolourpaint";
const DRAWING_SEARCH_NAME: &str = "Drawing";
const PINTA_SEARCH_NAME: &str = "Pinta";
const KOLOURPAINT_SEARCH_NAME: &str = "KolourPaint";
const DRAWING_DESKTOP_IDS: &[&str] = &["drawing", "com.github.maoschanz.drawing"];
const PINTA_DESKTOP_IDS: &[&str] = &["pinta", "com.github.PintaProject.Pinta"];
const KOLOURPAINT_DESKTOP_IDS: &[&str] = &["org.kde.kolourpaint", "kolourpaint"];
const PAINT_DESKTOP_IDS: &[&str] = &[
    "drawing",
    "com.github.maoschanz.drawing",
    "pinta",
    "com.github.PintaProject.Pinta",
    "org.kde.kolourpaint",
    "kolourpaint",
];
const PAINT_ALIASES: &[&str] = &[
    "paint",
    "drawing",
    "pinta",
    "kolourpaint",
    "org.gnome.Drawing",
    "com.github.maoschanz.drawing",
];
const DRAWING_ALIASES: &[&str] = &[
    "drawing",
    "org.gnome.Drawing",
    "com.github.maoschanz.drawing",
];
const PINTA_ALIASES: &[&str] = &["pinta", "com.github.PintaProject.Pinta"];
const KOLOURPAINT_ALIASES: &[&str] = &["kolourpaint", "org.kde.kolourpaint"];
const TEXT_EDITOR_PROFILE_ID: &str = "text-editor";
const TEXT_EDITOR_SEARCH_NAME: &str = "Text Editor";
const TEXT_EDITOR_DESKTOP_IDS: &[&str] = &["org.gnome.TextEditor", "gnome-text-editor"];
const TEXT_EDITOR_ALIASES: &[&str] = &["text-editor", "gnome-text-editor", "org.gnome.TextEditor"];
const CALENDAR_PROFILE_ID: &str = "calendar";
const CALENDAR_SEARCH_NAME: &str = "Calendar";
const CALENDAR_DESKTOP_IDS: &[&str] = &["org.gnome.Calendar", "gnome-calendar"];
const CALENDAR_ALIASES: &[&str] = &[
    "calendar",
    "gnome-calendar",
    "org.gnome.Calendar",
    "org.gnome.Calendar.desktop",
];
const BROWSER_PROFILE_ID: &str = "browser";
const BROWSER_SEARCH_NAME: &str = "Web";
const BROWSER_DESKTOP_IDS: &[&str] = &[
    "org.gnome.Epiphany",
    "firefox",
    "google-chrome",
    "chromium",
    "brave-browser",
];
const BROWSER_ALIASES: &[&str] = &[
    "browser",
    "web",
    "firefox",
    "chrome",
    "chromium",
    "brave-browser",
    "org.gnome.Epiphany",
];
const FILES_PROFILE_ID: &str = "files";
const FILES_SEARCH_NAME: &str = "Files";
const FILES_DESKTOP_IDS: &[&str] = &["org.gnome.Nautilus", "nautilus", "thunar", "dolphin"];
const FILES_ALIASES: &[&str] = &["files", "file-manager", "nautilus", "thunar", "dolphin"];
const TERMINAL_PROFILE_ID: &str = "terminal";
const TERMINAL_SEARCH_NAME: &str = "Terminal";
const TERMINAL_DESKTOP_IDS: &[&str] = &[
    "org.gnome.Terminal",
    "org.gnome.Console",
    "kgx",
    "konsole",
    "xfce4-terminal",
];
const TERMINAL_ALIASES: &[&str] = &[
    "terminal",
    "console",
    "gnome-terminal",
    "kgx",
    "konsole",
    "xfce4-terminal",
];
const OFFICE_PROFILE_ID: &str = "libreoffice";
const OFFICE_SEARCH_NAME: &str = "LibreOffice";
const OFFICE_DESKTOP_IDS: &[&str] = &[
    "libreoffice-startcenter",
    "libreoffice-writer",
    "libreoffice-calc",
    "org.libreoffice.LibreOffice",
];
const OFFICE_ALIASES: &[&str] = &["libreoffice", "writer", "calc", "office"];
const SUPPORTED_APPS: &[&str] = &[
    TELEGRAM_PROFILE_ID,
    PAINT_PROFILE_ID,
    DRAWING_PROFILE_ID,
    PINTA_PROFILE_ID,
    KOLOURPAINT_PROFILE_ID,
    TEXT_EDITOR_PROFILE_ID,
    CALENDAR_PROFILE_ID,
    BROWSER_PROFILE_ID,
    FILES_PROFILE_ID,
    TERMINAL_PROFILE_ID,
    OFFICE_PROFILE_ID,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopTargetSource {
    Accessibility,
    VisualLayout,
}

impl DesktopTargetSource {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Accessibility => "accessibility",
            Self::VisualLayout => "visual-layout",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDesktopTarget {
    pub app: String,
    pub target: String,
    pub point: Point,
    pub rect: Option<Rect>,
    pub source: DesktopTargetSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopActionResult {
    pub app: String,
    pub action: String,
    pub detail: String,
    pub backend_name: String,
    pub verified: bool,
    pub verification_detail: Option<String>,
    pub focus_diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusOptions {
    pub use_gnome_overview: bool,
    pub launch_if_needed: bool,
    pub wait_after_focus_ms: u64,
    pub overview_wait_ms: u64,
    pub window_title: Option<String>,
    pub window_id: Option<String>,
    pub verify: bool,
}

impl Default for FocusOptions {
    fn default() -> Self {
        Self {
            use_gnome_overview: true,
            launch_if_needed: true,
            wait_after_focus_ms: DEFAULT_FOCUS_WAIT_MS,
            overview_wait_ms: DEFAULT_OVERVIEW_WAIT_MS,
            window_title: None,
            window_id: None,
            verify: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocateOptions {
    pub image: Option<PathBuf>,
    pub prefer_accessibility: bool,
    pub window_title: Option<String>,
    pub window_id: Option<String>,
}

impl Default for LocateOptions {
    fn default() -> Self {
        Self {
            image: None,
            prefer_accessibility: true,
            window_title: None,
            window_id: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClickOptions {
    pub locate: LocateOptions,
    pub button: MouseButton,
    pub dry_run: bool,
    pub verify: bool,
}

impl Default for ClickOptions {
    fn default() -> Self {
        Self {
            locate: LocateOptions::default(),
            button: MouseButton::Left,
            dry_run: false,
            verify: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DesktopDragOptions {
    pub locate: LocateOptions,
    pub from_ratio: (f32, f32),
    pub to_ratio: (f32, f32),
    pub button: MouseButton,
    pub duration_ms: u64,
    pub dry_run: bool,
    pub verify: bool,
}

impl Default for DesktopDragOptions {
    fn default() -> Self {
        Self {
            locate: LocateOptions::default(),
            from_ratio: (0.5, 0.5),
            to_ratio: (0.5, 0.5),
            button: MouseButton::Left,
            duration_ms: 250,
            dry_run: false,
            verify: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TypeIntoOptions {
    pub locate: LocateOptions,
    pub clear: bool,
    pub dry_run: bool,
    pub verify: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesktopAssertion {
    Present,
    NotPresent,
    Active,
    NotActive,
    Contains(String),
    NotContains(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertOptions {
    pub locate: LocateOptions,
    pub assertion: DesktopAssertion,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopProfileInfo {
    pub id: String,
    pub aliases: Vec<String>,
    pub search_name: String,
    pub desktop_ids: Vec<String>,
    pub commands: Vec<DesktopProfileCommandInfo>,
    pub targets: Vec<DesktopProfileTargetInfo>,
    pub availability: DesktopProfileAvailability,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopProfileCommandInfo {
    pub program: String,
    pub args: Vec<String>,
    pub display: String,
    pub available: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopProfileTargetInfo {
    pub name: String,
    pub supports: Vec<String>,
    pub sources: Vec<String>,
    pub can_locate: bool,
    pub can_click: bool,
    pub can_drag: bool,
    pub can_type: bool,
    pub can_assert_present: bool,
    pub can_assert_active: bool,
    pub can_assert_contains: bool,
    pub accessibility_selector: Option<String>,
    pub visual_layout: bool,
    pub visual_rect: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopProfileAvailability {
    pub checked: bool,
    pub installed: Option<bool>,
    pub command_available: Option<bool>,
    pub desktop_entry_available: Option<bool>,
    pub available_commands: Vec<String>,
    pub available_desktop_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DesktopProfileQuery {
    pub app: Option<String>,
    pub target: Option<String>,
    pub command: Option<String>,
    pub desktop_id: Option<String>,
    pub supports: Option<String>,
    pub check_availability: bool,
    pub installed_only: bool,
    pub available_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopProfileList {
    pub schema_version: String,
    pub count: usize,
    pub profiles: Vec<DesktopProfileInfo>,
}

pub fn supported_apps() -> &'static [&'static str] {
    SUPPORTED_APPS
}

pub const DESKTOP_PROFILE_SCHEMA_VERSION: &str = "desktop-profiles.v1";

pub fn desktop_profiles() -> Vec<DesktopProfileInfo> {
    desktop_profiles_with_query(&DesktopProfileQuery::default())
        .map(|result| result.profiles)
        .unwrap_or_default()
}

pub fn desktop_profiles_with_query(query: &DesktopProfileQuery) -> Result<DesktopProfileList> {
    desktop_profiles_with_query_and_paths(query, &desktop_profile_search_paths())
}

fn desktop_profiles_with_query_and_paths(
    query: &DesktopProfileQuery,
    profile_paths: &[PathBuf],
) -> Result<DesktopProfileList> {
    let check_availability =
        query.check_availability || query.installed_only || query.available_only;
    let app = query
        .app
        .as_deref()
        .map(str::trim)
        .filter(|app| !app.is_empty());
    let profiles = profile_catalog_from_paths(profile_paths)?;
    let profiles = if let Some(app) = app {
        let matches = matching_profiles_for_app(&profiles, app);
        if matches.is_empty() {
            return Err(PeekabooXError::new(format!(
                "unsupported desktop app {app:?}; supported apps: {}",
                profile_ids_text(&profiles)
            )));
        }
        matches
    } else {
        profiles.iter().collect::<Vec<_>>()
    }
    .into_iter()
    .map(|profile| profile_info(profile, check_availability))
    .filter(|profile| profile_matches_query(profile, query))
    .collect::<Vec<_>>();

    let count = profiles.len();
    Ok(DesktopProfileList {
        schema_version: DESKTOP_PROFILE_SCHEMA_VERSION.to_owned(),
        count,
        profiles,
    })
}

fn profile_catalog_from_paths(profile_paths: &[PathBuf]) -> Result<Vec<AppProfile>> {
    let mut profiles = builtin_profiles();
    for profile in load_external_profiles(profile_paths)? {
        upsert_profile(&mut profiles, profile);
    }
    Ok(profiles)
}

fn matching_profiles_for_app<'a>(profiles: &'a [AppProfile], app: &str) -> Vec<&'a AppProfile> {
    let id_matches = profiles
        .iter()
        .filter(|profile| profile.id.eq_ignore_ascii_case(app))
        .collect::<Vec<_>>();
    if !id_matches.is_empty() {
        return id_matches;
    }

    let specific_matches = profiles
        .iter()
        .filter(|profile| profile.id != PAINT_PROFILE_ID && profile.matches_registry_filter(app))
        .collect::<Vec<_>>();
    if !specific_matches.is_empty() {
        return specific_matches;
    }

    profiles
        .iter()
        .filter(|profile| profile.matches_registry_filter(app))
        .collect()
}

pub fn desktop_profile(app: &str) -> Result<DesktopProfileInfo> {
    let query = DesktopProfileQuery {
        app: Some(app.to_owned()),
        ..Default::default()
    };
    desktop_profiles_with_query(&query)?
        .profiles
        .into_iter()
        .next()
        .ok_or_else(|| {
            PeekabooXError::new(format!(
                "unsupported desktop app {app:?}; supported apps: {}",
                SUPPORTED_APPS.join(", ")
            ))
        })
}

fn profile_matches_query(profile: &DesktopProfileInfo, query: &DesktopProfileQuery) -> bool {
    if let Some(target) = normalized_filter(query.target.as_deref())
        && !profile
            .targets
            .iter()
            .any(|candidate| candidate.name.eq_ignore_ascii_case(&target))
    {
        return false;
    }

    if let Some(command) = normalized_filter(query.command.as_deref())
        && !profile.commands.iter().any(|candidate| {
            candidate.program.eq_ignore_ascii_case(&command)
                || candidate.display.eq_ignore_ascii_case(&command)
                || contains_case_insensitive(&candidate.display, &command)
        })
    {
        return false;
    }

    if let Some(desktop_id) = normalized_filter(query.desktop_id.as_deref())
        && !profile
            .desktop_ids
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(&desktop_id))
    {
        return false;
    }

    if let Some(support) = normalized_support_filter(query.supports.as_deref())
        && !profile.targets.iter().any(|target| {
            target
                .supports
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(&support))
        })
    {
        return false;
    }

    if query.installed_only || query.available_only {
        return profile.availability.installed.unwrap_or(false);
    }

    true
}

fn normalized_filter(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalized_support_filter(value: Option<&str>) -> Option<String> {
    normalized_filter(value).map(|value| value.replace('_', "-").to_ascii_lowercase())
}

fn profile_info(profile: &AppProfile, check_availability: bool) -> DesktopProfileInfo {
    let availability = profile_availability(profile, check_availability);
    DesktopProfileInfo {
        id: profile.id.clone(),
        aliases: profile.aliases.clone(),
        search_name: profile.search_name.clone(),
        desktop_ids: profile.desktop_ids.clone(),
        commands: profile
            .commands
            .iter()
            .map(|command| command_info(command, check_availability))
            .collect(),
        targets: profile_target_infos(profile),
        availability,
    }
}

fn command_info(command: &CommandSpec, check_availability: bool) -> DesktopProfileCommandInfo {
    DesktopProfileCommandInfo {
        program: command.program.clone(),
        args: command.args.clone(),
        display: command.display(),
        available: check_availability.then(|| command_available(command)),
    }
}

fn profile_target_infos(profile: &AppProfile) -> Vec<DesktopProfileTargetInfo> {
    profile
        .supported_targets()
        .iter()
        .map(|target| target_info(profile, target))
        .collect()
}

fn target_info(profile: &AppProfile, target: &str) -> DesktopProfileTargetInfo {
    if let Some(custom) = profile.custom_target(target) {
        return custom_target_info(custom);
    }

    let accessibility_selector = profile.accessibility_selector(target);
    let visual_rect = target_has_visual_rect(profile.kind, target);
    let can_type = target_accepts_text(profile.kind, target);
    let can_assert_active = target_exposes_active_state(profile.kind, target);
    let can_assert_contains = visual_rect && target_can_contain_text(profile.kind, target);
    let can_drag = visual_rect && target_accepts_drag(profile.kind, target);

    let mut supports = vec![
        "locate".to_owned(),
        "click".to_owned(),
        "assert-present".to_owned(),
        "visual-layout".to_owned(),
    ];
    if accessibility_selector.is_some() {
        supports.push("accessibility".to_owned());
    }
    if can_drag {
        supports.push("drag".to_owned());
    }
    if can_type {
        supports.push("type-into".to_owned());
    }
    if can_assert_active {
        supports.push("assert-active".to_owned());
    }
    if can_assert_contains {
        supports.push("assert-contains".to_owned());
    }

    let mut sources = vec!["visual-layout".to_owned()];
    if accessibility_selector.is_some() {
        sources.push("accessibility".to_owned());
    }

    DesktopProfileTargetInfo {
        name: target.to_owned(),
        supports,
        sources,
        can_locate: true,
        can_click: true,
        can_drag,
        can_type,
        can_assert_present: true,
        can_assert_active,
        can_assert_contains,
        accessibility_selector: accessibility_selector.map(ToOwned::to_owned),
        visual_layout: true,
        visual_rect,
    }
}

fn profile_availability(
    profile: &AppProfile,
    check_availability: bool,
) -> DesktopProfileAvailability {
    if !check_availability {
        return DesktopProfileAvailability {
            checked: false,
            installed: None,
            command_available: None,
            desktop_entry_available: None,
            available_commands: Vec::new(),
            available_desktop_ids: Vec::new(),
        };
    }

    let available_commands = profile
        .commands
        .iter()
        .filter(|command| command_available(command))
        .map(CommandSpec::display)
        .collect::<Vec<_>>();
    let available_desktop_ids = profile
        .desktop_ids
        .iter()
        .filter(|desktop_id| desktop_entry_exists(desktop_id))
        .cloned()
        .collect::<Vec<_>>();
    let command_available = !available_commands.is_empty();
    let desktop_entry_available = !available_desktop_ids.is_empty();

    DesktopProfileAvailability {
        checked: true,
        installed: Some(command_available || desktop_entry_available),
        command_available: Some(command_available),
        desktop_entry_available: Some(desktop_entry_available),
        available_commands,
        available_desktop_ids,
    }
}

fn target_has_visual_rect(kind: ProfileKind, target: &str) -> bool {
    !matches!((kind, target), (ProfileKind::Telegram, "search-clear"))
}

fn target_accepts_text(kind: ProfileKind, target: &str) -> bool {
    matches!(
        (kind, target),
        (ProfileKind::Telegram, "search-input")
            | (ProfileKind::Telegram, "message-input")
            | (ProfileKind::TextEditor, "document")
    )
}

fn target_exposes_active_state(kind: ProfileKind, target: &str) -> bool {
    matches!((kind, target), (ProfileKind::Telegram, "send-button"))
}

fn target_can_contain_text(kind: ProfileKind, target: &str) -> bool {
    matches!(
        (kind, target),
        (ProfileKind::Telegram, "search-input")
            | (ProfileKind::Telegram, "search-result")
            | (ProfileKind::Telegram, "message-input")
            | (ProfileKind::Telegram, "header")
            | (ProfileKind::TextEditor, "document")
    )
}

fn target_accepts_drag(kind: ProfileKind, target: &str) -> bool {
    matches!(
        (kind, target),
        (ProfileKind::Paint, "canvas") | (ProfileKind::TextEditor, "document")
    )
}

fn desktop_entry_exists(desktop_id: &str) -> bool {
    let filename = if desktop_id.ends_with(".desktop") {
        desktop_id.to_owned()
    } else {
        format!("{desktop_id}.desktop")
    };

    desktop_entry_roots()
        .into_iter()
        .any(|root| root.join(&filename).is_file())
}

fn desktop_entry_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(value) = env::var_os("XDG_DATA_HOME") {
        roots.push(PathBuf::from(value).join("applications"));
    }
    if let Some(home) = env::var_os("HOME") {
        let home = PathBuf::from(home);
        roots.push(home.join(".local/share/applications"));
        roots.push(home.join(".local/share/flatpak/exports/share/applications"));
    }
    if let Some(value) = env::var_os("XDG_DATA_DIRS") {
        roots.extend(env::split_paths(&value).map(|path| path.join("applications")));
    } else {
        roots.push(PathBuf::from("/usr/local/share/applications"));
        roots.push(PathBuf::from("/usr/share/applications"));
    }
    roots.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));
    roots
}

pub fn focus_app(app: &str, options: &FocusOptions) -> Result<DesktopActionResult> {
    let profile = resolve_profile(app)?;
    let window_scope = normalized_window_scope(
        options.window_title.as_deref(),
        options.window_id.as_deref(),
    );
    let mut diagnostics = Vec::new();

    match peekaboox_windows::list_windows() {
        Ok(metadata) => {
            diagnostics.push(format!(
                "windows: listed {} windows via {}",
                metadata.windows.len(),
                metadata.backend_name
            ));

            if let Some(window) =
                preferred_profile_window(&profile, &metadata.windows, window_scope)
            {
                diagnostics.push(format!(
                    "windows: selected {} title {:?} focused={}",
                    window.id, window.title, window.focused
                ));

                if window.focused {
                    sleep_after_focus(options);
                    diagnostics.push(format!("already-focused: window {}", window.id));
                    let result = DesktopActionResult {
                        app: profile.id.to_owned(),
                        action: "focus".to_owned(),
                        detail: "already focused".to_owned(),
                        backend_name: metadata.backend_name,
                        verified: true,
                        verification_detail: Some(format!("window {} is focused", window.id)),
                        focus_diagnostics: focus_diagnostics_snapshot(&diagnostics),
                    };
                    return maybe_verify_action(result, options.verify, || {
                        verify_focused_window(&profile, window_scope)
                    });
                }

                let mut last_result = None;

                match peekaboox_windows::focus_window(&window.id) {
                    Ok(()) => {
                        diagnostics
                            .push(format!("window-manager: requested focus for {}", window.id));
                        let result = DesktopActionResult {
                            app: profile.id.to_owned(),
                            action: "focus".to_owned(),
                            detail: format!("focused existing window {}", window.id),
                            backend_name: "window-manager".to_owned(),
                            verified: false,
                            verification_detail: None,
                            focus_diagnostics: Vec::new(),
                        };
                        if let Some(result) = confirmed_focus_result(
                            &result,
                            options,
                            &profile,
                            window_scope,
                            &mut diagnostics,
                        ) {
                            return Ok(result);
                        }
                        last_result = Some(unconfirmed_focus_result(result, &diagnostics));
                    }
                    Err(error) => diagnostics.push(format!("window-manager: {error}")),
                }

                match focus_window_via_accessibility(window) {
                    Ok(detail) => {
                        diagnostics.push(format!("at-spi: {detail}"));
                        let result = DesktopActionResult {
                            app: profile.id.to_owned(),
                            action: "focus".to_owned(),
                            detail,
                            backend_name: "at-spi".to_owned(),
                            verified: false,
                            verification_detail: None,
                            focus_diagnostics: Vec::new(),
                        };
                        if let Some(result) = confirmed_focus_result(
                            &result,
                            options,
                            &profile,
                            window_scope,
                            &mut diagnostics,
                        ) {
                            return Ok(result);
                        }
                        last_result = Some(unconfirmed_focus_result(result, &diagnostics));
                    }
                    Err(error) => diagnostics.push(format!("at-spi: {error}")),
                }

                match focus_from_gnome_dock(&profile) {
                    Ok(detail) => {
                        diagnostics.push(format!("gnome-dock: {detail}"));
                        let result = DesktopActionResult {
                            app: profile.id.to_owned(),
                            action: "focus".to_owned(),
                            detail,
                            backend_name: "gnome-dock".to_owned(),
                            verified: false,
                            verification_detail: None,
                            focus_diagnostics: Vec::new(),
                        };
                        if let Some(result) = confirmed_focus_result(
                            &result,
                            options,
                            &profile,
                            window_scope,
                            &mut diagnostics,
                        ) {
                            return Ok(result);
                        }
                        last_result = Some(unconfirmed_focus_result(result, &diagnostics));
                    }
                    Err(error) => diagnostics.push(format!("gnome-dock: {error}")),
                }

                if options.use_gnome_overview {
                    match focus_from_gnome_overview(&profile, options) {
                        Ok(()) => {
                            diagnostics.push("gnome-overview: requested activation".to_owned());
                            let result = DesktopActionResult {
                                app: profile.id.to_owned(),
                                action: "focus".to_owned(),
                                detail: "focused existing app via GNOME overview".to_owned(),
                                backend_name: "gnome-overview".to_owned(),
                                verified: false,
                                verification_detail: None,
                                focus_diagnostics: Vec::new(),
                            };
                            if let Some(result) = confirmed_focus_result(
                                &result,
                                options,
                                &profile,
                                window_scope,
                                &mut diagnostics,
                            ) {
                                return Ok(result);
                            }
                            last_result = Some(unconfirmed_focus_result(result, &diagnostics));
                        }
                        Err(error) => diagnostics.push(format!("gnome-overview: {error}")),
                    }
                } else {
                    diagnostics.push("gnome-overview: skipped (--no-overview)".to_owned());
                }

                if window.bounds.width > 0 && window.bounds.height > 0 {
                    let center = Point::new(
                        window.bounds.x + i32::try_from(window.bounds.width / 2).unwrap_or(0),
                        window.bounds.y + i32::try_from(window.bounds.height / 2).unwrap_or(0),
                    );
                    match peekaboox_input::click(center, MouseButton::Left) {
                        Ok(metadata) => {
                            diagnostics.push(format!(
                                "coordinate-click: clicked {},{} via {}",
                                center.x, center.y, metadata.backend_name
                            ));
                            let result = DesktopActionResult {
                                app: profile.id.to_owned(),
                                action: "focus".to_owned(),
                                detail: format!(
                                    "clicked existing window at {},{}",
                                    center.x, center.y
                                ),
                                backend_name: metadata.backend_name,
                                verified: false,
                                verification_detail: None,
                                focus_diagnostics: Vec::new(),
                            };
                            if let Some(result) = confirmed_focus_result(
                                &result,
                                options,
                                &profile,
                                window_scope,
                                &mut diagnostics,
                            ) {
                                return Ok(result);
                            }
                            last_result = Some(unconfirmed_focus_result(result, &diagnostics));
                        }
                        Err(error) => diagnostics.push(format!("coordinate-click: {error}")),
                    }
                } else {
                    diagnostics.push("coordinate-click: skipped; window has no bounds".to_owned());
                }

                if let Some(result) = last_result {
                    if options.verify {
                        return Err(PeekabooXError::new(focus_error_with_diagnostics(
                            result.verification_detail.unwrap_or_else(|| {
                                format!("could not verify focused {}", profile.id)
                            }),
                            &result.focus_diagnostics,
                        )));
                    }
                    return Ok(result);
                }
            } else {
                diagnostics.push(format!(
                    "windows: no matching visible {} window for {}",
                    profile.id,
                    window_scope.description()
                ));
            }
        }
        Err(error) => diagnostics.push(format!("windows: {error}")),
    }

    if window_scope.has_constraints() {
        return Err(PeekabooXError::new(focus_error_with_diagnostics(
            format!(
                "could not find visible app {app:?} window matching {}",
                window_scope.description()
            ),
            &diagnostics,
        )));
    }

    if options.use_gnome_overview {
        match focus_from_gnome_overview(&profile, options) {
            Ok(()) => {
                diagnostics.push("gnome-overview: requested activation".to_owned());
                sleep_after_focus(options);
                let result = DesktopActionResult {
                    app: profile.id.to_owned(),
                    action: "focus".to_owned(),
                    detail: "focused via GNOME overview".to_owned(),
                    backend_name: "gnome-overview".to_owned(),
                    verified: false,
                    verification_detail: None,
                    focus_diagnostics: focus_diagnostics_snapshot(&diagnostics),
                };
                return maybe_verify_action(result, options.verify, || {
                    verify_focused_window(&profile, window_scope)
                });
            }
            Err(error) => diagnostics.push(format!("gnome-overview: {error}")),
        }
    } else {
        diagnostics.push("gnome-overview: skipped (--no-overview)".to_owned());
    }

    if options.launch_if_needed {
        if let Some(desktop_id) = launch_desktop_entry(&profile) {
            diagnostics.push(format!("gtk-launch: launched desktop entry {desktop_id}"));
            sleep_after_focus(options);
            let result = DesktopActionResult {
                app: profile.id.to_owned(),
                action: "focus".to_owned(),
                detail: format!("launched desktop entry {desktop_id}"),
                backend_name: "gtk-launch".to_owned(),
                verified: false,
                verification_detail: None,
                focus_diagnostics: focus_diagnostics_snapshot(&diagnostics),
            };
            return maybe_verify_action(result, options.verify, || {
                verify_focused_window(&profile, window_scope)
            });
        }

        if let Some(command) = launch_command(&profile) {
            diagnostics.push(format!("command: launched {}", command.program));
            sleep_after_focus(options);
            let result = DesktopActionResult {
                app: profile.id.to_owned(),
                action: "focus".to_owned(),
                detail: format!("launched {}", command.program),
                backend_name: "command".to_owned(),
                verified: false,
                verification_detail: None,
                focus_diagnostics: focus_diagnostics_snapshot(&diagnostics),
            };
            return maybe_verify_action(result, options.verify, || {
                verify_focused_window(&profile, window_scope)
            });
        }
    } else {
        diagnostics.push("launch: skipped (--no-launch)".to_owned());
    }

    Err(PeekabooXError::new(focus_error_with_diagnostics(
        format!("could not focus or launch app {:?}", app),
        &diagnostics,
    )))
}

pub fn locate_target(
    app: &str,
    target: &str,
    options: &LocateOptions,
) -> Result<ResolvedDesktopTarget> {
    let profile = resolve_profile(app)?;
    let window_scope = normalized_window_scope(
        options.window_title.as_deref(),
        options.window_id.as_deref(),
    );
    if options.prefer_accessibility
        && options.image.is_none()
        && !window_scope.has_constraints()
        && let Some(selector) = profile.accessibility_selector(target)
        && let Ok(resolved) = peekaboox_accessibility::resolve_click_target(selector)
    {
        return Ok(ResolvedDesktopTarget {
            app: profile.id.to_owned(),
            target: target.to_owned(),
            point: resolved.position,
            rect: Some(resolved.element.bounds),
            source: DesktopTargetSource::Accessibility,
        });
    }

    let frame = load_or_capture_frame(options.image.as_deref())?;
    profile.resolve_visual_target(target, &frame, window_scope)
}

pub fn click_target(
    app: &str,
    target: &str,
    options: &ClickOptions,
) -> Result<DesktopActionResult> {
    let profile = resolve_profile(app)?;
    let focus = focus_before_live_action(app, &options.locate, options.dry_run)?;
    if can_click_target_via_accessibility_action(&profile, target, options)
        && let Some(selector) = profile.accessibility_selector(target)
        && let Some(result) =
            click_target_via_accessibility_action(&profile, target, selector, focus.as_ref())?
    {
        return maybe_verify_action(result, options.verify, || {
            locate_target(app, target, &options.locate).map(|verified| {
                format!(
                    "target {} present at {},{} via {}",
                    verified.target,
                    verified.point.x,
                    verified.point.y,
                    verified.source.label()
                )
            })
        });
    }
    let resolved = locate_target(app, target, &options.locate)?;
    if options.dry_run {
        return Ok(DesktopActionResult {
            app: resolved.app,
            action: "click".to_owned(),
            detail: format!(
                "would click {} at {},{} via {}",
                resolved.target,
                resolved.point.x,
                resolved.point.y,
                resolved.source.label()
            ),
            backend_name: "dry-run".to_owned(),
            verified: false,
            verification_detail: None,
            focus_diagnostics: Vec::new(),
        });
    }

    let metadata = peekaboox_input::click(resolved.point, options.button)?;
    let result = DesktopActionResult {
        app: resolved.app,
        action: "click".to_owned(),
        detail: action_detail_with_focus(
            focus.as_ref(),
            format!(
                "clicked {} at {},{} via {}",
                resolved.target,
                resolved.point.x,
                resolved.point.y,
                resolved.source.label()
            ),
        ),
        backend_name: metadata.backend_name,
        verified: false,
        verification_detail: None,
        focus_diagnostics: focus_diagnostics_from(focus.as_ref()),
    };
    maybe_verify_action(result, options.verify, || {
        locate_target(app, target, &options.locate).map(|verified| {
            format!(
                "target {} present at {},{} via {}",
                verified.target,
                verified.point.x,
                verified.point.y,
                verified.source.label()
            )
        })
    })
}

fn can_click_target_via_accessibility_action(
    profile: &AppProfile,
    target: &str,
    options: &ClickOptions,
) -> bool {
    !options.dry_run
        && options.button == MouseButton::Left
        && options.locate.prefer_accessibility
        && options.locate.image.is_none()
        && options.locate.window_title.is_none()
        && options.locate.window_id.is_none()
        && profile.accessibility_selector(target).is_some()
}

fn click_target_via_accessibility_action(
    profile: &AppProfile,
    target: &str,
    selector: &str,
    focus: Option<&DesktopActionResult>,
) -> Result<Option<DesktopActionResult>> {
    for action in ["click", "press", "activate"] {
        match peekaboox_accessibility::perform_action(selector, Some(action), None) {
            Ok(result) if result.ok => {
                return Ok(Some(accessibility_action_desktop_result(
                    profile, target, selector, result, focus,
                )));
            }
            Ok(_) | Err(_) => {}
        }
    }
    match peekaboox_accessibility::perform_action(selector, None, Some(0)) {
        Ok(result) if result.ok => Ok(Some(accessibility_action_desktop_result(
            profile, target, selector, result, focus,
        ))),
        Ok(_) | Err(_) => Ok(None),
    }
}

fn accessibility_action_desktop_result(
    profile: &AppProfile,
    target: &str,
    selector: &str,
    result: peekaboox_accessibility::AccessibilityActionResult,
    focus: Option<&DesktopActionResult>,
) -> DesktopActionResult {
    let label = result
        .element
        .label
        .as_deref()
        .unwrap_or(result.element.role.as_str());
    DesktopActionResult {
        app: profile.id.to_owned(),
        action: "click".to_owned(),
        detail: action_detail_with_focus(
            focus,
            format!(
                "clicked {} via AT-SPI action {} on selector {:?} ({label})",
                target, result.action, selector
            ),
        ),
        backend_name: result.backend_name,
        verified: false,
        verification_detail: None,
        focus_diagnostics: focus_diagnostics_from(focus),
    }
}

pub fn drag_target(
    app: &str,
    target: &str,
    options: &DesktopDragOptions,
) -> Result<DesktopActionResult> {
    let profile = resolve_profile(app)?;
    ensure_target_capability(&profile, target, "drag")?;
    let focus = focus_before_live_action(app, &options.locate, options.dry_run)?;
    let resolved = locate_target(app, target, &options.locate)?;
    let rect = resolved.rect.ok_or_else(|| {
        PeekabooXError::new(format!(
            "target {target:?} has no rectangle; ratio-based drag is unavailable"
        ))
    })?;
    let from = point_in_rect_ratio(rect, options.from_ratio)?;
    let to = point_in_rect_ratio(rect, options.to_ratio)?;

    if options.dry_run {
        return Ok(DesktopActionResult {
            app: resolved.app,
            action: "drag".to_owned(),
            detail: format!(
                "would drag {} from {},{} to {},{} via {}",
                resolved.target,
                from.x,
                from.y,
                to.x,
                to.y,
                resolved.source.label()
            ),
            backend_name: "dry-run".to_owned(),
            verified: false,
            verification_detail: None,
            focus_diagnostics: Vec::new(),
        });
    }

    let metadata = peekaboox_input::drag(from, to, options.button, options.duration_ms)?;
    let result = DesktopActionResult {
        app: resolved.app,
        action: "drag".to_owned(),
        detail: action_detail_with_focus(
            focus.as_ref(),
            format!(
                "dragged {} from {},{} to {},{} via {}",
                resolved.target,
                from.x,
                from.y,
                to.x,
                to.y,
                resolved.source.label()
            ),
        ),
        backend_name: metadata.backend_name,
        verified: false,
        verification_detail: None,
        focus_diagnostics: focus_diagnostics_from(focus.as_ref()),
    };
    maybe_verify_action(result, options.verify, || {
        locate_target(app, target, &options.locate).map(|verified| {
            format!(
                "target {} present at {},{} via {}",
                verified.target,
                verified.point.x,
                verified.point.y,
                verified.source.label()
            )
        })
    })
}

pub fn type_into_target(
    app: &str,
    target: &str,
    text: &str,
    options: &TypeIntoOptions,
) -> Result<DesktopActionResult> {
    let profile = resolve_profile(app)?;
    ensure_target_capability(&profile, target, "type-into")?;
    let focus = focus_before_live_action(app, &options.locate, options.dry_run)?;
    let resolved = locate_target(app, target, &options.locate)?;
    if options.dry_run {
        return Ok(DesktopActionResult {
            app: resolved.app,
            action: "type-into".to_owned(),
            detail: format!(
                "would type into {} at {},{} via {}",
                resolved.target,
                resolved.point.x,
                resolved.point.y,
                resolved.source.label()
            ),
            backend_name: "dry-run".to_owned(),
            verified: false,
            verification_detail: None,
            focus_diagnostics: Vec::new(),
        });
    }

    peekaboox_input::click(resolved.point, MouseButton::Left)?;
    sleep(Duration::from_millis(250));
    if options.clear {
        clear_target(&profile, target)?;
    }
    let metadata = peekaboox_input::type_text(text.to_owned())?;

    let result = DesktopActionResult {
        app: resolved.app,
        action: "type-into".to_owned(),
        detail: action_detail_with_focus(focus.as_ref(), format!("typed into {}", resolved.target)),
        backend_name: metadata.backend_name,
        verified: false,
        verification_detail: None,
        focus_diagnostics: focus_diagnostics_from(focus.as_ref()),
    };
    maybe_verify_action(result, options.verify, || {
        if text.trim().is_empty() {
            return locate_target(app, target, &options.locate)
                .map(|_| "target still present after typing".to_owned());
        }
        if target_text_contains(
            &profile,
            target,
            text,
            options.locate.image.as_deref(),
            normalized_window_scope(
                options.locate.window_title.as_deref(),
                options.locate.window_id.as_deref(),
            ),
        )? {
            Ok(format!(
                "target contains typed text ({}) characters",
                text.chars().count()
            ))
        } else {
            Err(PeekabooXError::new(format!(
                "target {target:?} does not contain the typed text after input"
            )))
        }
    })
}

pub fn assert_target(
    app: &str,
    target: &str,
    options: &AssertOptions,
) -> Result<DesktopActionResult> {
    let profile = resolve_profile(app)?;
    match &options.assertion {
        DesktopAssertion::Present => {
            locate_target(app, target, &options.locate)?;
        }
        DesktopAssertion::NotPresent => match locate_target(app, target, &options.locate) {
            Ok(_) => {
                return Err(PeekabooXError::new(format!(
                    "target {target:?} is present but expected it to be absent"
                )));
            }
            Err(error) if is_target_absence_error(&error) => {}
            Err(error) => return Err(error),
        },
        DesktopAssertion::Active => {
            ensure_target_capability(&profile, target, "assert-active")?;
            if !profile.target_active(
                target,
                &load_or_capture_frame(options.locate.image.as_deref())?,
                normalized_window_scope(
                    options.locate.window_title.as_deref(),
                    options.locate.window_id.as_deref(),
                ),
            )? {
                return Err(PeekabooXError::new(format!(
                    "target {target:?} is not active"
                )));
            }
        }
        DesktopAssertion::NotActive => {
            ensure_target_capability(&profile, target, "assert-active")?;
            if profile.target_active(
                target,
                &load_or_capture_frame(options.locate.image.as_deref())?,
                normalized_window_scope(
                    options.locate.window_title.as_deref(),
                    options.locate.window_id.as_deref(),
                ),
            )? {
                return Err(PeekabooXError::new(format!(
                    "target {target:?} is active but expected inactive"
                )));
            }
        }
        DesktopAssertion::Contains(expected) => {
            ensure_target_capability(&profile, target, "assert-contains")?;
            if !target_text_contains(
                &profile,
                target,
                expected,
                options.locate.image.as_deref(),
                normalized_window_scope(
                    options.locate.window_title.as_deref(),
                    options.locate.window_id.as_deref(),
                ),
            )? {
                return Err(PeekabooXError::new(format!(
                    "target {target:?} does not contain {expected:?}"
                )));
            }
        }
        DesktopAssertion::NotContains(expected) => {
            ensure_target_capability(&profile, target, "assert-contains")?;
            if target_text_contains(
                &profile,
                target,
                expected,
                options.locate.image.as_deref(),
                normalized_window_scope(
                    options.locate.window_title.as_deref(),
                    options.locate.window_id.as_deref(),
                ),
            )? {
                return Err(PeekabooXError::new(format!(
                    "target {target:?} contains {expected:?} but expected it not to"
                )));
            }
        }
    }

    Ok(DesktopActionResult {
        app: profile.id.to_owned(),
        action: "assert".to_owned(),
        detail: format!("asserted {} {:?}", target, options.assertion),
        backend_name: "desktop-guard".to_owned(),
        verified: true,
        verification_detail: Some("assertion passed".to_owned()),
        focus_diagnostics: Vec::new(),
    })
}

fn sleep_after_focus(options: &FocusOptions) {
    if options.wait_after_focus_ms > 0 {
        sleep(Duration::from_millis(options.wait_after_focus_ms));
    }
}

fn maybe_verify_action(
    mut result: DesktopActionResult,
    verify: bool,
    verifier: impl FnOnce() -> Result<String>,
) -> Result<DesktopActionResult> {
    if verify {
        match verifier() {
            Ok(detail) => {
                if result.action == "focus" {
                    result.focus_diagnostics.push(format!("verify: {detail}"));
                }
                result.verification_detail = Some(detail);
            }
            Err(error) => {
                if result.action == "focus" {
                    result.focus_diagnostics.push(format!("verify: {error}"));
                    return Err(PeekabooXError::new(focus_error_with_diagnostics(
                        error.to_string(),
                        &result.focus_diagnostics,
                    )));
                }
                return Err(error);
            }
        }
        result.verified = true;
    }
    Ok(result)
}

fn focus_before_live_action(
    app: &str,
    locate: &LocateOptions,
    dry_run: bool,
) -> Result<Option<DesktopActionResult>> {
    if !should_focus_before_live_action(locate, dry_run) {
        return Ok(None);
    }
    focus_app(app, &focus_options_for_live_action(locate)).map(Some)
}

fn should_focus_before_live_action(locate: &LocateOptions, dry_run: bool) -> bool {
    !dry_run && locate.image.is_none()
}

fn focus_options_for_live_action(locate: &LocateOptions) -> FocusOptions {
    FocusOptions {
        overview_wait_ms: ACTION_FOCUS_OVERVIEW_WAIT_MS,
        window_title: locate.window_title.clone(),
        window_id: locate.window_id.clone(),
        verify: true,
        ..Default::default()
    }
}

fn action_detail_with_focus(focus: Option<&DesktopActionResult>, action_detail: String) -> String {
    match focus {
        Some(focus) => format!(
            "{action_detail}; focus {} via {}",
            focus.detail, focus.backend_name
        ),
        None => action_detail,
    }
}

fn focus_diagnostics_from(focus: Option<&DesktopActionResult>) -> Vec<String> {
    focus
        .map(|result| result.focus_diagnostics.clone())
        .unwrap_or_default()
}

fn confirmed_focus_result(
    result: &DesktopActionResult,
    options: &FocusOptions,
    profile: &AppProfile,
    scope: WindowScope<'_>,
    diagnostics: &mut Vec<String>,
) -> Option<DesktopActionResult> {
    sleep_after_focus(options);
    match verify_focused_window(profile, scope) {
        Ok(detail) => {
            diagnostics.push(format!("verify: {detail}"));
            let mut result = result.clone();
            result.verified = true;
            result.verification_detail = Some(detail);
            result.focus_diagnostics = focus_diagnostics_snapshot(diagnostics);
            Some(result)
        }
        Err(error) => {
            diagnostics.push(format!("verify: {error}"));
            None
        }
    }
}

fn unconfirmed_focus_result(
    mut result: DesktopActionResult,
    diagnostics: &[String],
) -> DesktopActionResult {
    result.verification_detail = latest_focus_verification_failure(diagnostics);
    result.focus_diagnostics = focus_diagnostics_snapshot(diagnostics);
    result
}

fn latest_focus_verification_failure(diagnostics: &[String]) -> Option<String> {
    diagnostics.iter().rev().find_map(|entry| {
        entry
            .strip_prefix("verify: ")
            .map(|detail| format!("focus not confirmed: {detail}"))
    })
}

fn focus_error_with_diagnostics(message: String, diagnostics: &[String]) -> String {
    if diagnostics.is_empty() {
        return message;
    }
    format!(
        "{message}; focus diagnostics: {}",
        focus_diagnostics_snapshot(diagnostics).join(" | ")
    )
}

fn focus_diagnostics_snapshot(diagnostics: &[String]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|entry| compact_diagnostic_text(entry))
        .collect()
}

fn ensure_target_capability(profile: &AppProfile, target: &str, capability: &str) -> Result<()> {
    let supported = profile
        .supported_targets()
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(target))
        && match capability {
            "drag" => target_info(profile, target).can_drag,
            "type-into" => target_info(profile, target).can_type,
            "assert-active" => target_info(profile, target).can_assert_active,
            "assert-contains" => target_info(profile, target).can_assert_contains,
            _ => false,
        };
    if supported {
        Ok(())
    } else {
        Err(PeekabooXError::new(format!(
            "target {target:?} for app {} does not support {capability}",
            profile.id
        )))
    }
}

fn is_target_absence_error(error: &PeekabooXError) -> bool {
    let message = error.message().to_ascii_lowercase();
    message.contains("could not locate")
}

fn compact_diagnostic_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn verify_focused_window(profile: &AppProfile, scope: WindowScope<'_>) -> Result<String> {
    let metadata = peekaboox_windows::list_windows()?;
    let window = preferred_profile_window(profile, &metadata.windows, scope).ok_or_else(|| {
        PeekabooXError::new(format!(
            "could not verify focused {}; no window matched {}",
            profile.id,
            scope.description()
        ))
    })?;
    if !window.focused {
        return Err(PeekabooXError::new(format!(
            "could not verify focused {}; matched window {} is not focused",
            profile.id, window.id
        )));
    }
    Ok(format!("window {} is focused", window.id))
}

fn preferred_profile_window<'a>(
    profile: &AppProfile,
    windows: &'a [peekaboox_core::WindowInfo],
    scope: WindowScope<'_>,
) -> Option<&'a peekaboox_core::WindowInfo> {
    windows
        .iter()
        .filter(|window| {
            profile.matches_window(window)
                && window.bounds.width > 0
                && window.bounds.height > 0
                && scope
                    .window_id
                    .is_none_or(|id| window.id.eq_ignore_ascii_case(id))
                && scope
                    .title_hint
                    .is_none_or(|hint| contains_case_insensitive(&window.title, hint))
        })
        .max_by_key(|window| {
            let area = u64::from(window.bounds.width) * u64::from(window.bounds.height);
            let focus_bonus = if window.focused { u64::MAX / 2 } else { 0 };
            focus_bonus.saturating_add(area)
        })
}

fn profile_window_rect(profile: &AppProfile, scope: WindowScope<'_>) -> Option<Rect> {
    let metadata = peekaboox_windows::list_windows().ok()?;
    preferred_profile_window(profile, &metadata.windows, scope).map(|window| window.bounds)
}

fn focus_window_via_accessibility(window: &WindowInfo) -> Result<String> {
    let candidate_ids = peekaboox_accessibility::semantic_tree()
        .map(|metadata| accessibility_focus_candidate_ids(window, &metadata.elements))
        .unwrap_or_else(|_| vec![window.id.clone()]);

    let mut errors = Vec::new();
    for element_id in candidate_ids.into_iter().take(12) {
        match peekaboox_accessibility::grab_focus_by_id(&element_id) {
            Ok(result) if result.ok => {
                return Ok(format!("grabbed AT-SPI focus on {}", result.element_id));
            }
            Ok(_) => errors.push(format!("{element_id}: GrabFocus returned false")),
            Err(error) => errors.push(format!("{element_id}: {error}")),
        }
    }

    Err(PeekabooXError::new(format!(
        "could not focus {} through AT-SPI{}",
        window.id,
        if errors.is_empty() {
            String::new()
        } else {
            format!(": {}", errors.join("; "))
        }
    )))
}

fn focus_from_gnome_dock(profile: &AppProfile) -> Result<String> {
    let metadata = peekaboox_accessibility::semantic_tree()?;
    let Some((label, point)) = gnome_dock_focus_candidate(profile, &metadata.elements) else {
        return Err(PeekabooXError::new(format!(
            "could not find GNOME dock entry for {}",
            profile.id
        )));
    };
    let input = peekaboox_input::click(point, MouseButton::Left)?;
    Ok(format!(
        "clicked GNOME dock entry {} at {},{} via {}",
        label, point.x, point.y, input.backend_name
    ))
}

fn gnome_dock_focus_candidate(
    profile: &AppProfile,
    elements: &[UiElement],
) -> Option<(String, Point)> {
    elements
        .iter()
        .filter(|element| {
            element
                .app_id
                .as_deref()
                .is_some_and(|app_id| app_id.eq_ignore_ascii_case("gnome-shell"))
                && element.role.eq_ignore_ascii_case("label")
        })
        .filter_map(|element| {
            let label = element.label.as_deref()?;
            if !profile_matches_dock_label(profile, label) {
                return None;
            }
            gnome_dock_icon_point_from_label(element.bounds).map(|point| (label.to_owned(), point))
        })
        .min_by_key(|(_, point)| (point.x, point.y))
}

fn profile_matches_dock_label(profile: &AppProfile, label: &str) -> bool {
    contains_case_insensitive(label, &profile.search_name)
        || profile
            .aliases
            .iter()
            .chain(profile.desktop_ids.iter())
            .any(|alias| contains_case_insensitive(label, alias))
}

fn gnome_dock_icon_point_from_label(label: Rect) -> Option<Point> {
    if label.width == 0 || label.height == 0 {
        return None;
    }

    // Ubuntu Dock exposes a text label just to the right of the left-side icon.
    // The label's vertical center aligns with the icon center.
    if (48..=240).contains(&label.x) {
        return Some(Point::new(
            (label.x / 2).clamp(16, 48),
            label.y + i32::try_from(label.height / 2).ok()?,
        ));
    }

    None
}

fn accessibility_focus_candidate_ids(window: &WindowInfo, elements: &[UiElement]) -> Vec<String> {
    let mut candidate_ids = vec![window.id.clone()];
    for element in elements {
        if element.window_id.as_deref() == Some(window.id.as_str())
            && element.states.iter().any(|state| state == "focusable")
            && !candidate_ids.iter().any(|id| id == &element.id)
        {
            candidate_ids.push(element.id.clone());
        }
    }
    candidate_ids
}

fn normalized_title_hint(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn normalized_window_scope<'a>(
    title_hint: Option<&'a str>,
    window_id: Option<&'a str>,
) -> WindowScope<'a> {
    WindowScope {
        title_hint: normalized_title_hint(title_hint),
        window_id: normalized_title_hint(window_id),
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct WindowScope<'a> {
    title_hint: Option<&'a str>,
    window_id: Option<&'a str>,
}

impl WindowScope<'_> {
    fn has_constraints(self) -> bool {
        self.title_hint.is_some() || self.window_id.is_some()
    }

    fn description(self) -> String {
        match (self.window_id, self.title_hint) {
            (Some(window_id), Some(title_hint)) => {
                format!("window_id={window_id:?} and title containing {title_hint:?}")
            }
            (Some(window_id), None) => format!("window_id={window_id:?}"),
            (None, Some(title_hint)) => format!("title containing {title_hint:?}"),
            (None, None) => "the selected app profile".to_owned(),
        }
    }
}

fn focus_from_gnome_overview(profile: &AppProfile, options: &FocusOptions) -> Result<()> {
    activate_gnome_overview()?;
    sleep(Duration::from_millis(options.overview_wait_ms));
    let _ = focus_hotkey(["ctrl+a"]);
    sleep(Duration::from_millis(200));
    let _ = focus_hotkey(["Backspace"]);
    sleep(Duration::from_millis(200));
    peekaboox_input::type_text(profile.search_name.to_owned())?;
    sleep(Duration::from_millis(options.overview_wait_ms));

    if profile.kind == ProfileKind::Telegram {
        let frame = peekaboox_capture::capture_screen_frame()?.frame;
        let target = locate_overview_icon(&frame)?;
        peekaboox_input::click(target.point, MouseButton::Left)?;
    } else {
        focus_hotkey(["Enter"])?;
    }
    Ok(())
}

fn activate_gnome_overview() -> Result<()> {
    if command_exists("gdbus") {
        let status = Command::new("gdbus")
            .args([
                "call",
                "--session",
                "--dest",
                "org.gnome.Shell",
                "--object-path",
                "/org/gnome/Shell",
                "--method",
                "org.freedesktop.DBus.Properties.Set",
                "org.gnome.Shell",
                "OverviewActive",
                "<true>",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if status.is_ok_and(|status| status.success()) {
            return Ok(());
        }
    }

    focus_hotkey(["super"])
}

fn focus_hotkey<const N: usize>(keys: [&str; N]) -> Result<()> {
    peekaboox_input::hotkey_with_options(
        keys.into_iter().map(str::to_owned).collect(),
        peekaboox_input::HotkeyOptions {
            release_before: true,
            release_after: true,
            ..Default::default()
        },
    )
    .map(|_| ())
}

fn launch_desktop_entry(profile: &AppProfile) -> Option<String> {
    if !command_exists("gtk-launch") {
        return None;
    }

    profile.desktop_ids.iter().find_map(|id| {
        Command::new("gtk-launch")
            .arg(id)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
            .then(|| id.clone())
    })
}

fn launch_command(profile: &AppProfile) -> Option<CommandSpec> {
    for command in &profile.commands {
        if !command_exists(&command.program) {
            continue;
        }

        if Command::new(&command.program)
            .args(&command.args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
        {
            return Some(command.clone());
        }
    }

    None
}

fn clear_target(profile: &AppProfile, target: &str) -> Result<()> {
    match (profile.kind, target) {
        (ProfileKind::Telegram, "search-input") => {
            peekaboox_input::hotkey(vec!["ctrl+a".to_owned()])?;
        }
        (ProfileKind::Telegram, "message-input") => {
            peekaboox_input::hotkey(vec!["ctrl+a".to_owned()])?;
            sleep(Duration::from_millis(200));
            peekaboox_input::hotkey(vec!["Backspace".to_owned()])?;
        }
        _ => {
            peekaboox_input::hotkey(vec!["ctrl+a".to_owned()])?;
            sleep(Duration::from_millis(200));
            peekaboox_input::hotkey(vec!["Backspace".to_owned()])?;
        }
    }
    sleep(Duration::from_millis(250));
    Ok(())
}

fn load_or_capture_frame(image: Option<&Path>) -> Result<CaptureFrame> {
    match image {
        Some(path) => peekaboox_vision::load_image_file(path),
        None => peekaboox_capture::capture_screen_frame().map(|metadata| metadata.frame),
    }
}

fn target_text_contains(
    profile: &AppProfile,
    target: &str,
    expected: &str,
    image: Option<&Path>,
    window_scope: WindowScope<'_>,
) -> Result<bool> {
    let frame = load_or_capture_frame(image)?;
    let rect = profile
        .resolve_visual_target(target, &frame, window_scope)?
        .rect;

    if accessibility_contains(expected, rect).unwrap_or(false) {
        return Ok(true);
    }

    let temporary;
    let image_path = match image {
        Some(path) => path,
        None => {
            temporary = capture_temp_path()?;
            peekaboox_capture::capture_screen_to_file(&temporary)?;
            temporary.as_path()
        }
    };

    if image.is_none() {
        let result = peekaboox_vision::ocr_image_file(image_path, rect);
        let _ = std::fs::remove_file(image_path);
        return result.map(|result| contains_case_insensitive(&result.text, expected));
    }

    let result = peekaboox_vision::ocr_image_file(image_path, rect)?;
    Ok(contains_case_insensitive(&result.text, expected))
}

fn accessibility_contains(expected: &str, bounds: Option<Rect>) -> Result<bool> {
    let metadata = peekaboox_accessibility::semantic_tree()?;
    Ok(metadata.elements.iter().any(|element| {
        element
            .label
            .as_deref()
            .is_some_and(|label| contains_case_insensitive(label, expected))
            && bounds.is_none_or(|rect| rects_intersect(rect, element.bounds))
    }))
}

fn capture_temp_path() -> Result<PathBuf> {
    for _ in 0..32 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let counter = TEMP_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!(
            "peekaboox-desktop-{}-{nanos}-{counter}.png",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(_) => return Ok(path),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(PeekabooXError::new(format!(
                    "failed to create desktop OCR temporary screenshot {}: {error}",
                    path.display()
                )));
            }
        }
    }

    Err(PeekabooXError::new(
        "failed to allocate unique desktop OCR temporary screenshot",
    ))
}

fn resolve_profile(app: &str) -> Result<AppProfile> {
    let app = app.trim();
    let profiles = profile_catalog_from_paths(&desktop_profile_search_paths())?;
    if let Some(profile) = matching_profiles_for_app(&profiles, app).into_iter().next() {
        return Ok(profile.clone());
    }
    if let Some(profile) = generic_profile_for_desktop_id(app) {
        return Ok(profile);
    }

    Err(PeekabooXError::new(format!(
        "unsupported desktop app {app:?}; supported apps: {}",
        profile_ids_text(&profiles)
    )))
}

fn generic_profile_for_desktop_id(app: &str) -> Option<AppProfile> {
    let trimmed = app.trim();
    if trimmed.is_empty() || trimmed.contains('/') || !desktop_entry_exists(trimmed) {
        return None;
    }
    let desktop_id = trimmed
        .strip_suffix(".desktop")
        .unwrap_or(trimmed)
        .to_owned();
    Some(AppProfile {
        id: desktop_id.clone(),
        aliases: vec![trimmed.to_owned(), desktop_id.clone()],
        search_name: desktop_id.clone(),
        desktop_ids: vec![desktop_id],
        commands: Vec::new(),
        kind: ProfileKind::Generic,
        targets: vec![default_window_target()],
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandSpec {
    program: String,
    args: Vec<String>,
}

impl CommandSpec {
    fn display(&self) -> String {
        if self.args.is_empty() {
            self.program.clone()
        } else {
            format!("{} {}", self.program, self.args.join(" "))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProfileKind {
    Telegram,
    Paint,
    TextEditor,
    Generic,
}

#[derive(Debug, Clone, PartialEq)]
struct AppProfile {
    id: String,
    aliases: Vec<String>,
    search_name: String,
    desktop_ids: Vec<String>,
    commands: Vec<CommandSpec>,
    kind: ProfileKind,
    targets: Vec<CustomTarget>,
}

impl AppProfile {
    fn matches_id(&self, value: &str) -> bool {
        self.id.eq_ignore_ascii_case(value)
            || self
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(value))
            || self
                .desktop_ids
                .iter()
                .any(|desktop_id| desktop_id.eq_ignore_ascii_case(value))
    }

    fn matches_registry_filter(&self, value: &str) -> bool {
        self.matches_id(value)
    }

    fn matches_window(&self, window: &peekaboox_core::WindowInfo) -> bool {
        let title = window.title.as_str();
        let app_id = window.app_id.as_deref().unwrap_or_default();
        self.aliases.iter().any(|alias| {
            contains_case_insensitive(title, alias) || contains_case_insensitive(app_id, alias)
        }) || contains_case_insensitive(title, &self.search_name)
            || contains_case_insensitive(app_id, &self.search_name)
    }

    fn accessibility_selector(&self, target: &str) -> Option<&str> {
        if let Some(target) = self.custom_target(target)
            && let Some(selector) = target.accessibility_selector.as_deref()
        {
            return Some(selector);
        }

        match (self.kind, target) {
            (ProfileKind::Paint, "save-button") => Some("Save"),
            (ProfileKind::TextEditor, "save-button") => Some("Save"),
            _ => None,
        }
    }

    fn resolve_visual_target(
        &self,
        target: &str,
        frame: &CaptureFrame,
        window_scope: WindowScope<'_>,
    ) -> Result<ResolvedDesktopTarget> {
        if let Some(custom) = self.custom_target(target) {
            return self.resolve_custom_visual_target(custom, frame, window_scope);
        }

        let scoped = scoped_visual_frame(self, frame, window_scope)?;
        let visual = match (self.kind, target) {
            (ProfileKind::Telegram, "overview-icon") => locate_overview_icon(&scoped.frame)?,
            (ProfileKind::Telegram, "search-input") => locate_search_input(&scoped.frame)?,
            (ProfileKind::Telegram, "search-clear") => locate_search_clear(&scoped.frame)?,
            (ProfileKind::Telegram, "search-result") => locate_search_result(&scoped.frame)?,
            (ProfileKind::Telegram, "message-input") => locate_message_input(&scoped.frame)?,
            (ProfileKind::Telegram, "send-button") => locate_send_button(&scoped.frame)?,
            (ProfileKind::Telegram, "header") => locate_header(&scoped.frame)?,
            (ProfileKind::Paint, "canvas") => locate_paint_canvas(&scoped.frame)?,
            (ProfileKind::Paint, "save-button") => locate_paint_save_button(&scoped.frame)?,
            (ProfileKind::TextEditor, "document") => locate_text_editor_document(
                self,
                &scoped.frame,
                (!window_scope.has_constraints()).then_some(window_scope),
            )?,
            (ProfileKind::TextEditor, "save-button") => locate_text_editor_save_button(
                self,
                &scoped.frame,
                (!window_scope.has_constraints()).then_some(window_scope),
            )?,
            _ => {
                let supported_targets = self.supported_targets();
                return Err(PeekabooXError::new(format!(
                    "unsupported target {target:?} for app {}; supported targets: {}",
                    self.id,
                    supported_targets.join(", ")
                )));
            }
        };
        let visual = scoped.translate(visual);

        Ok(ResolvedDesktopTarget {
            app: self.id.to_owned(),
            target: target.to_owned(),
            point: visual.point,
            rect: visual.rect,
            source: DesktopTargetSource::VisualLayout,
        })
    }

    fn target_active(
        &self,
        target: &str,
        frame: &CaptureFrame,
        window_scope: WindowScope<'_>,
    ) -> Result<bool> {
        let scoped = scoped_visual_frame(self, frame, window_scope)?;
        match (self.kind, target) {
            (ProfileKind::Telegram, "send-button") => Ok(draft_send_button_active(&scoped.frame)?),
            _ => Err(PeekabooXError::new(format!(
                "target {target:?} does not expose an active-state guard"
            ))),
        }
    }

    fn supported_targets(&self) -> Vec<String> {
        let mut targets = match self.kind {
            ProfileKind::Telegram => telegram_supported_targets(),
            ProfileKind::Paint => paint_supported_targets(),
            ProfileKind::TextEditor => text_editor_supported_targets(),
            ProfileKind::Generic => &[],
        }
        .iter()
        .map(|target| (*target).to_owned())
        .collect::<Vec<_>>();

        for target in &self.targets {
            if !targets
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(&target.name))
            {
                targets.push(target.name.clone());
            }
        }

        targets
    }

    fn custom_target(&self, target: &str) -> Option<&CustomTarget> {
        self.targets
            .iter()
            .find(|candidate| candidate.name.eq_ignore_ascii_case(target))
    }

    fn resolve_custom_visual_target(
        &self,
        target: &CustomTarget,
        frame: &CaptureFrame,
        window_scope: WindowScope<'_>,
    ) -> Result<ResolvedDesktopTarget> {
        let base = custom_visual_frame(self, frame, window_scope)?;
        if let Some(wait) = target.wait {
            sleep(Duration::from_millis(wait.before_ms));
        }
        let visual = resolve_custom_target_visual(&base.frame, target)?;
        let visual = base.translate(visual);

        Ok(ResolvedDesktopTarget {
            app: self.id.clone(),
            target: target.name.clone(),
            point: visual.point,
            rect: visual.rect,
            source: DesktopTargetSource::VisualLayout,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
struct CustomTarget {
    name: String,
    supports: Vec<String>,
    accessibility_selector: Option<String>,
    visual: CustomTargetVisual,
    text_anchor: Option<String>,
    color_anchor: Option<CustomColorAnchor>,
    wait: Option<CustomWaitRule>,
}

impl CustomTarget {
    fn anchor_point_ratio(&self) -> (f32, f32) {
        match self.visual {
            CustomTargetVisual::Window => (0.5, 0.5),
            CustomTargetVisual::RelativeRect {
                point_x, point_y, ..
            }
            | CustomTargetVisual::OcrText {
                point_x, point_y, ..
            } => (point_x, point_y),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CustomTargetVisual {
    Window,
    RelativeRect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        point_x: f32,
        point_y: f32,
    },
    OcrText {
        region: Option<RelativeRectSpec>,
        point_x: f32,
        point_y: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct RelativeRectSpec {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CustomColorAnchor {
    red: u8,
    green: u8,
    blue: u8,
    tolerance: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CustomWaitRule {
    before_ms: u64,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DesktopProfileDocument {
    Bundle {
        schema_version: String,
        profiles: Vec<ExternalProfileDefinition>,
    },
    Single(ExternalProfileDefinition),
}

#[derive(Debug, Deserialize)]
struct ExternalProfileDefinition {
    #[serde(default)]
    schema_version: Option<String>,
    id: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    aliases: Vec<String>,
    search_name: String,
    #[serde(default)]
    desktop_ids: Vec<String>,
    #[serde(default)]
    commands: Vec<ExternalCommandDefinition>,
    #[serde(default)]
    targets: Vec<ExternalTargetDefinition>,
}

#[derive(Debug, Deserialize)]
struct ExternalCommandDefinition {
    program: String,
    #[serde(default)]
    args: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ExternalTargetDefinition {
    name: String,
    #[serde(default)]
    supports: Vec<String>,
    #[serde(default)]
    accessibility_selector: Option<String>,
    #[serde(default)]
    visual: Option<ExternalVisualTargetDefinition>,
    #[serde(default)]
    text_anchor: Option<String>,
    #[serde(default)]
    ocr: Option<ExternalTextAnchorDefinition>,
    #[serde(default)]
    color_anchor: Option<ExternalColorAnchorDefinition>,
    #[serde(default)]
    wait: Option<ExternalWaitRuleDefinition>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum ExternalVisualTargetDefinition {
    Window,
    RelativeRect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        #[serde(default = "default_target_center")]
        point_x: f32,
        #[serde(default = "default_target_center")]
        point_y: f32,
    },
    OcrText {
        #[serde(default)]
        region: Option<ExternalRelativeRectDefinition>,
        #[serde(default = "default_target_center")]
        point_x: f32,
        #[serde(default = "default_target_center")]
        point_y: f32,
    },
}

#[derive(Debug, Deserialize)]
struct ExternalRelativeRectDefinition {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[derive(Debug, Deserialize)]
struct ExternalTextAnchorDefinition {
    text: String,
}

#[derive(Debug, Deserialize)]
struct ExternalColorAnchorDefinition {
    red: u8,
    green: u8,
    blue: u8,
    #[serde(default = "default_color_anchor_tolerance")]
    tolerance: u8,
}

#[derive(Debug, Deserialize)]
struct ExternalWaitRuleDefinition {
    #[serde(default)]
    before_ms: u64,
}

const fn default_target_center() -> f32 {
    0.5
}

const fn default_color_anchor_tolerance() -> u8 {
    8
}

fn builtin_profiles() -> Vec<AppProfile> {
    vec![
        builtin_profile(
            TELEGRAM_PROFILE_ID,
            TELEGRAM_ALIASES,
            TELEGRAM_SEARCH_NAME,
            TELEGRAM_DESKTOP_IDS,
            &[
                ("telegram-desktop", &[] as &[&str]),
                ("telegram", &[]),
                ("flatpak", &["run", "org.telegram.desktop"]),
            ],
            ProfileKind::Telegram,
        ),
        builtin_profile(
            PAINT_PROFILE_ID,
            PAINT_ALIASES,
            DRAWING_SEARCH_NAME,
            PAINT_DESKTOP_IDS,
            &[
                ("drawing", &[] as &[&str]),
                ("pinta", &[]),
                ("kolourpaint", &[]),
            ],
            ProfileKind::Paint,
        ),
        builtin_profile(
            DRAWING_PROFILE_ID,
            DRAWING_ALIASES,
            DRAWING_SEARCH_NAME,
            DRAWING_DESKTOP_IDS,
            &[("drawing", &[] as &[&str])],
            ProfileKind::Paint,
        ),
        builtin_profile(
            PINTA_PROFILE_ID,
            PINTA_ALIASES,
            PINTA_SEARCH_NAME,
            PINTA_DESKTOP_IDS,
            &[("pinta", &[] as &[&str])],
            ProfileKind::Paint,
        ),
        builtin_profile(
            KOLOURPAINT_PROFILE_ID,
            KOLOURPAINT_ALIASES,
            KOLOURPAINT_SEARCH_NAME,
            KOLOURPAINT_DESKTOP_IDS,
            &[("kolourpaint", &[] as &[&str])],
            ProfileKind::Paint,
        ),
        builtin_profile(
            TEXT_EDITOR_PROFILE_ID,
            TEXT_EDITOR_ALIASES,
            TEXT_EDITOR_SEARCH_NAME,
            TEXT_EDITOR_DESKTOP_IDS,
            &[("gnome-text-editor", &[] as &[&str])],
            ProfileKind::TextEditor,
        ),
        calendar_profile(),
        generic_builtin_profile(
            BROWSER_PROFILE_ID,
            BROWSER_ALIASES,
            BROWSER_SEARCH_NAME,
            BROWSER_DESKTOP_IDS,
            &[
                ("xdg-open", &["about:blank"] as &[&str]),
                ("firefox", &[]),
                ("google-chrome", &[]),
                ("chromium", &[]),
            ],
        ),
        generic_builtin_profile(
            FILES_PROFILE_ID,
            FILES_ALIASES,
            FILES_SEARCH_NAME,
            FILES_DESKTOP_IDS,
            &[
                ("nautilus", &[] as &[&str]),
                ("thunar", &[]),
                ("dolphin", &[]),
            ],
        ),
        generic_builtin_profile(
            TERMINAL_PROFILE_ID,
            TERMINAL_ALIASES,
            TERMINAL_SEARCH_NAME,
            TERMINAL_DESKTOP_IDS,
            &[
                ("gnome-terminal", &[] as &[&str]),
                ("kgx", &[]),
                ("konsole", &[]),
                ("x-terminal-emulator", &[]),
            ],
        ),
        generic_builtin_profile(
            OFFICE_PROFILE_ID,
            OFFICE_ALIASES,
            OFFICE_SEARCH_NAME,
            OFFICE_DESKTOP_IDS,
            &[
                ("libreoffice", &[] as &[&str]),
                ("libreoffice", &["--writer"]),
                ("libreoffice", &["--calc"]),
            ],
        ),
    ]
}

fn builtin_profile(
    id: &str,
    aliases: &[&str],
    search_name: &str,
    desktop_ids: &[&str],
    commands: &[(&str, &[&str])],
    kind: ProfileKind,
) -> AppProfile {
    AppProfile {
        id: id.to_owned(),
        aliases: string_vec(aliases),
        search_name: search_name.to_owned(),
        desktop_ids: string_vec(desktop_ids),
        commands: commands
            .iter()
            .map(|(program, args)| CommandSpec {
                program: (*program).to_owned(),
                args: string_vec(args),
            })
            .collect(),
        kind,
        targets: Vec::new(),
    }
}

fn string_vec(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn generic_builtin_profile(
    id: &str,
    aliases: &[&str],
    search_name: &str,
    desktop_ids: &[&str],
    commands: &[(&str, &[&str])],
) -> AppProfile {
    let mut profile = builtin_profile(
        id,
        aliases,
        search_name,
        desktop_ids,
        commands,
        ProfileKind::Generic,
    );
    profile.targets.push(default_window_target());
    profile
}

fn calendar_profile() -> AppProfile {
    AppProfile {
        id: CALENDAR_PROFILE_ID.to_owned(),
        aliases: string_vec(CALENDAR_ALIASES),
        search_name: CALENDAR_SEARCH_NAME.to_owned(),
        desktop_ids: string_vec(CALENDAR_DESKTOP_IDS),
        commands: vec![
            CommandSpec {
                program: "gnome-calendar".to_owned(),
                args: Vec::new(),
            },
            CommandSpec {
                program: "flatpak".to_owned(),
                args: string_vec(&["run", "org.gnome.Calendar"]),
            },
        ],
        kind: ProfileKind::Generic,
        targets: vec![
            default_window_target(),
            calendar_accessibility_target(
                "new-event-button",
                &["click", "assert-present"],
                "role=push button,label-regex=New Event|New event|Create Event|Create event|Add Event|Add event",
                "New",
                Some(RelativeRectSpec {
                    x: 0.0,
                    y: 0.0,
                    width: 1.0,
                    height: 0.22,
                }),
            ),
            calendar_accessibility_target(
                "edit-details-button",
                &["click", "assert-present"],
                "role=push button,label-regex=Edit Details|Edit details",
                "Edit Details",
                None,
            ),
            calendar_accessibility_target(
                "save-event-button",
                &["click", "assert-present"],
                "role=push button,label-regex=Save Event|Save event|Save",
                "Save",
                Some(RelativeRectSpec {
                    x: 0.45,
                    y: 0.0,
                    width: 0.55,
                    height: 0.35,
                }),
            ),
            calendar_accessibility_target(
                "save-button",
                &["click", "assert-present"],
                "role=push button,label-regex=Save Event|Save event|Save",
                "Save",
                Some(RelativeRectSpec {
                    x: 0.45,
                    y: 0.0,
                    width: 0.55,
                    height: 0.35,
                }),
            ),
            calendar_accessibility_target(
                "title-field",
                &["click", "type-into", "assert-present", "assert-contains"],
                "role-regex=text|entry,label-regex=Title|Summary|Event",
                "Title",
                None,
            ),
        ],
    }
}

fn default_window_target() -> CustomTarget {
    CustomTarget {
        name: "window".to_owned(),
        supports: string_vec(&["locate", "click", "drag", "assert-present"]),
        accessibility_selector: None,
        visual: CustomTargetVisual::Window,
        text_anchor: None,
        color_anchor: None,
        wait: None,
    }
}

fn calendar_accessibility_target(
    name: &str,
    supports: &[&str],
    accessibility_selector: &str,
    text_anchor: &str,
    region: Option<RelativeRectSpec>,
) -> CustomTarget {
    CustomTarget {
        name: name.to_owned(),
        supports: string_vec(supports),
        accessibility_selector: Some(accessibility_selector.to_owned()),
        visual: CustomTargetVisual::OcrText {
            region,
            point_x: 0.5,
            point_y: 0.5,
        },
        text_anchor: Some(text_anchor.to_owned()),
        color_anchor: None,
        wait: None,
    }
}

fn desktop_profile_search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(value) = env::var_os(DESKTOP_PROFILE_PATH_ENV) {
        paths.extend(env::split_paths(&value));
    }
    if let Some(value) = env::var_os("XDG_CONFIG_HOME") {
        paths.push(PathBuf::from(value).join("peekaboox/desktop-profiles"));
    } else if let Some(home) = env::var_os("HOME") {
        paths.push(
            PathBuf::from(home)
                .join(".config")
                .join("peekaboox/desktop-profiles"),
        );
    }
    paths.push(PathBuf::from("/etc/peekaboox/desktop-profiles"));
    paths.push(PathBuf::from("/usr/share/peekaboox/desktop-profiles"));
    dedupe_paths(paths)
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for path in paths {
        if seen.insert(path.clone()) {
            deduped.push(path);
        }
    }
    deduped
}

fn load_external_profiles(paths: &[PathBuf]) -> Result<Vec<AppProfile>> {
    let mut profiles = Vec::new();
    for path in expand_profile_paths(paths)? {
        profiles.extend(load_profile_document(&path)?);
    }
    Ok(profiles)
}

fn expand_profile_paths(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut expanded = Vec::new();
    for path in paths {
        if path.is_dir() {
            let mut files = fs::read_dir(path)
                .map_err(|error| {
                    PeekabooXError::new(format!(
                        "failed to read desktop profile directory {}: {error}",
                        path.display()
                    ))
                })?
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|candidate| {
                    candidate
                        .extension()
                        .is_some_and(|extension| extension == "json")
                })
                .collect::<Vec<_>>();
            files.sort();
            expanded.extend(files);
        } else if path.is_file() {
            expanded.push(path.clone());
        }
    }
    Ok(expanded)
}

fn load_profile_document(path: &Path) -> Result<Vec<AppProfile>> {
    let data = fs::read_to_string(path).map_err(|error| {
        PeekabooXError::new(format!(
            "failed to read desktop profile file {}: {error}",
            path.display()
        ))
    })?;
    let document = serde_json::from_str::<DesktopProfileDocument>(&data).map_err(|error| {
        PeekabooXError::new(format!(
            "failed to parse desktop profile file {}: {error}",
            path.display()
        ))
    })?;

    match document {
        DesktopProfileDocument::Bundle {
            schema_version,
            profiles,
        } => {
            validate_profile_schema(path, Some(&schema_version))?;
            profiles
                .into_iter()
                .map(|profile| external_profile(path, profile))
                .collect()
        }
        DesktopProfileDocument::Single(profile) => {
            validate_profile_schema(path, profile.schema_version.as_deref())?;
            Ok(vec![external_profile(path, profile)?])
        }
    }
}

fn validate_profile_schema(path: &Path, schema_version: Option<&str>) -> Result<()> {
    match schema_version {
        Some(DESKTOP_PROFILE_FILE_SCHEMA_VERSION) => Ok(()),
        Some(version) => Err(PeekabooXError::new(format!(
            "unsupported desktop profile schema_version {version:?} in {}; expected {DESKTOP_PROFILE_FILE_SCHEMA_VERSION}",
            path.display()
        ))),
        None => Err(PeekabooXError::new(format!(
            "missing desktop profile schema_version in {}; expected {DESKTOP_PROFILE_FILE_SCHEMA_VERSION}",
            path.display()
        ))),
    }
}

fn external_profile(path: &Path, definition: ExternalProfileDefinition) -> Result<AppProfile> {
    validate_profile_id(path, &definition.id)?;
    let kind = parse_profile_kind(path, definition.kind.as_deref())?;
    let mut targets = definition
        .targets
        .into_iter()
        .map(|target| external_target(path, target))
        .collect::<Result<Vec<_>>>()?;
    if kind == ProfileKind::Generic
        && !targets
            .iter()
            .any(|target| target.name.eq_ignore_ascii_case("window"))
    {
        targets.push(default_window_target());
    }

    Ok(AppProfile {
        id: definition.id.trim().to_owned(),
        aliases: normalized_string_list(definition.aliases),
        search_name: definition.search_name.trim().to_owned(),
        desktop_ids: normalized_string_list(definition.desktop_ids),
        commands: definition
            .commands
            .into_iter()
            .map(|command| external_command(path, command))
            .collect::<Result<Vec<_>>>()?,
        kind,
        targets,
    })
}

fn validate_profile_id(path: &Path, id: &str) -> Result<()> {
    let id = id.trim();
    if id.is_empty() {
        return Err(PeekabooXError::new(format!(
            "desktop profile in {} has an empty id",
            path.display()
        )));
    }
    if id.contains(char::is_whitespace) {
        return Err(PeekabooXError::new(format!(
            "desktop profile id {id:?} in {} must not contain whitespace",
            path.display()
        )));
    }
    Ok(())
}

fn parse_profile_kind(path: &Path, kind: Option<&str>) -> Result<ProfileKind> {
    match kind
        .map(str::trim)
        .filter(|kind| !kind.is_empty())
        .unwrap_or("generic")
        .replace('_', "-")
        .to_ascii_lowercase()
        .as_str()
    {
        "telegram" => Ok(ProfileKind::Telegram),
        "paint" | "drawing" | "pinta" | "kolourpaint" => Ok(ProfileKind::Paint),
        "text-editor" | "texteditor" | "gnome-text-editor" => Ok(ProfileKind::TextEditor),
        "generic" | "window" => Ok(ProfileKind::Generic),
        other => Err(PeekabooXError::new(format!(
            "unsupported desktop profile kind {other:?} in {}; expected generic, telegram, paint, or text-editor",
            path.display()
        ))),
    }
}

fn external_command(path: &Path, command: ExternalCommandDefinition) -> Result<CommandSpec> {
    let program = command.program.trim();
    if program.is_empty() {
        return Err(PeekabooXError::new(format!(
            "desktop profile command in {} has an empty program",
            path.display()
        )));
    }
    Ok(CommandSpec {
        program: program.to_owned(),
        args: normalized_string_list(command.args),
    })
}

fn external_target(path: &Path, target: ExternalTargetDefinition) -> Result<CustomTarget> {
    let name = target.name.trim();
    if name.is_empty() {
        return Err(PeekabooXError::new(format!(
            "desktop profile target in {} has an empty name",
            path.display()
        )));
    }
    let visual = match target.visual {
        Some(ExternalVisualTargetDefinition::Window) | None => CustomTargetVisual::Window,
        Some(ExternalVisualTargetDefinition::RelativeRect {
            x,
            y,
            width,
            height,
            point_x,
            point_y,
        }) => CustomTargetVisual::RelativeRect {
            x,
            y,
            width,
            height,
            point_x,
            point_y,
        },
        Some(ExternalVisualTargetDefinition::OcrText {
            region,
            point_x,
            point_y,
        }) => CustomTargetVisual::OcrText {
            region: region.map(relative_rect_spec_from_external),
            point_x,
            point_y,
        },
    };
    validate_relative_rect(path, name, &visual)?;
    let text_anchor = target
        .ocr
        .map(|anchor| anchor.text)
        .or(target.text_anchor)
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty());
    if matches!(visual, CustomTargetVisual::OcrText { .. }) && text_anchor.is_none() {
        return Err(PeekabooXError::new(format!(
            "desktop profile target {name:?} in {} uses visual.type=ocr-text without text_anchor or ocr.text",
            path.display()
        )));
    }
    let color_anchor = target.color_anchor.map(|anchor| CustomColorAnchor {
        red: anchor.red,
        green: anchor.green,
        blue: anchor.blue,
        tolerance: anchor.tolerance,
    });
    let wait = target.wait.map(|wait| CustomWaitRule {
        before_ms: wait.before_ms,
    });
    Ok(CustomTarget {
        name: name.to_owned(),
        supports: normalized_supports(target.supports),
        accessibility_selector: target
            .accessibility_selector
            .map(|selector| selector.trim().to_owned())
            .filter(|selector| !selector.is_empty()),
        visual,
        text_anchor,
        color_anchor,
        wait,
    })
}

fn validate_relative_rect(path: &Path, target: &str, visual: &CustomTargetVisual) -> Result<()> {
    match *visual {
        CustomTargetVisual::Window => Ok(()),
        CustomTargetVisual::RelativeRect {
            x,
            y,
            width,
            height,
            point_x,
            point_y,
        } => {
            validate_relative_rect_spec(
                path,
                target,
                RelativeRectSpec {
                    x,
                    y,
                    width,
                    height,
                },
            )?;
            validate_target_point_ratio(path, target, point_x, "point_x")?;
            validate_target_point_ratio(path, target, point_y, "point_y")
        }
        CustomTargetVisual::OcrText {
            region,
            point_x,
            point_y,
        } => {
            if let Some(region) = region {
                validate_relative_rect_spec(path, target, region)?;
            }
            validate_target_point_ratio(path, target, point_x, "point_x")?;
            validate_target_point_ratio(path, target, point_y, "point_y")
        }
    }
}

fn validate_relative_rect_spec(path: &Path, target: &str, spec: RelativeRectSpec) -> Result<()> {
    for (name, value) in [
        ("x", spec.x),
        ("y", spec.y),
        ("width", spec.width),
        ("height", spec.height),
    ] {
        if !value.is_finite() {
            return Err(PeekabooXError::new(format!(
                "desktop profile target {target:?} in {} has non-finite {name}",
                path.display()
            )));
        }
    }
    if spec.x < 0.0
        || spec.y < 0.0
        || spec.width <= 0.0
        || spec.height <= 0.0
        || spec.x + spec.width > 1.0
        || spec.y + spec.height > 1.0
    {
        return Err(PeekabooXError::new(format!(
            "desktop profile target {target:?} in {} has invalid relative rectangle; values must stay within 0.0..1.0",
            path.display()
        )));
    }
    Ok(())
}

fn validate_target_point_ratio(path: &Path, target: &str, value: f32, name: &str) -> Result<()> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(PeekabooXError::new(format!(
            "desktop profile target {target:?} in {} has invalid {name}; expected 0.0..1.0",
            path.display()
        )));
    }
    Ok(())
}

fn relative_rect_spec_from_external(value: ExternalRelativeRectDefinition) -> RelativeRectSpec {
    RelativeRectSpec {
        x: value.x,
        y: value.y,
        width: value.width,
        height: value.height,
    }
}

fn normalized_string_list(values: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in values {
        let value = value.trim();
        if !value.is_empty()
            && !normalized
                .iter()
                .any(|candidate: &String| candidate.eq_ignore_ascii_case(value))
        {
            normalized.push(value.to_owned());
        }
    }
    normalized
}

fn normalized_supports(values: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in values {
        let value = value.trim().replace('_', "-").to_ascii_lowercase();
        if !value.is_empty()
            && !normalized
                .iter()
                .any(|candidate: &String| candidate.eq_ignore_ascii_case(&value))
        {
            normalized.push(value);
        }
    }
    normalized
}

fn upsert_profile(profiles: &mut Vec<AppProfile>, profile: AppProfile) {
    if let Some(existing) = profiles
        .iter_mut()
        .find(|candidate| candidate.id.eq_ignore_ascii_case(&profile.id))
    {
        *existing = profile;
    } else {
        profiles.push(profile);
    }
}

fn profile_ids_text(profiles: &[AppProfile]) -> String {
    profiles
        .iter()
        .map(|profile| profile.id.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn custom_target_info(target: &CustomTarget) -> DesktopProfileTargetInfo {
    let visual_rect = true;
    let mut supports = vec![
        "locate".to_owned(),
        "click".to_owned(),
        "assert-present".to_owned(),
        "visual-layout".to_owned(),
    ];
    if target.accessibility_selector.is_some() {
        supports.push("accessibility".to_owned());
    }
    if target.text_anchor.is_some() || matches!(target.visual, CustomTargetVisual::OcrText { .. }) {
        supports.push("ocr".to_owned());
        supports.push("text-anchor".to_owned());
    }
    if target.color_anchor.is_some() {
        supports.push("color-anchor".to_owned());
    }
    if target.wait.is_some() {
        supports.push("wait".to_owned());
    }
    for support in &target.supports {
        if !supports
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(support))
        {
            supports.push(support.clone());
        }
    }

    let can_drag = supports_contains(&supports, "drag");
    let can_type = supports_contains(&supports, "type-into");
    let can_assert_active = false;
    let can_assert_contains = supports_contains(&supports, "assert-contains");

    let mut sources = vec!["visual-layout".to_owned()];
    if target.accessibility_selector.is_some() {
        sources.push("accessibility".to_owned());
    }
    if target.text_anchor.is_some() || matches!(target.visual, CustomTargetVisual::OcrText { .. }) {
        sources.push("ocr".to_owned());
    }
    if target.color_anchor.is_some() {
        sources.push("color-anchor".to_owned());
    }

    DesktopProfileTargetInfo {
        name: target.name.clone(),
        supports,
        sources,
        can_locate: true,
        can_click: true,
        can_drag,
        can_type,
        can_assert_present: true,
        can_assert_active,
        can_assert_contains,
        accessibility_selector: target.accessibility_selector.clone(),
        visual_layout: true,
        visual_rect,
    }
}

fn supports_contains(supports: &[String], expected: &str) -> bool {
    supports
        .iter()
        .any(|support| support.eq_ignore_ascii_case(expected))
}

fn custom_visual_frame(
    profile: &AppProfile,
    frame: &CaptureFrame,
    scope: WindowScope<'_>,
) -> Result<ScopedFrame> {
    if let Some(rect) = profile_window_rect(profile, scope) {
        return Ok(ScopedFrame {
            frame: crop_frame(frame, rect)?,
            offset: Point::new(max(0, rect.x), max(0, rect.y)),
        });
    }
    if scope.has_constraints() {
        return Err(PeekabooXError::new(format!(
            "could not locate visible {} window matching {}",
            profile.id,
            scope.description()
        )));
    }
    Ok(ScopedFrame {
        frame: frame.clone(),
        offset: Point::new(0, 0),
    })
}

fn resolve_custom_target_visual(
    frame: &CaptureFrame,
    target: &CustomTarget,
) -> Result<VisualTarget> {
    let rect = custom_visual_rect(frame, target.visual)?;
    if let Some(anchor) = target.color_anchor {
        let point = find_color_anchor(frame, rect, anchor).ok_or_else(|| {
            PeekabooXError::new(format!(
                "desktop profile target {:?} color anchor was not found",
                target.name
            ))
        })?;
        return Ok(VisualTarget::with_rect(point, rect));
    }
    if let Some(text) = target.text_anchor.as_deref() {
        if let Some(visual) = locate_text_anchor(frame, text, rect, target.anchor_point_ratio())? {
            return Ok(visual);
        }
        if matches!(target.visual, CustomTargetVisual::OcrText { .. }) {
            return Err(PeekabooXError::new(format!(
                "desktop profile target {:?} OCR text anchor {:?} was not found",
                target.name, text
            )));
        }
    }
    let point = point_in_rect_ratio(rect, target.anchor_point_ratio())?;
    Ok(VisualTarget::with_rect(point, rect))
}

fn custom_visual_rect(frame: &CaptureFrame, visual: CustomTargetVisual) -> Result<Rect> {
    match visual {
        CustomTargetVisual::Window => Ok(Rect::new(0, 0, frame.width, frame.height)),
        CustomTargetVisual::RelativeRect {
            x,
            y,
            width,
            height,
            ..
        } => relative_rect(frame, x, y, width, height),
        CustomTargetVisual::OcrText { region, .. } => region
            .map(|spec| relative_rect_from_spec(frame, spec))
            .unwrap_or_else(|| Ok(Rect::new(0, 0, frame.width, frame.height))),
    }
}

fn locate_text_anchor(
    frame: &CaptureFrame,
    text: &str,
    region: Rect,
    fallback_ratio: (f32, f32),
) -> Result<Option<VisualTarget>> {
    let image_path = capture_temp_path()?;
    let result = (|| {
        peekaboox_capture::write_frame_png(frame, &image_path)?;
        peekaboox_vision::ocr_image_file(&image_path, Some(region))
    })();
    let _ = std::fs::remove_file(&image_path);
    let result = result?;
    if let Some(found) = result
        .words
        .iter()
        .chain(result.blocks.iter())
        .find(|candidate| contains_case_insensitive(&candidate.text, text))
    {
        let rect = found.element.bounds;
        return Ok(Some(VisualTarget::with_rect(
            point_in_rect_ratio(rect, (0.5, 0.5))?,
            rect,
        )));
    }
    if contains_case_insensitive(&result.text, text) {
        return Ok(Some(VisualTarget::with_rect(
            point_in_rect_ratio(region, fallback_ratio)?,
            region,
        )));
    }
    Ok(None)
}

fn find_color_anchor(frame: &CaptureFrame, rect: Rect, anchor: CustomColorAnchor) -> Option<Point> {
    let view = FrameView::new(frame);
    let left = rect.x.max(0);
    let top = rect.y.max(0);
    let right = (rect.x + i32::try_from(rect.width).ok()?).min(view.width());
    let bottom = (rect.y + i32::try_from(rect.height).ok()?).min(view.height());
    for y in top..bottom {
        for x in left..right {
            let (red, green, blue) = view.pixel(x, y);
            if red.abs_diff(anchor.red) <= anchor.tolerance
                && green.abs_diff(anchor.green) <= anchor.tolerance
                && blue.abs_diff(anchor.blue) <= anchor.tolerance
            {
                return Some(Point::new(x, y));
            }
        }
    }
    None
}

fn relative_rect_from_spec(frame: &CaptureFrame, spec: RelativeRectSpec) -> Result<Rect> {
    relative_rect(frame, spec.x, spec.y, spec.width, spec.height)
}

fn relative_rect(frame: &CaptureFrame, x: f32, y: f32, width: f32, height: f32) -> Result<Rect> {
    validate_runtime_ratio(x, "x")?;
    validate_runtime_ratio(y, "y")?;
    validate_runtime_ratio(width, "width")?;
    validate_runtime_ratio(height, "height")?;
    if width <= 0.0 || height <= 0.0 || x + width > 1.0 || y + height > 1.0 {
        return Err(PeekabooXError::new(
            "relative target rectangle must be positive and contained in the base frame",
        ));
    }

    let frame_width = frame.width as f32;
    let frame_height = frame.height as f32;
    let left = (frame_width * x).round() as i32;
    let top = (frame_height * y).round() as i32;
    let right = (frame_width * (x + width)).round() as i32;
    let bottom = (frame_height * (y + height)).round() as i32;
    Ok(Rect::new(
        left,
        top,
        positive_extent(right - left),
        positive_extent(bottom - top),
    ))
}

fn validate_runtime_ratio(value: f32, name: &str) -> Result<()> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(PeekabooXError::new(format!(
            "relative target {name} must be finite and between 0.0 and 1.0"
        )))
    }
}

fn telegram_supported_targets() -> &'static [&'static str] {
    &[
        "overview-icon",
        "search-input",
        "search-clear",
        "search-result",
        "message-input",
        "send-button",
        "header",
    ]
}

fn paint_supported_targets() -> &'static [&'static str] {
    &["canvas", "save-button"]
}

fn text_editor_supported_targets() -> &'static [&'static str] {
    &["document", "save-button"]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VisualTarget {
    point: Point,
    rect: Option<Rect>,
}

impl VisualTarget {
    const fn point(point: Point) -> Self {
        Self { point, rect: None }
    }

    const fn with_rect(point: Point, rect: Rect) -> Self {
        Self {
            point,
            rect: Some(rect),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ScopedFrame {
    frame: CaptureFrame,
    offset: Point,
}

impl ScopedFrame {
    fn translate(&self, target: VisualTarget) -> VisualTarget {
        VisualTarget {
            point: Point::new(
                target.point.x + self.offset.x,
                target.point.y + self.offset.y,
            ),
            rect: target.rect.map(|rect| translate_rect(rect, self.offset)),
        }
    }
}

fn scoped_visual_frame(
    profile: &AppProfile,
    frame: &CaptureFrame,
    scope: WindowScope<'_>,
) -> Result<ScopedFrame> {
    if !scope.has_constraints() {
        return Ok(ScopedFrame {
            frame: frame.clone(),
            offset: Point::new(0, 0),
        });
    }

    let rect = profile_window_rect(profile, scope).ok_or_else(|| {
        PeekabooXError::new(format!(
            "could not locate visible {} window matching {}",
            profile.id,
            scope.description()
        ))
    })?;
    Ok(ScopedFrame {
        frame: crop_frame(frame, rect)?,
        offset: Point::new(max(0, rect.x), max(0, rect.y)),
    })
}

fn crop_frame(frame: &CaptureFrame, rect: Rect) -> Result<CaptureFrame> {
    let bytes_per_pixel = bytes_per_pixel(frame.format);
    let frame_width = i32::try_from(frame.width).unwrap_or(i32::MAX);
    let frame_height = i32::try_from(frame.height).unwrap_or(i32::MAX);
    let left = rect.x.clamp(0, frame_width);
    let top = rect.y.clamp(0, frame_height);
    let right = (rect.x + i32::try_from(rect.width).unwrap_or(i32::MAX)).clamp(0, frame_width);
    let bottom = (rect.y + i32::try_from(rect.height).unwrap_or(i32::MAX)).clamp(0, frame_height);
    if right <= left || bottom <= top {
        return Err(PeekabooXError::new(format!(
            "window crop is outside captured frame: {},{},{}x{}",
            rect.x, rect.y, rect.width, rect.height
        )));
    }

    let width = u32::try_from(right - left).unwrap_or(0);
    let height = u32::try_from(bottom - top).unwrap_or(0);
    let stride = width.saturating_mul(u32::try_from(bytes_per_pixel).unwrap_or(4));
    let mut data = Vec::with_capacity(usize::try_from(stride.saturating_mul(height)).unwrap_or(0));
    for row in top..bottom {
        let start = usize::try_from(row)
            .unwrap_or(0)
            .saturating_mul(usize::try_from(frame.stride).unwrap_or(0))
            .saturating_add(
                usize::try_from(left)
                    .unwrap_or(0)
                    .saturating_mul(bytes_per_pixel),
            );
        let end = start.saturating_add(usize::try_from(stride).unwrap_or(0));
        let Some(slice) = frame.data.get(start..end) else {
            return Err(PeekabooXError::new(
                "window crop exceeds captured frame buffer",
            ));
        };
        data.extend_from_slice(slice);
    }

    Ok(CaptureFrame {
        width,
        height,
        stride,
        format: frame.format,
        data,
    })
}

fn bytes_per_pixel(format: PixelFormat) -> usize {
    match format {
        PixelFormat::Rgb8 => 3,
        PixelFormat::Rgba8 | PixelFormat::Bgra8 => 4,
    }
}

fn translate_rect(rect: Rect, offset: Point) -> Rect {
    Rect::new(
        rect.x + offset.x,
        rect.y + offset.y,
        rect.width,
        rect.height,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Component {
    pixels: u32,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl Component {
    const fn center_x(self) -> i32 {
        (self.left + self.right) / 2
    }

    const fn center_y(self) -> i32 {
        (self.top + self.bottom) / 2
    }

    const fn rect(self) -> Rect {
        Rect::new(
            self.left,
            self.top,
            positive_extent(self.right - self.left),
            positive_extent(self.bottom - self.top),
        )
    }
}

struct FrameView<'a> {
    frame: &'a CaptureFrame,
}

impl<'a> FrameView<'a> {
    const fn new(frame: &'a CaptureFrame) -> Self {
        Self { frame }
    }

    const fn width(&self) -> i32 {
        self.frame.width as i32
    }

    const fn height(&self) -> i32 {
        self.frame.height as i32
    }

    fn pixel(&self, x: i32, y: i32) -> (u8, u8, u8) {
        if x < 0 || y < 0 || x >= self.width() || y >= self.height() {
            return (0, 0, 0);
        }

        let bytes_per_pixel = match self.frame.format {
            PixelFormat::Rgb8 => 3_usize,
            PixelFormat::Rgba8 | PixelFormat::Bgra8 => 4_usize,
        };
        let stride = if self.frame.stride == 0 {
            usize::try_from(self.frame.width).unwrap_or(0) * bytes_per_pixel
        } else {
            usize::try_from(self.frame.stride).unwrap_or(0)
        };
        let index = usize::try_from(y).unwrap_or(0) * stride
            + usize::try_from(x).unwrap_or(0) * bytes_per_pixel;
        if index + bytes_per_pixel > self.frame.data.len() {
            return (0, 0, 0);
        }

        match self.frame.format {
            PixelFormat::Rgb8 | PixelFormat::Rgba8 => (
                self.frame.data[index],
                self.frame.data[index + 1],
                self.frame.data[index + 2],
            ),
            PixelFormat::Bgra8 => (
                self.frame.data[index + 2],
                self.frame.data[index + 1],
                self.frame.data[index],
            ),
        }
    }
}

fn locate_overview_icon(frame: &CaptureFrame) -> Result<VisualTarget> {
    let view = FrameView::new(frame);
    let candidates = connected_components(&view, is_telegram_blue, 2)
        .into_iter()
        .filter(|component| {
            component.pixels >= 300
                && component.top >= view.height() * 8 / 100
                && component.bottom <= view.height() * 65 / 100
        })
        .collect::<Vec<_>>();

    let Some(target) = candidates.into_iter().max_by_key(|component| {
        let center_penalty = (component.center_x() - view.width() / 2).abs()
            + (component.center_y() - view.height() * 22 / 100).abs();
        i64::from(component.pixels) - i64::from(center_penalty)
    }) else {
        return Err(PeekabooXError::new(
            "could not locate Telegram result in GNOME overview",
        ));
    };

    Ok(VisualTarget::with_rect(
        Point::new(target.center_x(), target.center_y()),
        target.rect(),
    ))
}

fn locate_search_input(frame: &CaptureFrame) -> Result<VisualTarget> {
    let metrics = locate_window_metrics(frame)?;
    Ok(VisualTarget::with_rect(
        Point::new(metrics.left + 250, metrics.top + 50),
        Rect::new(metrics.left + 82, metrics.top + 31, 330, 38),
    ))
}

fn locate_search_clear(frame: &CaptureFrame) -> Result<VisualTarget> {
    let metrics = locate_window_metrics(frame)?;
    Ok(VisualTarget::point(Point::new(
        metrics.left + 380,
        metrics.top + 50,
    )))
}

fn locate_search_result(frame: &CaptureFrame) -> Result<VisualTarget> {
    let metrics = locate_window_metrics(frame)?;
    Ok(VisualTarget::with_rect(
        Point::new(metrics.left + 178, metrics.top + 108),
        Rect::new(metrics.left + 70, metrics.top + 76, 350, 70),
    ))
}

fn locate_message_input(frame: &CaptureFrame) -> Result<VisualTarget> {
    let bar = locate_message_bar(frame)?;
    let width = bar.right - bar.left;
    Ok(VisualTarget::with_rect(
        Point::new(bar.left + width * 42 / 100, max(0, bar.y - 17)),
        Rect::new(
            bar.left + 58,
            max(0, bar.y - 38),
            positive_extent(width - 140),
            44,
        ),
    ))
}

fn locate_send_button(frame: &CaptureFrame) -> Result<VisualTarget> {
    let bar = locate_message_bar(frame)?;
    Ok(VisualTarget::with_rect(
        Point::new(max(0, bar.right - 24), max(0, bar.y - 17)),
        Rect::new(max(0, bar.right - 60), max(0, bar.y - 45), 60, 55),
    ))
}

fn locate_header(frame: &CaptureFrame) -> Result<VisualTarget> {
    let metrics = locate_window_metrics(frame)?;
    let left = metrics.left + 410;
    let width = positive_extent(metrics.right - left - 90);
    Ok(VisualTarget::with_rect(
        Point::new(
            left + i32::try_from(width / 2).unwrap_or(0),
            metrics.top + 50,
        ),
        Rect::new(left, metrics.top + 16, width, 70),
    ))
}

fn locate_paint_canvas(frame: &CaptureFrame) -> Result<VisualTarget> {
    let view = FrameView::new(frame);
    if let Some(rect) = locate_paint_canvas_outline(&view) {
        return Ok(VisualTarget::with_rect(
            point_in_rect_ratio(rect, (0.35, 0.35))?,
            rect,
        ));
    }

    let min_width = max(180, view.width() * 12 / 100);
    let min_height = max(160, view.height() * 12 / 100);
    let max_area = i64::from(view.width()) * i64::from(view.height()) * 92 / 100;

    let candidates = connected_components(&view, is_paint_canvas_pixel, 4)
        .into_iter()
        .filter(|component| {
            let width = component.right - component.left;
            let height = component.bottom - component.top;
            let area = i64::from(width) * i64::from(height);
            width >= min_width
                && height >= min_height
                && component.top >= view.height() * 6 / 100
                && component.bottom >= view.height() * 30 / 100
                && component.left <= view.width() * 90 / 100
                && component.right >= view.width() * 10 / 100
                && area <= max_area
        })
        .collect::<Vec<_>>();

    let Some(canvas) = candidates.into_iter().max_by_key(|component| {
        let width = component.right - component.left;
        let height = component.bottom - component.top;
        let area = i64::from(width) * i64::from(height);
        let center_penalty = (component.center_x() - view.width() / 2).abs()
            + (component.center_y() - view.height() * 58 / 100).abs();
        area - i64::from(center_penalty * 20)
    }) else {
        return Err(PeekabooXError::new("could not locate paint canvas"));
    };

    let rect = canvas.rect();
    Ok(VisualTarget::with_rect(
        point_in_rect_ratio(rect, (0.35, 0.35))?,
        rect,
    ))
}

fn locate_paint_canvas_outline(view: &FrameView<'_>) -> Option<Rect> {
    let min_width = max(220, view.width() * 12 / 100);
    let min_height = max(160, view.height() * 12 / 100);
    connected_components(view, is_paint_canvas_outline_pixel, 2)
        .into_iter()
        .filter(|component| {
            let width = component.right - component.left;
            let height = component.bottom - component.top;
            let aspect = f64::from(width) / f64::from(max(1, height));
            width >= min_width
                && height >= min_height
                && (0.5..=2.2).contains(&aspect)
                && component.pixels >= u32::try_from(width + height).unwrap_or(u32::MAX)
                && component.top >= view.height() * 8 / 100
                && component.bottom <= view.height() * 94 / 100
                && component.left <= view.width() * 85 / 100
                && component.right >= view.width() * 12 / 100
        })
        .max_by_key(|component| {
            let width = component.right - component.left;
            let height = component.bottom - component.top;
            let area = i64::from(width) * i64::from(height);
            let center_penalty = (component.center_x() - view.width() * 42 / 100).abs()
                + (component.center_y() - view.height() * 42 / 100).abs();
            area - i64::from(center_penalty * 40)
        })
        .map(Component::rect)
}

fn locate_paint_save_button(frame: &CaptureFrame) -> Result<VisualTarget> {
    let canvas = locate_paint_canvas(frame)?;
    let Some(rect) = canvas.rect else {
        return Err(PeekabooXError::new("paint canvas has no rectangle"));
    };
    let offset_x = min(96_i32, i32::try_from(rect.width / 8).unwrap_or(0));
    let x = rect.x + max(28, offset_x);
    let y = max(20, rect.y - 24);
    Ok(VisualTarget::with_rect(
        Point::new(x, y),
        Rect::new(max(0, x - 24), max(0, y - 24), 48, 48),
    ))
}

fn locate_text_editor_document(
    profile: &AppProfile,
    frame: &CaptureFrame,
    window_scope: Option<WindowScope<'_>>,
) -> Result<VisualTarget> {
    let rect = match window_scope.and_then(|scope| profile_window_rect(profile, scope)) {
        Some(window) => text_editor_document_rect(window),
        None if window_scope.is_some_and(WindowScope::has_constraints) => {
            return Err(PeekabooXError::new(format!(
                "could not locate visible {} window matching {}",
                profile.id,
                window_scope
                    .map(WindowScope::description)
                    .unwrap_or_default()
            )));
        }
        None => {
            let view = FrameView::new(frame);
            Rect::new(
                max(0, view.width() * 8 / 100),
                max(0, view.height() * 14 / 100),
                positive_extent(view.width() * 84 / 100),
                positive_extent(view.height() * 74 / 100),
            )
        }
    };
    Ok(VisualTarget::with_rect(
        point_in_rect_ratio(rect, (0.5, 0.35))?,
        rect,
    ))
}

fn locate_text_editor_save_button(
    profile: &AppProfile,
    frame: &CaptureFrame,
    window_scope: Option<WindowScope<'_>>,
) -> Result<VisualTarget> {
    let rect = match window_scope.and_then(|scope| profile_window_rect(profile, scope)) {
        Some(window) => window,
        None if window_scope.is_some_and(WindowScope::has_constraints) => {
            return Err(PeekabooXError::new(format!(
                "could not locate visible {} window matching {}",
                profile.id,
                window_scope
                    .map(WindowScope::description)
                    .unwrap_or_default()
            )));
        }
        None => {
            let view = FrameView::new(frame);
            Rect::new(
                0,
                0,
                positive_extent(view.width()),
                positive_extent(view.height()),
            )
        }
    };
    let x = rect.x + i32::try_from(rect.width).unwrap_or(0) - 76;
    let y = rect.y + 38;
    Ok(VisualTarget::with_rect(
        Point::new(x, y),
        Rect::new(max(0, x - 36), max(0, y - 18), 72, 36),
    ))
}

fn text_editor_document_rect(window: Rect) -> Rect {
    let header_height = i32::try_from(window.height / 10)
        .unwrap_or(58)
        .clamp(58, 96);
    let horizontal_margin = i32::try_from(window.width / 40).unwrap_or(18).clamp(18, 48);
    let bottom_margin = i32::try_from(window.height / 18)
        .unwrap_or(24)
        .clamp(24, 52);
    let width = i32::try_from(window.width)
        .unwrap_or(0)
        .saturating_sub(horizontal_margin * 2);
    let height = i32::try_from(window.height)
        .unwrap_or(0)
        .saturating_sub(header_height + bottom_margin);

    Rect::new(
        window.x + horizontal_margin,
        window.y + header_height,
        positive_extent(width),
        positive_extent(height),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MessageBar {
    left: i32,
    right: i32,
    y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WindowMetrics {
    left: i32,
    top: i32,
    right: i32,
}

fn locate_message_bar(frame: &CaptureFrame) -> Result<MessageBar> {
    let view = FrameView::new(frame);
    let min_run_width = max(300, view.width() * 25 / 100);
    let mut best: Option<(i32, i32, i32, i32)> = None;
    for y in (view.height() * 45 / 100)..(view.height() * 95 / 100) {
        for (left, right) in horizontal_runs(&view, y, is_telegram_panel, min_run_width) {
            let width = right - left + 1;
            if best.is_none_or(|(best_width, best_y, _, _)| {
                width > best_width || (width == best_width && y > best_y)
            }) {
                best = Some((width, y, left, right));
            }
        }
    }

    let Some((_width, y, left, right)) = best else {
        return Err(PeekabooXError::new("could not locate Telegram message bar"));
    };

    Ok(MessageBar { left, right, y })
}

fn locate_window_metrics(frame: &CaptureFrame) -> Result<WindowMetrics> {
    let view = FrameView::new(frame);
    let bar = locate_message_bar(frame)?;
    let sidebar_width = 72;
    let left = max(0, bar.left - sidebar_width);
    let right = bar.right;
    let min_run_width = max(300, view.width() * 25 / 100);
    let mut top_candidates = Vec::new();

    for y in 0..max(1, view.height() * 45 / 100) {
        for (run_left, run_right) in horizontal_runs(&view, y, is_telegram_panel, min_run_width) {
            if (run_right - right).abs() <= 90 && run_left <= bar.left + 90 {
                top_candidates.push(y);
            }
        }
    }

    let top = top_candidates
        .into_iter()
        .min()
        .unwrap_or_else(|| max(0, min(view.height(), bar.y + 5) - 760));
    Ok(WindowMetrics { left, top, right })
}

fn draft_send_button_active(frame: &CaptureFrame) -> Result<bool> {
    let view = FrameView::new(frame);
    let bar = locate_message_bar(frame)?;
    let left = max(0, bar.right - 60);
    let top = max(0, bar.y - 45);
    let bottom = min(view.height(), bar.y + 10);
    let mut blue_pixels = 0_u32;
    for y in top..bottom {
        for x in left..=bar.right {
            if is_telegram_action(view.pixel(x, y)) {
                blue_pixels += 1;
            }
        }
    }
    Ok(blue_pixels >= 80)
}

fn connected_components(
    view: &FrameView<'_>,
    predicate: fn((u8, u8, u8)) -> bool,
    step: i32,
) -> Vec<Component> {
    let grid_width = (view.width() + step - 1) / step;
    let grid_height = (view.height() + step - 1) / step;
    let grid_width_usize = usize::try_from(grid_width).unwrap_or(0);
    let grid_height_usize = usize::try_from(grid_height).unwrap_or(0);
    let mut mask = vec![false; grid_width_usize * grid_height_usize];
    let mut seen = vec![false; mask.len()];

    for grid_y in 0..grid_height_usize {
        for grid_x in 0..grid_width_usize {
            let x = min(
                i32::try_from(grid_x).unwrap_or(0) * step,
                view.width().saturating_sub(1),
            );
            let y = min(
                i32::try_from(grid_y).unwrap_or(0) * step,
                view.height().saturating_sub(1),
            );
            mask[grid_y * grid_width_usize + grid_x] = predicate(view.pixel(x, y));
        }
    }

    let mut components = Vec::new();
    for start_y in 0..grid_height_usize {
        for start_x in 0..grid_width_usize {
            let index = start_y * grid_width_usize + start_x;
            if seen[index] || !mask[index] {
                continue;
            }

            let mut stack = vec![(start_x, start_y)];
            seen[index] = true;
            let mut pixels = 0_u32;
            let mut min_x = start_x;
            let mut max_x = start_x;
            let mut min_y = start_y;
            let mut max_y = start_y;

            while let Some((x, y)) = stack.pop() {
                pixels += 1;
                min_x = min(min_x, x);
                max_x = max(max_x, x);
                min_y = min(min_y, y);
                max_y = max(max_y, y);

                let neighbors = [
                    (x.saturating_add(1), y),
                    (x.saturating_sub(1), y),
                    (x, y.saturating_add(1)),
                    (x, y.saturating_sub(1)),
                ];
                for (next_x, next_y) in neighbors {
                    if next_x >= grid_width_usize || next_y >= grid_height_usize {
                        continue;
                    }
                    if (next_x == x && next_y == y) || next_x.abs_diff(x) + next_y.abs_diff(y) != 1
                    {
                        continue;
                    }
                    let next_index = next_y * grid_width_usize + next_x;
                    if !seen[next_index] && mask[next_index] {
                        seen[next_index] = true;
                        stack.push((next_x, next_y));
                    }
                }
            }

            components.push(Component {
                pixels: pixels * u32::try_from(step * step).unwrap_or(1),
                left: i32::try_from(min_x).unwrap_or(0) * step,
                top: i32::try_from(min_y).unwrap_or(0) * step,
                right: min(i32::try_from(max_x + 1).unwrap_or(0) * step, view.width()),
                bottom: min(i32::try_from(max_y + 1).unwrap_or(0) * step, view.height()),
            });
        }
    }

    components
}

fn horizontal_runs(
    view: &FrameView<'_>,
    y: i32,
    predicate: fn((u8, u8, u8)) -> bool,
    min_width: i32,
) -> Vec<(i32, i32)> {
    let mut runs = Vec::new();
    let mut start: Option<i32> = None;
    let mut previous: Option<i32> = None;

    for x in 0..view.width() {
        if predicate(view.pixel(x, y)) {
            if start.is_none() {
                start = Some(x);
            }
            previous = Some(x);
            continue;
        }

        if let (Some(left), Some(right)) = (start, previous)
            && right - left + 1 >= min_width
        {
            runs.push((left, right));
        }
        start = None;
        previous = None;
    }

    if let (Some(left), Some(right)) = (start, previous)
        && right - left + 1 >= min_width
    {
        runs.push((left, right));
    }

    runs
}

fn is_telegram_blue((red, green, blue): (u8, u8, u8)) -> bool {
    red <= 90 && (110..=230).contains(&green) && (160..=255).contains(&blue) && blue > green + 15
}

fn is_telegram_panel((red, green, blue): (u8, u8, u8)) -> bool {
    (30..=65).contains(&red)
        && (35..=75).contains(&green)
        && (38..=85).contains(&blue)
        && green >= red.saturating_sub(7)
        && blue >= green.saturating_sub(12)
}

fn is_telegram_action((red, green, blue): (u8, u8, u8)) -> bool {
    red <= 100 && (120..=230).contains(&green) && (120..=230).contains(&blue) && green >= red + 50
}

fn is_paint_canvas_pixel((red, green, blue): (u8, u8, u8)) -> bool {
    red >= 235
        && green >= 235
        && blue >= 235
        && red.abs_diff(green) <= 18
        && red.abs_diff(blue) <= 18
        && green.abs_diff(blue) <= 18
}

fn is_paint_canvas_outline_pixel((red, green, blue): (u8, u8, u8)) -> bool {
    red <= 90 && (85..=180).contains(&green) && blue >= 150 && blue > green + 25
}

fn point_in_rect_ratio(rect: Rect, ratio: (f32, f32)) -> Result<Point> {
    let (ratio_x, ratio_y) = ratio;
    if !ratio_x.is_finite()
        || !ratio_y.is_finite()
        || !(0.0..=1.0).contains(&ratio_x)
        || !(0.0..=1.0).contains(&ratio_y)
    {
        return Err(PeekabooXError::new(format!(
            "ratio must be finite and between 0.0 and 1.0, got {ratio_x},{ratio_y}"
        )));
    }

    let width = rect.width.saturating_sub(1) as f32;
    let height = rect.height.saturating_sub(1) as f32;
    Ok(Point::new(
        rect.x + (width * ratio_x).round() as i32,
        rect.y + (height * ratio_y).round() as i32,
    ))
}

fn rects_intersect(left: Rect, right: Rect) -> bool {
    let left_right = i64::from(left.x) + i64::from(left.width);
    let left_bottom = i64::from(left.y) + i64::from(left.height);
    let right_right = i64::from(right.x) + i64::from(right.width);
    let right_bottom = i64::from(right.y) + i64::from(right.height);

    i64::from(left.x) < right_right
        && i64::from(right.x) < left_right
        && i64::from(left.y) < right_bottom
        && i64::from(right.y) < left_bottom
}

const fn positive_extent(value: i32) -> u32 {
    if value <= 0 { 1 } else { value as u32 }
}

fn contains_case_insensitive(haystack: &str, needle: &str) -> bool {
    haystack
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

fn command_available(command: &CommandSpec) -> bool {
    if command.program == "flatpak"
        && command.args.first().is_some_and(|arg| arg == "run")
        && let Some(app_id) = command.args.get(1)
    {
        return command_exists("flatpak")
            && Command::new("flatpak")
                .arg("info")
                .arg(app_id)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success());
    }

    command_exists(&command.program)
}

fn command_exists(command: &str) -> bool {
    if command.contains('/') {
        return Path::new(command).is_file();
    }

    env::var_os("PATH")
        .is_some_and(|paths| env::split_paths(&paths).any(|path| path.join(command).is_file()))
}

#[cfg(test)]
mod tests;
