use super::*;

pub(super) fn proto_window_info(window: &WindowInfo) -> proto::WindowInfo {
    proto::WindowInfo {
        id: window.id.clone(),
        title: window.title.clone(),
        app_id: window.app_id.clone(),
        bounds: Some(proto_rect(window.bounds)),
        focused: window.focused,
        state: format!("{:?}", window.state).to_ascii_lowercase(),
    }
}

pub(super) fn proto_window_backend_report(
    report: &peekaboox_windows::WindowBackendReport,
) -> proto::WindowBackendReport {
    proto::WindowBackendReport {
        backend_name: report.backend_name.clone(),
        backend_kind: backend_kind_name(report.backend_kind),
        raw_window_count: report.raw_window_count as u32,
        matched_window_count: report.matched_window_count as u32,
        selected: report.selected,
        error: report.error.clone(),
    }
}

pub(super) fn proto_ui_element(element: &UiElement) -> proto::UiElement {
    proto::UiElement {
        id: element.id.clone(),
        role: element.role.clone(),
        label: element.label.clone(),
        bounds: Some(proto_rect(element.bounds)),
        confidence: element.confidence,
        states: element.states.clone(),
        center: element
            .center
            .or_else(|| element.bounds.center())
            .map(proto_point),
        window_id: element.window_id.clone(),
        window_title: element.window_title.clone(),
        app_id: element.app_id.clone(),
        parent_id: element.parent_id.clone(),
        child_ids: element.child_ids.clone(),
    }
}

pub(super) fn proto_detect_ui_elements_response(
    elements: &[UiElement],
) -> proto::DetectUiElementsResponse {
    proto::DetectUiElementsResponse {
        backend_name: VISION_UI_BACKEND_NAME.to_owned(),
        backend_kind: VISION_UI_BACKEND_KIND.to_owned(),
        warnings: Vec::new(),
        elements: elements.iter().map(proto_ui_element).collect(),
    }
}

pub(super) fn ui_element_list_dto(elements: &[UiElement]) -> ElementListResultDto {
    ElementListResultDto {
        backend_name: VISION_UI_BACKEND_NAME.to_owned(),
        backend_kind: VISION_UI_BACKEND_KIND.to_owned(),
        warnings: Vec::new(),
        cache_hit: false,
        cache_age_ms: 0,
        vision_fallback_used: false,
        elements: elements.iter().map(ElementDto::from).collect(),
    }
}

pub(super) fn element_lookup_dto(result: &ElementLookupResult) -> ElementListResultDto {
    ElementListResultDto {
        backend_name: result.backend_name.clone(),
        backend_kind: result.backend_kind.clone(),
        warnings: result.warnings.clone(),
        cache_hit: result.cache_hit,
        cache_age_ms: result.cache_age_ms,
        vision_fallback_used: result.vision_fallback_used,
        elements: result.elements.iter().map(ElementDto::from).collect(),
    }
}

pub(super) fn proto_ocr_response(result: &OcrResult) -> proto::OcrResponse {
    proto::OcrResponse {
        backend_name: result.backend_name.clone(),
        text: result.text.clone(),
        blocks: result.blocks.iter().map(proto_ocr_block).collect(),
        warnings: result.warnings.clone(),
        words: result.words.iter().map(proto_ocr_block).collect(),
    }
}

pub(super) fn proto_ocr_block(block: &peekaboox_vision::OcrText) -> proto::OcrBlock {
    proto::OcrBlock {
        text: block.text.clone(),
        element: Some(proto_ui_element(&block.element)),
    }
}

pub(super) fn ocr_result_dto(result: &OcrResult) -> OcrResultDto {
    OcrResultDto {
        backend_name: result.backend_name.clone(),
        text: result.text.clone(),
        blocks: result
            .blocks
            .iter()
            .map(|block| OcrBlockDto {
                text: block.text.clone(),
                element: ElementDto::from(&block.element),
            })
            .collect(),
        words: result
            .words
            .iter()
            .map(|word| OcrBlockDto {
                text: word.text.clone(),
                element: ElementDto::from(&word.element),
            })
            .collect(),
        warnings: result.warnings.clone(),
    }
}

pub(super) fn proto_capture_delta_response(data: &CaptureDeltaData) -> proto::CaptureDeltaResponse {
    proto::CaptureDeltaResponse {
        stream_id: data.stream_id.clone(),
        sequence: data.delta.sequence,
        low_bandwidth: data.low_bandwidth,
        full_frame: data.delta.full_frame,
        frame_width: data.delta.frame_width,
        frame_height: data.delta.frame_height,
        pixel_format: proto_pixel_format(data.delta.format),
        changed_bounds: data.delta.changed_bounds.map(proto_rect),
        changed_pixels: data.delta.changed_pixels,
        changed_ratio: data.delta.changed_ratio,
        patch_stride: data.delta.patch_stride,
        patch: data.delta.patch_data.clone(),
        metadata: Some(capture_delta_metadata(data)),
        capture_region: data.capture_region.map(proto_rect),
    }
}

pub(super) fn capture_delta_dto(data: &CaptureDeltaData) -> CaptureDeltaResultDto {
    CaptureDeltaResultDto {
        stream_id: data.stream_id.clone(),
        sequence: data.delta.sequence,
        low_bandwidth: data.low_bandwidth,
        frame_width: data.delta.frame_width,
        frame_height: data.delta.frame_height,
        pixel_format: pixel_format_name(data.delta.format).to_owned(),
        full_frame: data.delta.full_frame,
        capture_region: data.capture_region.map(Into::into),
        changed_bounds: data.delta.changed_bounds.map(Into::into),
        changed_pixels: data.delta.changed_pixels,
        changed_ratio: data.delta.changed_ratio,
        patch_stride: data.delta.patch_stride,
        patch_base64: BASE64_STANDARD.encode(&data.delta.patch_data),
        backend_name: data.backend_name.clone(),
        backend_kind: backend_kind_name(data.backend_kind),
        captured_at_unix_ms: data.captured_at_unix_ms,
    }
}

pub(super) fn proto_capture_backends_response(
    result: CaptureBackendsResultDto,
) -> proto::CaptureBackendsResponse {
    proto::CaptureBackendsResponse {
        session_type: result.session_type,
        desktop: result.desktop,
        pipewire_session_available: result.pipewire_session_available,
        pipewire_backend_feature_enabled: result.pipewire_backend_feature_enabled,
        egl_backend_feature_enabled: result.egl_backend_feature_enabled,
        output_path: result.output_path,
        region: result.region.map(rect_dto_to_proto),
        image_backends: result
            .image_backends
            .into_iter()
            .map(proto_capture_backend)
            .collect(),
        zero_copy_backends: result
            .zero_copy_backends
            .into_iter()
            .map(proto_zero_copy_backend)
            .collect(),
        probes: result
            .probes
            .into_iter()
            .map(proto_capture_backend_probe_result)
            .collect(),
        warnings: result.warnings,
    }
}

pub(super) fn proto_capture_backend(backend: CaptureBackendDto) -> proto::CaptureBackend {
    proto::CaptureBackend {
        name: backend.name,
        backend_kind: backend.backend_kind,
        command: backend.command,
        available: backend.available,
        supports_output: backend.supports_output,
        supports_file_capture: backend.supports_file_capture,
        supports_stdout_capture: backend.supports_stdout_capture,
        supports_stdout_region_capture: backend.supports_stdout_region_capture,
        selected: backend.selected,
        reason: backend.reason,
    }
}

pub(super) fn proto_zero_copy_backend(backend: ZeroCopyBackendDto) -> proto::ZeroCopyBackend {
    proto::ZeroCopyBackend {
        name: backend.name,
        backend_kind: backend.backend_kind,
        transport: backend.transport,
        availability: backend.availability,
        selected: backend.selected,
        pipewire_backend_feature_enabled: backend.pipewire_backend_feature_enabled,
        egl_backend_feature_enabled: backend.egl_backend_feature_enabled,
        reason: backend.reason,
    }
}

pub(super) fn proto_capture_backend_probe_result(
    probe: CaptureBackendProbeResultDto,
) -> proto::CaptureBackendProbeResult {
    proto::CaptureBackendProbeResult {
        probe: probe.probe,
        ok: probe.ok,
        backend_name: probe.backend_name,
        backend_kind: probe.backend_kind,
        detail: probe.detail,
        output_path: probe.output_path,
        bytes_written: probe.bytes_written,
        width: probe.width,
        height: probe.height,
    }
}

pub(super) fn plugin_list_dto(
    result: peekaboox_plugins::PluginDiscoveryResult,
) -> PluginListResultDto {
    PluginListResultDto {
        sdk_version: peekaboox_plugins::PLUGIN_SDK_VERSION.to_owned(),
        plugins: result.plugins.iter().map(plugin_dto).collect(),
        errors: result
            .errors
            .iter()
            .map(|error| PluginDiscoveryErrorDto {
                path: error.path.display().to_string(),
                message: error.message.clone(),
            })
            .collect(),
    }
}

pub(super) fn plugin_dto(plugin: &peekaboox_plugins::PluginDescriptor) -> PluginDto {
    let entrypoint = plugin.manifest.entrypoint.as_ref();
    PluginDto {
        id: plugin.manifest.id.clone(),
        name: plugin.manifest.name.clone(),
        version: plugin.manifest.version.clone(),
        description: plugin.manifest.description.clone(),
        root_dir: plugin.root_dir.display().to_string(),
        manifest_path: plugin.manifest_path.display().to_string(),
        capabilities: plugin.manifest.capabilities.clone(),
        entrypoint_kind: entrypoint.map(|entrypoint| match entrypoint.kind {
            peekaboox_plugins::PluginEntrypointKind::Process => "process".to_owned(),
        }),
        entrypoint_command: entrypoint
            .map(|entrypoint| entrypoint.command.clone())
            .unwrap_or_default(),
        tools: plugin
            .manifest
            .tools
            .iter()
            .map(|tool| PluginToolDto {
                name: tool.name.clone(),
                description: tool.description.clone(),
                capabilities: tool.capabilities.clone(),
                input_schema_json: serde_json::to_string(&tool.input_schema)
                    .unwrap_or_else(|_| "{}".to_owned()),
            })
            .collect(),
        metadata: plugin.manifest.metadata.clone(),
    }
}

pub(super) fn plugin_execution_dto(
    result: peekaboox_plugins::PluginToolExecutionResult,
) -> PluginToolExecutionResultDto {
    PluginToolExecutionResultDto {
        ok: result.ok,
        plugin_id: result.plugin_id,
        tool: result.tool,
        exit_code: result.exit_code,
        stdout: result.stdout,
        stderr: result.stderr,
        result: result.result,
        error: result.error,
    }
}

pub(super) fn proto_plugin_list_response(
    result: peekaboox_plugins::PluginDiscoveryResult,
) -> proto::PluginListResponse {
    proto::PluginListResponse {
        sdk_version: peekaboox_plugins::PLUGIN_SDK_VERSION.to_owned(),
        plugins: result.plugins.iter().map(proto_plugin).collect(),
        errors: result
            .errors
            .iter()
            .map(|error| proto::PluginDiscoveryError {
                path: error.path.display().to_string(),
                message: error.message.clone(),
            })
            .collect(),
    }
}

pub(super) fn proto_plugin(plugin: &peekaboox_plugins::PluginDescriptor) -> proto::Plugin {
    let entrypoint = plugin.manifest.entrypoint.as_ref();
    proto::Plugin {
        id: plugin.manifest.id.clone(),
        name: plugin.manifest.name.clone(),
        version: plugin.manifest.version.clone(),
        description: plugin.manifest.description.clone(),
        root_dir: plugin.root_dir.display().to_string(),
        manifest_path: plugin.manifest_path.display().to_string(),
        capabilities: plugin.manifest.capabilities.clone(),
        entrypoint_kind: entrypoint.map(|entrypoint| match entrypoint.kind {
            peekaboox_plugins::PluginEntrypointKind::Process => "process".to_owned(),
        }),
        entrypoint_command: entrypoint
            .map(|entrypoint| entrypoint.command.clone())
            .unwrap_or_default(),
        tools: plugin
            .manifest
            .tools
            .iter()
            .map(|tool| proto::PluginTool {
                name: tool.name.clone(),
                description: tool.description.clone(),
                capabilities: tool.capabilities.clone(),
                input_schema_json: serde_json::to_string(&tool.input_schema)
                    .unwrap_or_else(|_| "{}".to_owned()),
            })
            .collect(),
        metadata: plugin.manifest.metadata.clone().into_iter().collect(),
    }
}

pub(super) fn proto_plugin_execution_response(
    result: peekaboox_plugins::PluginToolExecutionResult,
) -> proto::PluginToolExecutionResponse {
    proto::PluginToolExecutionResponse {
        ok: result.ok,
        plugin_id: result.plugin_id,
        tool: result.tool,
        exit_code: result.exit_code,
        stdout: result.stdout,
        stderr: result.stderr,
        result_json: result
            .result
            .and_then(|value| serde_json::to_string(&value).ok()),
        error: result.error,
    }
}

pub(super) fn proto_desktop_action_response(
    result: peekaboox_desktop::DesktopActionResult,
) -> proto::DesktopActionResponse {
    proto::DesktopActionResponse {
        app: result.app,
        action: result.action,
        detail: result.detail,
        backend_name: result.backend_name,
        verified: result.verified,
        verification_detail: result.verification_detail,
        focus_diagnostics: result.focus_diagnostics,
    }
}

pub(super) fn proto_desktop_locate_response(
    result: peekaboox_desktop::ResolvedDesktopTarget,
) -> proto::DesktopLocateResponse {
    proto::DesktopLocateResponse {
        app: result.app,
        target: result.target,
        point: Some(proto::Point {
            x: result.point.x,
            y: result.point.y,
        }),
        rect: result.rect.map(proto_rect),
        source: result.source.label().to_owned(),
    }
}

pub(super) fn proto_desktop_profiles_response(
    result: peekaboox_desktop::DesktopProfileList,
) -> proto::DesktopProfilesResponse {
    proto::DesktopProfilesResponse {
        schema_version: result.schema_version,
        count: result.count as u64,
        profiles: result
            .profiles
            .into_iter()
            .map(proto_desktop_profile)
            .collect(),
    }
}

pub(super) fn proto_desktop_profile(
    profile: peekaboox_desktop::DesktopProfileInfo,
) -> proto::DesktopProfile {
    proto::DesktopProfile {
        id: profile.id,
        aliases: profile.aliases,
        search_name: profile.search_name,
        desktop_ids: profile.desktop_ids,
        commands: profile
            .commands
            .into_iter()
            .map(|command| proto::DesktopProfileCommand {
                program: command.program,
                args: command.args,
                display: command.display,
                available: command.available,
            })
            .collect(),
        targets: profile
            .targets
            .into_iter()
            .map(|target| proto::DesktopProfileTarget {
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
        availability: Some(proto::DesktopProfileAvailability {
            checked: profile.availability.checked,
            installed: profile.availability.installed,
            command_available: profile.availability.command_available,
            desktop_entry_available: profile.availability.desktop_entry_available,
            available_commands: profile.availability.available_commands,
            available_desktop_ids: profile.availability.available_desktop_ids,
        }),
    }
}

pub(super) fn proto_desktop_assertion(
    value: i32,
    expected_text: Option<String>,
) -> Result<DesktopAssertion, Status> {
    match proto::DesktopAssertionKind::try_from(value) {
        Ok(proto::DesktopAssertionKind::Unspecified) | Ok(proto::DesktopAssertionKind::Present) => {
            Ok(DesktopAssertion::Present)
        }
        Ok(proto::DesktopAssertionKind::NotPresent) => Ok(DesktopAssertion::NotPresent),
        Ok(proto::DesktopAssertionKind::Active) => Ok(DesktopAssertion::Active),
        Ok(proto::DesktopAssertionKind::NotActive) => Ok(DesktopAssertion::NotActive),
        Ok(proto::DesktopAssertionKind::Contains) => Ok(DesktopAssertion::Contains(
            required_expected_text("contains", expected_text).map_err(Status::invalid_argument)?,
        )),
        Ok(proto::DesktopAssertionKind::NotContains) => Ok(DesktopAssertion::NotContains(
            required_expected_text("not_contains", expected_text)
                .map_err(Status::invalid_argument)?,
        )),
        Err(_) => Err(Status::invalid_argument("unknown desktop assertion")),
    }
}

pub(super) fn capture_backend_probe_from_proto(
    value: i32,
) -> Result<CaptureBackendProbeDto, Status> {
    match value {
        0 | 1 => Ok(CaptureBackendProbeDto::None),
        2 => Ok(CaptureBackendProbeDto::File),
        3 => Ok(CaptureBackendProbeDto::Frame),
        4 => Ok(CaptureBackendProbeDto::Region),
        5 => Ok(CaptureBackendProbeDto::DmaBuf),
        6 => Ok(CaptureBackendProbeDto::All),
        other => Err(Status::invalid_argument(format!(
            "unknown capture backend probe: {other}"
        ))),
    }
}

pub(super) fn proto_dmabuf_import_target(value: i32) -> Result<DmaBufImportTargetDto, Status> {
    match value {
        0 | 1 => Ok(DmaBufImportTargetDto::Compute),
        2 => Ok(DmaBufImportTargetDto::Egl),
        3 => Ok(DmaBufImportTargetDto::EglTexture),
        _ => Err(Status::invalid_argument("unknown import_target")),
    }
}

pub(super) fn proto_dmabuf_import_target_value(value: DmaBufImportTargetDto) -> i32 {
    match value {
        DmaBufImportTargetDto::Compute => 1,
        DmaBufImportTargetDto::Egl => 2,
        DmaBufImportTargetDto::EglTexture => 3,
    }
}

pub(super) fn proto_dmabuf_probe_response(
    result: DmaBufProbeResultDto,
) -> proto::DmaBufProbeResponse {
    proto::DmaBufProbeResponse {
        import_target: proto_dmabuf_import_target_value(result.import_target),
        backend_name: result.backend_name,
        stream_node_id: result.stream_node_id,
        pipewire_serial: result.pipewire_serial,
        width: result.width,
        height: result.height,
        pixel_format: result.pixel_format,
        fourcc: result.fourcc,
        planes: result.planes as u32,
        memory_layout: result.memory_layout,
        synchronization: result.synchronization,
        egl_version: result.egl_version,
        egl_modifiers: result.egl_modifiers,
        texture_id: result.texture_id,
    }
}

pub(super) fn capture_delta_metadata(data: &CaptureDeltaData) -> proto::CaptureMetadata {
    proto::CaptureMetadata {
        width: data.delta.frame_width,
        height: data.delta.frame_height,
        backend: format!(
            "{}/{}",
            data.backend_name,
            backend_kind_name(data.backend_kind)
        ),
        captured_at_unix_ms: data.captured_at_unix_ms,
    }
}

pub(super) fn proto_pixel_format(format: PixelFormat) -> i32 {
    match format {
        PixelFormat::Rgb8 => proto::PixelFormat::Rgb8 as i32,
        PixelFormat::Rgba8 => proto::PixelFormat::Rgba8 as i32,
        PixelFormat::Bgra8 => proto::PixelFormat::Bgra8 as i32,
    }
}

pub(super) fn pixel_format_name(format: PixelFormat) -> &'static str {
    match format {
        PixelFormat::Rgb8 => "rgb8",
        PixelFormat::Rgba8 => "rgba8",
        PixelFormat::Bgra8 => "bgra8",
    }
}

pub(super) fn proto_visual_diff_response(result: &VisualDiffResult) -> proto::VisualDiffResponse {
    proto::VisualDiffResponse {
        compared_region: Some(proto_rect(result.compared_region)),
        compared_pixels: result.compared_pixels,
        changed_pixels: result.changed_pixels,
        changed_ratio: result.changed_ratio,
        mean_absolute_error: result.mean_absolute_error,
        max_channel_delta: u32::from(result.max_channel_delta),
        changed_bounds: result.changed_bounds.map(proto_rect),
        matches: result.matches,
    }
}

pub(super) fn visual_diff_dto(result: &VisualDiffResult) -> VisualDiffDto {
    VisualDiffDto {
        compared_region: result.compared_region.into(),
        compared_pixels: result.compared_pixels,
        changed_pixels: result.changed_pixels,
        changed_ratio: result.changed_ratio,
        mean_absolute_error: result.mean_absolute_error,
        max_channel_delta: result.max_channel_delta,
        changed_bounds: result.changed_bounds.map(Into::into),
        matches: result.matches,
    }
}

pub(super) fn proto_ui_state_response(result: &UiStateResult) -> proto::UiStateResponse {
    proto::UiStateResponse {
        state: proto_ui_state_kind(result.state),
        compared_transitions: result.compared_transitions as u64,
        stable_transitions: result.stable_transitions as u64,
        loading_transitions: result.loading_transitions as u64,
        trailing_stable_transitions: result.trailing_stable_transitions as u64,
        latest_diff: Some(proto_visual_diff_response(&result.latest_diff)),
        max_changed_ratio: result.max_changed_ratio,
        mean_changed_ratio: result.mean_changed_ratio,
        changed_bounds: result.changed_bounds.map(proto_rect),
    }
}

pub(super) fn ui_state_dto(result: &UiStateResult) -> UiStateDto {
    UiStateDto {
        state: ui_state_name(result.state).to_owned(),
        compared_transitions: result.compared_transitions as u64,
        stable_transitions: result.stable_transitions as u64,
        loading_transitions: result.loading_transitions as u64,
        trailing_stable_transitions: result.trailing_stable_transitions as u64,
        latest_diff: visual_diff_dto(&result.latest_diff),
        max_changed_ratio: result.max_changed_ratio,
        mean_changed_ratio: result.mean_changed_ratio,
        changed_bounds: result.changed_bounds.map(Into::into),
    }
}

pub(super) fn proto_ui_state_kind(kind: UiStateKind) -> i32 {
    match kind {
        UiStateKind::Stable => 1,
        UiStateKind::Loading => 2,
        UiStateKind::Changing => 3,
    }
}

pub(super) fn ui_state_name(kind: UiStateKind) -> &'static str {
    match kind {
        UiStateKind::Stable => "stable",
        UiStateKind::Loading => "loading",
        UiStateKind::Changing => "changing",
    }
}

pub(super) fn proto_rect(rect: Rect) -> proto::Rect {
    proto::Rect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}

pub(super) fn rect_dto_to_proto(rect: RectDto) -> proto::Rect {
    proto::Rect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}

pub(super) fn proto_point(point: Point) -> proto::Point {
    proto::Point {
        x: point.x,
        y: point.y,
    }
}

pub(super) fn rect_from_proto(rect: proto::Rect) -> Rect {
    Rect::new(rect.x, rect.y, rect.width, rect.height)
}
