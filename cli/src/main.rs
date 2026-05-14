use std::path::PathBuf;

use peekaboox_accessibility::{AccessibilityTreeMetadata, ElementQuery};
use peekaboox_core::{BackendKind, Point, Rect, UiElement};
use peekaboox_input::MouseButton;
use peekaboox_ipc::{
    ActionResultDto, ApiRequest, ApiResponse, ApiResult, CaptureDeltaResultDto,
    DmaBufImportTargetDto, DmaBufProbeResultDto, ElementDto, ElementListResultDto, MouseButtonDto,
    OcrResultDto, PluginDiscoveryErrorDto, PluginDto, PluginListResultDto, PluginToolDto, RectDto,
    UiStateDto, VisualDiffDto, WindowDto, WindowListResultDto, default_socket_path, send_request,
};
use peekaboox_vision::{
    OcrOptions, OcrResult, TesseractOcrBackend, UiElementDetectionOptions, UiStateOptions,
    UiStateResult, VisualCompareOptions, VisualDiffResult,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    let global = match parse_global_args(std::env::args().skip(1).collect()) {
        Ok(global) => global,
        Err(error) => {
            eprintln!("peekaboox failed: {error}");
            std::process::exit(1);
        }
    };
    let mut args = global.args.into_iter();

    match args.next().as_deref() {
        Some("--version") | Some("-V") => println!("peekaboox {VERSION}"),
        Some("capture") => match capture(args.collect(), &global.context) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("capture failed: {error}");
                std::process::exit(1);
            }
        },
        Some("capture-delta") | Some("delta") => {
            match capture_delta(args.collect(), &global.context) {
                Ok(()) => {}
                Err(CliError::HelpRequested) => {}
                Err(CliError::Failure(error)) => {
                    eprintln!("capture-delta failed: {error}");
                    std::process::exit(1);
                }
            }
        }
        Some("capture-backends") | Some("backends") => match capture_backends(args.collect()) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("capture-backends failed: {error}");
                std::process::exit(1);
            }
        },
        Some("capture-dmabuf") | Some("dmabuf") => {
            match capture_dmabuf(args.collect(), &global.context) {
                Ok(()) => {}
                Err(CliError::HelpRequested) => {}
                Err(CliError::Failure(error)) => {
                    eprintln!("capture-dmabuf failed: {error}");
                    std::process::exit(1);
                }
            }
        }
        Some("plugins") | Some("plugin") => match plugins(args.collect(), &global.context) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("plugins failed: {error}");
                std::process::exit(1);
            }
        },
        Some("windows") => match windows(args.collect(), &global.context) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("windows failed: {error}");
                std::process::exit(1);
            }
        },
        Some("elements") | Some("find") => match elements(args.collect(), &global.context) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("elements failed: {error}");
                std::process::exit(1);
            }
        },
        Some("ocr") => match ocr(args.collect(), &global.context) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("ocr failed: {error}");
                std::process::exit(1);
            }
        },
        Some("compare") | Some("diff") => match compare(args.collect(), &global.context) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("compare failed: {error}");
                std::process::exit(1);
            }
        },
        Some("state") | Some("ui-state") => match ui_state(args.collect(), &global.context) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("state failed: {error}");
                std::process::exit(1);
            }
        },
        Some("vision-elements") | Some("detect-elements") => {
            match vision_elements(args.collect(), &global.context) {
                Ok(()) => {}
                Err(CliError::HelpRequested) => {}
                Err(CliError::Failure(error)) => {
                    eprintln!("vision-elements failed: {error}");
                    std::process::exit(1);
                }
            }
        }
        Some("click") => match click(args.collect(), &global.context) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("click failed: {error}");
                std::process::exit(1);
            }
        },
        Some("move") | Some("move-mouse") => match move_mouse(args.collect(), &global.context) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("move failed: {error}");
                std::process::exit(1);
            }
        },
        Some("drag") => match drag(args.collect(), &global.context) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("drag failed: {error}");
                std::process::exit(1);
            }
        },
        Some("type") => match type_text(args.collect(), &global.context) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("type failed: {error}");
                std::process::exit(1);
            }
        },
        Some("hotkey") | Some("key") => match hotkey(args.collect(), &global.context) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("hotkey failed: {error}");
                std::process::exit(1);
            }
        },
        Some("--help") | Some("-h") => print_usage(),
        Some(command) => {
            eprintln!("unknown peekaboox command: {command}");
            print_usage();
            std::process::exit(2);
        }
        None => print_usage(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GlobalArgs {
    context: CliContext,
    args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliContext {
    use_daemon: bool,
    socket: PathBuf,
}

fn parse_global_args(args: Vec<String>) -> Result<GlobalArgs, String> {
    let mut use_daemon = false;
    let mut socket = default_socket_path();
    let mut remaining = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--daemon" => use_daemon = true,
            "--socket" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("missing value for --socket".to_owned());
                };
                socket = PathBuf::from(value);
            }
            _ => {
                remaining.extend_from_slice(&args[index..]);
                break;
            }
        }

        index += 1;
    }

    Ok(GlobalArgs {
        context: CliContext { use_daemon, socket },
        args: remaining,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CaptureArgs {
    output: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CaptureCommand {
    Run(CaptureArgs),
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CaptureDeltaArgs {
    stream_id: Option<String>,
    reset: bool,
    region: Option<Rect>,
    per_channel_threshold: u8,
    low_bandwidth: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CaptureDeltaCommand {
    Run(CaptureDeltaArgs),
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CaptureBackendsCommand {
    Run,
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CaptureDmaBufArgs {
    import_target: CaptureDmaBufImportTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureDmaBufImportTarget {
    Compute,
    Egl,
    EglTexture,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CaptureDmaBufCommand {
    Run(CaptureDmaBufArgs),
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PluginsArgs {
    paths: Vec<PathBuf>,
    json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PluginsCommand {
    Run(PluginsArgs),
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CliError {
    HelpRequested,
    Failure(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum WindowsCommand {
    Run,
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ElementsArgs {
    selector: String,
    limit: usize,
    vision_fallback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ElementsCommand {
    Run(ElementsArgs),
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OcrArgs {
    region: Option<Rect>,
    language: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum OcrCommand {
    Run(OcrArgs),
    Help,
}

#[derive(Debug, Clone, PartialEq)]
struct CompareArgs {
    expected: PathBuf,
    actual: PathBuf,
    region: Option<Rect>,
    per_channel_threshold: u8,
    max_changed_ratio: f32,
}

#[derive(Debug, Clone, PartialEq)]
enum CompareCommand {
    Run(CompareArgs),
    Help,
}

#[derive(Debug, Clone, PartialEq)]
struct UiStateArgs {
    image_paths: Vec<PathBuf>,
    region: Option<Rect>,
    per_channel_threshold: u8,
    stable_max_changed_ratio: f32,
    loading_min_changed_ratio: f32,
    required_stable_transitions: u32,
}

#[derive(Debug, Clone, PartialEq)]
enum UiStateCommand {
    Run(UiStateArgs),
    Help,
}

#[derive(Debug, Clone, PartialEq)]
struct VisionElementsArgs {
    image: PathBuf,
    region: Option<Rect>,
    edge_threshold: u8,
    min_width: u32,
    min_height: u32,
    min_component_pixels: u32,
    max_elements: u32,
    merge_distance: u32,
}

#[derive(Debug, Clone, PartialEq)]
enum VisionElementsCommand {
    Run(VisionElementsArgs),
    Help,
}

fn capture(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let CaptureCommand::Run(args) = parse_capture_args(args)? else {
        print_capture_usage();
        return Err(CliError::HelpRequested);
    };

    if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::Capture {
                output: args.output.display().to_string(),
            },
        )?;
        let ApiResult::Capture(metadata) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected capture response".to_owned(),
            ));
        };
        println!(
            "captured {} bytes to {} via {}",
            metadata.bytes_written, metadata.output_path, metadata.backend_name
        );
        return Ok(());
    }

    let metadata = peekaboox_capture::capture_screen_to_file(&args.output)
        .map_err(|error| CliError::Failure(error.to_string()))?;

    println!(
        "captured {} bytes to {} via {}",
        metadata.bytes_written,
        metadata.output_path.display(),
        metadata.backend_name
    );

    Ok(())
}

fn parse_capture_args(args: Vec<String>) -> Result<CaptureCommand, CliError> {
    let mut output = PathBuf::from("screenshot.png");
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--output" | "-o" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --output".to_owned()));
                };
                output = PathBuf::from(value);
            }
            "--help" | "-h" => return Ok(CaptureCommand::Help),
            unknown => {
                return Err(CliError::Failure(format!(
                    "unknown capture argument: {unknown}"
                )));
            }
        }

        index += 1;
    }

    Ok(CaptureCommand::Run(CaptureArgs { output }))
}

fn capture_delta(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
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
            per_channel_threshold: args.per_channel_threshold,
            low_bandwidth: args.low_bandwidth,
        },
    )?;
    let ApiResult::CaptureDelta(delta) = result else {
        return Err(CliError::Failure(
            "daemon returned unexpected capture delta response".to_owned(),
        ));
    };
    print_capture_delta_dto(&delta);
    Ok(())
}

fn parse_capture_delta_args(args: Vec<String>) -> Result<CaptureDeltaCommand, CliError> {
    let mut stream_id = None;
    let mut reset = false;
    let mut region = None;
    let mut per_channel_threshold = 0_u8;
    let mut low_bandwidth = true;
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
            "--help" | "-h" => return Ok(CaptureDeltaCommand::Help),
            unknown => {
                return Err(CliError::Failure(format!(
                    "unknown capture-delta argument: {unknown}"
                )));
            }
        }

        index += 1;
    }

    Ok(CaptureDeltaCommand::Run(CaptureDeltaArgs {
        stream_id,
        reset,
        region,
        per_channel_threshold,
        low_bandwidth,
    }))
}

fn capture_backends(args: Vec<String>) -> Result<(), CliError> {
    let CaptureBackendsCommand::Run = parse_capture_backends_args(args)? else {
        print_capture_backends_usage();
        return Err(CliError::HelpRequested);
    };

    let environment = peekaboox_capture::CaptureEnvironment::detect();
    println!(
        "session={:?} desktop={} pipewire_session={}",
        environment.session_type,
        environment.current_desktop.as_deref().unwrap_or("-"),
        environment.pipewire_session_available
    );

    let image_backends =
        peekaboox_capture::candidate_backends(&environment, std::path::Path::new("screenshot.png"));
    if image_backends.is_empty() {
        println!("image_backend none");
    } else {
        for backend in image_backends {
            println!(
                "image_backend name={} kind={}",
                backend.name(),
                backend_kind_label(backend.backend_kind())
            );
        }
    }

    for capability in peekaboox_capture::zero_copy_capture_capabilities(&environment) {
        println!(
            "zero_copy_backend name={} kind={} transport={} availability={:?}",
            capability.backend_name,
            backend_kind_label(capability.backend_kind),
            capability.transport.name(),
            capability.availability
        );
    }

    Ok(())
}

fn parse_capture_backends_args(args: Vec<String>) -> Result<CaptureBackendsCommand, CliError> {
    match args.as_slice() {
        [] => Ok(CaptureBackendsCommand::Run),
        [arg] => match arg.as_str() {
            "--help" | "-h" => Ok(CaptureBackendsCommand::Help),
            unknown => Err(CliError::Failure(format!(
                "unknown capture-backends argument: {unknown}"
            ))),
        },
        _ => Err(CliError::Failure(
            "capture-backends does not accept positional arguments".to_owned(),
        )),
    }
}

fn capture_dmabuf(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
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

fn dmabuf_import_target_dto(target: CaptureDmaBufImportTarget) -> DmaBufImportTargetDto {
    match target {
        CaptureDmaBufImportTarget::Compute => DmaBufImportTargetDto::Compute,
        CaptureDmaBufImportTarget::Egl => DmaBufImportTargetDto::Egl,
        CaptureDmaBufImportTarget::EglTexture => DmaBufImportTargetDto::EglTexture,
    }
}

fn print_dmabuf_probe_dto(probe: &DmaBufProbeResultDto) {
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

fn dmabuf_import_target_label(target: DmaBufImportTargetDto) -> &'static str {
    match target {
        DmaBufImportTargetDto::Compute => "compute",
        DmaBufImportTargetDto::Egl => "egl",
        DmaBufImportTargetDto::EglTexture => "egl-texture",
    }
}

fn parse_capture_dmabuf_args(args: Vec<String>) -> Result<CaptureDmaBufCommand, CliError> {
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

fn parse_capture_dmabuf_import_target(value: &str) -> Result<CaptureDmaBufImportTarget, CliError> {
    match value {
        "compute" => Ok(CaptureDmaBufImportTarget::Compute),
        "egl" => Ok(CaptureDmaBufImportTarget::Egl),
        "egl-texture" | "texture" => Ok(CaptureDmaBufImportTarget::EglTexture),
        unknown => Err(CliError::Failure(format!(
            "unsupported capture-dmabuf import target: {unknown}"
        ))),
    }
}

fn print_dmabuf_import(
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
fn print_egl_dmabuf_import(
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
fn print_egl_dmabuf_import(
    _descriptor: &peekaboox_capture::DmaBufFrameDescriptor,
) -> Result<(), CliError> {
    Err(CliError::Failure(
        "capture-dmabuf --import egl requires the `egl-backend` feature".to_owned(),
    ))
}

#[cfg(feature = "egl-backend")]
fn print_egl_texture_dmabuf_import(
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
fn print_egl_texture_dmabuf_import(
    _descriptor: &peekaboox_capture::DmaBufFrameDescriptor,
) -> Result<(), CliError> {
    Err(CliError::Failure(
        "capture-dmabuf --import egl-texture requires the `egl-backend` feature".to_owned(),
    ))
}

fn plugins(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let PluginsCommand::Run(args) = parse_plugins_args(args)? else {
        print_plugins_usage();
        return Err(CliError::HelpRequested);
    };

    let result = if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::ListPlugins {
                paths: args
                    .paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect(),
            },
        )?;
        let ApiResult::Plugins(plugins) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected plugin list response".to_owned(),
            ));
        };
        plugins
    } else {
        plugin_list_dto(peekaboox_plugins::discover_plugins(&args.paths))
    };

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&result)
                .map_err(|error| CliError::Failure(error.to_string()))?
        );
    } else {
        print_plugin_list_dto(&result);
    }
    Ok(())
}

fn parse_plugins_args(args: Vec<String>) -> Result<PluginsCommand, CliError> {
    let mut paths = Vec::new();
    let mut json = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--path" | "-p" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --path".to_owned()));
                };
                paths.push(PathBuf::from(value));
            }
            "--json" => json = true,
            "--help" | "-h" => return Ok(PluginsCommand::Help),
            unknown => {
                return Err(CliError::Failure(format!(
                    "unknown plugins argument: {unknown}"
                )));
            }
        }

        index += 1;
    }

    Ok(PluginsCommand::Run(PluginsArgs { paths, json }))
}

fn plugin_list_dto(result: peekaboox_plugins::PluginDiscoveryResult) -> PluginListResultDto {
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

fn plugin_dto(plugin: &peekaboox_plugins::PluginDescriptor) -> PluginDto {
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

fn print_plugin_list_dto(result: &PluginListResultDto) {
    println!(
        "plugins sdk_version={} count={} errors={}",
        result.sdk_version,
        result.plugins.len(),
        result.errors.len()
    );
    for plugin in &result.plugins {
        let tool_names = plugin
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "plugin id={} name={} version={} capabilities={} tools={} path={}",
            plugin.id,
            plugin.name,
            plugin.version,
            join_or_dash(&plugin.capabilities),
            string_or_dash(&tool_names),
            plugin.manifest_path
        );
    }
    for error in &result.errors {
        println!(
            "plugin_error path={} message={}",
            error.path,
            error.message.replace('\n', " ")
        );
    }
}

fn join_or_dash(values: &[String]) -> String {
    if values.is_empty() {
        "-".to_owned()
    } else {
        values.join(",")
    }
}

fn string_or_dash(value: &str) -> &str {
    if value.is_empty() { "-" } else { value }
}

fn windows(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let WindowsCommand::Run = parse_windows_args(args)? else {
        print_windows_usage();
        return Err(CliError::HelpRequested);
    };

    if context.use_daemon {
        let result = daemon_request(context, ApiRequest::ListWindows)?;
        let ApiResult::ListWindows(metadata) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected windows response".to_owned(),
            ));
        };
        print_window_dto_table(metadata);
        return Ok(());
    }

    let metadata =
        peekaboox_windows::list_windows().map_err(|error| CliError::Failure(error.to_string()))?;

    for warning in metadata.warnings {
        eprintln!("warning: {warning}");
    }

    if metadata.windows.is_empty() {
        println!("no windows found via {}", metadata.backend_name);
        return Ok(());
    }

    println!(
        "{:<14} {:<7} {:<10} {:<11} {:<11} {:<18} TITLE",
        "ID", "FOCUS", "STATE", "POSITION", "SIZE", "APP"
    );

    for window in metadata.windows {
        println!(
            "{:<14} {:<7} {:<10} {:<11} {:<11} {:<18} {}",
            window.id,
            if window.focused { "yes" } else { "no" },
            format!("{:?}", window.state).to_ascii_lowercase(),
            format!("{},{}", window.bounds.x, window.bounds.y),
            format!("{}x{}", window.bounds.width, window.bounds.height),
            window.app_id.unwrap_or_else(|| "-".to_owned()),
            window.title
        );
    }

    Ok(())
}

fn print_window_dto_table(metadata: WindowListResultDto) {
    for warning in metadata.warnings {
        eprintln!("warning: {warning}");
    }

    if metadata.windows.is_empty() {
        println!("no windows found via {}", metadata.backend_name);
        return;
    }

    println!(
        "{:<14} {:<7} {:<10} {:<11} {:<11} {:<18} TITLE",
        "ID", "FOCUS", "STATE", "POSITION", "SIZE", "APP"
    );

    for window in metadata.windows {
        print_window_dto(window);
    }
}

fn print_window_dto(window: WindowDto) {
    println!(
        "{:<14} {:<7} {:<10} {:<11} {:<11} {:<18} {}",
        window.id,
        if window.focused { "yes" } else { "no" },
        window.state,
        format!("{},{}", window.bounds.x, window.bounds.y),
        format!("{}x{}", window.bounds.width, window.bounds.height),
        window.app_id.unwrap_or_else(|| "-".to_owned()),
        window.title
    );
}

fn parse_windows_args(args: Vec<String>) -> Result<WindowsCommand, CliError> {
    match args.as_slice() {
        [] => Ok(WindowsCommand::Run),
        [arg] => match arg.as_str() {
            "--help" | "-h" => Ok(WindowsCommand::Help),
            unknown => Err(CliError::Failure(format!(
                "unknown windows argument: {unknown}"
            ))),
        },
        _ => Err(CliError::Failure(
            "windows does not accept positional arguments".to_owned(),
        )),
    }
}

fn elements(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let ElementsCommand::Run(args) = parse_elements_args(args)? else {
        print_elements_usage();
        return Err(CliError::HelpRequested);
    };

    if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::FindElements {
                selector: args.selector.clone(),
                vision_fallback: args.vision_fallback,
            },
        )?;
        let ApiResult::FindElements(metadata) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected elements response".to_owned(),
            ));
        };
        print_element_dto_table(metadata, args.limit);
        return Ok(());
    }

    let metadata = find_elements_metadata(&args.selector, args.vision_fallback)?;
    print_element_table(metadata, args.limit);

    Ok(())
}

fn find_elements_metadata(
    selector: &str,
    vision_fallback: bool,
) -> Result<AccessibilityTreeMetadata, CliError> {
    let query = ElementQuery::from_selector(selector);
    match peekaboox_accessibility::semantic_tree() {
        Ok(mut metadata) => {
            metadata.elements.retain(|element| query.matches(element));
            if !metadata.elements.is_empty() || !vision_fallback {
                return Ok(metadata);
            }

            let mut fallback = vision_fallback_metadata(&query)?;
            fallback
                .warnings
                .push("no accessibility elements matched; used vision fallback".to_owned());
            Ok(fallback)
        }
        Err(error) if vision_fallback => {
            let mut fallback = vision_fallback_metadata(&query)?;
            fallback.warnings.push(format!(
                "accessibility lookup failed: {error}; used vision fallback"
            ));
            Ok(fallback)
        }
        Err(error) => Err(CliError::Failure(error.to_string())),
    }
}

fn vision_fallback_metadata(query: &ElementQuery) -> Result<AccessibilityTreeMetadata, CliError> {
    let screenshot = vision_fallback_temp_path();
    peekaboox_capture::capture_screen_to_file(&screenshot)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    let result = peekaboox_vision::detect_ui_elements_from_image_file(
        &screenshot,
        &UiElementDetectionOptions::default(),
    )
    .map_err(|error| CliError::Failure(error.to_string()));
    remove_temp_file(&screenshot, "vision fallback screenshot");

    let mut elements = result?;
    elements.retain(|element| query.matches(element));
    Ok(AccessibilityTreeMetadata {
        backend_name: "heuristic_vision".to_owned(),
        backend_kind: BackendKind::Vision,
        warnings: Vec::new(),
        elements,
    })
}

fn vision_fallback_temp_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "peekaboox-vision-fallback-{}-{}.png",
        std::process::id(),
        monotonic_ms()
    ))
}

fn remove_temp_file(path: &PathBuf, description: &str) {
    if let Err(error) = std::fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        eprintln!("failed to remove {description} {}: {error}", path.display());
    }
}

fn monotonic_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

fn print_element_table(mut metadata: AccessibilityTreeMetadata, limit: usize) {
    for warning in metadata.warnings {
        eprintln!("warning: {warning}");
    }

    if metadata.elements.is_empty() {
        println!("no elements found via {}", metadata.backend_name);
        return;
    }

    let total = metadata.elements.len();
    if limit > 0 && metadata.elements.len() > limit {
        metadata.elements.truncate(limit);
    }

    println!(
        "{:<20} {:<5} {:<11} {:<11} {:<24} LABEL",
        "ROLE", "CONF", "POSITION", "SIZE", "STATES"
    );

    for element in metadata.elements {
        print_element(element);
    }

    if limit > 0 && total > limit {
        eprintln!("showing {limit} of {total} elements");
    }
}

fn print_element(element: UiElement) {
    println!(
        "{:<20} {:<5} {:<11} {:<11} {:<24} {}",
        element.role,
        format!("{:.2}", element.confidence),
        format!("{},{}", element.bounds.x, element.bounds.y),
        format!("{}x{}", element.bounds.width, element.bounds.height),
        format_states(&element.states),
        element.label.unwrap_or_else(|| "-".to_owned())
    );
}

fn print_element_dto_table(mut metadata: ElementListResultDto, limit: usize) {
    for warning in metadata.warnings {
        eprintln!("warning: {warning}");
    }

    if metadata.elements.is_empty() {
        println!("no elements found via {}", metadata.backend_name);
        return;
    }

    let total = metadata.elements.len();
    if limit > 0 && metadata.elements.len() > limit {
        metadata.elements.truncate(limit);
    }

    println!(
        "{:<20} {:<5} {:<11} {:<11} {:<24} LABEL",
        "ROLE", "CONF", "POSITION", "SIZE", "STATES"
    );

    for element in metadata.elements {
        print_element_dto(element);
    }

    if limit > 0 && total > limit {
        eprintln!("showing {limit} of {total} elements");
    }
}

fn print_element_dto(element: ElementDto) {
    println!(
        "{:<20} {:<5} {:<11} {:<11} {:<24} {}",
        element.role,
        format!("{:.2}", element.confidence),
        format!("{},{}", element.bounds.x, element.bounds.y),
        format!("{}x{}", element.bounds.width, element.bounds.height),
        format_states(&element.states),
        element.label.unwrap_or_else(|| "-".to_owned())
    );
}

fn format_states(states: &[String]) -> String {
    if states.is_empty() {
        "-".to_owned()
    } else {
        states.join("|")
    }
}

fn parse_elements_args(args: Vec<String>) -> Result<ElementsCommand, CliError> {
    let mut selector = None;
    let mut selector_parts = Vec::new();
    let mut limit = 50;
    let mut vision_fallback = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--selector" | "-s" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --selector".to_owned()));
                };
                selector = Some(value.to_owned());
            }
            "--role" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --role".to_owned()));
                };
                selector_parts.push(format!("role={value}"));
            }
            "--label" | "--text" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(format!(
                        "missing value for {}",
                        args[index - 1]
                    )));
                };
                selector_parts.push(format!("label={value}"));
            }
            "--state" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --state".to_owned()));
                };
                selector_parts.push(format!("state={value}"));
            }
            "--bounds" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --bounds".to_owned()));
                };
                selector_parts.push(format!("bounds={value}"));
            }
            "--contains" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --contains".to_owned()));
                };
                selector_parts.push(format!("contains={value}"));
            }
            "--min-confidence" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --min-confidence".to_owned(),
                    ));
                };
                value.parse::<f32>().map_err(|_| {
                    CliError::Failure(format!("--min-confidence must be a float, got {value:?}"))
                })?;
                selector_parts.push(format!("confidence>={value}"));
            }
            "--limit" | "-n" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --limit".to_owned()));
                };
                limit = parse_usize("--limit", value)?;
            }
            "--vision-fallback" => vision_fallback = true,
            "--help" | "-h" => return Ok(ElementsCommand::Help),
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown elements argument: {value}"
                )));
            }
            value => {
                if selector.is_some() {
                    return Err(CliError::Failure(
                        "provide only one positional selector".to_owned(),
                    ));
                }
                selector = Some(value.to_owned());
            }
        }

        index += 1;
    }

    let selector = match selector {
        Some(selector) if selector_parts.is_empty() => selector,
        Some(selector) => {
            selector_parts.insert(0, selector);
            selector_parts.join(",")
        }
        None => selector_parts.join(","),
    };

    Ok(ElementsCommand::Run(ElementsArgs {
        selector,
        limit,
        vision_fallback,
    }))
}

fn ocr(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let OcrCommand::Run(args) = parse_ocr_args(args)? else {
        print_ocr_usage();
        return Err(CliError::HelpRequested);
    };

    if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::Ocr {
                region: args.region.map(RectDto::from),
                language: args.language.clone(),
            },
        )?;
        let ApiResult::Ocr(result) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected OCR response".to_owned(),
            ));
        };
        print_ocr_dto_result(result);
        return Ok(());
    }

    let backend = TesseractOcrBackend::new("tesseract", ocr_options(args.language.clone()));
    if !backend.is_available() {
        return Err(CliError::Failure(
            "OCR backend tesseract is not available; install tesseract-ocr".to_owned(),
        ));
    }

    let result = match args.region {
        Some(region) => peekaboox_vision::ocr_region_with_backend(&backend, region),
        None => peekaboox_vision::ocr_screen_with_backend(&backend),
    }
    .map_err(|error| CliError::Failure(error.to_string()))?;
    print_ocr_result(result);

    Ok(())
}

fn parse_ocr_args(args: Vec<String>) -> Result<OcrCommand, CliError> {
    let mut region = None;
    let mut language = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--region" | "-r" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --region".to_owned()));
                };
                region = Some(parse_rect("--region", value)?);
            }
            "--language" | "-l" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --language".to_owned()));
                };
                language = non_empty_cli_string(value);
            }
            "--help" | "-h" => return Ok(OcrCommand::Help),
            unknown => {
                return Err(CliError::Failure(format!(
                    "unknown ocr argument: {unknown}"
                )));
            }
        }

        index += 1;
    }

    Ok(OcrCommand::Run(OcrArgs { region, language }))
}

fn print_ocr_result(result: OcrResult) {
    for warning in result.warnings {
        eprintln!("warning: {warning}");
    }

    if result.text.trim().is_empty() {
        println!("no OCR text found via {}", result.backend_name);
    } else {
        println!("{}", result.text);
    }
}

fn print_ocr_dto_result(result: OcrResultDto) {
    for warning in result.warnings {
        eprintln!("warning: {warning}");
    }

    if result.text.trim().is_empty() {
        println!("no OCR text found via {}", result.backend_name);
    } else {
        println!("{}", result.text);
    }
}

fn ocr_options(language: Option<String>) -> OcrOptions {
    let mut options = OcrOptions::default();
    if let Some(language) = language {
        options.language = Some(language);
    }
    options
}

fn compare(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let CompareCommand::Run(args) = parse_compare_args(args)? else {
        print_compare_usage();
        return Err(CliError::HelpRequested);
    };

    if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::CompareImages {
                expected_path: args.expected.display().to_string(),
                actual_path: args.actual.display().to_string(),
                region: args.region.map(RectDto::from),
                per_channel_threshold: args.per_channel_threshold,
                max_changed_ratio: args.max_changed_ratio,
            },
        )?;
        let ApiResult::VisualDiff(result) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected visual diff response".to_owned(),
            ));
        };
        print_visual_diff_dto(&result);
        return visual_diff_exit_status(result.matches);
    }

    let options = visual_compare_options(&args);
    let result = peekaboox_vision::compare_image_files(&args.expected, &args.actual, &options)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    print_visual_diff(&result);
    visual_diff_exit_status(result.matches)
}

fn parse_compare_args(args: Vec<String>) -> Result<CompareCommand, CliError> {
    let mut expected = None;
    let mut actual = None;
    let mut region = None;
    let mut per_channel_threshold = 0_u8;
    let mut max_changed_ratio = 0.0_f32;
    let mut positional = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--expected" | "-e" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --expected".to_owned()));
                };
                expected = Some(PathBuf::from(value));
            }
            "--actual" | "-a" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --actual".to_owned()));
                };
                actual = Some(PathBuf::from(value));
            }
            "--region" | "-r" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --region".to_owned()));
                };
                region = Some(parse_rect("--region", value)?);
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
            "--max-changed-ratio" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --max-changed-ratio".to_owned(),
                    ));
                };
                max_changed_ratio = parse_unit_f32("--max-changed-ratio", value)?;
            }
            "--help" | "-h" => return Ok(CompareCommand::Help),
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown compare argument: {value}"
                )));
            }
            value => positional.push(PathBuf::from(value)),
        }

        index += 1;
    }

    match positional.as_slice() {
        [] => {}
        [expected_path] if expected.is_none() => expected = Some(expected_path.clone()),
        [actual_path] if actual.is_none() => actual = Some(actual_path.clone()),
        [expected_path, actual_path] => {
            if expected.is_none() {
                expected = Some(expected_path.clone());
            }
            if actual.is_none() {
                actual = Some(actual_path.clone());
            }
        }
        _ => {
            return Err(CliError::Failure(
                "compare accepts at most two positional paths".to_owned(),
            ));
        }
    }

    let expected =
        expected.ok_or_else(|| CliError::Failure("missing --expected image path".to_owned()))?;
    let actual =
        actual.ok_or_else(|| CliError::Failure("missing --actual image path".to_owned()))?;

    Ok(CompareCommand::Run(CompareArgs {
        expected,
        actual,
        region,
        per_channel_threshold,
        max_changed_ratio,
    }))
}

fn visual_compare_options(args: &CompareArgs) -> VisualCompareOptions {
    VisualCompareOptions {
        region: args.region,
        per_channel_threshold: args.per_channel_threshold,
        max_changed_ratio: args.max_changed_ratio,
    }
}

fn print_visual_diff(result: &VisualDiffResult) {
    println!(
        "matches={} changed={}/{} ratio={:.6} mae={:.3} max_delta={} region={} changed_bounds={}",
        result.matches,
        result.changed_pixels,
        result.compared_pixels,
        result.changed_ratio,
        result.mean_absolute_error,
        result.max_channel_delta,
        format_rect(result.compared_region),
        result
            .changed_bounds
            .map(format_rect)
            .unwrap_or_else(|| "-".to_owned())
    );
}

fn print_visual_diff_dto(result: &VisualDiffDto) {
    println!(
        "matches={} changed={}/{} ratio={:.6} mae={:.3} max_delta={} region={} changed_bounds={}",
        result.matches,
        result.changed_pixels,
        result.compared_pixels,
        result.changed_ratio,
        result.mean_absolute_error,
        result.max_channel_delta,
        format_rect(Rect::from(result.compared_region)),
        result
            .changed_bounds
            .map(Rect::from)
            .map(format_rect)
            .unwrap_or_else(|| "-".to_owned())
    );
}

fn print_capture_delta_dto(result: &CaptureDeltaResultDto) {
    println!(
        "stream={} sequence={} low_bandwidth={} full_frame={} frame={}x{} format={} capture_region={} changed_pixels={} ratio={:.6} changed_bounds={} patch_stride={} patch_base64_bytes={} backend={}",
        result.stream_id,
        result.sequence,
        result.low_bandwidth,
        result.full_frame,
        result.frame_width,
        result.frame_height,
        result.pixel_format,
        result
            .capture_region
            .map(Rect::from)
            .map(format_rect)
            .unwrap_or_else(|| "-".to_owned()),
        result.changed_pixels,
        result.changed_ratio,
        result
            .changed_bounds
            .map(Rect::from)
            .map(format_rect)
            .unwrap_or_else(|| "-".to_owned()),
        result.patch_stride,
        result.patch_base64.len(),
        result.backend_name
    );
}

fn visual_diff_exit_status(matches: bool) -> Result<(), CliError> {
    if matches {
        Ok(())
    } else {
        Err(CliError::Failure(
            "visual comparison did not match tolerance".to_owned(),
        ))
    }
}

fn ui_state(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let UiStateCommand::Run(args) = parse_ui_state_args(args)? else {
        print_ui_state_usage();
        return Err(CliError::HelpRequested);
    };

    if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::DetectUiState {
                image_paths: args
                    .image_paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect(),
                region: args.region.map(RectDto::from),
                per_channel_threshold: args.per_channel_threshold,
                stable_max_changed_ratio: args.stable_max_changed_ratio,
                loading_min_changed_ratio: args.loading_min_changed_ratio,
                required_stable_transitions: args.required_stable_transitions,
            },
        )?;
        let ApiResult::UiState(result) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected UI state response".to_owned(),
            ));
        };
        print_ui_state_dto(&result);
        return Ok(());
    }

    let options = ui_state_options(&args);
    let result = peekaboox_vision::detect_ui_state_from_image_files(&args.image_paths, &options)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    print_ui_state(&result);
    Ok(())
}

fn parse_ui_state_args(args: Vec<String>) -> Result<UiStateCommand, CliError> {
    let mut image_paths = Vec::new();
    let mut region = None;
    let mut per_channel_threshold = UiStateOptions::default().per_channel_threshold;
    let mut stable_max_changed_ratio = UiStateOptions::default().stable_max_changed_ratio;
    let mut loading_min_changed_ratio = UiStateOptions::default().loading_min_changed_ratio;
    let mut required_stable_transitions =
        u32::try_from(UiStateOptions::default().required_stable_transitions).unwrap_or(1);
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--image" | "-i" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --image".to_owned()));
                };
                image_paths.push(PathBuf::from(value));
            }
            "--region" | "-r" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --region".to_owned()));
                };
                region = Some(parse_rect("--region", value)?);
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
            "--stable-max-changed-ratio" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --stable-max-changed-ratio".to_owned(),
                    ));
                };
                stable_max_changed_ratio = parse_unit_f32("--stable-max-changed-ratio", value)?;
            }
            "--loading-min-changed-ratio" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --loading-min-changed-ratio".to_owned(),
                    ));
                };
                loading_min_changed_ratio = parse_unit_f32("--loading-min-changed-ratio", value)?;
            }
            "--required-stable-transitions" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --required-stable-transitions".to_owned(),
                    ));
                };
                required_stable_transitions =
                    parse_positive_u32("--required-stable-transitions", value)?;
            }
            "--help" | "-h" => return Ok(UiStateCommand::Help),
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown state argument: {value}"
                )));
            }
            value => image_paths.push(PathBuf::from(value)),
        }

        index += 1;
    }

    if image_paths.len() < 2 {
        return Err(CliError::Failure(
            "state requires at least two image paths".to_owned(),
        ));
    }
    if stable_max_changed_ratio > loading_min_changed_ratio {
        return Err(CliError::Failure(
            "--stable-max-changed-ratio must be less than or equal to --loading-min-changed-ratio"
                .to_owned(),
        ));
    }

    Ok(UiStateCommand::Run(UiStateArgs {
        image_paths,
        region,
        per_channel_threshold,
        stable_max_changed_ratio,
        loading_min_changed_ratio,
        required_stable_transitions,
    }))
}

fn ui_state_options(args: &UiStateArgs) -> UiStateOptions {
    UiStateOptions {
        region: args.region,
        per_channel_threshold: args.per_channel_threshold,
        stable_max_changed_ratio: args.stable_max_changed_ratio,
        loading_min_changed_ratio: args.loading_min_changed_ratio,
        required_stable_transitions: usize::try_from(args.required_stable_transitions).unwrap_or(1),
    }
}

fn print_ui_state(result: &UiStateResult) {
    println!(
        "state={} transitions={} stable={} loading={} trailing_stable={} latest_ratio={:.6} max_ratio={:.6} mean_ratio={:.6} changed_bounds={}",
        format!("{:?}", result.state).to_ascii_lowercase(),
        result.compared_transitions,
        result.stable_transitions,
        result.loading_transitions,
        result.trailing_stable_transitions,
        result.latest_diff.changed_ratio,
        result.max_changed_ratio,
        result.mean_changed_ratio,
        result
            .changed_bounds
            .map(format_rect)
            .unwrap_or_else(|| "-".to_owned())
    );
}

fn print_ui_state_dto(result: &UiStateDto) {
    println!(
        "state={} transitions={} stable={} loading={} trailing_stable={} latest_ratio={:.6} max_ratio={:.6} mean_ratio={:.6} changed_bounds={}",
        result.state,
        result.compared_transitions,
        result.stable_transitions,
        result.loading_transitions,
        result.trailing_stable_transitions,
        result.latest_diff.changed_ratio,
        result.max_changed_ratio,
        result.mean_changed_ratio,
        result
            .changed_bounds
            .map(Rect::from)
            .map(format_rect)
            .unwrap_or_else(|| "-".to_owned())
    );
}

fn vision_elements(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let VisionElementsCommand::Run(args) = parse_vision_elements_args(args)? else {
        print_vision_elements_usage();
        return Err(CliError::HelpRequested);
    };

    if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::DetectUiElements {
                image_path: args.image.display().to_string(),
                region: args.region.map(RectDto::from),
                edge_threshold: args.edge_threshold,
                min_width: args.min_width,
                min_height: args.min_height,
                min_component_pixels: args.min_component_pixels,
                max_elements: args.max_elements,
                merge_distance: args.merge_distance,
            },
        )?;
        let ApiResult::DetectUiElements(metadata) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected vision elements response".to_owned(),
            ));
        };
        print_element_dto_table(metadata, 0);
        return Ok(());
    }

    let options = vision_element_options(&args)?;
    let elements = peekaboox_vision::detect_ui_elements_from_image_file(&args.image, &options)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    print_element_table(
        AccessibilityTreeMetadata {
            backend_name: "heuristic_vision".to_owned(),
            backend_kind: BackendKind::Vision,
            warnings: Vec::new(),
            elements,
        },
        0,
    );
    Ok(())
}

fn parse_vision_elements_args(args: Vec<String>) -> Result<VisionElementsCommand, CliError> {
    let defaults = UiElementDetectionOptions::default();
    let mut image = None;
    let mut region = None;
    let mut edge_threshold = defaults.edge_threshold;
    let mut min_width = defaults.min_width;
    let mut min_height = defaults.min_height;
    let mut min_component_pixels = defaults.min_component_pixels;
    let mut max_elements = u32::try_from(defaults.max_elements).unwrap_or(100);
    let mut merge_distance = defaults.merge_distance;
    let mut positional = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--image" | "-i" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --image".to_owned()));
                };
                image = Some(PathBuf::from(value));
            }
            "--region" | "-r" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --region".to_owned()));
                };
                region = Some(parse_rect("--region", value)?);
            }
            "--threshold" | "--edge-threshold" | "-t" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --threshold".to_owned(),
                    ));
                };
                edge_threshold = parse_u8("--threshold", value)?;
                if edge_threshold == 0 {
                    return Err(CliError::Failure(
                        "--threshold must be greater than zero".to_owned(),
                    ));
                }
            }
            "--min-width" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --min-width".to_owned(),
                    ));
                };
                min_width = parse_positive_u32("--min-width", value)?;
            }
            "--min-height" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --min-height".to_owned(),
                    ));
                };
                min_height = parse_positive_u32("--min-height", value)?;
            }
            "--min-component-pixels" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --min-component-pixels".to_owned(),
                    ));
                };
                min_component_pixels = parse_positive_u32("--min-component-pixels", value)?;
            }
            "--max-elements" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --max-elements".to_owned(),
                    ));
                };
                max_elements = parse_positive_u32("--max-elements", value)?;
            }
            "--merge-distance" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --merge-distance".to_owned(),
                    ));
                };
                merge_distance = value.parse::<u32>().map_err(|_| {
                    CliError::Failure(format!(
                        "--merge-distance must be an integer, got {value:?}"
                    ))
                })?;
            }
            "--help" | "-h" => return Ok(VisionElementsCommand::Help),
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown vision-elements argument: {value}"
                )));
            }
            value => positional.push(PathBuf::from(value)),
        }

        index += 1;
    }

    match positional.as_slice() {
        [] => {}
        [image_path] if image.is_none() => image = Some(image_path.clone()),
        _ => {
            return Err(CliError::Failure(
                "vision-elements accepts exactly one image path".to_owned(),
            ));
        }
    }
    let image = image.ok_or_else(|| CliError::Failure("missing --image path".to_owned()))?;

    Ok(VisionElementsCommand::Run(VisionElementsArgs {
        image,
        region,
        edge_threshold,
        min_width,
        min_height,
        min_component_pixels,
        max_elements,
        merge_distance,
    }))
}

fn vision_element_options(
    args: &VisionElementsArgs,
) -> Result<UiElementDetectionOptions, CliError> {
    Ok(UiElementDetectionOptions {
        region: args.region,
        edge_threshold: args.edge_threshold,
        min_width: args.min_width,
        min_height: args.min_height,
        min_component_pixels: args.min_component_pixels,
        max_elements: usize::try_from(args.max_elements)
            .map_err(|_| CliError::Failure("--max-elements is too large".to_owned()))?,
        merge_distance: args.merge_distance,
    })
}

fn format_rect(rect: Rect) -> String {
    format!("{},{},{}x{}", rect.x, rect.y, rect.width, rect.height)
}

fn backend_kind_label(kind: BackendKind) -> String {
    format!("{kind:?}").to_ascii_lowercase()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClickArgs {
    target: ClickTarget,
    button: MouseButton,
    dry_run: bool,
    vision_fallback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ClickTarget {
    Coordinates(Point),
    SemanticSelector(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedClickTarget {
    position: Point,
    description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ClickCommand {
    Run(ClickArgs),
    Help,
}

fn click(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let ClickCommand::Run(args) = parse_click_args(args)? else {
        print_click_usage();
        return Err(CliError::HelpRequested);
    };

    let target = resolve_click_target(&args)?;
    let action = peekaboox_input::InputAction::Click {
        position: target.position,
        button: args.button,
    };

    if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::Click {
                x: target.position.x,
                y: target.position.y,
                button: mouse_button_dto(args.button),
                dry_run: args.dry_run,
            },
        )?;
        let ApiResult::Click(metadata) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected click response".to_owned(),
            ));
        };
        print_click_result(&args, &target, metadata);
        return Ok(());
    }

    if args.dry_run {
        let backend = peekaboox_input::CommandInputBackend
            .detect_backend_for(&action)
            .map_err(|error| CliError::Failure(error.to_string()))?;
        println!("would click {} via {}", target.description, backend.name());
        return Ok(());
    }

    let metadata = peekaboox_input::click(target.position, args.button)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    print_click_result(
        &args,
        &target,
        ActionResultDto {
            backend_name: metadata.backend_name,
            backend_kind: format!("{:?}", metadata.backend_kind).to_ascii_lowercase(),
        },
    );

    Ok(())
}

fn resolve_click_target(args: &ClickArgs) -> Result<ResolvedClickTarget, CliError> {
    match &args.target {
        ClickTarget::Coordinates(position) => Ok(ResolvedClickTarget {
            position: *position,
            description: format!("{},{}", position.x, position.y),
        }),
        ClickTarget::SemanticSelector(selector) => {
            let target = resolve_semantic_click_target(selector, args.vision_fallback)?;
            let label = target
                .element
                .label
                .as_deref()
                .unwrap_or(target.element.role.as_str());
            Ok(ResolvedClickTarget {
                position: target.position,
                description: format!(
                    "selector {selector:?} at {},{} ({label})",
                    target.position.x, target.position.y
                ),
            })
        }
    }
}

fn resolve_semantic_click_target(
    selector: &str,
    vision_fallback: bool,
) -> Result<peekaboox_accessibility::ResolvedClickTarget, CliError> {
    match peekaboox_accessibility::resolve_click_target(selector) {
        Ok(target) => Ok(target),
        Err(error) if vision_fallback => {
            let query = ElementQuery::from_selector(selector);
            let elements = vision_fallback_metadata(&query)?.elements;
            peekaboox_accessibility::resolve_click_target_from_tree(selector, &elements).map_err(
                |fallback_error| {
                    CliError::Failure(format!(
                        "{}; vision fallback also failed: {fallback_error}",
                        error
                    ))
                },
            )
        }
        Err(error) => Err(CliError::Failure(error.to_string())),
    }
}

fn print_click_result(args: &ClickArgs, target: &ResolvedClickTarget, metadata: ActionResultDto) {
    if args.dry_run {
        println!(
            "would click {} via {}",
            target.description, metadata.backend_name
        );
    } else {
        println!(
            "clicked {} with {:?} via {}",
            target.description, args.button, metadata.backend_name
        );
    }
}

fn parse_click_args(args: Vec<String>) -> Result<ClickCommand, CliError> {
    let mut x = None;
    let mut y = None;
    let mut selector = None;
    let mut button = MouseButton::Left;
    let mut dry_run = false;
    let mut vision_fallback = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--x" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --x".to_owned()));
                };
                x = Some(parse_i32("--x", value)?);
            }
            "--y" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --y".to_owned()));
                };
                y = Some(parse_i32("--y", value)?);
            }
            "--selector" | "--text" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(format!(
                        "missing value for {}",
                        args[index - 1]
                    )));
                };
                selector = Some(value.to_owned());
            }
            "--button" | "-b" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --button".to_owned()));
                };
                button = parse_mouse_button(value)?;
            }
            "--dry-run" => dry_run = true,
            "--vision-fallback" => vision_fallback = true,
            "--help" | "-h" => return Ok(ClickCommand::Help),
            unknown => {
                return Err(CliError::Failure(format!(
                    "unknown click argument: {unknown}"
                )));
            }
        }

        index += 1;
    }

    let target = match (x, y, selector) {
        (Some(x), Some(y), None) => ClickTarget::Coordinates(Point::new(x, y)),
        (None, None, Some(selector)) => ClickTarget::SemanticSelector(selector),
        (Some(_), Some(_), Some(_)) => {
            return Err(CliError::Failure(
                "provide either coordinates or --selector/--text, not both".to_owned(),
            ));
        }
        (Some(_), None, None) => {
            return Err(CliError::Failure("missing required --y".to_owned()));
        }
        (None, Some(_), None) => {
            return Err(CliError::Failure("missing required --x".to_owned()));
        }
        (Some(_), None, Some(_)) | (None, Some(_), Some(_)) => {
            return Err(CliError::Failure(
                "provide either both --x/--y or --selector/--text".to_owned(),
            ));
        }
        (None, None, None) => {
            return Err(CliError::Failure(
                "missing click target; provide --x/--y or --selector/--text".to_owned(),
            ));
        }
    };

    Ok(ClickCommand::Run(ClickArgs {
        target,
        button,
        dry_run,
        vision_fallback,
    }))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MoveArgs {
    position: Point,
    dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MoveCommand {
    Run(MoveArgs),
    Help,
}

fn move_mouse(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let MoveCommand::Run(args) = parse_move_args(args)? else {
        print_move_usage();
        return Err(CliError::HelpRequested);
    };

    let action = peekaboox_input::InputAction::MoveMouse(args.position);

    if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::MoveMouse {
                x: args.position.x,
                y: args.position.y,
                dry_run: args.dry_run,
            },
        )?;
        let ApiResult::MoveMouse(metadata) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected move response".to_owned(),
            ));
        };
        print_move_result(&args, metadata);
        return Ok(());
    }

    if args.dry_run {
        let backend = peekaboox_input::CommandInputBackend
            .detect_backend_for(&action)
            .map_err(|error| CliError::Failure(error.to_string()))?;
        println!(
            "would move mouse to {},{} via {}",
            args.position.x,
            args.position.y,
            backend.name()
        );
        return Ok(());
    }

    let metadata = peekaboox_input::move_mouse(args.position)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    print_move_result(&args, input_metadata_dto(metadata));

    Ok(())
}

fn print_move_result(args: &MoveArgs, metadata: ActionResultDto) {
    if args.dry_run {
        println!(
            "would move mouse to {},{} via {}",
            args.position.x, args.position.y, metadata.backend_name
        );
    } else {
        println!(
            "moved mouse to {},{} via {}",
            args.position.x, args.position.y, metadata.backend_name
        );
    }
}

fn parse_move_args(args: Vec<String>) -> Result<MoveCommand, CliError> {
    let mut x = None;
    let mut y = None;
    let mut dry_run = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--x" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --x".to_owned()));
                };
                x = Some(parse_i32("--x", value)?);
            }
            "--y" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --y".to_owned()));
                };
                y = Some(parse_i32("--y", value)?);
            }
            "--dry-run" => dry_run = true,
            "--help" | "-h" => return Ok(MoveCommand::Help),
            unknown => {
                return Err(CliError::Failure(format!(
                    "unknown move argument: {unknown}"
                )));
            }
        }

        index += 1;
    }

    let position = match (x, y) {
        (Some(x), Some(y)) => Point::new(x, y),
        (Some(_), None) => return Err(CliError::Failure("missing required --y".to_owned())),
        (None, Some(_)) => return Err(CliError::Failure("missing required --x".to_owned())),
        (None, None) => {
            return Err(CliError::Failure(
                "missing move target; provide --x and --y".to_owned(),
            ));
        }
    };

    Ok(MoveCommand::Run(MoveArgs { position, dry_run }))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DragArgs {
    from: Point,
    to: Point,
    button: MouseButton,
    duration_ms: u32,
    dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DragCommand {
    Run(DragArgs),
    Help,
}

fn drag(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let DragCommand::Run(args) = parse_drag_args(args)? else {
        print_drag_usage();
        return Err(CliError::HelpRequested);
    };

    let action = peekaboox_input::InputAction::Drag {
        from: args.from,
        to: args.to,
        button: args.button,
        duration_ms: u64::from(args.duration_ms),
    };

    if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::Drag {
                from_x: args.from.x,
                from_y: args.from.y,
                to_x: args.to.x,
                to_y: args.to.y,
                button: mouse_button_dto(args.button),
                duration_ms: args.duration_ms,
                dry_run: args.dry_run,
            },
        )?;
        let ApiResult::Drag(metadata) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected drag response".to_owned(),
            ));
        };
        print_drag_result(&args, metadata);
        return Ok(());
    }

    if args.dry_run {
        let backend = peekaboox_input::CommandInputBackend
            .detect_backend_for(&action)
            .map_err(|error| CliError::Failure(error.to_string()))?;
        println!(
            "would drag from {},{} to {},{} via {}",
            args.from.x,
            args.from.y,
            args.to.x,
            args.to.y,
            backend.name()
        );
        return Ok(());
    }

    let metadata =
        peekaboox_input::drag(args.from, args.to, args.button, u64::from(args.duration_ms))
            .map_err(|error| CliError::Failure(error.to_string()))?;
    print_drag_result(&args, input_metadata_dto(metadata));

    Ok(())
}

fn print_drag_result(args: &DragArgs, metadata: ActionResultDto) {
    if args.dry_run {
        println!(
            "would drag from {},{} to {},{} via {}",
            args.from.x, args.from.y, args.to.x, args.to.y, metadata.backend_name
        );
    } else {
        println!(
            "dragged from {},{} to {},{} with {:?} via {}",
            args.from.x, args.from.y, args.to.x, args.to.y, args.button, metadata.backend_name
        );
    }
}

fn parse_drag_args(args: Vec<String>) -> Result<DragCommand, CliError> {
    let mut from = None;
    let mut to = None;
    let mut from_x = None;
    let mut from_y = None;
    let mut to_x = None;
    let mut to_y = None;
    let mut button = MouseButton::Left;
    let mut duration_ms = 250_u32;
    let mut dry_run = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--from" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --from".to_owned()));
                };
                from = Some(parse_point("--from", value)?);
            }
            "--to" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --to".to_owned()));
                };
                to = Some(parse_point("--to", value)?);
            }
            "--from-x" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --from-x".to_owned()));
                };
                from_x = Some(parse_i32("--from-x", value)?);
            }
            "--from-y" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --from-y".to_owned()));
                };
                from_y = Some(parse_i32("--from-y", value)?);
            }
            "--to-x" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --to-x".to_owned()));
                };
                to_x = Some(parse_i32("--to-x", value)?);
            }
            "--to-y" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --to-y".to_owned()));
                };
                to_y = Some(parse_i32("--to-y", value)?);
            }
            "--button" | "-b" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --button".to_owned()));
                };
                button = parse_mouse_button(value)?;
            }
            "--duration-ms" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --duration-ms".to_owned(),
                    ));
                };
                duration_ms = parse_u32("--duration-ms", value)?;
            }
            "--dry-run" => dry_run = true,
            "--help" | "-h" => return Ok(DragCommand::Help),
            unknown => {
                return Err(CliError::Failure(format!(
                    "unknown drag argument: {unknown}"
                )));
            }
        }

        index += 1;
    }

    let from = merge_drag_point("--from", from, from_x, from_y)?;
    let to = merge_drag_point("--to", to, to_x, to_y)?;

    Ok(DragCommand::Run(DragArgs {
        from,
        to,
        button,
        duration_ms,
        dry_run,
    }))
}

fn merge_drag_point(
    name: &str,
    point: Option<Point>,
    x: Option<i32>,
    y: Option<i32>,
) -> Result<Point, CliError> {
    match (point, x, y) {
        (Some(point), None, None) => Ok(point),
        (None, Some(x), Some(y)) => Ok(Point::new(x, y)),
        (Some(_), Some(_), _) | (Some(_), _, Some(_)) => Err(CliError::Failure(format!(
            "provide either {name} or {name}-x/{name}-y, not both"
        ))),
        (None, None, None) => Err(CliError::Failure(format!("missing required {name}"))),
        (None, Some(_), None) => Err(CliError::Failure(format!("missing required {name}-y"))),
        (None, None, Some(_)) => Err(CliError::Failure(format!("missing required {name}-x"))),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TypeArgs {
    text: String,
    dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TypeCommand {
    Run(TypeArgs),
    Help,
}

fn type_text(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let TypeCommand::Run(args) = parse_type_args(args)? else {
        print_type_usage();
        return Err(CliError::HelpRequested);
    };

    let action = peekaboox_input::InputAction::TypeText(args.text.clone());

    if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::TypeText {
                text: args.text.clone(),
                dry_run: args.dry_run,
            },
        )?;
        let ApiResult::TypeText(metadata) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected type response".to_owned(),
            ));
        };
        print_type_result(&args, metadata);
        return Ok(());
    }

    if args.dry_run {
        let backend = peekaboox_input::CommandInputBackend
            .detect_backend_for(&action)
            .map_err(|error| CliError::Failure(error.to_string()))?;
        println!("would type via {}", backend.name());
        return Ok(());
    }

    let metadata = peekaboox_input::type_text(args.text.clone())
        .map_err(|error| CliError::Failure(error.to_string()))?;
    print_type_result(
        &args,
        ActionResultDto {
            backend_name: metadata.backend_name,
            backend_kind: format!("{:?}", metadata.backend_kind).to_ascii_lowercase(),
        },
    );

    Ok(())
}

fn print_type_result(args: &TypeArgs, metadata: ActionResultDto) {
    if args.dry_run {
        println!("would type via {}", metadata.backend_name);
    } else {
        println!("typed text via {}", metadata.backend_name);
    }
}

fn parse_type_args(args: Vec<String>) -> Result<TypeCommand, CliError> {
    let mut dry_run = false;
    let mut text_parts = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--dry-run" => dry_run = true,
            "--help" | "-h" => return Ok(TypeCommand::Help),
            value => text_parts.push(value.to_owned()),
        }

        index += 1;
    }

    let text = text_parts.join(" ");
    if text.is_empty() {
        return Err(CliError::Failure("missing text to type".to_owned()));
    }

    Ok(TypeCommand::Run(TypeArgs { text, dry_run }))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HotkeyArgs {
    keys: Vec<String>,
    dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HotkeyCommand {
    Run(HotkeyArgs),
    Help,
}

fn hotkey(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let HotkeyCommand::Run(args) = parse_hotkey_args(args)? else {
        print_hotkey_usage();
        return Err(CliError::HelpRequested);
    };

    let action = peekaboox_input::InputAction::Hotkey(args.keys.clone());

    if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::Hotkey {
                keys: args.keys.clone(),
                dry_run: args.dry_run,
            },
        )?;
        let ApiResult::Hotkey(metadata) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected hotkey response".to_owned(),
            ));
        };
        print_hotkey_result(&args, metadata);
        return Ok(());
    }

    if args.dry_run {
        let backend = peekaboox_input::CommandInputBackend
            .detect_backend_for(&action)
            .map_err(|error| CliError::Failure(error.to_string()))?;
        println!(
            "would press hotkey {} via {}",
            args.keys.join("+"),
            backend.name()
        );
        return Ok(());
    }

    let metadata = peekaboox_input::hotkey(args.keys.clone())
        .map_err(|error| CliError::Failure(error.to_string()))?;
    print_hotkey_result(&args, input_metadata_dto(metadata));

    Ok(())
}

fn print_hotkey_result(args: &HotkeyArgs, metadata: ActionResultDto) {
    if args.dry_run {
        println!(
            "would press hotkey {} via {}",
            args.keys.join("+"),
            metadata.backend_name
        );
    } else {
        println!(
            "pressed hotkey {} via {}",
            args.keys.join("+"),
            metadata.backend_name
        );
    }
}

fn parse_hotkey_args(args: Vec<String>) -> Result<HotkeyCommand, CliError> {
    let mut keys = Vec::new();
    let mut dry_run = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--key" | "-k" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --key".to_owned()));
                };
                keys.push(value.to_owned());
            }
            "--dry-run" => dry_run = true,
            "--help" | "-h" => return Ok(HotkeyCommand::Help),
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown hotkey argument: {value}"
                )));
            }
            value => keys.push(value.to_owned()),
        }

        index += 1;
    }

    if keys.is_empty() {
        return Err(CliError::Failure(
            "missing hotkey; provide one or more keys".to_owned(),
        ));
    }

    if keys.iter().any(|key| key.trim().is_empty()) {
        return Err(CliError::Failure(
            "hotkey keys must not be empty".to_owned(),
        ));
    }

    Ok(HotkeyCommand::Run(HotkeyArgs { keys, dry_run }))
}

fn parse_i32(name: &str, value: &str) -> Result<i32, CliError> {
    value
        .parse::<i32>()
        .map_err(|_| CliError::Failure(format!("{name} must be an integer, got {value:?}")))
}

fn parse_usize(name: &str, value: &str) -> Result<usize, CliError> {
    value
        .parse::<usize>()
        .map_err(|_| CliError::Failure(format!("{name} must be an integer, got {value:?}")))
}

fn parse_u32(name: &str, value: &str) -> Result<u32, CliError> {
    value
        .parse::<u32>()
        .map_err(|_| CliError::Failure(format!("{name} must be an integer, got {value:?}")))
}

fn parse_positive_u32(name: &str, value: &str) -> Result<u32, CliError> {
    let parsed = parse_u32(name, value)?;
    if parsed == 0 {
        return Err(CliError::Failure(format!(
            "{name} must be greater than zero"
        )));
    }

    Ok(parsed)
}

fn parse_u8(name: &str, value: &str) -> Result<u8, CliError> {
    value
        .parse::<u8>()
        .map_err(|_| CliError::Failure(format!("{name} must be 0..255, got {value:?}")))
}

fn parse_unit_f32(name: &str, value: &str) -> Result<f32, CliError> {
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

fn parse_rect(name: &str, value: &str) -> Result<Rect, CliError> {
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

fn parse_point(name: &str, value: &str) -> Result<Point, CliError> {
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

fn non_empty_cli_string(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

fn parse_mouse_button(value: &str) -> Result<MouseButton, CliError> {
    match value {
        "left" => Ok(MouseButton::Left),
        "middle" => Ok(MouseButton::Middle),
        "right" => Ok(MouseButton::Right),
        _ => Err(CliError::Failure(format!(
            "--button must be left, middle, or right, got {value:?}"
        ))),
    }
}

fn mouse_button_dto(button: MouseButton) -> MouseButtonDto {
    match button {
        MouseButton::Left => MouseButtonDto::Left,
        MouseButton::Middle => MouseButtonDto::Middle,
        MouseButton::Right => MouseButtonDto::Right,
    }
}

fn input_metadata_dto(metadata: peekaboox_input::InputExecutionMetadata) -> ActionResultDto {
    ActionResultDto {
        backend_name: metadata.backend_name,
        backend_kind: format!("{:?}", metadata.backend_kind).to_ascii_lowercase(),
    }
}

fn daemon_request(context: &CliContext, request: ApiRequest) -> Result<ApiResult, CliError> {
    let response = send_request(&context.socket, request).map_err(|error| {
        CliError::Failure(format!(
            "failed to talk to daemon at {}: {error}",
            context.socket.display()
        ))
    })?;

    match response.response {
        ApiResponse::Ok { result } => Ok(result),
        ApiResponse::Error { message } => Err(CliError::Failure(message)),
    }
}

fn print_usage() {
    println!(
        "Usage: peekaboox [--daemon] [--socket <path>] <capture|capture-delta|capture-backends|capture-dmabuf|plugins|windows|elements|ocr|compare|state|vision-elements|click|move|drag|type|hotkey>"
    );
    println!("Try:   peekaboox capture --output screenshot.png");
    println!("Try:   peekaboox --daemon capture-delta --stream agent-loop");
    println!("Try:   peekaboox capture-backends");
    println!("Try:   peekaboox capture-dmabuf");
    println!("Try:   peekaboox plugins --path examples/plugins");
    println!("Try:   peekaboox --daemon windows");
    println!("Try:   peekaboox windows");
    println!("Try:   peekaboox elements --role \"push button\" --state enabled");
    println!("Try:   peekaboox ocr --region 10,20,400,120 --language eng");
    println!("Try:   peekaboox compare before.png after.png --max-changed-ratio 0.01");
    println!("Try:   peekaboox state frame1.png frame2.png frame3.png");
    println!("Try:   peekaboox vision-elements screenshot.png --min-width 8");
    println!("Try:   peekaboox click --x 100 --y 200");
    println!("Try:   peekaboox click --text \"Submit\"");
    println!("Try:   peekaboox move --x 100 --y 200");
    println!("Try:   peekaboox drag --from 100,200 --to 300,240 --duration-ms 350");
    println!("Try:   peekaboox type \"Hello World\"");
    println!("Try:   peekaboox hotkey ctrl+s");
}

fn print_capture_usage() {
    println!("Usage: peekaboox capture [--output <path>]");
}

fn print_capture_delta_usage() {
    println!(
        "Usage: peekaboox --daemon capture-delta [--stream <id>] [--reset] [--region x,y,width,height] [--threshold <0-255>] [--low-bandwidth|--full-frame]"
    );
}

fn print_capture_backends_usage() {
    println!("Usage: peekaboox capture-backends");
}

fn print_capture_dmabuf_usage() {
    println!("Usage: peekaboox capture-dmabuf [--import <compute|egl|egl-texture>]");
}

fn print_plugins_usage() {
    println!("Usage: peekaboox [--daemon] plugins [--path <plugin-dir-or-manifest>]... [--json]");
}

fn print_windows_usage() {
    println!("Usage: peekaboox windows");
}

fn print_elements_usage() {
    println!(
        "Usage: peekaboox elements [<selector>|--selector <query>] [--role <role>] [--text <label>] [--state <state>] [--bounds x,y,w,h] [--contains x,y] [--min-confidence <float>] [--limit <n>] [--vision-fallback]"
    );
}

fn print_ocr_usage() {
    println!("Usage: peekaboox ocr [--region x,y,width,height] [--language <code>]");
}

fn print_compare_usage() {
    println!(
        "Usage: peekaboox compare [--expected <path>] [--actual <path>] [--region x,y,width,height] [--threshold 0..255] [--max-changed-ratio 0.0..1.0]"
    );
    println!("       peekaboox compare <expected-path> <actual-path>");
}

fn print_ui_state_usage() {
    println!(
        "Usage: peekaboox state [--image <path>]... [--region x,y,width,height] [--threshold 0..255] [--stable-max-changed-ratio 0.0..1.0] [--loading-min-changed-ratio 0.0..1.0] [--required-stable-transitions <n>]"
    );
    println!("       peekaboox state <image-path> <image-path> [more-image-paths...]");
}

fn print_vision_elements_usage() {
    println!(
        "Usage: peekaboox vision-elements [--image <path>] [--region x,y,width,height] [--threshold 1..255] [--min-width <pixels>] [--min-height <pixels>] [--min-component-pixels <pixels>] [--max-elements <n>] [--merge-distance <pixels>]"
    );
    println!("       peekaboox vision-elements <image-path>");
}

fn print_click_usage() {
    println!(
        "Usage: peekaboox click (--x <pixels> --y <pixels> | --text <label> | --selector <query>) [--button left|middle|right] [--dry-run] [--vision-fallback]"
    );
}

fn print_move_usage() {
    println!("Usage: peekaboox move --x <pixels> --y <pixels> [--dry-run]");
}

fn print_drag_usage() {
    println!(
        "Usage: peekaboox drag (--from x,y --to x,y | --from-x <px> --from-y <px> --to-x <px> --to-y <px>) [--button left|middle|right] [--duration-ms <ms>] [--dry-run]"
    );
}

fn print_type_usage() {
    println!("Usage: peekaboox type [--dry-run] <text>");
}

fn print_hotkey_usage() {
    println!("Usage: peekaboox hotkey [--dry-run] <key-or-combo> [more-keys]");
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use peekaboox_input::MouseButton;

    use super::{
        CaptureArgs, CaptureBackendsCommand, CaptureCommand, CaptureDeltaArgs, CaptureDeltaCommand,
        CaptureDmaBufArgs, CaptureDmaBufCommand, CaptureDmaBufImportTarget, CliContext, CliError,
        ClickArgs, ClickCommand, ClickTarget, CompareArgs, CompareCommand, DragArgs, DragCommand,
        ElementsArgs, ElementsCommand, GlobalArgs, HotkeyArgs, HotkeyCommand, MoveArgs,
        MoveCommand, OcrArgs, OcrCommand, PluginsArgs, PluginsCommand, TypeArgs, TypeCommand,
        UiStateArgs, UiStateCommand, VisionElementsArgs, VisionElementsCommand, WindowsCommand,
        parse_capture_args, parse_capture_backends_args, parse_capture_delta_args,
        parse_capture_dmabuf_args, parse_click_args, parse_compare_args, parse_drag_args,
        parse_elements_args, parse_global_args, parse_hotkey_args, parse_move_args, parse_ocr_args,
        parse_plugins_args, parse_type_args, parse_ui_state_args, parse_vision_elements_args,
        parse_windows_args,
    };
    use peekaboox_core::{Point, Rect};

    #[test]
    fn capture_defaults_to_screenshot_png() {
        let args = parse_capture_args(vec![]).unwrap();

        assert_eq!(
            args,
            CaptureCommand::Run(CaptureArgs {
                output: PathBuf::from("screenshot.png")
            })
        );
    }

    #[test]
    fn parses_global_daemon_flags() {
        let args = parse_global_args(vec![
            "--daemon".to_owned(),
            "--socket".to_owned(),
            "/tmp/peekaboox.sock".to_owned(),
            "windows".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            args,
            GlobalArgs {
                context: CliContext {
                    use_daemon: true,
                    socket: PathBuf::from("/tmp/peekaboox.sock")
                },
                args: vec!["windows".to_owned()]
            }
        );
    }

    #[test]
    fn capture_accepts_output_argument() {
        let args = parse_capture_args(vec!["--output".to_owned(), "tmp/screenshot.png".to_owned()])
            .unwrap();

        assert_eq!(
            args,
            CaptureCommand::Run(CaptureArgs {
                output: PathBuf::from("tmp/screenshot.png")
            })
        );
    }

    #[test]
    fn capture_rejects_missing_output_value() {
        let error = parse_capture_args(vec!["--output".to_owned()]).unwrap_err();

        assert_eq!(
            error,
            CliError::Failure("missing value for --output".to_owned())
        );
    }

    #[test]
    fn capture_help_is_not_a_failure() {
        let command = parse_capture_args(vec!["--help".to_owned()]).unwrap();

        assert_eq!(command, CaptureCommand::Help);
    }

    #[test]
    fn capture_delta_accepts_stream_reset_region_and_threshold() {
        let args = parse_capture_delta_args(vec![
            "--stream".to_owned(),
            "agent-loop".to_owned(),
            "--reset".to_owned(),
            "--region".to_owned(),
            "10,20,100,40".to_owned(),
            "--threshold".to_owned(),
            "3".to_owned(),
            "--full-frame".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            args,
            CaptureDeltaCommand::Run(CaptureDeltaArgs {
                stream_id: Some("agent-loop".to_owned()),
                reset: true,
                region: Some(Rect::new(10, 20, 100, 40)),
                per_channel_threshold: 3,
                low_bandwidth: false,
            })
        );
    }

    #[test]
    fn capture_delta_help_is_not_a_failure() {
        let command = parse_capture_delta_args(vec!["--help".to_owned()]).unwrap();

        assert_eq!(command, CaptureDeltaCommand::Help);
    }

    #[test]
    fn capture_backends_accepts_no_arguments() {
        let command = parse_capture_backends_args(vec![]).unwrap();

        assert_eq!(command, CaptureBackendsCommand::Run);
    }

    #[test]
    fn capture_backends_help_is_not_a_failure() {
        let command = parse_capture_backends_args(vec!["--help".to_owned()]).unwrap();

        assert_eq!(command, CaptureBackendsCommand::Help);
    }

    #[test]
    fn capture_backends_rejects_positional_arguments() {
        let error = parse_capture_backends_args(vec!["extra".to_owned()]).unwrap_err();

        assert_eq!(
            error,
            CliError::Failure("unknown capture-backends argument: extra".to_owned())
        );
    }

    #[test]
    fn capture_dmabuf_accepts_no_arguments() {
        let command = parse_capture_dmabuf_args(vec![]).unwrap();

        assert_eq!(
            command,
            CaptureDmaBufCommand::Run(CaptureDmaBufArgs {
                import_target: CaptureDmaBufImportTarget::Compute
            })
        );
    }

    #[test]
    fn capture_dmabuf_accepts_egl_import_target() {
        let command =
            parse_capture_dmabuf_args(vec!["--import".to_owned(), "egl".to_owned()]).unwrap();

        assert_eq!(
            command,
            CaptureDmaBufCommand::Run(CaptureDmaBufArgs {
                import_target: CaptureDmaBufImportTarget::Egl
            })
        );
    }

    #[test]
    fn capture_dmabuf_accepts_egl_texture_import_target() {
        let command =
            parse_capture_dmabuf_args(vec!["--import".to_owned(), "egl-texture".to_owned()])
                .unwrap();

        assert_eq!(
            command,
            CaptureDmaBufCommand::Run(CaptureDmaBufArgs {
                import_target: CaptureDmaBufImportTarget::EglTexture
            })
        );
    }

    #[test]
    fn capture_dmabuf_help_is_not_a_failure() {
        let command = parse_capture_dmabuf_args(vec!["--help".to_owned()]).unwrap();

        assert_eq!(command, CaptureDmaBufCommand::Help);
    }

    #[test]
    fn capture_dmabuf_rejects_positional_arguments() {
        let error = parse_capture_dmabuf_args(vec!["extra".to_owned()]).unwrap_err();

        assert_eq!(
            error,
            CliError::Failure("unknown capture-dmabuf argument: extra".to_owned())
        );
    }

    #[test]
    fn capture_dmabuf_rejects_missing_import_target() {
        let error = parse_capture_dmabuf_args(vec!["--import".to_owned()]).unwrap_err();

        assert_eq!(
            error,
            CliError::Failure("missing value for --import".to_owned())
        );
    }

    #[test]
    fn capture_dmabuf_rejects_unknown_import_target() {
        let error = parse_capture_dmabuf_args(vec!["--import".to_owned(), "vulkan".to_owned()])
            .unwrap_err();

        assert_eq!(
            error,
            CliError::Failure("unsupported capture-dmabuf import target: vulkan".to_owned())
        );
    }

    #[test]
    fn plugins_accepts_paths_and_json_flag() {
        let command = parse_plugins_args(vec![
            "--path".to_owned(),
            "examples/plugins".to_owned(),
            "--json".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            PluginsCommand::Run(PluginsArgs {
                paths: vec![PathBuf::from("examples/plugins")],
                json: true,
            })
        );
    }

    #[test]
    fn plugins_help_is_not_a_failure() {
        let command = parse_plugins_args(vec!["--help".to_owned()]).unwrap();

        assert_eq!(command, PluginsCommand::Help);
    }

    #[test]
    fn plugins_rejects_missing_path() {
        let error = parse_plugins_args(vec!["--path".to_owned()]).unwrap_err();

        assert_eq!(
            error,
            CliError::Failure("missing value for --path".to_owned())
        );
    }

    #[test]
    fn windows_accepts_no_arguments() {
        let command = parse_windows_args(vec![]).unwrap();

        assert_eq!(command, WindowsCommand::Run);
    }

    #[test]
    fn windows_help_is_not_a_failure() {
        let command = parse_windows_args(vec!["--help".to_owned()]).unwrap();

        assert_eq!(command, WindowsCommand::Help);
    }

    #[test]
    fn elements_defaults_to_all_with_limit() {
        let command = parse_elements_args(vec![]).unwrap();

        assert_eq!(
            command,
            ElementsCommand::Run(ElementsArgs {
                selector: String::new(),
                limit: 50,
                vision_fallback: false
            })
        );
    }

    #[test]
    fn elements_accepts_structured_selector_parts() {
        let command = parse_elements_args(vec![
            "--role".to_owned(),
            "push button".to_owned(),
            "--text".to_owned(),
            "Submit".to_owned(),
            "--state".to_owned(),
            "enabled".to_owned(),
            "--contains".to_owned(),
            "25,30".to_owned(),
            "--min-confidence".to_owned(),
            "0.9".to_owned(),
            "--limit".to_owned(),
            "5".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            ElementsCommand::Run(ElementsArgs {
                selector:
                    "role=push button,label=Submit,state=enabled,contains=25,30,confidence>=0.9"
                        .to_owned(),
                limit: 5,
                vision_fallback: false
            })
        );
    }

    #[test]
    fn elements_accepts_vision_fallback_flag() {
        let command = parse_elements_args(vec![
            "--role".to_owned(),
            "visual-region".to_owned(),
            "--vision-fallback".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            ElementsCommand::Run(ElementsArgs {
                selector: "role=visual-region".to_owned(),
                limit: 50,
                vision_fallback: true
            })
        );
    }

    #[test]
    fn ocr_accepts_region_and_language() {
        let command = parse_ocr_args(vec![
            "--region".to_owned(),
            "10,20,300,80".to_owned(),
            "--language".to_owned(),
            "eng".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            OcrCommand::Run(OcrArgs {
                region: Some(Rect::new(10, 20, 300, 80)),
                language: Some("eng".to_owned())
            })
        );
    }

    #[test]
    fn ocr_rejects_bad_region() {
        let error =
            parse_ocr_args(vec!["--region".to_owned(), "10,20,0,80".to_owned()]).unwrap_err();

        assert_eq!(
            error,
            CliError::Failure("--region width and height must be greater than zero".to_owned())
        );
    }

    #[test]
    fn compare_accepts_positional_paths_and_tolerance() {
        let command = parse_compare_args(vec![
            "before.png".to_owned(),
            "after.png".to_owned(),
            "--threshold".to_owned(),
            "4".to_owned(),
            "--max-changed-ratio".to_owned(),
            "0.01".to_owned(),
            "--region".to_owned(),
            "10,20,300,80".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            CompareCommand::Run(CompareArgs {
                expected: PathBuf::from("before.png"),
                actual: PathBuf::from("after.png"),
                region: Some(Rect::new(10, 20, 300, 80)),
                per_channel_threshold: 4,
                max_changed_ratio: 0.01
            })
        );
    }

    #[test]
    fn compare_rejects_missing_actual_path() {
        let error = parse_compare_args(vec!["before.png".to_owned()]).unwrap_err();

        assert_eq!(
            error,
            CliError::Failure("missing --actual image path".to_owned())
        );
    }

    #[test]
    fn compare_rejects_bad_ratio() {
        let error = parse_compare_args(vec![
            "before.png".to_owned(),
            "after.png".to_owned(),
            "--max-changed-ratio".to_owned(),
            "1.1".to_owned(),
        ])
        .unwrap_err();

        assert_eq!(
            error,
            CliError::Failure(
                "--max-changed-ratio must be between 0.0 and 1.0, got \"1.1\"".to_owned()
            )
        );
    }

    #[test]
    fn ui_state_accepts_paths_and_thresholds() {
        let command = parse_ui_state_args(vec![
            "first.png".to_owned(),
            "--image".to_owned(),
            "second.png".to_owned(),
            "third.png".to_owned(),
            "--threshold".to_owned(),
            "4".to_owned(),
            "--stable-max-changed-ratio".to_owned(),
            "0.002".to_owned(),
            "--loading-min-changed-ratio".to_owned(),
            "0.03".to_owned(),
            "--required-stable-transitions".to_owned(),
            "2".to_owned(),
            "--region".to_owned(),
            "10,20,300,80".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            UiStateCommand::Run(UiStateArgs {
                image_paths: vec![
                    PathBuf::from("first.png"),
                    PathBuf::from("second.png"),
                    PathBuf::from("third.png")
                ],
                region: Some(Rect::new(10, 20, 300, 80)),
                per_channel_threshold: 4,
                stable_max_changed_ratio: 0.002,
                loading_min_changed_ratio: 0.03,
                required_stable_transitions: 2
            })
        );
    }

    #[test]
    fn ui_state_rejects_single_path() {
        let error = parse_ui_state_args(vec!["first.png".to_owned()]).unwrap_err();

        assert_eq!(
            error,
            CliError::Failure("state requires at least two image paths".to_owned())
        );
    }

    #[test]
    fn ui_state_rejects_inverted_thresholds() {
        let error = parse_ui_state_args(vec![
            "first.png".to_owned(),
            "second.png".to_owned(),
            "--stable-max-changed-ratio".to_owned(),
            "0.1".to_owned(),
            "--loading-min-changed-ratio".to_owned(),
            "0.01".to_owned(),
        ])
        .unwrap_err();

        assert_eq!(
            error,
            CliError::Failure(
                "--stable-max-changed-ratio must be less than or equal to --loading-min-changed-ratio"
                    .to_owned()
            )
        );
    }

    #[test]
    fn vision_elements_accepts_image_and_detection_options() {
        let command = parse_vision_elements_args(vec![
            "screen.png".to_owned(),
            "--threshold".to_owned(),
            "32".to_owned(),
            "--min-width".to_owned(),
            "9".to_owned(),
            "--min-height".to_owned(),
            "7".to_owned(),
            "--min-component-pixels".to_owned(),
            "20".to_owned(),
            "--max-elements".to_owned(),
            "12".to_owned(),
            "--merge-distance".to_owned(),
            "3".to_owned(),
            "--region".to_owned(),
            "10,20,300,80".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            VisionElementsCommand::Run(VisionElementsArgs {
                image: PathBuf::from("screen.png"),
                region: Some(Rect::new(10, 20, 300, 80)),
                edge_threshold: 32,
                min_width: 9,
                min_height: 7,
                min_component_pixels: 20,
                max_elements: 12,
                merge_distance: 3
            })
        );
    }

    #[test]
    fn vision_elements_rejects_missing_image() {
        let error = parse_vision_elements_args(vec!["--threshold".to_owned(), "24".to_owned()])
            .unwrap_err();

        assert_eq!(error, CliError::Failure("missing --image path".to_owned()));
    }

    #[test]
    fn vision_elements_rejects_zero_threshold() {
        let error = parse_vision_elements_args(vec![
            "screen.png".to_owned(),
            "--threshold".to_owned(),
            "0".to_owned(),
        ])
        .unwrap_err();

        assert_eq!(
            error,
            CliError::Failure("--threshold must be greater than zero".to_owned())
        );
    }

    #[test]
    fn click_requires_coordinates() {
        let error = parse_click_args(vec!["--x".to_owned(), "10".to_owned()]).unwrap_err();

        assert_eq!(error, CliError::Failure("missing required --y".to_owned()));
    }

    #[test]
    fn click_accepts_coordinates_button_and_dry_run() {
        let command = parse_click_args(vec![
            "--x".to_owned(),
            "10".to_owned(),
            "--y".to_owned(),
            "20".to_owned(),
            "--button".to_owned(),
            "right".to_owned(),
            "--dry-run".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            ClickCommand::Run(ClickArgs {
                target: ClickTarget::Coordinates(Point::new(10, 20)),
                button: MouseButton::Right,
                dry_run: true,
                vision_fallback: false
            })
        );
    }

    #[test]
    fn click_accepts_text_selector() {
        let command = parse_click_args(vec![
            "--text".to_owned(),
            "Submit".to_owned(),
            "--dry-run".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            ClickCommand::Run(ClickArgs {
                target: ClickTarget::SemanticSelector("Submit".to_owned()),
                button: MouseButton::Left,
                dry_run: true,
                vision_fallback: false
            })
        );
    }

    #[test]
    fn click_accepts_vision_fallback_flag() {
        let command = parse_click_args(vec![
            "--selector".to_owned(),
            "role=visual-region,contains=10,20".to_owned(),
            "--vision-fallback".to_owned(),
            "--dry-run".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            ClickCommand::Run(ClickArgs {
                target: ClickTarget::SemanticSelector(
                    "role=visual-region,contains=10,20".to_owned()
                ),
                button: MouseButton::Left,
                dry_run: true,
                vision_fallback: true
            })
        );
    }

    #[test]
    fn click_rejects_coordinates_and_selector_together() {
        let error = parse_click_args(vec![
            "--x".to_owned(),
            "10".to_owned(),
            "--y".to_owned(),
            "20".to_owned(),
            "--selector".to_owned(),
            "role=button".to_owned(),
        ])
        .unwrap_err();

        assert_eq!(
            error,
            CliError::Failure(
                "provide either coordinates or --selector/--text, not both".to_owned()
            )
        );
    }

    #[test]
    fn move_accepts_coordinates_and_dry_run() {
        let command = parse_move_args(vec![
            "--x".to_owned(),
            "10".to_owned(),
            "--y".to_owned(),
            "20".to_owned(),
            "--dry-run".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            MoveCommand::Run(MoveArgs {
                position: Point::new(10, 20),
                dry_run: true
            })
        );
    }

    #[test]
    fn move_requires_y_coordinate() {
        let error = parse_move_args(vec!["--x".to_owned(), "10".to_owned()]).unwrap_err();

        assert_eq!(error, CliError::Failure("missing required --y".to_owned()));
    }

    #[test]
    fn drag_accepts_compact_points() {
        let command = parse_drag_args(vec![
            "--from".to_owned(),
            "10,20".to_owned(),
            "--to".to_owned(),
            "40,80".to_owned(),
            "--button".to_owned(),
            "middle".to_owned(),
            "--duration-ms".to_owned(),
            "500".to_owned(),
            "--dry-run".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            DragCommand::Run(DragArgs {
                from: Point::new(10, 20),
                to: Point::new(40, 80),
                button: MouseButton::Middle,
                duration_ms: 500,
                dry_run: true
            })
        );
    }

    #[test]
    fn drag_accepts_split_points() {
        let command = parse_drag_args(vec![
            "--from-x".to_owned(),
            "10".to_owned(),
            "--from-y".to_owned(),
            "20".to_owned(),
            "--to-x".to_owned(),
            "40".to_owned(),
            "--to-y".to_owned(),
            "80".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            DragCommand::Run(DragArgs {
                from: Point::new(10, 20),
                to: Point::new(40, 80),
                button: MouseButton::Left,
                duration_ms: 250,
                dry_run: false
            })
        );
    }

    #[test]
    fn drag_rejects_mixed_point_styles() {
        let error = parse_drag_args(vec![
            "--from".to_owned(),
            "10,20".to_owned(),
            "--from-x".to_owned(),
            "10".to_owned(),
            "--to".to_owned(),
            "40,80".to_owned(),
        ])
        .unwrap_err();

        assert_eq!(
            error,
            CliError::Failure("provide either --from or --from-x/--from-y, not both".to_owned())
        );
    }

    #[test]
    fn type_joins_remaining_text_arguments() {
        let command = parse_type_args(vec!["hello".to_owned(), "world".to_owned()]).unwrap();

        assert_eq!(
            command,
            TypeCommand::Run(TypeArgs {
                text: "hello world".to_owned(),
                dry_run: false
            })
        );
    }

    #[test]
    fn type_accepts_dry_run() {
        let command = parse_type_args(vec!["--dry-run".to_owned(), "hello".to_owned()]).unwrap();

        assert_eq!(
            command,
            TypeCommand::Run(TypeArgs {
                text: "hello".to_owned(),
                dry_run: true
            })
        );
    }

    #[test]
    fn hotkey_accepts_positional_keys_and_dry_run() {
        let command = parse_hotkey_args(vec![
            "--dry-run".to_owned(),
            "ctrl".to_owned(),
            "s".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            HotkeyCommand::Run(HotkeyArgs {
                keys: vec!["ctrl".to_owned(), "s".to_owned()],
                dry_run: true
            })
        );
    }

    #[test]
    fn hotkey_requires_keys() {
        let error = parse_hotkey_args(vec![]).unwrap_err();

        assert_eq!(
            error,
            CliError::Failure("missing hotkey; provide one or more keys".to_owned())
        );
    }
}
