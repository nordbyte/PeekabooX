use super::*;

pub(super) fn grpc_ocr_screen(
    request: proto::OcrScreenRequest,
) -> Result<proto::OcrResponse, Status> {
    let result = run_ocr(OcrRunRequest {
        image_path: request.image_path,
        region: request.region.map(rect_from_proto),
        app: request.app,
        window_title: request.window_title,
        window_id: request.window_id,
        options: ocr_options(OcrOptionInput {
            language: request.language,
            page_segmentation_mode: request
                .page_segmentation_mode
                .map(|value| u8::try_from(value).unwrap_or(u8::MAX)),
            engine_mode: request
                .engine_mode
                .map(|value| u8::try_from(value).unwrap_or(u8::MAX)),
            dpi: request.dpi,
            min_confidence: request.min_confidence,
            whitelist: request.whitelist,
            config: request.config,
            scale: request.scale,
            grayscale: request.grayscale.unwrap_or(false),
            threshold: request
                .threshold
                .map(|value| u8::try_from(value).unwrap_or(u8::MAX)),
            invert: request.invert.unwrap_or(false),
            contrast: request.contrast,
            deskew: request.deskew.unwrap_or(false),
        })
        .map_err(ocr_status)?,
    })
    .map_err(ocr_status)?;

    Ok(proto_ocr_response(&result))
}

pub(super) fn grpc_compare_images(
    request: proto::CompareImagesRequest,
) -> Result<proto::VisualDiffResponse, Status> {
    if request.expected_image.is_empty() || request.actual_image.is_empty() {
        return Err(Status::invalid_argument(
            "expected_image and actual_image must not be empty",
        ));
    }

    let options = visual_compare_options(VisualCompareRequestOptions {
        region: request.region.map(rect_from_proto),
        ignore_regions: request
            .ignore_regions
            .into_iter()
            .map(rect_from_proto)
            .collect(),
        per_channel_threshold: request.per_channel_threshold.unwrap_or_default(),
        max_changed_ratio: request.max_changed_ratio,
        max_changed_pixels: request.max_changed_pixels,
        max_mean_absolute_error: request.max_mean_absolute_error,
        max_channel_delta: request.max_channel_delta,
        size_policy: request.size_policy.as_deref(),
        alpha_mode: request.alpha.as_deref(),
    })?;
    let result = peekaboox_vision::compare_image_bytes(
        &request.expected_image,
        &request.actual_image,
        &options,
    )
    .map_err(|error| Status::invalid_argument(error.to_string()))?;

    Ok(proto_visual_diff_response(&result))
}

pub(super) fn grpc_detect_ui_state(
    request: proto::DetectUiStateRequest,
) -> Result<proto::UiStateResponse, Status> {
    if request.images.len() < 2 {
        return Err(Status::invalid_argument(
            "UI state detection requires at least two images",
        ));
    }
    if request.images.iter().any(Vec::is_empty) {
        return Err(Status::invalid_argument("images must not be empty"));
    }

    let options = ui_state_options(UiStateRequestOptions {
        region: request.region.map(rect_from_proto),
        ignore_regions: request
            .ignore_regions
            .into_iter()
            .map(rect_from_proto)
            .collect(),
        per_channel_threshold: request.per_channel_threshold,
        stable_max_changed_ratio: request.stable_max_changed_ratio,
        stable_max_changed_pixels: request.stable_max_changed_pixels,
        stable_max_mean_absolute_error: request.stable_max_mean_absolute_error,
        stable_max_channel_delta: request.stable_max_channel_delta,
        loading_min_changed_ratio: request.loading_min_changed_ratio,
        loading_min_changed_pixels: request.loading_min_changed_pixels,
        required_stable_transitions: request.required_stable_transitions,
        size_policy: request.size_policy.as_deref(),
        alpha_mode: request.alpha.as_deref(),
    })?;
    let result = peekaboox_vision::detect_ui_state_from_image_bytes(&request.images, &options)
        .map_err(|error| Status::invalid_argument(error.to_string()))?;

    Ok(proto_ui_state_response(&result))
}

pub(super) fn grpc_detect_ui_elements(
    request: proto::DetectUiElementsRequest,
) -> Result<proto::DetectUiElementsResponse, Status> {
    let proto::DetectUiElementsRequest {
        image,
        region,
        edge_threshold,
        min_width,
        min_height,
        min_component_pixels,
        max_elements,
        merge_distance,
        ignore_regions,
        min_confidence,
        max_width,
        max_height,
        min_area,
        max_area,
        padding,
        sort,
        mask_output_path,
        overlay_output_path,
    } = request;

    if image.is_empty() {
        return Err(Status::invalid_argument("image must not be empty"));
    }

    let options = ui_element_detection_options(UiElementDetectionRequestOptions {
        region: region.map(rect_from_proto),
        ignore_regions: ignore_regions.into_iter().map(rect_from_proto).collect(),
        edge_threshold,
        min_width,
        min_height,
        min_component_pixels,
        min_confidence,
        max_width,
        max_height,
        min_area,
        max_area,
        max_elements,
        merge_distance,
        padding,
        sort: sort.as_deref(),
    })?;
    let elements = peekaboox_vision::detect_ui_elements_from_image_bytes_with_outputs(
        &image,
        &options,
        mask_output_path.as_deref().map(Path::new),
        overlay_output_path.as_deref().map(Path::new),
    )
    .map_err(|error| Status::invalid_argument(error.to_string()))?;

    Ok(proto_detect_ui_elements_response(&elements))
}

pub(super) fn grpc_probe_dmabuf(
    request: proto::ProbeDmaBufRequest,
) -> Result<proto::DmaBufProbeResponse, Status> {
    let target = proto_dmabuf_import_target(request.import_target)?;
    let result = probe_dmabuf_import(target).map_err(Status::internal)?;
    Ok(proto_dmabuf_probe_response(result))
}

pub(super) fn grpc_list_plugins(
    request: proto::ListPluginsRequest,
    config: &ServerConfig,
) -> Result<proto::PluginListResponse, Status> {
    let paths = if request.paths.is_empty() {
        config.plugin_paths.clone()
    } else {
        request.paths.into_iter().map(PathBuf::from).collect()
    };
    Ok(proto_plugin_list_response(
        peekaboox_plugins::discover_plugins(&paths),
    ))
}

pub(super) fn grpc_call_plugin_tool(
    request: proto::CallPluginToolRequest,
    config: &ServerConfig,
) -> Result<proto::PluginToolExecutionResponse, Status> {
    ensure_plugin_execution_allowed(config).map_err(Status::permission_denied)?;
    if !request.paths.is_empty() {
        return Err(Status::permission_denied(
            "daemon plugin execution only uses plugin paths configured at daemon startup",
        ));
    }
    if request.plugin_id.trim().is_empty() {
        return Err(Status::invalid_argument("plugin_id must not be empty"));
    }
    if request.tool.trim().is_empty() {
        return Err(Status::invalid_argument("tool must not be empty"));
    }
    let arguments = if request.arguments_json.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(&request.arguments_json)
            .map_err(|error| Status::invalid_argument(format!("invalid arguments_json: {error}")))?
    };
    let paths = config.plugin_paths.clone();
    let discovery = peekaboox_plugins::discover_plugins(&paths);
    if !discovery.errors.is_empty() {
        return Err(Status::failed_precondition(format!(
            "plugin discovery failed: {}",
            discovery
                .errors
                .iter()
                .map(|error| format!("{}: {}", error.path.display(), error.message))
                .collect::<Vec<_>>()
                .join("; ")
        )));
    }
    let plugin = discovery
        .plugins
        .iter()
        .find(|plugin| plugin.manifest.id == request.plugin_id)
        .ok_or_else(|| Status::not_found(format!("unknown plugin: {}", request.plugin_id)))?;
    if request.require_trusted {
        let trust_policy = request.trust_policy.as_deref().map(PathBuf::from);
        peekaboox_plugins::require_plugin_trust(plugin, trust_policy.as_deref())
            .map_err(Status::permission_denied)?;
    }
    let policy = peekaboox_plugins::PluginExecutionPolicy {
        timeout: Duration::from_millis(u64::from(request.timeout_ms.unwrap_or(10_000))),
        max_output_bytes: request
            .max_output_bytes
            .map(|value| value as usize)
            .unwrap_or(1_048_576),
        ..Default::default()
    };
    let result = peekaboox_plugins::execute_plugin_tool(plugin, &request.tool, arguments, &policy)
        .map_err(Status::invalid_argument)?;
    Ok(proto_plugin_execution_response(result))
}

pub(super) fn grpc_desktop_focus(
    request: proto::DesktopFocusRequest,
    config: &ServerConfig,
) -> Result<proto::DesktopActionResponse, Status> {
    ensure_input_allowed(config).map_err(Status::permission_denied)?;
    let result = peekaboox_desktop::focus_app(
        &request.app,
        &DesktopFocusOptions {
            use_gnome_overview: request.use_gnome_overview.unwrap_or(true),
            launch_if_needed: request.launch_if_needed.unwrap_or(true),
            wait_after_focus_ms: request.wait_after_focus_ms.unwrap_or(1_000),
            overview_wait_ms: request.overview_wait_ms.unwrap_or(800),
            window_title: request.window_title,
            window_id: request.window_id,
            verify: request.verify,
        },
    )
    .map_err(|error| Status::failed_precondition(error.to_string()))?;
    Ok(proto_desktop_action_response(result))
}

pub(super) fn grpc_desktop_locate(
    request: proto::DesktopLocateRequest,
) -> Result<proto::DesktopLocateResponse, Status> {
    let result = peekaboox_desktop::locate_target(
        &request.app,
        &request.target,
        &DesktopLocateOptions {
            image: request.image_path.map(PathBuf::from),
            prefer_accessibility: request.prefer_accessibility.unwrap_or(true),
            window_title: request.window_title,
            window_id: request.window_id,
        },
    )
    .map_err(|error| Status::failed_precondition(error.to_string()))?;
    Ok(proto_desktop_locate_response(result))
}

pub(super) fn grpc_desktop_click(
    request: proto::DesktopClickRequest,
    config: &ServerConfig,
) -> Result<proto::DesktopActionResponse, Status> {
    if !request.dry_run {
        ensure_input_allowed(config).map_err(Status::permission_denied)?;
    }
    let result = peekaboox_desktop::click_target(
        &request.app,
        &request.target,
        &DesktopClickOptions {
            locate: DesktopLocateOptions {
                image: request.image_path.map(PathBuf::from),
                prefer_accessibility: request.prefer_accessibility.unwrap_or(true),
                window_title: request.window_title,
                window_id: request.window_id,
            },
            button: proto_mouse_button(request.button)?,
            dry_run: request.dry_run,
            verify: request.verify,
        },
    )
    .map_err(|error| Status::failed_precondition(error.to_string()))?;
    Ok(proto_desktop_action_response(result))
}

pub(super) fn grpc_desktop_drag(
    request: proto::DesktopDragRequest,
    config: &ServerConfig,
) -> Result<proto::DesktopActionResponse, Status> {
    if !request.dry_run {
        ensure_input_allowed(config).map_err(Status::permission_denied)?;
    }
    let from_ratio = (
        request.from_ratio_x.unwrap_or(0.5),
        request.from_ratio_y.unwrap_or(0.5),
    );
    let to_ratio = (
        request.to_ratio_x.unwrap_or(0.5),
        request.to_ratio_y.unwrap_or(0.5),
    );
    validate_ratio_status("from_ratio_x", from_ratio.0)?;
    validate_ratio_status("from_ratio_y", from_ratio.1)?;
    validate_ratio_status("to_ratio_x", to_ratio.0)?;
    validate_ratio_status("to_ratio_y", to_ratio.1)?;
    let result = peekaboox_desktop::drag_target(
        &request.app,
        &request.target,
        &DesktopDragOptions {
            locate: DesktopLocateOptions {
                image: request.image_path.map(PathBuf::from),
                prefer_accessibility: request.prefer_accessibility.unwrap_or(true),
                window_title: request.window_title,
                window_id: request.window_id,
            },
            from_ratio,
            to_ratio,
            button: proto_mouse_button(request.button)?,
            duration_ms: request.duration_ms.unwrap_or(250),
            dry_run: request.dry_run,
            verify: request.verify,
        },
    )
    .map_err(|error| Status::failed_precondition(error.to_string()))?;
    Ok(proto_desktop_action_response(result))
}

pub(super) fn grpc_desktop_type_into(
    request: proto::DesktopTypeIntoRequest,
    config: &ServerConfig,
) -> Result<proto::DesktopActionResponse, Status> {
    if !request.dry_run {
        ensure_input_allowed(config).map_err(Status::permission_denied)?;
    }
    let result = peekaboox_desktop::type_into_target(
        &request.app,
        &request.target,
        &request.text,
        &DesktopTypeIntoOptions {
            locate: DesktopLocateOptions {
                image: request.image_path.map(PathBuf::from),
                prefer_accessibility: request.prefer_accessibility.unwrap_or(true),
                window_title: request.window_title,
                window_id: request.window_id,
            },
            clear: request.clear,
            dry_run: request.dry_run,
            verify: request.verify,
        },
    )
    .map_err(|error| Status::failed_precondition(error.to_string()))?;
    Ok(proto_desktop_action_response(result))
}

pub(super) fn grpc_desktop_assert(
    request: proto::DesktopAssertRequest,
) -> Result<proto::DesktopActionResponse, Status> {
    let result = peekaboox_desktop::assert_target(
        &request.app,
        &request.target,
        &DesktopAssertOptions {
            locate: DesktopLocateOptions {
                image: request.image_path.map(PathBuf::from),
                prefer_accessibility: request.prefer_accessibility.unwrap_or(true),
                window_title: request.window_title,
                window_id: request.window_id,
            },
            assertion: proto_desktop_assertion(request.assertion, request.expected_text)?,
        },
    )
    .map_err(|error| Status::failed_precondition(error.to_string()))?;
    Ok(proto_desktop_action_response(result))
}

pub(super) fn grpc_desktop_profiles(
    request: proto::DesktopProfilesRequest,
) -> Result<proto::DesktopProfilesResponse, Status> {
    let result = peekaboox_desktop::desktop_profiles_with_query(&DesktopProfileQuery {
        app: request.app,
        target: request.target,
        command: request.command,
        desktop_id: request.desktop_id,
        supports: request.supports,
        check_availability: request.check,
        installed_only: request.installed,
        available_only: request.available,
    })
    .map_err(|error| Status::failed_precondition(error.to_string()))?;
    Ok(proto_desktop_profiles_response(result))
}

pub(super) struct VisualCompareRequestOptions<'a> {
    pub(super) region: Option<Rect>,
    pub(super) ignore_regions: Vec<Rect>,
    pub(super) per_channel_threshold: u32,
    pub(super) max_changed_ratio: Option<f32>,
    pub(super) max_changed_pixels: Option<u64>,
    pub(super) max_mean_absolute_error: Option<f32>,
    pub(super) max_channel_delta: Option<u32>,
    pub(super) size_policy: Option<&'a str>,
    pub(super) alpha_mode: Option<&'a str>,
}

pub(super) fn visual_compare_options(
    input: VisualCompareRequestOptions<'_>,
) -> Result<VisualCompareOptions, Status> {
    let per_channel_threshold = u8::try_from(input.per_channel_threshold)
        .map_err(|_| Status::invalid_argument("per_channel_threshold must be between 0 and 255"))?;
    let max_channel_delta = input
        .max_channel_delta
        .map(|value| {
            u8::try_from(value).map_err(|_| {
                Status::invalid_argument("max_channel_delta must be between 0 and 255")
            })
        })
        .transpose()?;
    let max_changed_ratio = input.max_changed_ratio.unwrap_or_default();
    if !max_changed_ratio.is_finite() || !(0.0..=1.0).contains(&max_changed_ratio) {
        return Err(Status::invalid_argument(
            "max_changed_ratio must be between 0.0 and 1.0",
        ));
    }
    if input
        .max_mean_absolute_error
        .is_some_and(|value| !value.is_finite() || !(0.0..=255.0).contains(&value))
    {
        return Err(Status::invalid_argument(
            "max_mean_absolute_error must be between 0.0 and 255.0",
        ));
    }
    let size_policy = visual_size_policy_from_name(input.size_policy.unwrap_or("error"))?;
    let alpha_mode = visual_alpha_mode_from_name(input.alpha_mode.unwrap_or("ignore"))?;

    Ok(VisualCompareOptions {
        region: input.region,
        ignore_regions: input.ignore_regions,
        per_channel_threshold,
        max_changed_ratio,
        max_changed_pixels: input.max_changed_pixels,
        max_mean_absolute_error: input.max_mean_absolute_error,
        max_channel_delta,
        size_policy,
        alpha_mode,
    })
}

pub(super) struct UiStateRequestOptions<'a> {
    pub(super) region: Option<Rect>,
    pub(super) ignore_regions: Vec<Rect>,
    pub(super) per_channel_threshold: Option<u32>,
    pub(super) stable_max_changed_ratio: Option<f32>,
    pub(super) stable_max_changed_pixels: Option<u64>,
    pub(super) stable_max_mean_absolute_error: Option<f32>,
    pub(super) stable_max_channel_delta: Option<u32>,
    pub(super) loading_min_changed_ratio: Option<f32>,
    pub(super) loading_min_changed_pixels: Option<u64>,
    pub(super) required_stable_transitions: Option<u32>,
    pub(super) size_policy: Option<&'a str>,
    pub(super) alpha_mode: Option<&'a str>,
}

pub(super) fn ui_state_options(input: UiStateRequestOptions<'_>) -> Result<UiStateOptions, Status> {
    let mut options = UiStateOptions {
        region: input.region,
        ignore_regions: input.ignore_regions,
        ..UiStateOptions::default()
    };
    if let Some(per_channel_threshold) = input.per_channel_threshold {
        options.per_channel_threshold = u8::try_from(per_channel_threshold).map_err(|_| {
            Status::invalid_argument("per_channel_threshold must be between 0 and 255")
        })?;
    }
    if let Some(stable_max_changed_ratio) = input.stable_max_changed_ratio {
        options.stable_max_changed_ratio = stable_max_changed_ratio;
    }
    options.stable_max_changed_pixels = input.stable_max_changed_pixels;
    options.stable_max_mean_absolute_error = input.stable_max_mean_absolute_error;
    options.stable_max_channel_delta = input
        .stable_max_channel_delta
        .map(|value| {
            u8::try_from(value).map_err(|_| {
                Status::invalid_argument("stable_max_channel_delta must be between 0 and 255")
            })
        })
        .transpose()?;
    if let Some(loading_min_changed_ratio) = input.loading_min_changed_ratio {
        options.loading_min_changed_ratio = loading_min_changed_ratio;
    }
    options.loading_min_changed_pixels = input.loading_min_changed_pixels;
    if let Some(required_stable_transitions) = input.required_stable_transitions {
        options.required_stable_transitions = usize::try_from(required_stable_transitions)
            .map_err(|_| Status::invalid_argument("required_stable_transitions is too large"))?;
    }
    options.size_policy = visual_size_policy_from_name(input.size_policy.unwrap_or("error"))?;
    options.alpha_mode = visual_alpha_mode_from_name(input.alpha_mode.unwrap_or("ignore"))?;

    Ok(options)
}

pub(super) fn visual_size_policy_from_name(value: &str) -> Result<VisualSizePolicy, Status> {
    match value {
        "error" => Ok(VisualSizePolicy::Error),
        "common-region" => Ok(VisualSizePolicy::CommonRegion),
        "resize-actual" => Ok(VisualSizePolicy::ResizeActual),
        value => Err(Status::invalid_argument(format!(
            "size_policy must be error, common-region, or resize-actual, got {value:?}"
        ))),
    }
}

pub(super) fn visual_alpha_mode_from_name(value: &str) -> Result<VisualAlphaMode, Status> {
    match value {
        "ignore" => Ok(VisualAlphaMode::Ignore),
        "compare" => Ok(VisualAlphaMode::Compare),
        value => Err(Status::invalid_argument(format!(
            "alpha must be ignore or compare, got {value:?}"
        ))),
    }
}

pub(super) struct UiElementDetectionRequestOptions<'a> {
    pub(super) region: Option<Rect>,
    pub(super) ignore_regions: Vec<Rect>,
    pub(super) edge_threshold: Option<u32>,
    pub(super) min_width: Option<u32>,
    pub(super) min_height: Option<u32>,
    pub(super) min_component_pixels: Option<u32>,
    pub(super) min_confidence: Option<f32>,
    pub(super) max_width: Option<u32>,
    pub(super) max_height: Option<u32>,
    pub(super) min_area: Option<u64>,
    pub(super) max_area: Option<u64>,
    pub(super) max_elements: Option<u32>,
    pub(super) merge_distance: Option<u32>,
    pub(super) padding: Option<u32>,
    pub(super) sort: Option<&'a str>,
}

pub(super) fn ui_element_detection_options(
    input: UiElementDetectionRequestOptions<'_>,
) -> Result<UiElementDetectionOptions, Status> {
    let mut options = UiElementDetectionOptions {
        region: input.region,
        ignore_regions: input.ignore_regions,
        ..UiElementDetectionOptions::default()
    };
    if let Some(edge_threshold) = input.edge_threshold {
        options.edge_threshold = u8::try_from(edge_threshold)
            .map_err(|_| Status::invalid_argument("edge_threshold must be between 0 and 255"))?;
    }
    if let Some(min_width) = input.min_width {
        options.min_width = min_width;
    }
    if let Some(min_height) = input.min_height {
        options.min_height = min_height;
    }
    if let Some(min_component_pixels) = input.min_component_pixels {
        options.min_component_pixels = min_component_pixels;
    }
    options.min_confidence = input.min_confidence;
    options.max_width = input.max_width;
    options.max_height = input.max_height;
    options.min_area = input.min_area;
    options.max_area = input.max_area;
    if let Some(max_elements) = input.max_elements {
        options.max_elements = usize::try_from(max_elements)
            .map_err(|_| Status::invalid_argument("max_elements is too large"))?;
    }
    if let Some(merge_distance) = input.merge_distance {
        options.merge_distance = merge_distance;
    }
    if let Some(padding) = input.padding {
        options.padding = padding;
    }
    if let Some(sort) = input.sort {
        options.sort = ui_element_sort_from_name(sort)?;
    }

    Ok(options)
}

pub(super) fn ui_element_sort_from_name(value: &str) -> Result<UiElementSort, Status> {
    match value {
        "position" => Ok(UiElementSort::Position),
        "area" => Ok(UiElementSort::Area),
        "confidence" => Ok(UiElementSort::Confidence),
        _ => Err(Status::invalid_argument(format!(
            "sort must be position, area, or confidence, got {value:?}"
        ))),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn element_lookup_options_from_request(
    app: Option<String>,
    window_title: Option<String>,
    window_id: Option<String>,
    vision_region: Option<Rect>,
    vision_edge_threshold: Option<u32>,
    vision_min_width: Option<u32>,
    vision_min_height: Option<u32>,
    vision_min_component_pixels: Option<u32>,
    vision_max_elements: Option<u32>,
    vision_merge_distance: Option<u32>,
) -> Result<ElementLookupOptions, String> {
    let mut vision_options = UiElementDetectionOptions::default();
    if let Some(edge_threshold) = vision_edge_threshold {
        vision_options.edge_threshold = u8::try_from(edge_threshold)
            .map_err(|_| "vision_edge_threshold must be between 0 and 255".to_owned())?;
    }
    if let Some(min_width) = vision_min_width {
        vision_options.min_width = min_width;
    }
    if let Some(min_height) = vision_min_height {
        vision_options.min_height = min_height;
    }
    if let Some(min_component_pixels) = vision_min_component_pixels {
        vision_options.min_component_pixels = min_component_pixels;
    }
    if let Some(max_elements) = vision_max_elements {
        vision_options.max_elements = usize::try_from(max_elements)
            .map_err(|_| "vision_max_elements is too large".to_owned())?;
    }
    if let Some(merge_distance) = vision_merge_distance {
        vision_options.merge_distance = merge_distance;
    }

    Ok(ElementLookupOptions {
        scope: ElementLookupScope {
            app: normalize_optional_string(app),
            window_title: normalize_optional_string(window_title),
            window_id: normalize_optional_string(window_id),
        },
        vision: ElementVisionFallbackConfig {
            region: vision_region,
            options: vision_options,
        },
    })
}

pub(super) fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct OcrRunRequest {
    pub(super) image_path: Option<String>,
    pub(super) region: Option<Rect>,
    pub(super) app: Option<String>,
    pub(super) window_title: Option<String>,
    pub(super) window_id: Option<String>,
    pub(super) options: OcrOptions,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct OcrOptionInput {
    pub(super) language: Option<String>,
    pub(super) page_segmentation_mode: Option<u8>,
    pub(super) engine_mode: Option<u8>,
    pub(super) dpi: Option<u32>,
    pub(super) min_confidence: Option<f32>,
    pub(super) whitelist: Option<String>,
    pub(super) config: Vec<String>,
    pub(super) scale: Option<f32>,
    pub(super) grayscale: bool,
    pub(super) threshold: Option<u8>,
    pub(super) invert: bool,
    pub(super) contrast: Option<f32>,
    pub(super) deskew: bool,
}

pub(super) fn run_ocr(
    request: OcrRunRequest,
) -> std::result::Result<OcrResult, peekaboox_core::PeekabooXError> {
    let backend = TesseractOcrBackend::new("tesseract", request.options);
    if !backend.is_available() {
        return Err(peekaboox_core::PeekabooXError::new(
            "OCR backend tesseract is not available; install tesseract-ocr",
        ));
    }

    if let Some(image_path) = request
        .image_path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
    {
        return peekaboox_vision::ocr_image_file_with_backend(&backend, image_path, request.region);
    }

    let region = match (
        request.window_id.as_deref(),
        request.window_title.as_deref(),
        request.app.as_deref(),
    ) {
        (None, None, None) => request.region,
        _ => Some(resolve_ocr_window_region(
            request.region,
            request.window_id.as_deref(),
            request.window_title.as_deref(),
            request.app.as_deref(),
        )?),
    };

    match region {
        Some(region) => peekaboox_vision::ocr_region_with_backend(&backend, region),
        None => peekaboox_vision::ocr_screen_with_backend(&backend),
    }
}

pub(super) fn ocr_options(
    input: OcrOptionInput,
) -> std::result::Result<OcrOptions, peekaboox_core::PeekabooXError> {
    let mut options = OcrOptions::default();
    if let Some(language) = input
        .language
        .map(|language| language.trim().to_owned())
        .filter(|language| !language.is_empty())
    {
        options.language = Some(language);
    }
    if let Some(psm) = input.page_segmentation_mode {
        options.page_segmentation_mode = Some(psm);
    }
    if let Some(oem) = input.engine_mode {
        options.engine_mode = Some(oem);
    }
    if let Some(dpi) = input.dpi {
        options.dpi = Some(dpi);
    }
    if let Some(min_confidence) = input.min_confidence {
        options.min_confidence = min_confidence;
    }
    if let Some(whitelist) = input
        .whitelist
        .map(|whitelist| whitelist.trim().to_owned())
        .filter(|whitelist| !whitelist.is_empty())
    {
        options.whitelist = Some(whitelist);
    }
    options.config = input
        .config
        .into_iter()
        .map(|entry| parse_ocr_config(&entry))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    options.preprocessing = OcrPreprocessingOptions {
        scale: input.scale,
        grayscale: input.grayscale,
        threshold: input.threshold,
        invert: input.invert,
        contrast: input.contrast,
        deskew: input.deskew,
    };
    Ok(options)
}

pub(super) fn parse_ocr_config(
    entry: &str,
) -> std::result::Result<OcrConfig, peekaboox_core::PeekabooXError> {
    let Some((key, value)) = entry.split_once('=') else {
        return Err(peekaboox_core::PeekabooXError::new(
            "OCR config entries must be key=value",
        ));
    };
    let key = key.trim();
    if key.is_empty() || key.contains(char::is_whitespace) {
        return Err(peekaboox_core::PeekabooXError::new(
            "OCR config keys must be non-empty and contain no whitespace",
        ));
    }
    Ok(OcrConfig {
        key: key.to_owned(),
        value: value.to_owned(),
    })
}

pub(super) fn resolve_ocr_window_region(
    region: Option<Rect>,
    window_id: Option<&str>,
    window_title: Option<&str>,
    app: Option<&str>,
) -> std::result::Result<Rect, peekaboox_core::PeekabooXError> {
    let metadata = peekaboox_windows::list_windows()?;
    let window_id = window_id.map(str::trim).filter(|value| !value.is_empty());
    let title = window_title
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());
    let app = app
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());
    let mut matches = metadata
        .windows
        .into_iter()
        .filter(|window| {
            window_id.is_none_or(|id| window.id == id)
                && title
                    .as_deref()
                    .is_none_or(|title| window.title.to_ascii_lowercase().contains(title))
                && app.as_deref().is_none_or(|app| {
                    window
                        .app_id
                        .as_deref()
                        .unwrap_or_default()
                        .to_ascii_lowercase()
                        .contains(app)
                        || window.title.to_ascii_lowercase().contains(app)
                })
        })
        .collect::<Vec<_>>();

    if matches.is_empty() {
        return Err(peekaboox_core::PeekabooXError::new(
            "no window matched OCR window filters",
        ));
    }
    matches.sort_by_key(|window| !window.focused);
    let window = matches.remove(0);
    if window.bounds.width == 0 || window.bounds.height == 0 {
        return Err(peekaboox_core::PeekabooXError::new(format!(
            "window {} has empty bounds",
            window.id
        )));
    }

    match region {
        Some(region) => offset_ocr_region(window.bounds, region),
        None => Ok(window.bounds),
    }
}

pub(super) fn offset_ocr_region(
    origin: Rect,
    region: Rect,
) -> std::result::Result<Rect, peekaboox_core::PeekabooXError> {
    let x = i64::from(origin.x) + i64::from(region.x);
    let y = i64::from(origin.y) + i64::from(region.y);
    Ok(Rect::new(
        i32::try_from(x).map_err(|_| {
            peekaboox_core::PeekabooXError::new("OCR region x coordinate overflows i32")
        })?,
        i32::try_from(y).map_err(|_| {
            peekaboox_core::PeekabooXError::new("OCR region y coordinate overflows i32")
        })?,
        region.width,
        region.height,
    ))
}

pub(super) fn ocr_status(error: peekaboox_core::PeekabooXError) -> Status {
    let message = error.to_string();
    if message.contains("not available") {
        Status::failed_precondition(message)
    } else {
        Status::internal(message)
    }
}

pub(super) fn capture_target_name(target: Option<&proto::CaptureTarget>) -> &'static str {
    match target.and_then(|target| target.target.as_ref()) {
        None => "full_screen_default",
        Some(capture_target::Target::FullScreen(true)) => "full_screen",
        Some(capture_target::Target::FullScreen(false)) => "full_screen_false",
        Some(capture_target::Target::Region(_)) => "region",
        Some(capture_target::Target::WindowId(_)) => "window",
    }
}
