use std::cmp::{max, min};
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use peekaboox_core::{CaptureFrame, PeekabooXError, PixelFormat, Point, Rect, Result};
use peekaboox_input::MouseButton;

const DEFAULT_FOCUS_WAIT_MS: u64 = 1_000;
const DEFAULT_OVERVIEW_WAIT_MS: u64 = 800;
const TELEGRAM_PROFILE_ID: &str = "telegram";
const TELEGRAM_SEARCH_NAME: &str = "Telegram";
const TELEGRAM_DESKTOP_IDS: &[&str] = &[
    "telegram-desktop",
    "org.telegram.desktop",
    "telegram-desktop_telegram-desktop",
];
const TELEGRAM_ALIASES: &[&str] = &["telegram", "telegram-desktop", "org.telegram.desktop"];
const NO_ARGS: &[&str] = &[];
const FLATPAK_TELEGRAM_ARGS: &[&str] = &["run", "org.telegram.desktop"];
const TELEGRAM_COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        program: "telegram-desktop",
        args: NO_ARGS,
    },
    CommandSpec {
        program: "telegram",
        args: NO_ARGS,
    },
    CommandSpec {
        program: "flatpak",
        args: FLATPAK_TELEGRAM_ARGS,
    },
];
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
const DRAWING_COMMANDS: &[CommandSpec] = &[CommandSpec {
    program: "drawing",
    args: NO_ARGS,
}];
const PINTA_COMMANDS: &[CommandSpec] = &[CommandSpec {
    program: "pinta",
    args: NO_ARGS,
}];
const KOLOURPAINT_COMMANDS: &[CommandSpec] = &[CommandSpec {
    program: "kolourpaint",
    args: NO_ARGS,
}];
const PAINT_COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        program: "drawing",
        args: NO_ARGS,
    },
    CommandSpec {
        program: "pinta",
        args: NO_ARGS,
    },
    CommandSpec {
        program: "kolourpaint",
        args: NO_ARGS,
    },
];
const TEXT_EDITOR_PROFILE_ID: &str = "text-editor";
const TEXT_EDITOR_SEARCH_NAME: &str = "Text Editor";
const TEXT_EDITOR_DESKTOP_IDS: &[&str] = &["org.gnome.TextEditor", "gnome-text-editor"];
const TEXT_EDITOR_ALIASES: &[&str] = &["text-editor", "gnome-text-editor", "org.gnome.TextEditor"];
const TEXT_EDITOR_COMMANDS: &[CommandSpec] = &[CommandSpec {
    program: "gnome-text-editor",
    args: NO_ARGS,
}];
const SUPPORTED_APPS: &[&str] = &[
    TELEGRAM_PROFILE_ID,
    PAINT_PROFILE_ID,
    DRAWING_PROFILE_ID,
    PINTA_PROFILE_ID,
    KOLOURPAINT_PROFILE_ID,
    TEXT_EDITOR_PROFILE_ID,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusOptions {
    pub use_gnome_overview: bool,
    pub launch_if_needed: bool,
    pub wait_after_focus_ms: u64,
    pub overview_wait_ms: u64,
    pub window_title: Option<String>,
}

impl Default for FocusOptions {
    fn default() -> Self {
        Self {
            use_gnome_overview: true,
            launch_if_needed: true,
            wait_after_focus_ms: DEFAULT_FOCUS_WAIT_MS,
            overview_wait_ms: DEFAULT_OVERVIEW_WAIT_MS,
            window_title: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocateOptions {
    pub image: Option<PathBuf>,
    pub prefer_accessibility: bool,
    pub window_title: Option<String>,
}

impl Default for LocateOptions {
    fn default() -> Self {
        Self {
            image: None,
            prefer_accessibility: true,
            window_title: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClickOptions {
    pub locate: LocateOptions,
    pub button: MouseButton,
    pub dry_run: bool,
}

impl Default for ClickOptions {
    fn default() -> Self {
        Self {
            locate: LocateOptions::default(),
            button: MouseButton::Left,
            dry_run: false,
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
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TypeIntoOptions {
    pub locate: LocateOptions,
    pub clear: bool,
    pub dry_run: bool,
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

pub fn supported_apps() -> &'static [&'static str] {
    SUPPORTED_APPS
}

pub fn focus_app(app: &str, options: &FocusOptions) -> Result<DesktopActionResult> {
    let profile = resolve_profile(app)?;
    let title_hint = normalized_title_hint(options.window_title.as_deref());

    if let Ok(metadata) = peekaboox_windows::list_windows()
        && let Some(window) = preferred_profile_window(profile, &metadata.windows, title_hint)
    {
        if window.focused {
            sleep_after_focus(options);
            return Ok(DesktopActionResult {
                app: profile.id.to_owned(),
                action: "focus".to_owned(),
                detail: "already focused".to_owned(),
                backend_name: metadata.backend_name,
            });
        }

        if window.bounds.width > 0 && window.bounds.height > 0 {
            let center = Point::new(
                window.bounds.x + i32::try_from(window.bounds.width / 2).unwrap_or(0),
                window.bounds.y + i32::try_from(window.bounds.height / 2).unwrap_or(0),
            );
            let metadata = peekaboox_input::click(center, MouseButton::Left)?;
            sleep_after_focus(options);
            return Ok(DesktopActionResult {
                app: profile.id.to_owned(),
                action: "focus".to_owned(),
                detail: format!("clicked existing window at {},{}", center.x, center.y),
                backend_name: metadata.backend_name,
            });
        }
    }

    if let Some(title_hint) = title_hint {
        return Err(PeekabooXError::new(format!(
            "could not find visible app {app:?} window with title containing {title_hint:?}"
        )));
    }

    if options.use_gnome_overview && focus_from_gnome_overview(profile, options).is_ok() {
        sleep_after_focus(options);
        return Ok(DesktopActionResult {
            app: profile.id.to_owned(),
            action: "focus".to_owned(),
            detail: "focused via GNOME overview".to_owned(),
            backend_name: "gnome-overview".to_owned(),
        });
    }

    if options.launch_if_needed {
        if let Some(desktop_id) = launch_desktop_entry(profile) {
            sleep_after_focus(options);
            return Ok(DesktopActionResult {
                app: profile.id.to_owned(),
                action: "focus".to_owned(),
                detail: format!("launched desktop entry {desktop_id}"),
                backend_name: "gtk-launch".to_owned(),
            });
        }

        if let Some(command) = launch_command(profile) {
            sleep_after_focus(options);
            return Ok(DesktopActionResult {
                app: profile.id.to_owned(),
                action: "focus".to_owned(),
                detail: format!("launched {}", command.program),
                backend_name: "command".to_owned(),
            });
        }
    }

    Err(PeekabooXError::new(format!(
        "could not focus or launch app {:?}",
        app
    )))
}

pub fn locate_target(
    app: &str,
    target: &str,
    options: &LocateOptions,
) -> Result<ResolvedDesktopTarget> {
    let profile = resolve_profile(app)?;
    let title_hint = normalized_title_hint(options.window_title.as_deref());
    if options.prefer_accessibility
        && options.image.is_none()
        && title_hint.is_none()
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
    profile.resolve_visual_target(target, &frame, title_hint)
}

pub fn click_target(
    app: &str,
    target: &str,
    options: &ClickOptions,
) -> Result<DesktopActionResult> {
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
        });
    }

    let metadata = peekaboox_input::click(resolved.point, options.button)?;
    Ok(DesktopActionResult {
        app: resolved.app,
        action: "click".to_owned(),
        detail: format!(
            "clicked {} at {},{} via {}",
            resolved.target,
            resolved.point.x,
            resolved.point.y,
            resolved.source.label()
        ),
        backend_name: metadata.backend_name,
    })
}

pub fn drag_target(
    app: &str,
    target: &str,
    options: &DesktopDragOptions,
) -> Result<DesktopActionResult> {
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
        });
    }

    let metadata = peekaboox_input::drag(from, to, options.button, options.duration_ms)?;
    Ok(DesktopActionResult {
        app: resolved.app,
        action: "drag".to_owned(),
        detail: format!(
            "dragged {} from {},{} to {},{} via {}",
            resolved.target,
            from.x,
            from.y,
            to.x,
            to.y,
            resolved.source.label()
        ),
        backend_name: metadata.backend_name,
    })
}

pub fn type_into_target(
    app: &str,
    target: &str,
    text: &str,
    options: &TypeIntoOptions,
) -> Result<DesktopActionResult> {
    let profile = resolve_profile(app)?;
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
        });
    }

    peekaboox_input::click(resolved.point, MouseButton::Left)?;
    sleep(Duration::from_millis(250));
    if options.clear {
        clear_target(profile, target)?;
    }
    let metadata = peekaboox_input::type_text(text.to_owned())?;

    Ok(DesktopActionResult {
        app: resolved.app,
        action: "type-into".to_owned(),
        detail: format!("typed into {}", resolved.target),
        backend_name: metadata.backend_name,
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
        DesktopAssertion::NotPresent => {
            if locate_target(app, target, &options.locate).is_ok() {
                return Err(PeekabooXError::new(format!(
                    "target {target:?} is present but expected it to be absent"
                )));
            }
        }
        DesktopAssertion::Active => {
            if !profile.target_active(
                target,
                &load_or_capture_frame(options.locate.image.as_deref())?,
            )? {
                return Err(PeekabooXError::new(format!(
                    "target {target:?} is not active"
                )));
            }
        }
        DesktopAssertion::NotActive => {
            if profile.target_active(
                target,
                &load_or_capture_frame(options.locate.image.as_deref())?,
            )? {
                return Err(PeekabooXError::new(format!(
                    "target {target:?} is active but expected inactive"
                )));
            }
        }
        DesktopAssertion::Contains(expected) => {
            if !target_text_contains(
                profile,
                target,
                expected,
                options.locate.image.as_deref(),
                normalized_title_hint(options.locate.window_title.as_deref()),
            )? {
                return Err(PeekabooXError::new(format!(
                    "target {target:?} does not contain {expected:?}"
                )));
            }
        }
        DesktopAssertion::NotContains(expected) => {
            if target_text_contains(
                profile,
                target,
                expected,
                options.locate.image.as_deref(),
                normalized_title_hint(options.locate.window_title.as_deref()),
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
    })
}

fn sleep_after_focus(options: &FocusOptions) {
    if options.wait_after_focus_ms > 0 {
        sleep(Duration::from_millis(options.wait_after_focus_ms));
    }
}

fn preferred_profile_window<'a>(
    profile: &AppProfile,
    windows: &'a [peekaboox_core::WindowInfo],
    title_hint: Option<&str>,
) -> Option<&'a peekaboox_core::WindowInfo> {
    windows
        .iter()
        .filter(|window| {
            profile.matches_window(window)
                && window.bounds.width > 0
                && window.bounds.height > 0
                && title_hint.is_none_or(|hint| contains_case_insensitive(&window.title, hint))
        })
        .max_by_key(|window| {
            let area = u64::from(window.bounds.width) * u64::from(window.bounds.height);
            let focus_bonus = if window.focused { u64::MAX / 2 } else { 0 };
            focus_bonus.saturating_add(area)
        })
}

fn profile_window_rect(profile: &AppProfile, title_hint: Option<&str>) -> Option<Rect> {
    let metadata = peekaboox_windows::list_windows().ok()?;
    preferred_profile_window(profile, &metadata.windows, title_hint).map(|window| window.bounds)
}

fn normalized_title_hint(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn focus_from_gnome_overview(profile: &AppProfile, options: &FocusOptions) -> Result<()> {
    activate_gnome_overview()?;
    sleep(Duration::from_millis(options.overview_wait_ms));
    let _ = peekaboox_input::hotkey(vec!["ctrl+a".to_owned()]);
    sleep(Duration::from_millis(200));
    let _ = peekaboox_input::hotkey(vec!["Backspace".to_owned()]);
    sleep(Duration::from_millis(200));
    peekaboox_input::type_text(profile.search_name.to_owned())?;
    sleep(Duration::from_millis(options.overview_wait_ms));

    if profile.kind == ProfileKind::Telegram {
        let frame = peekaboox_capture::capture_screen_frame()?.frame;
        let target = locate_overview_icon(&frame)?;
        peekaboox_input::click(target.point, MouseButton::Left)?;
    } else {
        peekaboox_input::hotkey(vec!["Enter".to_owned()])?;
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

    peekaboox_input::hotkey(vec!["super".to_owned()]).map(|_| ())
}

fn launch_desktop_entry(profile: &AppProfile) -> Option<&'static str> {
    if !command_exists("gtk-launch") {
        return None;
    }

    profile.desktop_ids.iter().copied().find(|id| {
        Command::new("gtk-launch")
            .arg(id)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    })
}

fn launch_command(profile: &AppProfile) -> Option<&'static CommandSpec> {
    for command in profile.commands {
        if !command_exists(command.program) {
            continue;
        }

        if Command::new(command.program)
            .args(command.args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
        {
            return Some(command);
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
    window_title: Option<&str>,
) -> Result<bool> {
    let frame = load_or_capture_frame(image)?;
    let rect = profile
        .resolve_visual_target(target, &frame, window_title)?
        .rect;

    if accessibility_contains(expected, rect).unwrap_or(false) {
        return Ok(true);
    }

    let temporary;
    let image_path = match image {
        Some(path) => path,
        None => {
            temporary = capture_temp_path();
            peekaboox_capture::capture_screen_to_file(&temporary)?;
            temporary.as_path()
        }
    };

    let result = peekaboox_vision::ocr_image_file(image_path, rect)?;
    if image.is_none() {
        let _ = std::fs::remove_file(image_path);
    }

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

fn capture_temp_path() -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    env::temp_dir().join(format!(
        "peekaboox-desktop-{}-{millis}.png",
        std::process::id()
    ))
}

fn resolve_profile(app: &str) -> Result<&'static AppProfile> {
    let app = app.trim();
    if TELEGRAM_PROFILE.matches_id(app) {
        return Ok(&TELEGRAM_PROFILE);
    }
    for profile in [
        &DRAWING_PROFILE,
        &PINTA_PROFILE,
        &KOLOURPAINT_PROFILE,
        &PAINT_PROFILE,
        &TEXT_EDITOR_PROFILE,
    ] {
        if profile.matches_id(app) {
            return Ok(profile);
        }
    }

    Err(PeekabooXError::new(format!(
        "unsupported desktop app {app:?}; supported apps: {}",
        SUPPORTED_APPS.join(", ")
    )))
}

#[derive(Debug, Clone, Copy)]
struct CommandSpec {
    program: &'static str,
    args: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProfileKind {
    Telegram,
    Paint,
    TextEditor,
}

#[derive(Debug, Clone, Copy)]
struct AppProfile {
    id: &'static str,
    aliases: &'static [&'static str],
    search_name: &'static str,
    desktop_ids: &'static [&'static str],
    commands: &'static [CommandSpec],
    kind: ProfileKind,
}

impl AppProfile {
    fn matches_id(self, value: &str) -> bool {
        self.id.eq_ignore_ascii_case(value)
            || self
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(value))
    }

    fn matches_window(self, window: &peekaboox_core::WindowInfo) -> bool {
        let title = window.title.as_str();
        let app_id = window.app_id.as_deref().unwrap_or_default();
        self.aliases.iter().any(|alias| {
            contains_case_insensitive(title, alias) || contains_case_insensitive(app_id, alias)
        }) || contains_case_insensitive(title, self.search_name)
            || contains_case_insensitive(app_id, self.search_name)
    }

    fn accessibility_selector(self, target: &str) -> Option<&'static str> {
        match (self.kind, target) {
            (ProfileKind::Paint, "save-button") => Some("Save"),
            (ProfileKind::TextEditor, "save-button") => Some("Save"),
            _ => None,
        }
    }

    fn resolve_visual_target(
        self,
        target: &str,
        frame: &CaptureFrame,
        window_title: Option<&str>,
    ) -> Result<ResolvedDesktopTarget> {
        let visual = match (self.kind, target) {
            (ProfileKind::Telegram, "overview-icon") => locate_overview_icon(frame)?,
            (ProfileKind::Telegram, "search-input") => locate_search_input(frame)?,
            (ProfileKind::Telegram, "search-clear") => locate_search_clear(frame)?,
            (ProfileKind::Telegram, "search-result") => locate_search_result(frame)?,
            (ProfileKind::Telegram, "message-input") => locate_message_input(frame)?,
            (ProfileKind::Telegram, "send-button") => locate_send_button(frame)?,
            (ProfileKind::Telegram, "header") => locate_header(frame)?,
            (ProfileKind::Paint, "canvas") => locate_paint_canvas(frame)?,
            (ProfileKind::Paint, "save-button") => locate_paint_save_button(frame)?,
            (ProfileKind::TextEditor, "document") => {
                locate_text_editor_document(self, frame, window_title)?
            }
            (ProfileKind::TextEditor, "save-button") => {
                locate_text_editor_save_button(self, frame, window_title)?
            }
            _ => {
                let supported_targets = match self.kind {
                    ProfileKind::Telegram => telegram_supported_targets(),
                    ProfileKind::Paint => paint_supported_targets(),
                    ProfileKind::TextEditor => text_editor_supported_targets(),
                };
                return Err(PeekabooXError::new(format!(
                    "unsupported target {target:?} for app {}; supported targets: {}",
                    self.id,
                    supported_targets.join(", ")
                )));
            }
        };

        Ok(ResolvedDesktopTarget {
            app: self.id.to_owned(),
            target: target.to_owned(),
            point: visual.point,
            rect: visual.rect,
            source: DesktopTargetSource::VisualLayout,
        })
    }

    fn target_active(self, target: &str, frame: &CaptureFrame) -> Result<bool> {
        match (self.kind, target) {
            (ProfileKind::Telegram, "send-button") => Ok(draft_send_button_active(frame)?),
            _ => Err(PeekabooXError::new(format!(
                "target {target:?} does not expose an active-state guard"
            ))),
        }
    }
}

static TELEGRAM_PROFILE: AppProfile = AppProfile {
    id: TELEGRAM_PROFILE_ID,
    aliases: TELEGRAM_ALIASES,
    search_name: TELEGRAM_SEARCH_NAME,
    desktop_ids: TELEGRAM_DESKTOP_IDS,
    commands: TELEGRAM_COMMANDS,
    kind: ProfileKind::Telegram,
};

static PAINT_PROFILE: AppProfile = AppProfile {
    id: PAINT_PROFILE_ID,
    aliases: PAINT_ALIASES,
    search_name: DRAWING_SEARCH_NAME,
    desktop_ids: PAINT_DESKTOP_IDS,
    commands: PAINT_COMMANDS,
    kind: ProfileKind::Paint,
};

static DRAWING_PROFILE: AppProfile = AppProfile {
    id: DRAWING_PROFILE_ID,
    aliases: DRAWING_ALIASES,
    search_name: DRAWING_SEARCH_NAME,
    desktop_ids: DRAWING_DESKTOP_IDS,
    commands: DRAWING_COMMANDS,
    kind: ProfileKind::Paint,
};

static PINTA_PROFILE: AppProfile = AppProfile {
    id: PINTA_PROFILE_ID,
    aliases: PINTA_ALIASES,
    search_name: PINTA_SEARCH_NAME,
    desktop_ids: PINTA_DESKTOP_IDS,
    commands: PINTA_COMMANDS,
    kind: ProfileKind::Paint,
};

static KOLOURPAINT_PROFILE: AppProfile = AppProfile {
    id: KOLOURPAINT_PROFILE_ID,
    aliases: KOLOURPAINT_ALIASES,
    search_name: KOLOURPAINT_SEARCH_NAME,
    desktop_ids: KOLOURPAINT_DESKTOP_IDS,
    commands: KOLOURPAINT_COMMANDS,
    kind: ProfileKind::Paint,
};

static TEXT_EDITOR_PROFILE: AppProfile = AppProfile {
    id: TEXT_EDITOR_PROFILE_ID,
    aliases: TEXT_EDITOR_ALIASES,
    search_name: TEXT_EDITOR_SEARCH_NAME,
    desktop_ids: TEXT_EDITOR_DESKTOP_IDS,
    commands: TEXT_EDITOR_COMMANDS,
    kind: ProfileKind::TextEditor,
};

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
    profile: AppProfile,
    frame: &CaptureFrame,
    window_title: Option<&str>,
) -> Result<VisualTarget> {
    let rect = match profile_window_rect(&profile, window_title) {
        Some(window) => text_editor_document_rect(window),
        None if window_title.is_some() => {
            return Err(PeekabooXError::new(format!(
                "could not locate visible {} window with title containing {:?}",
                profile.id,
                window_title.unwrap_or_default()
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
    profile: AppProfile,
    frame: &CaptureFrame,
    window_title: Option<&str>,
) -> Result<VisualTarget> {
    let rect = match profile_window_rect(&profile, window_title) {
        Some(window) => window,
        None if window_title.is_some() => {
            return Err(PeekabooXError::new(format!(
                "could not locate visible {} window with title containing {:?}",
                profile.id,
                window_title.unwrap_or_default()
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
    let header_height = min(
        96_i32,
        max(58_i32, i32::try_from(window.height / 10).unwrap_or(58)),
    );
    let horizontal_margin = min(
        48_i32,
        max(18_i32, i32::try_from(window.width / 40).unwrap_or(18)),
    );
    let bottom_margin = min(
        52_i32,
        max(24_i32, i32::try_from(window.height / 18).unwrap_or(24)),
    );
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

fn command_exists(command: &str) -> bool {
    if command.contains('/') {
        return Path::new(command).is_file();
    }

    env::var_os("PATH")
        .is_some_and(|paths| env::split_paths(&paths).any(|path| path.join(command).is_file()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_apps_contains_desktop_profiles() {
        assert_eq!(
            supported_apps(),
            &[
                "telegram",
                "paint",
                "drawing",
                "pinta",
                "kolourpaint",
                "text-editor"
            ]
        );
        assert!(resolve_profile("telegram-desktop").is_ok());
        assert_eq!(resolve_profile("pinta").unwrap().id, "pinta");
        assert_eq!(
            resolve_profile("gnome-text-editor").unwrap().id,
            "text-editor"
        );
    }

    #[test]
    fn locates_telegram_targets_from_visual_layout() {
        let frame = synthetic_telegram_frame(false);

        let search = locate_search_input(&frame).unwrap();
        let result = locate_search_result(&frame).unwrap();
        let input = locate_message_input(&frame).unwrap();
        let send = locate_send_button(&frame).unwrap();

        assert_eq!(search.point, Point::new(897, 158));
        assert_eq!(result.point, Point::new(825, 216));
        assert_eq!(input.point, Point::new(1118, 845));
        assert_eq!(send.point, Point::new(1645, 845));
    }

    #[test]
    fn detects_active_telegram_draft_send_button() {
        assert!(!draft_send_button_active(&synthetic_telegram_frame(false)).unwrap());
        assert!(draft_send_button_active(&synthetic_telegram_frame(true)).unwrap());
    }

    #[test]
    fn locates_overview_icon_by_telegram_blue_component() {
        let mut frame = blank_frame(1_920, 1_200, (10, 10, 10));
        fill_rect(&mut frame, 970, 220, 48, 48, (50, 170, 230));

        let target = locate_overview_icon(&frame).unwrap();

        assert_eq!(target.point, Point::new(994, 244));
    }

    #[test]
    fn locates_paint_canvas_from_visual_layout() {
        let mut frame = blank_frame(1_280, 900, (34, 36, 40));
        fill_rect(&mut frame, 220, 150, 820, 620, (248, 248, 247));
        fill_rect(&mut frame, 20, 20, 200, 70, (245, 245, 245));

        let target = locate_paint_canvas(&frame).unwrap();

        assert_eq!(target.rect, Some(Rect::new(220, 152, 820, 620)));
        assert_eq!(target.point, Point::new(507, 369));
    }

    #[test]
    fn locates_paint_canvas_outline_inside_white_workspace() {
        let mut frame = blank_frame(1_920, 1_200, (34, 36, 40));
        fill_rect(&mut frame, 68, 140, 1_852, 1_060, (248, 248, 248));
        fill_rect(&mut frame, 938, 200, 4, 608, (44, 130, 230));
        fill_rect(&mut frame, 134, 804, 808, 4, (44, 130, 230));

        let target = locate_paint_canvas(&frame).unwrap();

        assert_eq!(target.rect, Some(Rect::new(134, 200, 808, 608)));
        assert_eq!(target.point, Point::new(416, 412));
    }

    #[test]
    fn point_in_rect_ratio_maps_inside_rectangle() {
        let point = point_in_rect_ratio(Rect::new(100, 200, 401, 201), (0.25, 0.5)).unwrap();

        assert_eq!(point, Point::new(200, 300));
    }

    #[test]
    fn text_editor_document_rect_stays_inside_window_chrome() {
        let rect = text_editor_document_rect(Rect::new(10, 20, 1_000, 700));

        assert_eq!(rect, Rect::new(35, 90, 950, 592));
    }

    #[test]
    fn preferred_profile_window_respects_title_hint() {
        let windows = vec![
            peekaboox_core::WindowInfo {
                id: "focused-user-doc".to_owned(),
                title: "notes.txt - Text Editor".to_owned(),
                app_id: Some("gnome-text-editor".to_owned()),
                bounds: Rect::new(0, 0, 900, 700),
                focused: true,
                state: peekaboox_core::WindowState::Normal,
            },
            peekaboox_core::WindowInfo {
                id: "draft".to_owned(),
                title: "peekaboox-draft.txt - Text Editor".to_owned(),
                app_id: Some("gnome-text-editor".to_owned()),
                bounds: Rect::new(200, 120, 700, 520),
                focused: false,
                state: peekaboox_core::WindowState::Normal,
            },
        ];

        let selected =
            preferred_profile_window(&TEXT_EDITOR_PROFILE, &windows, Some("peekaboox-draft"))
                .unwrap();

        assert_eq!(selected.id, "draft");
    }

    fn synthetic_telegram_frame(active_send: bool) -> CaptureFrame {
        let mut frame = blank_frame(1_920, 1_200, (10, 10, 10));
        let panel = (44, 52, 60);
        fill_rect(&mut frame, 647, 108, 1_023, 77, panel);
        fill_rect(&mut frame, 719, 862, 951, 1, panel);
        fill_rect(&mut frame, 719, 823, 951, 40, panel);
        fill_rect(&mut frame, 647, 185, 1_023, 638, (20, 24, 28));
        if active_send {
            fill_rect(&mut frame, 1610, 817, 48, 38, (40, 190, 190));
        }
        frame
    }

    fn blank_frame(width: u32, height: u32, color: (u8, u8, u8)) -> CaptureFrame {
        let mut data = vec![0; usize::try_from(width * height * 3).unwrap()];
        for y in 0..height {
            for x in 0..width {
                let index = usize::try_from((y * width + x) * 3).unwrap();
                data[index] = color.0;
                data[index + 1] = color.1;
                data[index + 2] = color.2;
            }
        }
        CaptureFrame {
            width,
            height,
            stride: width * 3,
            format: PixelFormat::Rgb8,
            data,
        }
    }

    fn fill_rect(
        frame: &mut CaptureFrame,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        color: (u8, u8, u8),
    ) {
        for py in y..(y + height) {
            for px in x..(x + width) {
                let index = usize::try_from((py * frame.width + px) * 3).unwrap();
                frame.data[index] = color.0;
                frame.data[index + 1] = color.1;
                frame.data[index + 2] = color.2;
            }
        }
    }
}
