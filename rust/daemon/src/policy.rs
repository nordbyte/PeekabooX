use super::*;

pub(super) fn audit_grpc_result<T>(
    audit: &SharedAudit,
    event: &str,
    result: &Result<T, Status>,
    details: serde_json::Value,
) {
    match result {
        Ok(_) => audit_write(audit, event, Some(API_VERSION), "ok", None, details),
        Err(status) => audit_write(
            audit,
            event,
            Some(API_VERSION),
            "error",
            Some(status.message()),
            details,
        ),
    }
}

pub(super) fn ensure_input_allowed(config: &ServerConfig) -> Result<(), String> {
    if config.allow_input {
        return Ok(());
    }

    Err(
        "permission denied: non-dry-run input actions require peekabooxd --profile operator, --allow-input, or PEEKABOOX_ALLOW_INPUT=1"
            .to_owned(),
    )
}

pub(super) fn ensure_plugin_execution_allowed(config: &ServerConfig) -> Result<(), String> {
    if config.allow_plugins {
        return Ok(());
    }

    Err(
        "permission denied: plugin execution requires peekabooxd --profile operator, --allow-plugins, or PEEKABOOX_ALLOW_PLUGINS=1"
            .to_owned(),
    )
}

pub(super) fn prepare_socket_path(socket: &PathBuf) -> Result<(), String> {
    if let Some(parent) = socket.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }

    if !socket.exists() {
        return Ok(());
    }

    let metadata = fs::metadata(socket)
        .map_err(|error| format!("failed to inspect {}: {error}", socket.display()))?;
    if metadata.file_type().is_socket() {
        if UnixStream::connect(socket).is_ok() {
            return Err(format!(
                "{} is already in use by a running PeekabooX daemon",
                socket.display()
            ));
        }
        fs::remove_file(socket).map_err(|error| {
            format!(
                "failed to remove stale socket {}: {error}",
                socket.display()
            )
        })?;
        return Ok(());
    }

    Err(format!(
        "{} already exists and is not a Unix socket",
        socket.display()
    ))
}

pub(super) struct SocketGuard {
    pub(super) path: PathBuf,
    pub(super) dev: u64,
    pub(super) ino: u64,
}

impl SocketGuard {
    pub(super) fn new(path: PathBuf) -> Result<Self, String> {
        let metadata = fs::metadata(&path)
            .map_err(|error| format!("failed to inspect socket {}: {error}", path.display()))?;
        Ok(Self {
            path,
            dev: metadata.dev(),
            ino: metadata.ino(),
        })
    }
}

impl Drop for SocketGuard {
    fn drop(&mut self) {
        let Ok(metadata) = fs::metadata(&self.path) else {
            return;
        };
        if metadata.dev() != self.dev || metadata.ino() != self.ino {
            return;
        }
        if let Err(error) = fs::remove_file(&self.path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!("failed to remove socket {}: {error}", self.path.display());
        }
    }
}

pub(super) fn mouse_button(button: MouseButtonDto) -> MouseButton {
    match button {
        MouseButtonDto::Left => MouseButton::Left,
        MouseButtonDto::Middle => MouseButton::Middle,
        MouseButtonDto::Right => MouseButton::Right,
    }
}

pub(super) fn proto_mouse_button(button: Option<i32>) -> Result<MouseButton, Status> {
    match proto::MouseButton::try_from(button.unwrap_or(proto::MouseButton::Left as i32)) {
        Ok(proto::MouseButton::Unspecified) | Ok(proto::MouseButton::Left) => Ok(MouseButton::Left),
        Ok(proto::MouseButton::Middle) => Ok(MouseButton::Middle),
        Ok(proto::MouseButton::Right) => Ok(MouseButton::Right),
        Err(_) => Err(Status::invalid_argument("unknown mouse button")),
    }
}

pub(super) fn input_metadata_dto(
    metadata: peekaboox_input::InputExecutionMetadata,
) -> ActionResultDto {
    ActionResultDto {
        backend_name: metadata.backend_name,
        backend_kind: backend_kind_name(metadata.backend_kind),
    }
}

pub(super) fn detected_input_backend_dto(
    backend: peekaboox_input::DetectedInputBackend,
) -> ActionResultDto {
    ActionResultDto {
        backend_name: backend.name().to_owned(),
        backend_kind: backend_kind_name(backend.backend_kind()),
    }
}

pub(super) fn detected_paste_backend_dto(
    backend: peekaboox_input::DetectedPasteBackend,
) -> ActionResultDto {
    ActionResultDto {
        backend_name: backend.name(),
        backend_kind: backend_kind_name(backend.backend_kind()),
    }
}

pub(super) fn desktop_action_dto(
    result: peekaboox_desktop::DesktopActionResult,
) -> DesktopActionResultDto {
    DesktopActionResultDto {
        app: result.app,
        action: result.action,
        detail: result.detail,
        backend_name: result.backend_name,
        verified: result.verified,
        verification_detail: result.verification_detail,
        focus_diagnostics: result.focus_diagnostics,
    }
}

pub(super) fn desktop_locate_dto(
    result: peekaboox_desktop::ResolvedDesktopTarget,
) -> DesktopLocateResultDto {
    DesktopLocateResultDto {
        app: result.app,
        target: result.target,
        point: PointDto {
            x: result.point.x,
            y: result.point.y,
        },
        rect: result.rect.map(Into::into),
        source: result.source.label().to_owned(),
    }
}

pub(super) fn desktop_profiles_dto(
    result: peekaboox_desktop::DesktopProfileList,
) -> DesktopProfilesResultDto {
    DesktopProfilesResultDto {
        schema_version: result.schema_version,
        count: result.count,
        profiles: result
            .profiles
            .into_iter()
            .map(desktop_profile_dto)
            .collect(),
    }
}

pub(super) fn desktop_profile_dto(
    profile: peekaboox_desktop::DesktopProfileInfo,
) -> DesktopProfileDto {
    DesktopProfileDto {
        id: profile.id,
        aliases: profile.aliases,
        search_name: profile.search_name,
        desktop_ids: profile.desktop_ids,
        commands: profile
            .commands
            .into_iter()
            .map(|command| DesktopProfileCommandDto {
                program: command.program,
                args: command.args,
                display: command.display,
                available: command.available,
            })
            .collect(),
        targets: profile
            .targets
            .into_iter()
            .map(|target| DesktopProfileTargetDto {
                name: target.name,
                supports: target.supports,
                sources: target.sources,
                can_locate: target.can_locate,
                can_click: target.can_click,
                can_drag: target.can_drag,
                can_type: target.can_type,
                can_assert_present: target.can_assert_present,
                can_assert_active: target.can_assert_active,
                can_assert_contains: target.can_assert_contains,
                accessibility_selector: target.accessibility_selector,
                visual_layout: target.visual_layout,
                visual_rect: target.visual_rect,
            })
            .collect(),
        availability: DesktopProfileAvailabilityDto {
            checked: profile.availability.checked,
            installed: profile.availability.installed,
            command_available: profile.availability.command_available,
            desktop_entry_available: profile.availability.desktop_entry_available,
            available_commands: profile.availability.available_commands,
            available_desktop_ids: profile.availability.available_desktop_ids,
        },
    }
}

pub(super) fn desktop_assertion(
    assertion: DesktopAssertionDto,
    expected_text: Option<String>,
) -> Result<DesktopAssertion, String> {
    match assertion {
        DesktopAssertionDto::Present => Ok(DesktopAssertion::Present),
        DesktopAssertionDto::NotPresent => Ok(DesktopAssertion::NotPresent),
        DesktopAssertionDto::Active => Ok(DesktopAssertion::Active),
        DesktopAssertionDto::NotActive => Ok(DesktopAssertion::NotActive),
        DesktopAssertionDto::Contains => Ok(DesktopAssertion::Contains(required_expected_text(
            "contains",
            expected_text,
        )?)),
        DesktopAssertionDto::NotContains => Ok(DesktopAssertion::NotContains(
            required_expected_text("not_contains", expected_text)?,
        )),
    }
}

pub(super) fn required_expected_text(
    assertion: &str,
    expected_text: Option<String>,
) -> Result<String, String> {
    expected_text
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("desktop assertion {assertion} requires expected_text"))
}

pub(super) fn validate_ratio(name: &str, value: f32) -> Result<(), String> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(format!("{name} must be between 0.0 and 1.0"))
    }
}

pub(super) fn validate_ratio_status(name: &str, value: f32) -> Result<(), Status> {
    validate_ratio(name, value).map_err(Status::invalid_argument)
}

pub(super) fn move_options_from_fields(
    duration_ms: Option<u64>,
    steps: Option<u32>,
    bounds_policy: Option<&str>,
    backend: Option<&str>,
) -> Result<peekaboox_input::MoveMouseOptions, String> {
    if steps == Some(0) {
        return Err("move steps must be greater than zero".to_owned());
    }

    Ok(peekaboox_input::MoveMouseOptions {
        duration_ms: duration_ms.unwrap_or_default(),
        steps,
        bounds_policy: parse_move_bounds_policy(bounds_policy.unwrap_or("allow"))?,
        backend: parse_input_backend_selection(backend.unwrap_or("auto"))?,
    })
}

pub(super) fn click_options_from_fields(
    bounds_policy: Option<&str>,
    backend: Option<&str>,
) -> Result<peekaboox_input::ClickMouseOptions, String> {
    Ok(peekaboox_input::ClickMouseOptions {
        bounds_policy: parse_move_bounds_policy(bounds_policy.unwrap_or("allow"))?,
        backend: parse_input_backend_selection(backend.unwrap_or("auto"))?,
    })
}

pub(super) fn type_options_from_fields(
    typing_speed_chars_per_second: Option<u32>,
    delay_ms: Option<u64>,
    key_delay_ms: Option<u64>,
    backend: Option<&str>,
) -> Result<peekaboox_input::TypeTextOptions, String> {
    if typing_speed_chars_per_second == Some(0) {
        return Err("typing_speed_chars_per_second must be greater than zero".to_owned());
    }
    if typing_speed_chars_per_second.is_some() && key_delay_ms.is_some() {
        return Err(
            "typing_speed_chars_per_second cannot be combined with key_delay_ms".to_owned(),
        );
    }

    Ok(peekaboox_input::TypeTextOptions {
        typing_speed_chars_per_second,
        delay_ms,
        key_delay_ms,
        backend: parse_type_backend_selection(backend.unwrap_or("auto"))?,
    })
}

pub(super) fn paste_options_from_fields(
    preserve_clipboard: bool,
    clipboard_backend: Option<&str>,
    hotkey_backend: Option<&str>,
    delay_ms: Option<u64>,
    restore_delay_ms: Option<u64>,
    restore_policy: Option<&str>,
) -> Result<peekaboox_input::PasteTextOptions, String> {
    Ok(peekaboox_input::PasteTextOptions {
        preserve_clipboard,
        clipboard_backend: parse_clipboard_backend_selection(clipboard_backend.unwrap_or("auto"))?,
        hotkey_backend: parse_paste_hotkey_backend_selection(hotkey_backend.unwrap_or("auto"))?,
        delay_ms: delay_ms.unwrap_or(80),
        restore_delay_ms: restore_delay_ms.unwrap_or(120),
        restore_policy: parse_clipboard_restore_policy(restore_policy.unwrap_or("strict"))?,
    })
}

pub(super) fn hotkey_options_from_fields(
    backend: Option<&str>,
    delay_ms: Option<u64>,
    key_delay_ms: Option<u64>,
    repeat: Option<u32>,
    interval_ms: Option<u64>,
    release_before: bool,
    release_after: bool,
) -> Result<peekaboox_input::HotkeyOptions, String> {
    if repeat == Some(0) {
        return Err("hotkey repeat must be greater than zero".to_owned());
    }

    Ok(peekaboox_input::HotkeyOptions {
        backend: parse_hotkey_backend_selection(backend.unwrap_or("auto"))?,
        delay_ms,
        key_delay_ms,
        repeat: repeat.unwrap_or(1),
        interval_ms: interval_ms.unwrap_or(0),
        release_before,
        release_after,
    })
}

pub(super) fn drag_options_from_fields(
    duration_ms: Option<u64>,
    steps: Option<u32>,
    bounds_policy: Option<&str>,
    backend: Option<&str>,
) -> Result<peekaboox_input::DragMouseOptions, String> {
    if steps == Some(0) {
        return Err("drag steps must be greater than zero".to_owned());
    }

    Ok(peekaboox_input::DragMouseOptions {
        duration_ms: duration_ms.unwrap_or(250),
        steps,
        bounds_policy: parse_move_bounds_policy(bounds_policy.unwrap_or("allow"))?,
        backend: parse_drag_backend_selection(backend.unwrap_or("auto"))?,
    })
}

pub(super) fn parse_move_bounds_policy(
    value: &str,
) -> Result<peekaboox_input::MoveBoundsPolicy, String> {
    match value.trim() {
        "" | "allow" => Ok(peekaboox_input::MoveBoundsPolicy::Allow),
        "clamp" => Ok(peekaboox_input::MoveBoundsPolicy::Clamp),
        "fail" | "fail-out-of-bounds" => Ok(peekaboox_input::MoveBoundsPolicy::Fail),
        value => Err(format!(
            "bounds_policy must be allow, clamp, or fail, got {value:?}"
        )),
    }
}

pub(super) fn parse_input_backend_selection(
    value: &str,
) -> Result<peekaboox_input::InputToolSelection, String> {
    match value.trim() {
        "" | "auto" => Ok(peekaboox_input::InputToolSelection::Auto),
        "uinput" => Ok(peekaboox_input::InputToolSelection::Uinput),
        "ydotool" => Ok(peekaboox_input::InputToolSelection::Ydotool),
        "xdotool" => Ok(peekaboox_input::InputToolSelection::Xdotool),
        value => Err(format!(
            "backend must be auto, uinput, ydotool, or xdotool, got {value:?}"
        )),
    }
}

pub(super) fn parse_type_backend_selection(
    value: &str,
) -> Result<peekaboox_input::InputToolSelection, String> {
    match value.trim() {
        "" | "auto" => Ok(peekaboox_input::InputToolSelection::Auto),
        "wtype" => Ok(peekaboox_input::InputToolSelection::Wtype),
        "ydotool" => Ok(peekaboox_input::InputToolSelection::Ydotool),
        "xdotool" => Ok(peekaboox_input::InputToolSelection::Xdotool),
        value => Err(format!(
            "backend must be auto, wtype, ydotool, or xdotool for type, got {value:?}"
        )),
    }
}

pub(super) fn parse_clipboard_backend_selection(
    value: &str,
) -> Result<peekaboox_input::ClipboardBackendSelection, String> {
    match value.trim() {
        "" | "auto" => Ok(peekaboox_input::ClipboardBackendSelection::Auto),
        "wl-copy" | "wlcopy" => Ok(peekaboox_input::ClipboardBackendSelection::WlCopy),
        "xclip" => Ok(peekaboox_input::ClipboardBackendSelection::Xclip),
        "xsel" => Ok(peekaboox_input::ClipboardBackendSelection::Xsel),
        value => Err(format!(
            "clipboard_backend must be auto, wl-copy, xclip, or xsel, got {value:?}"
        )),
    }
}

pub(super) fn parse_paste_hotkey_backend_selection(
    value: &str,
) -> Result<peekaboox_input::PasteHotkeyBackendSelection, String> {
    match value.trim() {
        "" | "auto" => Ok(peekaboox_input::PasteHotkeyBackendSelection::Auto),
        "ydotool" => Ok(peekaboox_input::PasteHotkeyBackendSelection::Ydotool),
        "xdotool" => Ok(peekaboox_input::PasteHotkeyBackendSelection::Xdotool),
        value => Err(format!(
            "hotkey_backend must be auto, ydotool, or xdotool, got {value:?}"
        )),
    }
}

pub(super) fn parse_hotkey_backend_selection(
    value: &str,
) -> Result<peekaboox_input::InputToolSelection, String> {
    match value.trim() {
        "" | "auto" => Ok(peekaboox_input::InputToolSelection::Auto),
        "ydotool" => Ok(peekaboox_input::InputToolSelection::Ydotool),
        "xdotool" => Ok(peekaboox_input::InputToolSelection::Xdotool),
        value => Err(format!(
            "backend must be auto, ydotool, or xdotool for hotkey, got {value:?}"
        )),
    }
}

pub(super) fn parse_clipboard_restore_policy(
    value: &str,
) -> Result<peekaboox_input::ClipboardRestorePolicy, String> {
    match value.trim() {
        "" | "strict" => Ok(peekaboox_input::ClipboardRestorePolicy::Strict),
        "best-effort" | "best_effort" => Ok(peekaboox_input::ClipboardRestorePolicy::BestEffort),
        "off" => Ok(peekaboox_input::ClipboardRestorePolicy::Off),
        value => Err(format!(
            "restore_policy must be strict, best-effort, or off, got {value:?}"
        )),
    }
}

pub(super) fn parse_drag_backend_selection(
    value: &str,
) -> Result<peekaboox_input::InputToolSelection, String> {
    match value.trim() {
        "" | "auto" => Ok(peekaboox_input::InputToolSelection::Auto),
        "uinput" => Ok(peekaboox_input::InputToolSelection::Uinput),
        "xdotool" => Ok(peekaboox_input::InputToolSelection::Xdotool),
        value => Err(format!(
            "backend must be auto, uinput, or xdotool for drag, got {value:?}"
        )),
    }
}

pub(super) fn validate_hotkey_keys(keys: &[String]) -> Result<(), Status> {
    if keys.is_empty() {
        return Err(Status::invalid_argument(
            "hotkey must contain at least one key",
        ));
    }

    if keys.iter().any(|key| key.trim().is_empty()) {
        return Err(Status::invalid_argument("hotkey keys must not be empty"));
    }

    Ok(())
}

pub(super) fn backend_kind_name(kind: BackendKind) -> String {
    format!("{kind:?}").to_ascii_lowercase()
}
