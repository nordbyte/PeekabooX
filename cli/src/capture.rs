use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CaptureArgs {
    pub(super) output: PathBuf,
    pub(super) region: Option<Rect>,
    pub(super) window_id: Option<String>,
    pub(super) app: Option<String>,
    pub(super) window_title: Option<String>,
    pub(super) title_regex: Option<String>,
    pub(super) format: CaptureOutputFormat,
    pub(super) jpeg_quality: u8,
    pub(super) json: bool,
    pub(super) stdout: bool,
    pub(super) no_overwrite: bool,
    pub(super) include_semantic_tree: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CaptureCommand {
    Run(CaptureArgs),
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CaptureOutputFormat {
    Png,
    Jpeg,
    Xwd,
}

impl CaptureOutputFormat {
    fn as_str(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Xwd => "xwd",
        }
    }

    fn mime_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Xwd => "image/x-xwindowdump",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CaptureDeltaArgs {
    pub(super) stream_id: Option<String>,
    pub(super) reset: bool,
    pub(super) region: Option<Rect>,
    pub(super) window_id: Option<String>,
    pub(super) per_channel_threshold: u8,
    pub(super) low_bandwidth: bool,
    pub(super) json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CaptureDeltaCommand {
    Run(CaptureDeltaArgs),
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CaptureBackendsArgs {
    pub(super) output: PathBuf,
    pub(super) region: Option<Rect>,
    pub(super) diagnose: bool,
    pub(super) json: bool,
    pub(super) probe: CaptureBackendProbeDto,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CaptureBackendsCommand {
    Run(CaptureBackendsArgs),
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CaptureDmaBufArgs {
    pub(super) import_target: CaptureDmaBufImportTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CaptureDmaBufImportTarget {
    Compute,
    Egl,
    EglTexture,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CaptureDmaBufCommand {
    Run(CaptureDmaBufArgs),
    Help,
}

pub(super) fn capture(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let CaptureCommand::Run(args) = parse_capture_args(args)? else {
        print_capture_usage();
        return Err(CliError::HelpRequested);
    };

    if context.use_daemon {
        if args.stdout {
            return Err(CliError::Failure(
                "capture --stdout is only supported without --daemon".to_owned(),
            ));
        }
        let result = daemon_request(
            context,
            ApiRequest::Capture {
                output: path_to_daemon_string(&args.output)?,
                region: args.region.map(RectDto::from),
                window_id: args.window_id,
                app: args.app,
                window_title: args.window_title,
                title_regex: args.title_regex,
                format: Some(args.format.as_str().to_owned()),
                no_overwrite: args.no_overwrite,
                include_semantic_tree: args.include_semantic_tree,
            },
        )?;
        let ApiResult::Capture(metadata) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected capture response".to_owned(),
            ));
        };
        if args.json {
            print_json_pretty(&metadata)?;
        } else {
            print_capture_result_dto(&metadata);
        }
        return Ok(());
    }

    let target = capture_target_from_args(&args)?;
    let result = capture_cli_execute(&args, target)?;
    if let Some(bytes) = result.stdout_bytes {
        std::io::stdout()
            .write_all(&bytes)
            .map_err(|error| CliError::Failure(error.to_string()))?;
        return Ok(());
    }
    if args.json {
        print_json_pretty(&result.metadata)?;
    } else {
        print_capture_result_dto(&result.metadata);
    }

    Ok(())
}

pub(super) fn parse_capture_args(args: Vec<String>) -> Result<CaptureCommand, CliError> {
    let mut output = None;
    let mut region = None;
    let mut window_id = None;
    let mut app = None;
    let mut window_title = None;
    let mut title_regex = None;
    let mut format = CaptureOutputFormat::Png;
    let mut jpeg_quality = 90_u8;
    let mut json = false;
    let mut stdout = false;
    let mut no_overwrite = false;
    let mut include_semantic_tree = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--output" | "-o" => {
                output = Some(PathBuf::from(parse_next_string(
                    &args, &mut index, "--output",
                )?));
            }
            "--region" | "-r" => {
                let value = parse_next_string(&args, &mut index, "--region")?;
                region = Some(parse_rect("--region", &value)?);
            }
            "--window-id" | "--window" => {
                let value = parse_next_string(&args, &mut index, "--window-id")?;
                window_id = non_empty_cli_string(&value);
            }
            "--app" => {
                let value = parse_next_string(&args, &mut index, "--app")?;
                app = non_empty_cli_string(&value);
            }
            "--window-title" | "--title" => {
                let value = parse_next_string(&args, &mut index, "--window-title")?;
                window_title = non_empty_cli_string(&value);
            }
            "--title-regex" => {
                let value = parse_next_string(&args, &mut index, "--title-regex")?;
                title_regex = non_empty_cli_string(&value);
            }
            "--format" => {
                let value = parse_next_string(&args, &mut index, "--format")?;
                format = parse_capture_output_format(&value)?;
            }
            "--quality" | "--jpeg-quality" => {
                let value = parse_next_string(&args, &mut index, "--quality")?;
                jpeg_quality = parse_jpeg_quality(&value)?;
            }
            "--json" => json = true,
            "--stdout" => stdout = true,
            "--no-overwrite" => no_overwrite = true,
            "--include-semantic-tree" => include_semantic_tree = true,
            "--help" | "-h" => return Ok(CaptureCommand::Help),
            unknown => {
                return Err(CliError::Failure(format!(
                    "unknown capture argument: {unknown}"
                )));
            }
        }

        index += 1;
    }

    if stdout && json {
        return Err(CliError::Failure(
            "capture --stdout cannot be combined with --json".to_owned(),
        ));
    }
    if stdout && no_overwrite {
        return Err(CliError::Failure(
            "capture --stdout cannot be combined with --no-overwrite".to_owned(),
        ));
    }
    if stdout && format == CaptureOutputFormat::Xwd {
        return Err(CliError::Failure(
            "capture --stdout does not support xwd output".to_owned(),
        ));
    }
    if include_semantic_tree && !json {
        return Err(CliError::Failure(
            "capture --include-semantic-tree requires --json".to_owned(),
        ));
    }
    if format == CaptureOutputFormat::Xwd
        && (region.is_some()
            || window_id.is_some()
            || app.is_some()
            || window_title.is_some()
            || title_regex.is_some())
    {
        return Err(CliError::Failure(
            "capture --format xwd only supports full-screen file output".to_owned(),
        ));
    }

    let output = output.unwrap_or_else(|| match format {
        CaptureOutputFormat::Png => PathBuf::from("screenshot.png"),
        CaptureOutputFormat::Jpeg => PathBuf::from("screenshot.jpg"),
        CaptureOutputFormat::Xwd => PathBuf::from("screenshot.xwd"),
    });
    if format == CaptureOutputFormat::Xwd
        && !output
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("xwd"))
    {
        return Err(CliError::Failure(
            "capture --format xwd output path must end in .xwd".to_owned(),
        ));
    }

    Ok(CaptureCommand::Run(CaptureArgs {
        output,
        region,
        window_id,
        app,
        window_title,
        title_regex,
        format,
        jpeg_quality,
        json,
        stdout,
        no_overwrite,
        include_semantic_tree,
    }))
}

pub(super) fn capture_delta(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let CaptureDeltaCommand::Run(args) = parse_capture_delta_args(args)? else {
        print_capture_delta_usage();
        return Err(CliError::HelpRequested);
    };

    if !context.use_daemon {
        return Err(CliError::Failure(
            "capture-delta requires --daemon so frame state can persist between calls".to_owned(),
        ));
    }

    let result = daemon_request(
        context,
        ApiRequest::CaptureDelta {
            stream_id: args.stream_id,
            reset: args.reset,
            region: args.region.map(RectDto::from),
            window_id: args.window_id,
            per_channel_threshold: args.per_channel_threshold,
            low_bandwidth: args.low_bandwidth,
        },
    )?;
    let ApiResult::CaptureDelta(delta) = result else {
        return Err(CliError::Failure(
            "daemon returned unexpected capture delta response".to_owned(),
        ));
    };
    if args.json {
        print_json_pretty(&delta)?;
    } else {
        print_capture_delta_dto(&delta);
    }
    Ok(())
}

pub(super) fn parse_capture_delta_args(args: Vec<String>) -> Result<CaptureDeltaCommand, CliError> {
    let mut stream_id = None;
    let mut reset = false;
    let mut region = None;
    let mut window_id = None;
    let mut per_channel_threshold = 0_u8;
    let mut low_bandwidth = true;
    let mut json = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--stream" | "--stream-id" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --stream".to_owned()));
                };
                stream_id = Some(value.clone());
            }
            "--reset" => reset = true,
            "--region" | "-r" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --region".to_owned()));
                };
                region = Some(parse_rect("--region", value)?);
            }
            "--window-id" | "--window" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --window-id".to_owned(),
                    ));
                };
                window_id = non_empty_cli_string(value);
            }
            "--threshold" | "-t" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --threshold".to_owned(),
                    ));
                };
                per_channel_threshold = parse_u8("--threshold", value)?;
            }
            "--low-bandwidth" => low_bandwidth = true,
            "--full-frame" => low_bandwidth = false,
            "--json" => json = true,
            "--help" | "-h" => return Ok(CaptureDeltaCommand::Help),
            unknown => {
                return Err(CliError::Failure(format!(
                    "unknown capture-delta argument: {unknown}"
                )));
            }
        }

        index += 1;
    }

    if region.is_some() && window_id.is_some() {
        return Err(CliError::Failure(
            "provide either --region or --window-id, not both".to_owned(),
        ));
    }

    Ok(CaptureDeltaCommand::Run(CaptureDeltaArgs {
        stream_id,
        reset,
        region,
        window_id,
        per_channel_threshold,
        low_bandwidth,
        json,
    }))
}

pub(super) fn capture_backends(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let CaptureBackendsCommand::Run(args) = parse_capture_backends_args(args)? else {
        print_capture_backends_usage();
        return Err(CliError::HelpRequested);
    };

    let result = if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::CaptureBackends {
                output: path_to_daemon_string(&args.output)?,
                region: args.region.map(RectDto::from),
                diagnose: args.diagnose,
                probe: args.probe,
            },
        )?;
        let ApiResult::CaptureBackends(result) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected capture backend response".to_owned(),
            ));
        };
        result
    } else {
        capture_backends_result(&args)
    };

    if args.json {
        print_json_pretty(&result)?;
    } else {
        print_capture_backends_result(&result, args.diagnose);
    }

    if args.probe != CaptureBackendProbeDto::None && result.probes.iter().any(|probe| !probe.ok) {
        return Err(CliError::Failure(
            "one or more capture backend probes failed".to_owned(),
        ));
    }

    Ok(())
}

pub(super) fn parse_capture_backends_args(
    args: Vec<String>,
) -> Result<CaptureBackendsCommand, CliError> {
    let mut output = PathBuf::from("screenshot.png");
    let mut output_explicit = false;
    let mut format_explicit = false;
    let mut region = None;
    let mut diagnose = false;
    let mut json = false;
    let mut probe = CaptureBackendProbeDto::None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--output" | "-o" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --output".to_owned()));
                };
                if format_explicit {
                    return Err(CliError::Failure(
                        "provide either --output or --format, not both".to_owned(),
                    ));
                }
                output = PathBuf::from(value);
                output_explicit = true;
            }
            "--format" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --format".to_owned()));
                };
                if output_explicit {
                    return Err(CliError::Failure(
                        "provide either --output or --format, not both".to_owned(),
                    ));
                }
                output = default_capture_backends_output_for_format(value)?;
                format_explicit = true;
            }
            "--region" | "-r" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --region".to_owned()));
                };
                region = Some(parse_rect("--region", value)?);
            }
            "--probe" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --probe".to_owned()));
                };
                probe = parse_capture_backend_probe(value)?;
            }
            "--diagnose" | "--all" => diagnose = true,
            "--json" => json = true,
            "--help" | "-h" => return Ok(CaptureBackendsCommand::Help),
            unknown => {
                return Err(CliError::Failure(format!(
                    "unknown capture-backends argument: {unknown}"
                )));
            }
        }

        index += 1;
    }

    Ok(CaptureBackendsCommand::Run(CaptureBackendsArgs {
        output,
        region,
        diagnose,
        json,
        probe,
    }))
}

pub(super) fn default_capture_backends_output_for_format(value: &str) -> Result<PathBuf, CliError> {
    match value {
        "png" => Ok(PathBuf::from("screenshot.png")),
        "jpg" | "jpeg" => Ok(PathBuf::from("screenshot.jpg")),
        "xwd" => Ok(PathBuf::from("screenshot.xwd")),
        _ => Err(CliError::Failure(format!(
            "--format must be png, jpeg, or xwd, got {value:?}"
        ))),
    }
}

pub(super) fn parse_capture_backend_probe(value: &str) -> Result<CaptureBackendProbeDto, CliError> {
    match value {
        "none" => Ok(CaptureBackendProbeDto::None),
        "file" => Ok(CaptureBackendProbeDto::File),
        "frame" => Ok(CaptureBackendProbeDto::Frame),
        "region" => Ok(CaptureBackendProbeDto::Region),
        "dmabuf" | "dma-buf" | "zero-copy" | "zero_copy" => Ok(CaptureBackendProbeDto::DmaBuf),
        "all" => Ok(CaptureBackendProbeDto::All),
        _ => Err(CliError::Failure(format!(
            "--probe must be none, file, frame, region, dmabuf, or all, got {value:?}"
        ))),
    }
}

pub(super) fn capture_backends_result(args: &CaptureBackendsArgs) -> CaptureBackendsResultDto {
    let environment = peekaboox_capture::CaptureEnvironment::detect();
    let capabilities = peekaboox_capture::capture_backend_capabilities(&environment, &args.output);
    let image_backends = capabilities
        .into_iter()
        .filter(|capability| args.diagnose || capability.reason.is_none())
        .map(capture_backend_dto)
        .collect::<Vec<_>>();
    let zero_copy_backends = peekaboox_capture::zero_copy_capture_capabilities(&environment)
        .into_iter()
        .map(zero_copy_backend_dto)
        .collect::<Vec<_>>();
    let mut warnings = capture_backend_warnings(&zero_copy_backends);
    let probes = capture_backend_probe_steps(args.probe)
        .into_iter()
        .map(|probe| capture_backend_probe(probe, args))
        .collect::<Vec<_>>();

    if matches!(
        args.probe,
        CaptureBackendProbeDto::Region | CaptureBackendProbeDto::All
    ) && args.region.is_none()
    {
        warnings.push("region probe used default region 0,0,320,180".to_owned());
    }

    CaptureBackendsResultDto {
        session_type: environment.session_type.name().to_owned(),
        desktop: environment.current_desktop,
        pipewire_session_available: environment.pipewire_session_available,
        pipewire_backend_feature_enabled: peekaboox_capture::pipewire_backend_feature_enabled(),
        egl_backend_feature_enabled: peekaboox_capture::egl_backend_feature_enabled(),
        output_path: args.output.display().to_string(),
        region: args.region.map(RectDto::from),
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
        backend_kind: backend_kind_label(capability.backend_kind),
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
        backend_kind: backend_kind_label(capability.backend_kind),
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
    args: &CaptureBackendsArgs,
) -> CaptureBackendProbeResultDto {
    match probe {
        CaptureBackendProbeDto::File => capture_backend_probe_file(&args.output),
        CaptureBackendProbeDto::Frame => capture_backend_probe_frame(),
        CaptureBackendProbeDto::Region => {
            capture_backend_probe_region(args.region.unwrap_or(Rect::new(0, 0, 320, 180)))
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
            backend_kind: Some(backend_kind_label(metadata.backend_kind)),
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
            backend_kind: Some(backend_kind_label(metadata.backend_kind)),
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
            backend_kind: Some(backend_kind_label(metadata.backend_kind)),
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
    if !peekaboox_capture::pipewire_backend_feature_enabled() {
        return capture_backend_probe_error(
            "dmabuf",
            "compiled without pipewire-backend feature".to_owned(),
        );
    }

    let stream = match peekaboox_capture::open_pipewire_screencast() {
        Ok(stream) => stream,
        Err(error) => return capture_backend_probe_error("dmabuf", error.to_string()),
    };
    let stream_node_id = stream.stream_node_id;
    let pipewire_serial = stream.pipewire_serial;
    let descriptor = match peekaboox_capture::capture_pipewire_dmabuf_frame(stream) {
        Ok(descriptor) => descriptor,
        Err(error) => return capture_backend_probe_error("dmabuf", error.to_string()),
    };
    let imported = match peekaboox_capture::import_dmabuf_frame(
        &descriptor,
        peekaboox_capture::DmaBufImportTarget::Compute,
    ) {
        Ok(imported) => imported,
        Err(error) => return capture_backend_probe_error("dmabuf", error.to_string()),
    };

    CaptureBackendProbeResultDto {
        probe: "dmabuf".to_owned(),
        ok: true,
        backend_name: Some(imported.backend_name),
        backend_kind: Some(imported.backend_kind.name().to_owned()),
        detail: format!(
            "stream node_id={} pipewire_serial={} frame={}x{} format={:?} planes={}",
            stream_node_id,
            pipewire_serial
                .map(|serial| serial.to_string())
                .unwrap_or_else(|| "-".to_owned()),
            descriptor.width,
            descriptor.height,
            descriptor.format,
            descriptor.planes.len()
        ),
        output_path: None,
        bytes_written: None,
        width: Some(descriptor.width),
        height: Some(descriptor.height),
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

pub(super) fn print_capture_backends_result(result: &CaptureBackendsResultDto, diagnose: bool) {
    if diagnose {
        println!(
            "session={} desktop={} pipewire_session={} output={}",
            result.session_type,
            result.desktop.as_deref().unwrap_or("-"),
            result.pipewire_session_available,
            result.output_path
        );
        println!(
            "build pipewire_backend={} egl_backend={}",
            result.pipewire_backend_feature_enabled, result.egl_backend_feature_enabled
        );
    } else {
        println!(
            "session={} desktop={} pipewire_session={}",
            capture_session_display(&result.session_type),
            result.desktop.as_deref().unwrap_or("-"),
            result.pipewire_session_available
        );
    }

    if result.image_backends.is_empty() {
        println!("image_backend none");
    } else {
        for backend in &result.image_backends {
            if diagnose {
                println!(
                    "image_backend name={} kind={} command={} available={} output={} stdout_frame={} stdout_region={} selected={} reason={}",
                    backend.name,
                    backend.backend_kind,
                    backend.command.as_deref().unwrap_or("-"),
                    backend.available,
                    backend.supports_output,
                    backend.supports_stdout_capture,
                    backend.supports_stdout_region_capture,
                    backend.selected,
                    backend.reason.as_deref().unwrap_or("-")
                );
            } else {
                println!(
                    "image_backend name={} kind={}",
                    backend.name, backend.backend_kind
                );
            }
        }
    }

    for backend in &result.zero_copy_backends {
        if diagnose {
            println!(
                "zero_copy_backend name={} kind={} transport={} availability={} selected={} reason={}",
                backend.name,
                backend.backend_kind,
                backend.transport,
                backend.availability,
                backend.selected,
                backend.reason.as_deref().unwrap_or("-")
            );
        } else {
            println!(
                "zero_copy_backend name={} kind={} transport={} availability={}",
                backend.name,
                backend.backend_kind,
                backend.transport,
                capture_availability_display(&backend.availability)
            );
        }
    }

    for warning in &result.warnings {
        println!("warning {warning}");
    }

    for probe in &result.probes {
        println!(
            "probe name={} ok={} backend={} kind={} output={} bytes={} size={} detail={}",
            probe.probe,
            probe.ok,
            probe.backend_name.as_deref().unwrap_or("-"),
            probe.backend_kind.as_deref().unwrap_or("-"),
            probe.output_path.as_deref().unwrap_or("-"),
            probe
                .bytes_written
                .map(|bytes| bytes.to_string())
                .unwrap_or_else(|| "-".to_owned()),
            match (probe.width, probe.height) {
                (Some(width), Some(height)) => format!("{width}x{height}"),
                _ => "-".to_owned(),
            },
            probe.detail
        );
    }
}

pub(super) fn capture_session_display(value: &str) -> String {
    match value {
        "wayland" => "Wayland".to_owned(),
        "x11" => "X11".to_owned(),
        "unknown" => "Unknown".to_owned(),
        other => other.to_owned(),
    }
}

pub(super) fn capture_availability_display(value: &str) -> String {
    match value {
        "available" => "Available".to_owned(),
        "missing_pipewire_backend" => "MissingPipeWireBackend".to_owned(),
        "missing_pipewire_session" => "MissingPipeWireSession".to_owned(),
        "unsupported_session" => "UnsupportedSession".to_owned(),
        other => other.to_owned(),
    }
}

pub(super) fn capture_dmabuf(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let CaptureDmaBufCommand::Run(args) = parse_capture_dmabuf_args(args)? else {
        print_capture_dmabuf_usage();
        return Err(CliError::HelpRequested);
    };

    if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::ProbeDmaBuf {
                import_target: dmabuf_import_target_dto(args.import_target),
            },
        )?;
        let ApiResult::DmaBufProbe(probe) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected DMA-BUF probe response".to_owned(),
            ));
        };
        print_dmabuf_probe_dto(&probe);
        return Ok(());
    }

    let stream = peekaboox_capture::open_pipewire_screencast()
        .map_err(|error| CliError::Failure(error.to_string()))?;
    println!(
        "dmabuf_stream node_id={} pipewire_serial={}",
        stream.stream_node_id,
        stream
            .pipewire_serial
            .map(|serial| serial.to_string())
            .unwrap_or_else(|| "-".to_owned())
    );

    let descriptor = peekaboox_capture::capture_pipewire_dmabuf_frame(stream)
        .map_err(|error| CliError::Failure(error.to_string()))?;

    println!(
        "dmabuf_frame width={} height={} format={:?} fourcc=0x{:08x} planes={}",
        descriptor.width,
        descriptor.height,
        descriptor.format,
        descriptor.fourcc,
        descriptor.planes.len()
    );
    for (index, plane) in descriptor.planes.iter().enumerate() {
        println!(
            "dmabuf_plane index={} fd={} offset={} stride={} modifier=0x{:016x}",
            index, plane.fd, plane.offset, plane.stride, plane.modifier
        );
    }

    print_dmabuf_import(&descriptor, args.import_target)?;

    Ok(())
}

pub(super) fn dmabuf_import_target_dto(target: CaptureDmaBufImportTarget) -> DmaBufImportTargetDto {
    match target {
        CaptureDmaBufImportTarget::Compute => DmaBufImportTargetDto::Compute,
        CaptureDmaBufImportTarget::Egl => DmaBufImportTargetDto::Egl,
        CaptureDmaBufImportTarget::EglTexture => DmaBufImportTargetDto::EglTexture,
    }
}

pub(super) fn print_dmabuf_probe_dto(probe: &DmaBufProbeResultDto) {
    println!(
        "dmabuf_stream node_id={} pipewire_serial={}",
        probe.stream_node_id,
        probe
            .pipewire_serial
            .map(|serial| serial.to_string())
            .unwrap_or_else(|| "-".to_owned())
    );
    println!(
        "dmabuf_frame width={} height={} format={} fourcc=0x{:08x} planes={}",
        probe.width, probe.height, probe.pixel_format, probe.fourcc, probe.planes
    );
    println!(
        "dmabuf_import target={} backend={} layout={} synchronization={} planes={} egl_version={} egl_modifiers={} texture_id={}",
        dmabuf_import_target_label(probe.import_target),
        probe.backend_name,
        probe.memory_layout,
        probe.synchronization,
        probe.planes,
        probe.egl_version.as_deref().unwrap_or("-"),
        probe
            .egl_modifiers
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_owned()),
        probe
            .texture_id
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_owned())
    );
}

pub(super) fn dmabuf_import_target_label(target: DmaBufImportTargetDto) -> &'static str {
    match target {
        DmaBufImportTargetDto::Compute => "compute",
        DmaBufImportTargetDto::Egl => "egl",
        DmaBufImportTargetDto::EglTexture => "egl-texture",
    }
}

pub(super) fn parse_capture_dmabuf_args(
    args: Vec<String>,
) -> Result<CaptureDmaBufCommand, CliError> {
    let mut import_target = CaptureDmaBufImportTarget::Compute;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--import" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --import".to_owned()));
                };
                import_target = parse_capture_dmabuf_import_target(value)?;
            }
            "--help" | "-h" => return Ok(CaptureDmaBufCommand::Help),
            unknown => {
                return Err(CliError::Failure(format!(
                    "unknown capture-dmabuf argument: {unknown}"
                )));
            }
        }

        index += 1;
    }

    Ok(CaptureDmaBufCommand::Run(CaptureDmaBufArgs {
        import_target,
    }))
}

pub(super) fn parse_capture_dmabuf_import_target(
    value: &str,
) -> Result<CaptureDmaBufImportTarget, CliError> {
    match value {
        "compute" => Ok(CaptureDmaBufImportTarget::Compute),
        "egl" => Ok(CaptureDmaBufImportTarget::Egl),
        "egl-texture" | "texture" => Ok(CaptureDmaBufImportTarget::EglTexture),
        unknown => Err(CliError::Failure(format!(
            "unsupported capture-dmabuf import target: {unknown}"
        ))),
    }
}

pub(super) fn print_dmabuf_import(
    descriptor: &peekaboox_capture::DmaBufFrameDescriptor,
    import_target: CaptureDmaBufImportTarget,
) -> Result<(), CliError> {
    match import_target {
        CaptureDmaBufImportTarget::Compute => {
            let imported = peekaboox_capture::import_dmabuf_frame(
                descriptor,
                peekaboox_capture::DmaBufImportTarget::Compute,
            )
            .map_err(|error| CliError::Failure(error.to_string()))?;
            println!(
                "dmabuf_import target={} backend={} layout={} synchronization={} planes={}",
                imported.descriptor.target.name(),
                imported.backend_name,
                imported.descriptor.memory_layout.name(),
                imported.descriptor.synchronization.name(),
                imported.descriptor.planes.len()
            );
            Ok(())
        }
        CaptureDmaBufImportTarget::Egl => print_egl_dmabuf_import(descriptor),
        CaptureDmaBufImportTarget::EglTexture => print_egl_texture_dmabuf_import(descriptor),
    }
}

#[cfg(feature = "egl-backend")]
pub(super) fn print_egl_dmabuf_import(
    descriptor: &peekaboox_capture::DmaBufFrameDescriptor,
) -> Result<(), CliError> {
    let importer = peekaboox_capture::EglDmaBufImporter::new()
        .map_err(|error| CliError::Failure(error.to_string()))?;
    let imported = importer
        .import_image(descriptor)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    println!(
        "dmabuf_import target={} backend={} layout={} synchronization={} planes={} egl_version={}.{} egl_modifiers={} image={:p}",
        imported.descriptor.target.name(),
        imported.backend_name,
        imported.descriptor.memory_layout.name(),
        imported.descriptor.synchronization.name(),
        imported.descriptor.planes.len(),
        importer.egl_version().0,
        importer.egl_version().1,
        importer.supports_modifiers(),
        imported.native_image_handle()
    );
    Ok(())
}

#[cfg(not(feature = "egl-backend"))]
pub(super) fn print_egl_dmabuf_import(
    _descriptor: &peekaboox_capture::DmaBufFrameDescriptor,
) -> Result<(), CliError> {
    Err(CliError::Failure(
        "capture-dmabuf --import egl requires the `egl-backend` feature".to_owned(),
    ))
}

#[cfg(feature = "egl-backend")]
pub(super) fn print_egl_texture_dmabuf_import(
    descriptor: &peekaboox_capture::DmaBufFrameDescriptor,
) -> Result<(), CliError> {
    let importer = peekaboox_capture::EglTextureDmaBufImporter::new()
        .map_err(|error| CliError::Failure(error.to_string()))?;
    let imported = importer
        .import_texture(descriptor)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    println!(
        "dmabuf_import target=egl-texture backend={} layout={} synchronization={} planes={} egl_version={}.{} egl_modifiers={} texture_id={} image={:p}",
        imported.backend_name,
        imported.descriptor.memory_layout.name(),
        imported.descriptor.synchronization.name(),
        imported.descriptor.planes.len(),
        importer.egl_version().0,
        importer.egl_version().1,
        importer.supports_modifiers(),
        imported.texture_id(),
        imported.native_image_handle()
    );
    Ok(())
}

#[cfg(not(feature = "egl-backend"))]
pub(super) fn print_egl_texture_dmabuf_import(
    _descriptor: &peekaboox_capture::DmaBufFrameDescriptor,
) -> Result<(), CliError> {
    Err(CliError::Failure(
        "capture-dmabuf --import egl-texture requires the `egl-backend` feature".to_owned(),
    ))
}

pub(super) fn capture_cli_to_file(
    output: impl AsRef<std::path::Path>,
    region: Option<Rect>,
) -> peekaboox_core::Result<peekaboox_capture::CaptureFileMetadata> {
    match region {
        Some(region) => peekaboox_capture::capture_region_to_file(region, output),
        None => peekaboox_capture::capture_screen_to_file(output),
    }
}

#[derive(Debug, Clone)]
pub(super) struct CaptureTarget {
    pub(super) capture_region: Option<Rect>,
    pub(super) window: Option<WindowInfo>,
}

#[derive(Debug, Clone)]
pub(super) struct CaptureCliExecutionResult {
    pub(super) metadata: CaptureResultDto,
    pub(super) stdout_bytes: Option<Vec<u8>>,
}

pub(super) fn capture_cli_execute(
    args: &CaptureArgs,
    target: CaptureTarget,
) -> Result<CaptureCliExecutionResult, CliError> {
    if args.format == CaptureOutputFormat::Xwd {
        let output_path = ensure_capture_output_path(&args.output, args.no_overwrite)?;
        let metadata = peekaboox_capture::capture_screen_to_file(&output_path)
            .map_err(|error| CliError::Failure(error.to_string()))?;
        return Ok(CaptureCliExecutionResult {
            metadata: CaptureResultDto {
                output_path: metadata.output_path.display().to_string(),
                backend_name: metadata.backend_name,
                backend_kind: backend_kind_label(metadata.backend_kind),
                bytes_written: metadata.bytes_written,
                width: 0,
                height: 0,
                mime_type: args.format.mime_type().to_owned(),
                capture_region: None,
                window_id: None,
                window: None,
                captured_at_unix_ms: unix_time_ms_u64(),
                source: "file-backend".to_owned(),
                semantic_tree: Vec::new(),
            },
            stdout_bytes: None,
        });
    }

    let output_path = if args.stdout {
        None
    } else {
        Some(ensure_capture_output_path(&args.output, args.no_overwrite)?)
    };
    let frame_metadata = match target.capture_region {
        Some(region) => peekaboox_capture::capture_region_frame(region),
        None => peekaboox_capture::capture_screen_frame(),
    }
    .map_err(|error| CliError::Failure(error.to_string()))?;
    let width = frame_metadata.frame.width;
    let height = frame_metadata.frame.height;
    let source = capture_frame_source_label(frame_metadata.source).to_owned();
    let captured_at_unix_ms = unix_time_ms_u64();
    let (bytes_written, stdout_bytes, output_path_label) = if let Some(output_path) = output_path {
        let bytes_written = match args.format {
            CaptureOutputFormat::Png => {
                peekaboox_capture::write_frame_png(&frame_metadata.frame, &output_path)
                    .map_err(|error| CliError::Failure(error.to_string()))?
            }
            CaptureOutputFormat::Jpeg => peekaboox_capture::write_frame_jpeg(
                &frame_metadata.frame,
                &output_path,
                args.jpeg_quality,
            )
            .map_err(|error| CliError::Failure(error.to_string()))?,
            CaptureOutputFormat::Xwd => unreachable!("xwd handled before frame capture"),
        };
        (bytes_written, None, output_path.display().to_string())
    } else {
        let bytes = match args.format {
            CaptureOutputFormat::Png => peekaboox_capture::encode_frame_png(&frame_metadata.frame)
                .map_err(|error| CliError::Failure(error.to_string()))?,
            CaptureOutputFormat::Jpeg => {
                peekaboox_capture::encode_frame_jpeg(&frame_metadata.frame, args.jpeg_quality)
                    .map_err(|error| CliError::Failure(error.to_string()))?
            }
            CaptureOutputFormat::Xwd => unreachable!("xwd stdout rejected by parser"),
        };
        (bytes.len() as u64, Some(bytes), String::new())
    };
    let semantic_tree = if args.include_semantic_tree {
        peekaboox_accessibility::semantic_tree()
            .map_err(|error| CliError::Failure(error.to_string()))?
            .elements
            .iter()
            .map(ElementDto::from)
            .collect()
    } else {
        Vec::new()
    };
    let window_id = target.window.as_ref().map(|window| window.id.clone());
    let window = target.window.as_ref().map(WindowDto::from);

    Ok(CaptureCliExecutionResult {
        metadata: CaptureResultDto {
            output_path: output_path_label,
            backend_name: frame_metadata.backend_name,
            backend_kind: backend_kind_label(frame_metadata.backend_kind),
            bytes_written,
            width,
            height,
            mime_type: args.format.mime_type().to_owned(),
            capture_region: target.capture_region.map(RectDto::from),
            window_id,
            window,
            captured_at_unix_ms,
            source,
            semantic_tree,
        },
        stdout_bytes,
    })
}

pub(super) fn capture_target_from_args(args: &CaptureArgs) -> Result<CaptureTarget, CliError> {
    let window = resolve_capture_window(args)?;
    let capture_region = match (&window, args.region) {
        (Some(window), Some(region)) => Some(offset_window_relative_region(window.bounds, region)?),
        (Some(window), None) => Some(window.bounds),
        (None, Some(region)) => Some(region),
        (None, None) => None,
    };

    Ok(CaptureTarget {
        capture_region,
        window,
    })
}

pub(super) fn resolve_capture_window(args: &CaptureArgs) -> Result<Option<WindowInfo>, CliError> {
    if args.window_id.is_none()
        && args.app.is_none()
        && args.window_title.is_none()
        && args.title_regex.is_none()
    {
        return Ok(None);
    }

    let query = peekaboox_windows::WindowQuery {
        id: args.window_id.clone(),
        app: args.app.clone(),
        title: args.window_title.clone(),
        title_regex: args.title_regex.clone(),
        focused_only: false,
        limit: None,
        sort: peekaboox_windows::WindowSort::Focused,
        backend: peekaboox_windows::WindowBackendSelection::Auto,
        diagnose: false,
    };
    let metadata = peekaboox_windows::list_windows_with_query(query)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    let window = metadata
        .windows
        .into_iter()
        .next()
        .ok_or_else(|| CliError::Failure("no window matched capture filters".to_owned()))?;
    if window.bounds.width == 0 || window.bounds.height == 0 {
        return Err(CliError::Failure(format!(
            "window {} has empty bounds",
            window.id
        )));
    }
    Ok(Some(window))
}

pub(super) fn offset_window_relative_region(origin: Rect, region: Rect) -> Result<Rect, CliError> {
    if region.x < 0 || region.y < 0 {
        return Err(CliError::Failure(
            "window-relative capture region must start inside the window".to_owned(),
        ));
    }
    let right = i64::from(region.x) + i64::from(region.width);
    let bottom = i64::from(region.y) + i64::from(region.height);
    if right > i64::from(origin.width) || bottom > i64::from(origin.height) {
        return Err(CliError::Failure(
            "window-relative capture region must fit inside the window".to_owned(),
        ));
    }
    let x = i64::from(origin.x) + i64::from(region.x);
    let y = i64::from(origin.y) + i64::from(region.y);
    Ok(Rect::new(
        i32::try_from(x)
            .map_err(|_| CliError::Failure("window-relative region x overflow".to_owned()))?,
        i32::try_from(y)
            .map_err(|_| CliError::Failure("window-relative region y overflow".to_owned()))?,
        region.width,
        region.height,
    ))
}

pub(super) fn ensure_capture_output_path(
    output: &Path,
    no_overwrite: bool,
) -> Result<PathBuf, CliError> {
    let output = if output.is_absolute() {
        output.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| CliError::Failure(error.to_string()))?
            .join(output)
    };
    if no_overwrite && output.exists() {
        return Err(CliError::Failure(format!(
            "capture output already exists: {}",
            output.display()
        )));
    }
    Ok(output)
}

pub(super) fn unix_time_ms_u64() -> u64 {
    u64::try_from(monotonic_ms()).unwrap_or(u64::MAX)
}

pub(super) fn parse_capture_output_format(value: &str) -> Result<CaptureOutputFormat, CliError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "png" => Ok(CaptureOutputFormat::Png),
        "jpg" | "jpeg" => Ok(CaptureOutputFormat::Jpeg),
        "xwd" => Ok(CaptureOutputFormat::Xwd),
        other => Err(CliError::Failure(format!(
            "invalid capture format {other:?}; expected png, jpeg, or xwd"
        ))),
    }
}

pub(super) fn parse_jpeg_quality(value: &str) -> Result<u8, CliError> {
    let quality = parse_u8("--quality", value)?;
    if !(1..=100).contains(&quality) {
        return Err(CliError::Failure(format!(
            "--quality must be between 1 and 100, got {value:?}"
        )));
    }
    Ok(quality)
}

pub(super) fn print_capture_result_dto(metadata: &CaptureResultDto) {
    if metadata.output_path.is_empty() {
        println!(
            "captured {} bytes via {} ({}x{}, {})",
            metadata.bytes_written,
            metadata.backend_name,
            metadata.width,
            metadata.height,
            metadata.source
        );
    } else if metadata.width == 0 || metadata.height == 0 {
        println!(
            "captured {} bytes to {} via {}",
            metadata.bytes_written, metadata.output_path, metadata.backend_name
        );
    } else {
        println!(
            "captured {} bytes to {} via {} ({}x{}, {})",
            metadata.bytes_written,
            metadata.output_path,
            metadata.backend_name,
            metadata.width,
            metadata.height,
            metadata.source
        );
    }
}

pub(super) fn format_rect(rect: Rect) -> String {
    format!("{},{},{}x{}", rect.x, rect.y, rect.width, rect.height)
}

pub(super) fn contains_case_insensitive(value: &str, needle: &str) -> bool {
    value
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

pub(super) fn backend_kind_label(kind: BackendKind) -> String {
    format!("{kind:?}").to_ascii_lowercase()
}
