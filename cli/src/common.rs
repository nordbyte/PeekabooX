use super::*;

pub(super) fn require_slice_value(
    args: &[String],
    index: &mut usize,
    name: &str,
) -> Result<String, CliError> {
    *index += 1;
    args.get(*index)
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CliError::Failure(format!("missing value for {name}")))
}

pub(super) fn parse_i32(name: &str, value: &str) -> Result<i32, CliError> {
    value
        .parse::<i32>()
        .map_err(|_| CliError::Failure(format!("{name} must be an integer, got {value:?}")))
}

pub(super) fn parse_u64(name: &str, value: &str) -> Result<u64, CliError> {
    value
        .parse::<u64>()
        .map_err(|_| CliError::Failure(format!("{name} must be an integer, got {value:?}")))
}

pub(super) fn parse_usize(name: &str, value: &str) -> Result<usize, CliError> {
    value
        .parse::<usize>()
        .map_err(|_| CliError::Failure(format!("{name} must be an integer, got {value:?}")))
}

pub(super) fn parse_positive_usize(name: &str, value: &str) -> Result<usize, CliError> {
    let parsed = parse_usize(name, value)?;
    if parsed == 0 {
        return Err(CliError::Failure(format!(
            "{name} must be greater than zero"
        )));
    }

    Ok(parsed)
}

pub(super) fn parse_u32(name: &str, value: &str) -> Result<u32, CliError> {
    value
        .parse::<u32>()
        .map_err(|_| CliError::Failure(format!("{name} must be an integer, got {value:?}")))
}

pub(super) fn parse_positive_u32(name: &str, value: &str) -> Result<u32, CliError> {
    let parsed = parse_u32(name, value)?;
    if parsed == 0 {
        return Err(CliError::Failure(format!(
            "{name} must be greater than zero"
        )));
    }

    Ok(parsed)
}

pub(super) fn parse_u8(name: &str, value: &str) -> Result<u8, CliError> {
    value
        .parse::<u8>()
        .map_err(|_| CliError::Failure(format!("{name} must be 0..255, got {value:?}")))
}

pub(super) fn parse_ocr_psm(value: &str) -> Result<u8, CliError> {
    let parsed = parse_u8("--psm", value)?;
    if parsed > 13 {
        return Err(CliError::Failure(format!(
            "--psm must be between 0 and 13, got {value:?}"
        )));
    }
    Ok(parsed)
}

pub(super) fn parse_ocr_oem(value: &str) -> Result<u8, CliError> {
    let parsed = parse_u8("--oem", value)?;
    if parsed > 3 {
        return Err(CliError::Failure(format!(
            "--oem must be between 0 and 3, got {value:?}"
        )));
    }
    Ok(parsed)
}

pub(super) fn parse_ocr_confidence(value: &str) -> Result<f32, CliError> {
    parse_unit_f32("--min-confidence", value)
}

pub(super) fn parse_ocr_scale(value: &str) -> Result<f32, CliError> {
    let parsed = value
        .parse::<f32>()
        .map_err(|_| CliError::Failure(format!("--scale must be a float, got {value:?}")))?;
    if !parsed.is_finite() || !(0.1..=8.0).contains(&parsed) {
        return Err(CliError::Failure(format!(
            "--scale must be between 0.1 and 8.0, got {value:?}"
        )));
    }
    Ok(parsed)
}

pub(super) fn parse_ocr_contrast(value: &str) -> Result<f32, CliError> {
    let parsed = value
        .parse::<f32>()
        .map_err(|_| CliError::Failure(format!("--contrast must be a float, got {value:?}")))?;
    if !parsed.is_finite() || !(-255.0..=255.0).contains(&parsed) {
        return Err(CliError::Failure(format!(
            "--contrast must be between -255.0 and 255.0, got {value:?}"
        )));
    }
    Ok(parsed)
}

pub(super) fn parse_ocr_config(value: &str) -> Result<OcrConfig, CliError> {
    let Some((key, config_value)) = value.split_once('=') else {
        return Err(CliError::Failure(
            "--config must be key=value for Tesseract -c options".to_owned(),
        ));
    };
    let key = key.trim();
    if key.is_empty() || key.contains(char::is_whitespace) {
        return Err(CliError::Failure(
            "--config key must be non-empty and contain no whitespace".to_owned(),
        ));
    }
    Ok(OcrConfig {
        key: key.to_owned(),
        value: config_value.to_owned(),
    })
}

pub(super) fn parse_unit_f32(name: &str, value: &str) -> Result<f32, CliError> {
    let parsed = value
        .parse::<f32>()
        .map_err(|_| CliError::Failure(format!("{name} must be a float, got {value:?}")))?;
    if !parsed.is_finite() || !(0.0..=1.0).contains(&parsed) {
        return Err(CliError::Failure(format!(
            "{name} must be between 0.0 and 1.0, got {value:?}"
        )));
    }

    Ok(parsed)
}

pub(super) fn parse_ui_element_sort(value: &str) -> Result<UiElementSort, CliError> {
    match value {
        "position" => Ok(UiElementSort::Position),
        "area" => Ok(UiElementSort::Area),
        "confidence" => Ok(UiElementSort::Confidence),
        _ => Err(CliError::Failure(format!(
            "--sort must be position, area, or confidence, got {value:?}"
        ))),
    }
}

pub(super) fn ui_element_sort_name(sort: UiElementSort) -> &'static str {
    match sort {
        UiElementSort::Position => "position",
        UiElementSort::Area => "area",
        UiElementSort::Confidence => "confidence",
    }
}

pub(super) fn parse_visual_mae(name: &str, value: &str) -> Result<f32, CliError> {
    let parsed = value
        .parse::<f32>()
        .map_err(|_| CliError::Failure(format!("{name} must be a float, got {value:?}")))?;
    if !parsed.is_finite() || !(0.0..=255.0).contains(&parsed) {
        return Err(CliError::Failure(format!(
            "{name} must be between 0.0 and 255.0, got {value:?}"
        )));
    }

    Ok(parsed)
}

pub(super) fn parse_visual_size_policy(value: &str) -> Result<VisualSizePolicy, CliError> {
    match value {
        "error" => Ok(VisualSizePolicy::Error),
        "common-region" => Ok(VisualSizePolicy::CommonRegion),
        "resize-actual" => Ok(VisualSizePolicy::ResizeActual),
        _ => Err(CliError::Failure(format!(
            "--size-policy must be error, common-region, or resize-actual, got {value:?}"
        ))),
    }
}

pub(super) fn parse_visual_alpha_mode(value: &str) -> Result<VisualAlphaMode, CliError> {
    match value {
        "ignore" => Ok(VisualAlphaMode::Ignore),
        "compare" => Ok(VisualAlphaMode::Compare),
        _ => Err(CliError::Failure(format!(
            "--alpha must be ignore or compare, got {value:?}"
        ))),
    }
}

pub(super) fn visual_size_policy_name(policy: VisualSizePolicy) -> &'static str {
    match policy {
        VisualSizePolicy::Error => "error",
        VisualSizePolicy::CommonRegion => "common-region",
        VisualSizePolicy::ResizeActual => "resize-actual",
    }
}

pub(super) fn visual_alpha_mode_name(mode: VisualAlphaMode) -> &'static str {
    match mode {
        VisualAlphaMode::Ignore => "ignore",
        VisualAlphaMode::Compare => "compare",
    }
}

pub(super) fn parse_move_bounds_policy(
    value: &str,
) -> Result<peekaboox_input::MoveBoundsPolicy, CliError> {
    match value {
        "allow" => Ok(peekaboox_input::MoveBoundsPolicy::Allow),
        "clamp" => Ok(peekaboox_input::MoveBoundsPolicy::Clamp),
        "fail" | "fail-out-of-bounds" => Ok(peekaboox_input::MoveBoundsPolicy::Fail),
        _ => Err(CliError::Failure(format!(
            "--bounds must be allow, clamp, or fail, got {value:?}"
        ))),
    }
}

pub(super) fn parse_input_backend_selection(
    value: &str,
) -> Result<peekaboox_input::InputToolSelection, CliError> {
    match value {
        "auto" => Ok(peekaboox_input::InputToolSelection::Auto),
        "uinput" => Ok(peekaboox_input::InputToolSelection::Uinput),
        "ydotool" => Ok(peekaboox_input::InputToolSelection::Ydotool),
        "xdotool" => Ok(peekaboox_input::InputToolSelection::Xdotool),
        _ => Err(CliError::Failure(format!(
            "--backend must be auto, uinput, ydotool, or xdotool, got {value:?}"
        ))),
    }
}

pub(super) fn parse_type_backend_selection(
    value: &str,
) -> Result<peekaboox_input::InputToolSelection, CliError> {
    match value {
        "auto" => Ok(peekaboox_input::InputToolSelection::Auto),
        "wtype" => Ok(peekaboox_input::InputToolSelection::Wtype),
        "ydotool" => Ok(peekaboox_input::InputToolSelection::Ydotool),
        "xdotool" => Ok(peekaboox_input::InputToolSelection::Xdotool),
        _ => Err(CliError::Failure(format!(
            "--backend must be auto, wtype, ydotool, or xdotool for type, got {value:?}"
        ))),
    }
}

pub(super) fn parse_clipboard_backend_selection(
    value: &str,
) -> Result<peekaboox_input::ClipboardBackendSelection, CliError> {
    match value {
        "auto" => Ok(peekaboox_input::ClipboardBackendSelection::Auto),
        "wl-copy" | "wlcopy" => Ok(peekaboox_input::ClipboardBackendSelection::WlCopy),
        "xclip" => Ok(peekaboox_input::ClipboardBackendSelection::Xclip),
        "xsel" => Ok(peekaboox_input::ClipboardBackendSelection::Xsel),
        _ => Err(CliError::Failure(format!(
            "--clipboard-backend must be auto, wl-copy, xclip, or xsel, got {value:?}"
        ))),
    }
}

pub(super) fn parse_paste_hotkey_backend_selection(
    value: &str,
) -> Result<peekaboox_input::PasteHotkeyBackendSelection, CliError> {
    match value {
        "auto" => Ok(peekaboox_input::PasteHotkeyBackendSelection::Auto),
        "ydotool" => Ok(peekaboox_input::PasteHotkeyBackendSelection::Ydotool),
        "xdotool" => Ok(peekaboox_input::PasteHotkeyBackendSelection::Xdotool),
        _ => Err(CliError::Failure(format!(
            "--hotkey-backend must be auto, ydotool, or xdotool, got {value:?}"
        ))),
    }
}

pub(super) fn parse_hotkey_backend_selection(
    value: &str,
) -> Result<peekaboox_input::InputToolSelection, CliError> {
    match value {
        "auto" => Ok(peekaboox_input::InputToolSelection::Auto),
        "ydotool" => Ok(peekaboox_input::InputToolSelection::Ydotool),
        "xdotool" => Ok(peekaboox_input::InputToolSelection::Xdotool),
        _ => Err(CliError::Failure(format!(
            "--backend must be auto, ydotool, or xdotool for hotkey, got {value:?}"
        ))),
    }
}

pub(super) fn parse_clipboard_restore_policy(
    value: &str,
) -> Result<peekaboox_input::ClipboardRestorePolicy, CliError> {
    match value {
        "strict" => Ok(peekaboox_input::ClipboardRestorePolicy::Strict),
        "best-effort" | "best_effort" => Ok(peekaboox_input::ClipboardRestorePolicy::BestEffort),
        "off" => Ok(peekaboox_input::ClipboardRestorePolicy::Off),
        _ => Err(CliError::Failure(format!(
            "--restore-policy must be strict, best-effort, or off, got {value:?}"
        ))),
    }
}

pub(super) fn parse_drag_backend_selection(
    value: &str,
) -> Result<peekaboox_input::InputToolSelection, CliError> {
    match value {
        "auto" => Ok(peekaboox_input::InputToolSelection::Auto),
        "uinput" => Ok(peekaboox_input::InputToolSelection::Uinput),
        "xdotool" => Ok(peekaboox_input::InputToolSelection::Xdotool),
        _ => Err(CliError::Failure(format!(
            "--backend must be auto, uinput, or xdotool for drag, got {value:?}"
        ))),
    }
}

pub(super) fn parse_ratio_pair(name: &str, value: &str) -> Result<(f32, f32), CliError> {
    let parts = value
        .split([',', ':', ';', '/'])
        .flat_map(str::split_whitespace)
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() != 2 {
        return Err(CliError::Failure(format!(
            "{name} must be x,y ratios, got {value:?}"
        )));
    }

    Ok((
        parse_unit_f32(name, parts[0])?,
        parse_unit_f32(name, parts[1])?,
    ))
}

pub(super) fn parse_rect(name: &str, value: &str) -> Result<Rect, CliError> {
    let parts = value
        .split([',', ':', 'x', ';', '/'])
        .flat_map(str::split_whitespace)
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() != 4 {
        return Err(CliError::Failure(format!(
            "{name} must be x,y,width,height, got {value:?}"
        )));
    }

    let x = parse_i32(name, parts[0])?;
    let y = parse_i32(name, parts[1])?;
    let width = parts[2]
        .parse::<u32>()
        .map_err(|_| CliError::Failure(format!("{name} width must be positive, got {value:?}")))?;
    let height = parts[3]
        .parse::<u32>()
        .map_err(|_| CliError::Failure(format!("{name} height must be positive, got {value:?}")))?;
    if width == 0 || height == 0 {
        return Err(CliError::Failure(format!(
            "{name} width and height must be greater than zero"
        )));
    }

    Ok(Rect::new(x, y, width, height))
}

pub(super) fn parse_point(name: &str, value: &str) -> Result<Point, CliError> {
    let parts = value
        .split([',', ':', ';', '/'])
        .flat_map(str::split_whitespace)
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() != 2 {
        return Err(CliError::Failure(format!(
            "{name} must be x,y, got {value:?}"
        )));
    }

    Ok(Point::new(
        parse_i32(name, parts[0])?,
        parse_i32(name, parts[1])?,
    ))
}

pub(super) fn parse_next_string(
    args: &[String],
    index: &mut usize,
    name: &str,
) -> Result<String, CliError> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| CliError::Failure(format!("missing value for {name}")))
}

pub(super) fn require_next_arg(
    args: &mut impl Iterator<Item = String>,
    name: &str,
) -> Result<String, CliError> {
    args.next()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CliError::Failure(format!("missing value for {name}")))
}

pub(super) fn non_empty_cli_string(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

pub(super) fn parse_mouse_button(value: &str) -> Result<MouseButton, CliError> {
    match value {
        "left" => Ok(MouseButton::Left),
        "middle" => Ok(MouseButton::Middle),
        "right" => Ok(MouseButton::Right),
        _ => Err(CliError::Failure(format!(
            "--button must be left, middle, or right, got {value:?}"
        ))),
    }
}

pub(super) fn mouse_button_dto(button: MouseButton) -> MouseButtonDto {
    match button {
        MouseButton::Left => MouseButtonDto::Left,
        MouseButton::Middle => MouseButtonDto::Middle,
        MouseButton::Right => MouseButtonDto::Right,
    }
}

pub(super) fn mouse_button_label(button: MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "left",
        MouseButton::Middle => "middle",
        MouseButton::Right => "right",
    }
}

pub(super) fn desktop_assertion_dto(
    assertion: &DesktopAssertion,
) -> (DesktopAssertionDto, Option<String>) {
    match assertion {
        DesktopAssertion::Present => (DesktopAssertionDto::Present, None),
        DesktopAssertion::NotPresent => (DesktopAssertionDto::NotPresent, None),
        DesktopAssertion::Active => (DesktopAssertionDto::Active, None),
        DesktopAssertion::NotActive => (DesktopAssertionDto::NotActive, None),
        DesktopAssertion::Contains(expected) => {
            (DesktopAssertionDto::Contains, Some(expected.clone()))
        }
        DesktopAssertion::NotContains(expected) => {
            (DesktopAssertionDto::NotContains, Some(expected.clone()))
        }
    }
}

pub(super) fn path_to_daemon_string(path: &PathBuf) -> Result<String, CliError> {
    let absolute = if path.is_absolute() {
        path.clone()
    } else {
        std::env::current_dir()
            .map_err(|error| {
                CliError::Failure(format!("failed to resolve current directory: {error}"))
            })?
            .join(path)
    };
    Ok(absolute.display().to_string())
}

pub(super) fn input_metadata_dto(
    metadata: peekaboox_input::InputExecutionMetadata,
) -> ActionResultDto {
    ActionResultDto {
        backend_name: metadata.backend_name,
        backend_kind: format!("{:?}", metadata.backend_kind).to_ascii_lowercase(),
    }
}

pub(super) fn daemon_request(
    context: &CliContext,
    request: ApiRequest,
) -> Result<ApiResult, CliError> {
    let response = send_request(&context.socket, request).map_err(|error| {
        CliError::Failure(format!(
            "failed to talk to daemon at {}: {error}",
            context.socket.display()
        ))
    })?;

    match response.response {
        ApiResponse::Ok { result } => Ok(*result),
        ApiResponse::Error { message } => Err(CliError::Failure(message)),
    }
}

pub(super) fn print_json_pretty(value: &impl serde::Serialize) -> Result<(), CliError> {
    println!(
        "{}",
        serde_json::to_string_pretty(value)
            .map_err(|error| CliError::Failure(error.to_string()))?
    );
    Ok(())
}

pub(super) fn write_json_pretty_file(
    path: &Path,
    value: &impl serde::Serialize,
) -> Result<(), CliError> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|error| {
            CliError::Failure(format!("failed to create {}: {error}", parent.display()))
        })?;
    }
    let json = serde_json::to_string_pretty(value)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    std::fs::write(path, format!("{json}\n"))
        .map_err(|error| CliError::Failure(format!("failed to write {}: {error}", path.display())))
}

pub(super) fn write_json_pretty_file_no_overwrite(
    path: &Path,
    value: &impl serde::Serialize,
) -> Result<(), CliError> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|error| {
            CliError::Failure(format!("failed to create {}: {error}", parent.display()))
        })?;
    }
    ensure_path_absent(path, "JSON output")?;
    let json = serde_json::to_string_pretty(value)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            CliError::Failure(format!("failed to create {}: {error}", path.display()))
        })?;
    file.write_all(format!("{json}\n").as_bytes())
        .map_err(|error| CliError::Failure(format!("failed to write {}: {error}", path.display())))
}

pub(super) fn ensure_path_absent(path: &Path, description: &str) -> Result<(), CliError> {
    if path.exists() {
        return Err(CliError::Failure(format!(
            "{description} already exists: {}",
            path.display()
        )));
    }
    Ok(())
}
