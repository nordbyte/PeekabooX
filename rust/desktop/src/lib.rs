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
const SUPPORTED_APPS: &[&str] = &[TELEGRAM_PROFILE_ID];

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
}

impl Default for FocusOptions {
    fn default() -> Self {
        Self {
            use_gnome_overview: true,
            launch_if_needed: true,
            wait_after_focus_ms: DEFAULT_FOCUS_WAIT_MS,
            overview_wait_ms: DEFAULT_OVERVIEW_WAIT_MS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocateOptions {
    pub image: Option<PathBuf>,
    pub prefer_accessibility: bool,
}

impl Default for LocateOptions {
    fn default() -> Self {
        Self {
            image: None,
            prefer_accessibility: true,
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

    if let Ok(metadata) = peekaboox_windows::list_windows()
        && let Some(window) = metadata
            .windows
            .iter()
            .find(|window| profile.matches_window(window))
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
    if options.prefer_accessibility
        && options.image.is_none()
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
    profile.resolve_visual_target(target, &frame)
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
            if !target_text_contains(profile, target, expected, options.locate.image.as_deref())? {
                return Err(PeekabooXError::new(format!(
                    "target {target:?} does not contain {expected:?}"
                )));
            }
        }
        DesktopAssertion::NotContains(expected) => {
            if target_text_contains(profile, target, expected, options.locate.image.as_deref())? {
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

fn focus_from_gnome_overview(profile: &AppProfile, options: &FocusOptions) -> Result<()> {
    activate_gnome_overview()?;
    sleep(Duration::from_millis(options.overview_wait_ms));
    let _ = peekaboox_input::hotkey(vec!["ctrl+a".to_owned()]);
    sleep(Duration::from_millis(200));
    let _ = peekaboox_input::hotkey(vec!["Backspace".to_owned()]);
    sleep(Duration::from_millis(200));
    peekaboox_input::type_text(profile.search_name.to_owned())?;
    sleep(Duration::from_millis(options.overview_wait_ms));

    let frame = peekaboox_capture::capture_screen_frame()?.frame;
    let target = locate_overview_icon(&frame)?;
    peekaboox_input::click(target.point, MouseButton::Left)?;
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
) -> Result<bool> {
    let frame = load_or_capture_frame(image)?;
    let rect = profile.resolve_visual_target(target, &frame)?.rect;

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
        let _ = self;
        let _ = target;
        None
    }

    fn resolve_visual_target(
        self,
        target: &str,
        frame: &CaptureFrame,
    ) -> Result<ResolvedDesktopTarget> {
        let visual = match (self.kind, target) {
            (ProfileKind::Telegram, "overview-icon") => locate_overview_icon(frame)?,
            (ProfileKind::Telegram, "search-input") => locate_search_input(frame)?,
            (ProfileKind::Telegram, "search-clear") => locate_search_clear(frame)?,
            (ProfileKind::Telegram, "search-result") => locate_search_result(frame)?,
            (ProfileKind::Telegram, "message-input") => locate_message_input(frame)?,
            (ProfileKind::Telegram, "send-button") => locate_send_button(frame)?,
            (ProfileKind::Telegram, "header") => locate_header(frame)?,
            _ => {
                return Err(PeekabooXError::new(format!(
                    "unsupported target {target:?} for app {}; supported targets: {}",
                    self.id,
                    telegram_supported_targets().join(", ")
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
    fn supported_apps_contains_telegram() {
        assert_eq!(supported_apps(), &["telegram"]);
        assert!(resolve_profile("telegram-desktop").is_ok());
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
