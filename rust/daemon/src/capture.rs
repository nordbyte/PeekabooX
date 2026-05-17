use super::*;

pub(super) fn capture_screen_response(
    target: Option<proto::CaptureTarget>,
    include_semantic_tree: bool,
    accessibility_cache: &SharedAccessibilityCache,
) -> Result<proto::CaptureScreenResponse, String> {
    let capture_region = capture_screen_region(target)?;
    let CapturedFrame {
        frame,
        backend_name,
        backend_kind,
        captured_at_unix_ms,
    } = capture_current_frame(capture_region)?;
    let image = peekaboox_capture::encode_frame_png(&frame).map_err(|error| error.to_string())?;
    let semantic_tree = if include_semantic_tree {
        cached_accessibility_tree(accessibility_cache)?
            .metadata
            .elements
            .iter()
            .map(proto_ui_element)
            .collect()
    } else {
        Vec::new()
    };

    Ok(proto::CaptureScreenResponse {
        image,
        mime_type: "image/png".to_owned(),
        semantic_tree,
        metadata: Some(proto::CaptureMetadata {
            width: frame.width,
            height: frame.height,
            backend: format!("{}/{}", backend_name, backend_kind_name(backend_kind)),
            captured_at_unix_ms,
        }),
    })
}

pub(super) fn capture_screen_region(
    target: Option<proto::CaptureTarget>,
) -> Result<Option<Rect>, String> {
    match target.and_then(|target| target.target) {
        None | Some(capture_target::Target::FullScreen(true)) => Ok(None),
        Some(capture_target::Target::FullScreen(false)) => {
            Err("capture_screen full_screen target must be true".to_owned())
        }
        Some(capture_target::Target::Region(region)) => Ok(Some(rect_from_proto(region))),
        Some(capture_target::Target::WindowId(window_id)) => {
            capture_region_from_request(None, Some(&window_id))
        }
    }
}

pub(super) fn capture_delta_data(
    stream_id: Option<&str>,
    reset: bool,
    capture_region: Option<Rect>,
    per_channel_threshold: u8,
    low_bandwidth: bool,
    incremental_capture_state: &SharedIncrementalCaptureState,
) -> Result<CaptureDeltaData, String> {
    let stream_id = normalized_capture_stream_id(stream_id.unwrap_or_default());
    if stream_id.len() > MAX_CAPTURE_STREAM_ID_LEN {
        return Err(format!(
            "capture_delta stream_id is too long: maximum {MAX_CAPTURE_STREAM_ID_LEN} bytes"
        ));
    }
    let CapturedFrame {
        frame,
        backend_name,
        backend_kind,
        captured_at_unix_ms,
    } = capture_current_frame(capture_region)?;
    let options = IncrementalCaptureOptions {
        compare: VisualCompareOptions {
            region: None,
            per_channel_threshold,
            max_changed_ratio: 0.0,
            ..VisualCompareOptions::default()
        },
    };
    let mut state = incremental_capture_state
        .lock()
        .map_err(|_| "failed to lock incremental capture state".to_owned())?;
    let previous = if reset || !low_bandwidth {
        None
    } else {
        state
            .streams
            .get(&stream_id)
            .filter(|stream| {
                stream.frame.width == frame.width && stream.frame.height == frame.height
            })
            .map(|stream| &stream.frame)
    };
    let sequence = if reset {
        1
    } else {
        state
            .streams
            .get(&stream_id)
            .map_or(1, |stream| stream.sequence.saturating_add(1))
    };
    let delta = peekaboox_vision::incremental_capture_delta(previous, &frame, sequence, &options)
        .map_err(|error| error.to_string())?;
    state.insert(
        stream_id.clone(),
        IncrementalCaptureStream { sequence, frame },
    );

    Ok(CaptureDeltaData {
        stream_id,
        delta,
        low_bandwidth,
        capture_region,
        backend_name,
        backend_kind,
        captured_at_unix_ms,
    })
}

pub(super) fn grpc_capture_delta(
    request: proto::CaptureDeltaRequest,
    incremental_capture_state: &SharedIncrementalCaptureState,
) -> Result<proto::CaptureDeltaResponse, Status> {
    let capture_region =
        capture_delta_region(request.target, request.region).map_err(Status::invalid_argument)?;
    let per_channel_threshold = request.per_channel_threshold.unwrap_or_default();
    let per_channel_threshold = u8::try_from(per_channel_threshold)
        .map_err(|_| Status::invalid_argument("per_channel_threshold must be between 0 and 255"))?;
    let low_bandwidth = request.low_bandwidth.unwrap_or(true);
    let data = capture_delta_data(
        Some(&request.stream_id),
        request.reset,
        capture_region,
        per_channel_threshold,
        low_bandwidth,
        incremental_capture_state,
    )
    .map_err(Status::internal)?;

    Ok(proto_capture_delta_response(&data))
}

pub(super) fn grpc_capture_backends(
    request: proto::CaptureBackendsRequest,
) -> Result<proto::CaptureBackendsResponse, Status> {
    let output = if request.output.is_empty() {
        PathBuf::from("screenshot.png")
    } else {
        PathBuf::from(request.output)
    };
    let probe = capture_backend_probe_from_proto(request.probe)?;
    let result = capture_backends_result(
        &output,
        request.region.map(rect_from_proto),
        request.diagnose,
        probe,
    );
    Ok(proto_capture_backends_response(result))
}

#[cfg(feature = "pipewire-backend")]
pub(super) fn probe_dmabuf_import(
    import_target: DmaBufImportTargetDto,
) -> Result<DmaBufProbeResultDto, String> {
    let stream =
        peekaboox_capture::open_pipewire_screencast().map_err(|error| error.to_string())?;
    let stream_node_id = stream.stream_node_id;
    let pipewire_serial = stream.pipewire_serial;
    let descriptor = peekaboox_capture::capture_pipewire_dmabuf_frame(stream)
        .map_err(|error| error.to_string())?;

    match import_target {
        DmaBufImportTargetDto::Compute => {
            probe_compute_dmabuf_import(&descriptor, import_target, stream_node_id, pipewire_serial)
        }
        DmaBufImportTargetDto::Egl => {
            probe_egl_dmabuf_import(&descriptor, import_target, stream_node_id, pipewire_serial)
        }
        DmaBufImportTargetDto::EglTexture => probe_egl_texture_dmabuf_import(
            &descriptor,
            import_target,
            stream_node_id,
            pipewire_serial,
        ),
    }
}

#[cfg(not(feature = "pipewire-backend"))]
pub(super) fn probe_dmabuf_import(
    _import_target: DmaBufImportTargetDto,
) -> Result<DmaBufProbeResultDto, String> {
    Err(
        "DMA-BUF probing requires building peekabooxd with the `pipewire-backend` feature"
            .to_owned(),
    )
}

#[cfg(feature = "pipewire-backend")]
pub(super) fn probe_compute_dmabuf_import(
    descriptor: &peekaboox_capture::DmaBufFrameDescriptor,
    import_target: DmaBufImportTargetDto,
    stream_node_id: u32,
    pipewire_serial: Option<u64>,
) -> Result<DmaBufProbeResultDto, String> {
    let imported = peekaboox_capture::import_dmabuf_frame(
        descriptor,
        peekaboox_capture::DmaBufImportTarget::Compute,
    )
    .map_err(|error| error.to_string())?;
    Ok(dmabuf_probe_result(
        DmaBufProbeMetadata {
            import_target,
            backend_name: imported.backend_name.clone(),
            stream_node_id,
            pipewire_serial,
            egl_version: None,
            egl_modifiers: None,
            texture_id: None,
        },
        &imported.descriptor,
    ))
}

#[cfg(all(feature = "pipewire-backend", feature = "egl-backend"))]
pub(super) fn probe_egl_dmabuf_import(
    descriptor: &peekaboox_capture::DmaBufFrameDescriptor,
    import_target: DmaBufImportTargetDto,
    stream_node_id: u32,
    pipewire_serial: Option<u64>,
) -> Result<DmaBufProbeResultDto, String> {
    let importer =
        peekaboox_capture::EglDmaBufImporter::new().map_err(|error| error.to_string())?;
    let imported = importer
        .import_image(descriptor)
        .map_err(|error| error.to_string())?;
    Ok(dmabuf_probe_result(
        DmaBufProbeMetadata {
            import_target,
            backend_name: imported.backend_name.clone(),
            stream_node_id,
            pipewire_serial,
            egl_version: Some(egl_version_string(importer.egl_version())),
            egl_modifiers: Some(importer.supports_modifiers()),
            texture_id: None,
        },
        &imported.descriptor,
    ))
}

#[cfg(all(feature = "pipewire-backend", not(feature = "egl-backend")))]
pub(super) fn probe_egl_dmabuf_import(
    _descriptor: &peekaboox_capture::DmaBufFrameDescriptor,
    _import_target: DmaBufImportTargetDto,
    _stream_node_id: u32,
    _pipewire_serial: Option<u64>,
) -> Result<DmaBufProbeResultDto, String> {
    Err("EGL DMA-BUF probing requires building peekabooxd with `egl-backend`".to_owned())
}

#[cfg(all(feature = "pipewire-backend", feature = "egl-backend"))]
pub(super) fn probe_egl_texture_dmabuf_import(
    descriptor: &peekaboox_capture::DmaBufFrameDescriptor,
    import_target: DmaBufImportTargetDto,
    stream_node_id: u32,
    pipewire_serial: Option<u64>,
) -> Result<DmaBufProbeResultDto, String> {
    let importer =
        peekaboox_capture::EglTextureDmaBufImporter::new().map_err(|error| error.to_string())?;
    let imported = importer
        .import_texture(descriptor)
        .map_err(|error| error.to_string())?;
    Ok(dmabuf_probe_result(
        DmaBufProbeMetadata {
            import_target,
            backend_name: imported.backend_name.clone(),
            stream_node_id,
            pipewire_serial,
            egl_version: Some(egl_version_string(importer.egl_version())),
            egl_modifiers: Some(importer.supports_modifiers()),
            texture_id: Some(imported.texture_id()),
        },
        &imported.descriptor,
    ))
}

#[cfg(all(feature = "pipewire-backend", not(feature = "egl-backend")))]
pub(super) fn probe_egl_texture_dmabuf_import(
    _descriptor: &peekaboox_capture::DmaBufFrameDescriptor,
    _import_target: DmaBufImportTargetDto,
    _stream_node_id: u32,
    _pipewire_serial: Option<u64>,
) -> Result<DmaBufProbeResultDto, String> {
    Err("EGL texture DMA-BUF probing requires building peekabooxd with `egl-backend`".to_owned())
}

#[cfg(feature = "pipewire-backend")]
pub(super) struct DmaBufProbeMetadata {
    pub(super) import_target: DmaBufImportTargetDto,
    pub(super) backend_name: String,
    pub(super) stream_node_id: u32,
    pub(super) pipewire_serial: Option<u64>,
    pub(super) egl_version: Option<String>,
    pub(super) egl_modifiers: Option<bool>,
    pub(super) texture_id: Option<u32>,
}

#[cfg(feature = "pipewire-backend")]
pub(super) fn dmabuf_probe_result(
    metadata: DmaBufProbeMetadata,
    descriptor: &peekaboox_capture::DmaBufFrameImportDescriptor,
) -> DmaBufProbeResultDto {
    DmaBufProbeResultDto {
        import_target: metadata.import_target,
        backend_name: metadata.backend_name,
        stream_node_id: metadata.stream_node_id,
        pipewire_serial: metadata.pipewire_serial,
        width: descriptor.width,
        height: descriptor.height,
        pixel_format: pixel_format_name(descriptor.format).to_owned(),
        fourcc: descriptor.fourcc,
        planes: descriptor.planes.len(),
        memory_layout: descriptor.memory_layout.name().to_owned(),
        synchronization: descriptor.synchronization.name().to_owned(),
        egl_version: metadata.egl_version,
        egl_modifiers: metadata.egl_modifiers,
        texture_id: metadata.texture_id,
    }
}

#[cfg(all(feature = "pipewire-backend", feature = "egl-backend"))]
pub(super) fn egl_version_string(version: (i32, i32)) -> String {
    format!("{}.{}", version.0, version.1)
}

pub(super) fn capture_current_frame(region: Option<Rect>) -> Result<CapturedFrame, String> {
    let metadata = match region {
        Some(region) => peekaboox_capture::capture_region_frame(region),
        None => peekaboox_capture::capture_screen_frame(),
    }
    .map_err(|error| error.to_string())?;

    Ok(CapturedFrame {
        frame: metadata.frame,
        backend_name: metadata.backend_name,
        backend_kind: metadata.backend_kind,
        captured_at_unix_ms: unix_time_ms_u64(),
    })
}

pub(super) fn capture_to_file(
    output: impl AsRef<Path>,
    region: Option<Rect>,
) -> Result<peekaboox_capture::CaptureFileMetadata, peekaboox_core::PeekabooXError> {
    match region {
        Some(region) => peekaboox_capture::capture_region_to_file(region, output),
        None => peekaboox_capture::capture_screen_to_file(output),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CaptureFileFormat {
    Png,
    Xwd,
}

impl CaptureFileFormat {
    fn mime_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Xwd => "image/x-xwindowdump",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct CaptureFileTarget {
    pub(super) capture_region: Option<Rect>,
    pub(super) window: Option<WindowInfo>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn capture_to_file_response(
    output: &str,
    region: Option<Rect>,
    window_id: Option<&str>,
    app: Option<&str>,
    window_title: Option<&str>,
    title_regex: Option<&str>,
    format: Option<&str>,
    no_overwrite: bool,
    include_semantic_tree: bool,
    accessibility_cache: &SharedAccessibilityCache,
) -> Result<CaptureResultDto, String> {
    let format = capture_file_format_from_request(format)?;
    if format == CaptureFileFormat::Xwd
        && (region.is_some() || has_capture_window_scope(window_id, app, window_title, title_regex))
    {
        return Err("capture format xwd only supports full-screen file output".to_owned());
    }
    let output_path = ensure_capture_output_path(Path::new(output), no_overwrite)?;
    if format == CaptureFileFormat::Xwd
        && !output_path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("xwd"))
    {
        return Err("capture format xwd output path must end in .xwd".to_owned());
    }

    if format == CaptureFileFormat::Xwd {
        let metadata = if no_overwrite {
            capture_screen_to_file_no_overwrite(&output_path).map_err(|error| error.to_string())?
        } else {
            peekaboox_capture::capture_screen_to_file(&output_path)
                .map_err(|error| error.to_string())?
        };
        return Ok(CaptureResultDto {
            output_path: metadata.output_path.display().to_string(),
            backend_name: metadata.backend_name,
            backend_kind: backend_kind_name(metadata.backend_kind),
            bytes_written: metadata.bytes_written,
            width: 0,
            height: 0,
            mime_type: format.mime_type().to_owned(),
            capture_region: None,
            window_id: None,
            window: None,
            captured_at_unix_ms: unix_time_ms_u64(),
            source: "file-backend".to_owned(),
            semantic_tree: capture_semantic_tree_dto(include_semantic_tree, accessibility_cache)?,
        });
    }

    let target = resolve_capture_file_target(region, window_id, app, window_title, title_regex)?;
    let frame_metadata = match target.capture_region {
        Some(region) => peekaboox_capture::capture_region_frame(region),
        None => peekaboox_capture::capture_screen_frame(),
    }
    .map_err(|error| error.to_string())?;
    let bytes_written = if no_overwrite {
        write_frame_png_no_overwrite(&frame_metadata.frame, &output_path)
            .map_err(|error| error.to_string())?
    } else {
        peekaboox_capture::write_frame_png(&frame_metadata.frame, &output_path)
            .map_err(|error| error.to_string())?
    };
    let window_id = target.window.as_ref().map(|window| window.id.clone());
    let window = target.window.as_ref().map(WindowDto::from);

    Ok(CaptureResultDto {
        output_path: output_path.display().to_string(),
        backend_name: frame_metadata.backend_name,
        backend_kind: backend_kind_name(frame_metadata.backend_kind),
        bytes_written,
        width: frame_metadata.frame.width,
        height: frame_metadata.frame.height,
        mime_type: format.mime_type().to_owned(),
        capture_region: target.capture_region.map(RectDto::from),
        window_id,
        window,
        captured_at_unix_ms: unix_time_ms_u64(),
        source: capture_frame_source_label(frame_metadata.source).to_owned(),
        semantic_tree: capture_semantic_tree_dto(include_semantic_tree, accessibility_cache)?,
    })
}

pub(super) fn capture_file_format_from_request(
    format: Option<&str>,
) -> Result<CaptureFileFormat, String> {
    match format.unwrap_or("png").trim().to_ascii_lowercase().as_str() {
        "" | "png" => Ok(CaptureFileFormat::Png),
        "xwd" => Ok(CaptureFileFormat::Xwd),
        other => Err(format!(
            "invalid capture format {other:?}; expected png or xwd"
        )),
    }
}

pub(super) fn has_capture_window_scope(
    window_id: Option<&str>,
    app: Option<&str>,
    window_title: Option<&str>,
    title_regex: Option<&str>,
) -> bool {
    clean_str(window_id).is_some()
        || clean_str(app).is_some()
        || clean_str(window_title).is_some()
        || clean_str(title_regex).is_some()
}

pub(super) fn clean_str(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

pub(super) fn resolve_capture_file_target(
    region: Option<Rect>,
    window_id: Option<&str>,
    app: Option<&str>,
    window_title: Option<&str>,
    title_regex: Option<&str>,
) -> Result<CaptureFileTarget, String> {
    let Some(window) = resolve_capture_file_window(window_id, app, window_title, title_regex)?
    else {
        return Ok(CaptureFileTarget {
            capture_region: region,
            window: None,
        });
    };
    let capture_region = match region {
        Some(region) => offset_window_relative_capture_region(window.bounds, region)?,
        None => window.bounds,
    };
    Ok(CaptureFileTarget {
        capture_region: Some(capture_region),
        window: Some(window),
    })
}

pub(super) fn resolve_capture_file_window(
    window_id: Option<&str>,
    app: Option<&str>,
    window_title: Option<&str>,
    title_regex: Option<&str>,
) -> Result<Option<WindowInfo>, String> {
    let id = clean_str(window_id).map(str::to_owned);
    let app = clean_str(app).map(str::to_owned);
    let title = clean_str(window_title).map(str::to_owned);
    let title_regex = clean_str(title_regex).map(str::to_owned);
    if id.is_none() && app.is_none() && title.is_none() && title_regex.is_none() {
        return Ok(None);
    }

    let query = peekaboox_windows::WindowQuery {
        id,
        app,
        title,
        title_regex,
        sort: peekaboox_windows::WindowSort::Focused,
        ..peekaboox_windows::WindowQuery::default()
    };
    let metadata =
        peekaboox_windows::list_windows_with_query(query).map_err(|error| error.to_string())?;
    let window = metadata
        .windows
        .into_iter()
        .next()
        .ok_or_else(|| "no window matched capture filters".to_owned())?;
    if window.bounds.width == 0 || window.bounds.height == 0 {
        return Err(format!("window {} has empty bounds", window.id));
    }
    Ok(Some(window))
}

pub(super) fn offset_window_relative_capture_region(
    origin: Rect,
    region: Rect,
) -> Result<Rect, String> {
    if region.x < 0 || region.y < 0 {
        return Err("window-relative capture region must start inside the window".to_owned());
    }
    let right = i64::from(region.x) + i64::from(region.width);
    let bottom = i64::from(region.y) + i64::from(region.height);
    if right > i64::from(origin.width) || bottom > i64::from(origin.height) {
        return Err("window-relative capture region must fit inside the window".to_owned());
    }
    let x = i64::from(origin.x) + i64::from(region.x);
    let y = i64::from(origin.y) + i64::from(region.y);
    Ok(Rect::new(
        i32::try_from(x).map_err(|_| "window-relative region x overflow".to_owned())?,
        i32::try_from(y).map_err(|_| "window-relative region y overflow".to_owned())?,
        region.width,
        region.height,
    ))
}

pub(super) fn ensure_capture_output_path(
    output: &Path,
    no_overwrite: bool,
) -> Result<PathBuf, String> {
    let output = if output.is_absolute() {
        output.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(output)
    };
    if no_overwrite && output.exists() {
        return Err(format!(
            "capture output already exists: {}",
            output.display()
        ));
    }
    Ok(output)
}

pub(super) fn write_frame_png_no_overwrite(
    frame: &CaptureFrame,
    output: &Path,
) -> Result<u64, peekaboox_core::PeekabooXError> {
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            peekaboox_core::PeekabooXError::new(format!(
                "failed to create {}: {error}",
                parent.display()
            ))
        })?;
    }
    let png = peekaboox_capture::encode_frame_png(frame)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|error| {
            peekaboox_core::PeekabooXError::new(format!(
                "failed to create {} without overwriting: {error}",
                output.display()
            ))
        })?;
    file.write_all(&png).map_err(|error| {
        peekaboox_core::PeekabooXError::new(format!(
            "failed to write {}: {error}",
            output.display()
        ))
    })?;
    Ok(png.len() as u64)
}

pub(super) fn capture_screen_to_file_no_overwrite(
    output: &Path,
) -> Result<peekaboox_capture::CaptureFileMetadata, peekaboox_core::PeekabooXError> {
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            peekaboox_core::PeekabooXError::new(format!(
                "failed to create {}: {error}",
                parent.display()
            ))
        })?;
    }
    if output.exists() {
        return Err(peekaboox_core::PeekabooXError::new(format!(
            "capture output already exists: {}",
            output.display()
        )));
    }
    let temp_parent = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let temp = reserve_unique_temp_path_in(
        temp_parent,
        ".peekaboox-capture",
        output
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("tmp"),
    )
    .map_err(|error| {
        peekaboox_core::PeekabooXError::new(format!(
            "failed to reserve temporary capture output in {}: {error}",
            temp_parent.display()
        ))
    })?;
    let result = peekaboox_capture::capture_screen_to_file(&temp);
    match result {
        Ok(mut metadata) => {
            fs::hard_link(&temp, output).map_err(|error| {
                peekaboox_core::PeekabooXError::new(format!(
                    "failed to install {} without overwriting: {error}",
                    output.display()
                ))
            })?;
            let _ = fs::remove_file(&temp);
            metadata.output_path = output.to_path_buf();
            Ok(metadata)
        }
        Err(error) => {
            let _ = fs::remove_file(&temp);
            Err(error)
        }
    }
}

pub(super) fn capture_semantic_tree_dto(
    include_semantic_tree: bool,
    accessibility_cache: &SharedAccessibilityCache,
) -> Result<Vec<ElementDto>, String> {
    if !include_semantic_tree {
        return Ok(Vec::new());
    }
    Ok(cached_accessibility_tree(accessibility_cache)?
        .metadata
        .elements
        .iter()
        .map(ElementDto::from)
        .collect())
}

pub(super) fn capture_backends_result(
    output: &Path,
    region: Option<Rect>,
    diagnose: bool,
    probe: CaptureBackendProbeDto,
) -> CaptureBackendsResultDto {
    let environment = peekaboox_capture::CaptureEnvironment::detect();
    let capabilities = peekaboox_capture::capture_backend_capabilities(&environment, output);
    let image_backends = capabilities
        .into_iter()
        .filter(|capability| diagnose || capability.reason.is_none())
        .map(capture_backend_dto)
        .collect::<Vec<_>>();
    let zero_copy_backends = peekaboox_capture::zero_copy_capture_capabilities(&environment)
        .into_iter()
        .map(zero_copy_backend_dto)
        .collect::<Vec<_>>();
    let mut warnings = capture_backend_warnings(&zero_copy_backends);
    let probes = capture_backend_probe_steps(probe)
        .into_iter()
        .map(|probe| capture_backend_probe(probe, output, region))
        .collect::<Vec<_>>();

    if matches!(
        probe,
        CaptureBackendProbeDto::Region | CaptureBackendProbeDto::All
    ) && region.is_none()
    {
        warnings.push("region probe used default region 0,0,320,180".to_owned());
    }

    CaptureBackendsResultDto {
        session_type: environment.session_type.name().to_owned(),
        desktop: environment.current_desktop,
        pipewire_session_available: environment.pipewire_session_available,
        pipewire_backend_feature_enabled: peekaboox_capture::pipewire_backend_feature_enabled(),
        egl_backend_feature_enabled: peekaboox_capture::egl_backend_feature_enabled(),
        output_path: output.display().to_string(),
        region: region.map(RectDto::from),
        image_backends,
        zero_copy_backends,
        probes,
        warnings,
    }
}

pub(super) fn capture_backend_dto(
    capability: peekaboox_capture::CaptureBackendCapability,
) -> CaptureBackendDto {
    CaptureBackendDto {
        name: capability.name.to_owned(),
        backend_kind: backend_kind_name(capability.backend_kind),
        command: capability.command.map(str::to_owned),
        available: capability.available,
        supports_output: capability.supports_output,
        supports_file_capture: capability.supports_file_capture,
        supports_stdout_capture: capability.supports_stdout_capture,
        supports_stdout_region_capture: capability.supports_stdout_region_capture,
        selected: capability.selected,
        reason: capability.reason,
    }
}

pub(super) fn zero_copy_backend_dto(
    capability: peekaboox_capture::ZeroCopyCaptureCapability,
) -> ZeroCopyBackendDto {
    let pipewire_feature = peekaboox_capture::pipewire_backend_feature_enabled();
    let selected = capability.availability.is_available() && pipewire_feature;
    let reason = if !capability.availability.is_available() {
        Some(capability.availability.name().to_owned())
    } else if !pipewire_feature {
        Some("compiled without pipewire-backend feature".to_owned())
    } else {
        None
    };

    ZeroCopyBackendDto {
        name: capability.backend_name,
        backend_kind: backend_kind_name(capability.backend_kind),
        transport: capability.transport.name().to_owned(),
        availability: capability.availability.name().to_owned(),
        selected,
        pipewire_backend_feature_enabled: pipewire_feature,
        egl_backend_feature_enabled: peekaboox_capture::egl_backend_feature_enabled(),
        reason,
    }
}

pub(super) fn capture_backend_warnings(backends: &[ZeroCopyBackendDto]) -> Vec<String> {
    let mut warnings = Vec::new();
    for backend in backends {
        if backend.availability == "available" && !backend.pipewire_backend_feature_enabled {
            warnings.push(format!(
                "{} is available in the session, but this build was compiled without pipewire-backend",
                backend.name
            ));
        }
    }
    warnings
}

pub(super) fn capture_backend_probe_steps(
    probe: CaptureBackendProbeDto,
) -> Vec<CaptureBackendProbeDto> {
    match probe {
        CaptureBackendProbeDto::None => Vec::new(),
        CaptureBackendProbeDto::All => vec![
            CaptureBackendProbeDto::File,
            CaptureBackendProbeDto::Frame,
            CaptureBackendProbeDto::Region,
            CaptureBackendProbeDto::DmaBuf,
        ],
        other => vec![other],
    }
}

pub(super) fn capture_backend_probe(
    probe: CaptureBackendProbeDto,
    output: &Path,
    region: Option<Rect>,
) -> CaptureBackendProbeResultDto {
    match probe {
        CaptureBackendProbeDto::File => capture_backend_probe_file(output),
        CaptureBackendProbeDto::Frame => capture_backend_probe_frame(),
        CaptureBackendProbeDto::Region => {
            capture_backend_probe_region(region.unwrap_or(Rect::new(0, 0, 320, 180)))
        }
        CaptureBackendProbeDto::DmaBuf => capture_backend_probe_dmabuf(),
        CaptureBackendProbeDto::None | CaptureBackendProbeDto::All => capture_backend_probe_error(
            capture_backend_probe_name(probe),
            "invalid internal probe step".to_owned(),
        ),
    }
}

pub(super) fn capture_backend_probe_file(output: &Path) -> CaptureBackendProbeResultDto {
    match peekaboox_capture::capture_screen_to_file(output) {
        Ok(metadata) => CaptureBackendProbeResultDto {
            probe: "file".to_owned(),
            ok: true,
            backend_name: Some(metadata.backend_name),
            backend_kind: Some(backend_kind_name(metadata.backend_kind)),
            detail: format!("wrote {} bytes", metadata.bytes_written),
            output_path: Some(metadata.output_path.display().to_string()),
            bytes_written: Some(metadata.bytes_written),
            width: None,
            height: None,
        },
        Err(error) => capture_backend_probe_error("file", error.to_string()),
    }
}

pub(super) fn capture_backend_probe_frame() -> CaptureBackendProbeResultDto {
    match peekaboox_capture::capture_screen_frame() {
        Ok(metadata) => CaptureBackendProbeResultDto {
            probe: "frame".to_owned(),
            ok: true,
            backend_name: Some(metadata.backend_name),
            backend_kind: Some(backend_kind_name(metadata.backend_kind)),
            detail: format!(
                "captured {}x{} via {}",
                metadata.frame.width,
                metadata.frame.height,
                capture_frame_source_label(metadata.source)
            ),
            output_path: None,
            bytes_written: None,
            width: Some(metadata.frame.width),
            height: Some(metadata.frame.height),
        },
        Err(error) => capture_backend_probe_error("frame", error.to_string()),
    }
}

pub(super) fn capture_backend_probe_region(region: Rect) -> CaptureBackendProbeResultDto {
    match peekaboox_capture::capture_region_frame(region) {
        Ok(metadata) => CaptureBackendProbeResultDto {
            probe: "region".to_owned(),
            ok: true,
            backend_name: Some(metadata.backend_name),
            backend_kind: Some(backend_kind_name(metadata.backend_kind)),
            detail: format!(
                "captured {}x{} region {} via {}",
                metadata.frame.width,
                metadata.frame.height,
                format_rect(region),
                capture_frame_source_label(metadata.source)
            ),
            output_path: None,
            bytes_written: None,
            width: Some(metadata.frame.width),
            height: Some(metadata.frame.height),
        },
        Err(error) => capture_backend_probe_error("region", error.to_string()),
    }
}

pub(super) fn capture_backend_probe_dmabuf() -> CaptureBackendProbeResultDto {
    match probe_dmabuf_import(DmaBufImportTargetDto::Compute) {
        Ok(probe) => CaptureBackendProbeResultDto {
            probe: "dmabuf".to_owned(),
            ok: true,
            backend_name: Some(probe.backend_name),
            backend_kind: Some("compute".to_owned()),
            detail: format!(
                "stream node_id={} pipewire_serial={} frame={}x{} format={} planes={}",
                probe.stream_node_id,
                probe
                    .pipewire_serial
                    .map(|serial| serial.to_string())
                    .unwrap_or_else(|| "-".to_owned()),
                probe.width,
                probe.height,
                probe.pixel_format,
                probe.planes
            ),
            output_path: None,
            bytes_written: None,
            width: Some(probe.width),
            height: Some(probe.height),
        },
        Err(error) => capture_backend_probe_error("dmabuf", error),
    }
}

pub(super) fn capture_backend_probe_error(
    probe: impl Into<String>,
    detail: String,
) -> CaptureBackendProbeResultDto {
    CaptureBackendProbeResultDto {
        probe: probe.into(),
        ok: false,
        backend_name: None,
        backend_kind: None,
        detail,
        output_path: None,
        bytes_written: None,
        width: None,
        height: None,
    }
}

pub(super) fn capture_backend_probe_name(probe: CaptureBackendProbeDto) -> &'static str {
    match probe {
        CaptureBackendProbeDto::None => "none",
        CaptureBackendProbeDto::File => "file",
        CaptureBackendProbeDto::Frame => "frame",
        CaptureBackendProbeDto::Region => "region",
        CaptureBackendProbeDto::DmaBuf => "dmabuf",
        CaptureBackendProbeDto::All => "all",
    }
}

pub(super) fn capture_frame_source_label(
    source: peekaboox_capture::CaptureFrameSource,
) -> &'static str {
    match source {
        peekaboox_capture::CaptureFrameSource::DirectStdout => "direct-stdout",
        peekaboox_capture::CaptureFrameSource::DmaBufZeroCopy => "dmabuf-zero-copy",
        peekaboox_capture::CaptureFrameSource::FileFallback => "file-fallback",
        peekaboox_capture::CaptureFrameSource::FullFrameCrop => "full-frame-crop",
    }
}

pub(super) fn format_rect(rect: Rect) -> String {
    format!("{},{},{}x{}", rect.x, rect.y, rect.width, rect.height)
}

pub(super) fn capture_region_from_request(
    region: Option<Rect>,
    window_id: Option<&str>,
) -> Result<Option<Rect>, String> {
    if region.is_some() && window_id.is_some_and(|value| !value.trim().is_empty()) {
        return Err("provide either capture region or window_id, not both".to_owned());
    }
    if let Some(region) = region {
        return Ok(Some(region));
    }
    let Some(window_id) = window_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let metadata = peekaboox_windows::list_windows().map_err(|error| error.to_string())?;
    let window = metadata
        .windows
        .iter()
        .find(|window| window.id == window_id)
        .ok_or_else(|| format!("window not found: {window_id}"))?;
    if window.bounds.width == 0 || window.bounds.height == 0 {
        return Err(format!("window {window_id} has empty bounds"));
    }
    Ok(Some(window.bounds))
}

pub(super) fn capture_delta_region(
    target: Option<proto::CaptureTarget>,
    legacy_region: Option<proto::Rect>,
) -> Result<Option<Rect>, String> {
    match target.and_then(|target| target.target) {
        None | Some(capture_target::Target::FullScreen(true)) => {
            Ok(legacy_region.map(rect_from_proto))
        }
        Some(capture_target::Target::FullScreen(false)) => {
            Err("capture_delta full_screen target must be true".to_owned())
        }
        Some(capture_target::Target::Region(region)) => Ok(Some(rect_from_proto(region))),
        Some(capture_target::Target::WindowId(window_id)) => {
            capture_region_from_request(None, Some(&window_id))
        }
    }
}

pub(super) fn normalized_capture_stream_id(stream_id: &str) -> String {
    let stream_id = stream_id.trim();
    if stream_id.is_empty() {
        "default".to_owned()
    } else {
        stream_id.to_owned()
    }
}
