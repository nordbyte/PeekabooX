use std::io::Write;
use std::path::{Path, PathBuf};

mod doctor;

use peekaboox_accessibility::{AccessibilityTreeMetadata, ElementQuery};
use peekaboox_core::{BackendKind, Point, Rect, UiElement, WindowInfo};
use peekaboox_desktop::{
    AssertOptions, ClickOptions as DesktopClickOptions, DesktopAssertion, DesktopDragOptions,
    FocusOptions, LocateOptions, TypeIntoOptions,
};
use peekaboox_input::MouseButton;
use peekaboox_ipc::{
    ActionResultDto, ApiRequest, ApiResponse, ApiResult, CaptureBackendDto, CaptureBackendProbeDto,
    CaptureBackendProbeResultDto, CaptureBackendsResultDto, CaptureDeltaResultDto,
    CaptureResultDto, DesktopActionResultDto, DesktopAssertionDto, DesktopLocateResultDto,
    DmaBufImportTargetDto, DmaBufProbeResultDto, ElementDto, ElementListResultDto, MouseButtonDto,
    OcrBlockDto, OcrResultDto, PluginDiscoveryErrorDto, PluginDto, PluginListResultDto,
    PluginToolDto, PluginToolExecutionResultDto, RectDto, UiStateDto, VisualDiffDto,
    WindowBackendReportDto, WindowDto, WindowListResultDto, ZeroCopyBackendDto,
    default_socket_path, send_request,
};
use peekaboox_vision::{
    OcrConfig, OcrOptions, OcrPreprocessingOptions, OcrResult, TesseractOcrBackend,
    UiElementDetectionOptions, UiElementSort, UiStateOptions, UiStateResult, VisualAlphaMode,
    VisualCompareOptions, VisualDiffResult, VisualSizePolicy,
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
        Some("capture-backends") | Some("backends") => {
            match capture_backends(args.collect(), &global.context) {
                Ok(()) => {}
                Err(CliError::HelpRequested) => {}
                Err(CliError::Failure(error)) => {
                    eprintln!("capture-backends failed: {error}");
                    std::process::exit(1);
                }
            }
        }
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
        Some("plugin-call") | Some("call-plugin") => {
            match plugin_call(args.collect(), &global.context) {
                Ok(()) => {}
                Err(CliError::HelpRequested) => {}
                Err(CliError::Failure(error)) => {
                    eprintln!("plugin-call failed: {error}");
                    std::process::exit(1);
                }
            }
        }
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
        Some("desktop") => match desktop(args.collect(), &global.context) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("desktop failed: {error}");
                std::process::exit(1);
            }
        },
        Some("doctor") => match doctor::run(args.collect()) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("doctor failed: {error}");
                std::process::exit(1);
            }
        },
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
        Some("paste") => match paste_text(args.collect(), &global.context) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("paste failed: {error}");
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
    region: Option<Rect>,
    window_id: Option<String>,
    app: Option<String>,
    window_title: Option<String>,
    title_regex: Option<String>,
    format: CaptureOutputFormat,
    json: bool,
    stdout: bool,
    no_overwrite: bool,
    include_semantic_tree: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CaptureCommand {
    Run(CaptureArgs),
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureOutputFormat {
    Png,
    Xwd,
}

impl CaptureOutputFormat {
    fn as_str(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Xwd => "xwd",
        }
    }

    fn mime_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Xwd => "image/x-xwindowdump",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CaptureDeltaArgs {
    stream_id: Option<String>,
    reset: bool,
    region: Option<Rect>,
    window_id: Option<String>,
    per_channel_threshold: u8,
    low_bandwidth: bool,
    json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CaptureDeltaCommand {
    Run(CaptureDeltaArgs),
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CaptureBackendsArgs {
    output: PathBuf,
    region: Option<Rect>,
    diagnose: bool,
    json: bool,
    probe: CaptureBackendProbeDto,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CaptureBackendsCommand {
    Run(CaptureBackendsArgs),
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

#[derive(Debug, Clone, PartialEq)]
struct PluginCallArgs {
    plugin_id: String,
    tool: String,
    arguments: serde_json::Value,
    paths: Vec<PathBuf>,
    timeout_ms: u64,
    max_output_bytes: usize,
    json: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum PluginCallCommand {
    Run(PluginCallArgs),
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CliError {
    HelpRequested,
    Failure(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WindowsArgs {
    json: bool,
    id: Option<String>,
    app: Option<String>,
    title: Option<String>,
    title_regex: Option<String>,
    focused: bool,
    limit: Option<usize>,
    sort: peekaboox_windows::WindowSort,
    backend: peekaboox_windows::WindowBackendSelection,
    diagnose: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum WindowsCommand {
    Run(WindowsArgs),
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ElementsArgs {
    selector: String,
    limit: usize,
    vision_fallback: bool,
    app: Option<String>,
    window_title: Option<String>,
    window_id: Option<String>,
    vision_region: Option<Rect>,
    vision_edge_threshold: Option<u8>,
    vision_min_width: Option<u32>,
    vision_min_height: Option<u32>,
    vision_min_component_pixels: Option<u32>,
    vision_max_elements: Option<u32>,
    vision_merge_distance: Option<u32>,
    json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ElementsCommand {
    Run(ElementsArgs),
    Help,
}

#[derive(Debug, Clone, PartialEq)]
struct OcrArgs {
    image: Option<PathBuf>,
    region: Option<Rect>,
    app: Option<String>,
    window_title: Option<String>,
    window_id: Option<String>,
    language: Option<String>,
    page_segmentation_mode: Option<u8>,
    engine_mode: Option<u8>,
    dpi: Option<u32>,
    min_confidence: Option<f32>,
    whitelist: Option<String>,
    config: Vec<OcrConfig>,
    preprocessing: OcrPreprocessingOptions,
    json: bool,
    words: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum OcrCommand {
    Run(Box<OcrArgs>),
    Help,
}

#[derive(Debug, Clone, PartialEq)]
struct CompareArgs {
    expected: PathBuf,
    actual: PathBuf,
    region: Option<Rect>,
    ignore_regions: Vec<Rect>,
    per_channel_threshold: u8,
    max_changed_ratio: f32,
    max_changed_pixels: Option<u64>,
    max_mean_absolute_error: Option<f32>,
    max_channel_delta: Option<u8>,
    size_policy: VisualSizePolicy,
    alpha_mode: VisualAlphaMode,
    diff_output: Option<PathBuf>,
    report: Option<PathBuf>,
    no_fail: bool,
    json: bool,
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
    ignore_regions: Vec<Rect>,
    per_channel_threshold: u8,
    stable_max_changed_ratio: f32,
    stable_max_changed_pixels: Option<u64>,
    stable_max_mean_absolute_error: Option<f32>,
    stable_max_channel_delta: Option<u8>,
    loading_min_changed_ratio: f32,
    loading_min_changed_pixels: Option<u64>,
    required_stable_transitions: u32,
    size_policy: VisualSizePolicy,
    alpha_mode: VisualAlphaMode,
    json: bool,
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
    ignore_regions: Vec<Rect>,
    edge_threshold: u8,
    min_width: u32,
    min_height: u32,
    min_component_pixels: u32,
    min_confidence: Option<f32>,
    max_width: Option<u32>,
    max_height: Option<u32>,
    min_area: Option<u64>,
    max_area: Option<u64>,
    max_elements: u32,
    merge_distance: u32,
    padding: u32,
    sort: UiElementSort,
    mask_output: Option<PathBuf>,
    overlay_output: Option<PathBuf>,
    json: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum VisionElementsCommand {
    Run(VisionElementsArgs),
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DesktopProfilesArgs {
    json: bool,
    app: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DesktopFocusArgs {
    app: String,
    use_gnome_overview: bool,
    launch_if_needed: bool,
    wait_after_focus_ms: u64,
    overview_wait_ms: u64,
    window_title: Option<String>,
    window_id: Option<String>,
    verify: bool,
    json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DesktopLocateArgs {
    app: String,
    target: String,
    image: Option<PathBuf>,
    prefer_accessibility: bool,
    window_title: Option<String>,
    window_id: Option<String>,
    json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DesktopClickArgs {
    app: String,
    target: String,
    image: Option<PathBuf>,
    prefer_accessibility: bool,
    window_title: Option<String>,
    window_id: Option<String>,
    button: MouseButton,
    dry_run: bool,
    verify: bool,
    json: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct DesktopDragArgs {
    app: String,
    target: String,
    image: Option<PathBuf>,
    prefer_accessibility: bool,
    window_title: Option<String>,
    window_id: Option<String>,
    button: MouseButton,
    from_ratio: (f32, f32),
    to_ratio: (f32, f32),
    duration_ms: u64,
    dry_run: bool,
    verify: bool,
    json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DesktopTypeIntoArgs {
    app: String,
    target: String,
    text: String,
    image: Option<PathBuf>,
    prefer_accessibility: bool,
    window_title: Option<String>,
    window_id: Option<String>,
    clear: bool,
    dry_run: bool,
    verify: bool,
    json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DesktopAssertArgs {
    app: String,
    target: String,
    image: Option<PathBuf>,
    prefer_accessibility: bool,
    window_title: Option<String>,
    window_id: Option<String>,
    assertion: DesktopAssertion,
    json: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum DesktopCommand {
    Profiles(DesktopProfilesArgs),
    Focus(DesktopFocusArgs),
    Locate(DesktopLocateArgs),
    Click(DesktopClickArgs),
    Drag(DesktopDragArgs),
    TypeInto(DesktopTypeIntoArgs),
    Assert(DesktopAssertArgs),
    Help,
}

fn capture(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
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
                output: args.output.display().to_string(),
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

fn parse_capture_args(args: Vec<String>) -> Result<CaptureCommand, CliError> {
    let mut output = None;
    let mut region = None;
    let mut window_id = None;
    let mut app = None;
    let mut window_title = None;
    let mut title_regex = None;
    let mut format = CaptureOutputFormat::Png;
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
    if stdout && format != CaptureOutputFormat::Png {
        return Err(CliError::Failure(
            "capture --stdout currently supports PNG output only".to_owned(),
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
        json,
        stdout,
        no_overwrite,
        include_semantic_tree,
    }))
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

fn parse_capture_delta_args(args: Vec<String>) -> Result<CaptureDeltaCommand, CliError> {
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

fn capture_backends(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let CaptureBackendsCommand::Run(args) = parse_capture_backends_args(args)? else {
        print_capture_backends_usage();
        return Err(CliError::HelpRequested);
    };

    let result = if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::CaptureBackends {
                output: args.output.display().to_string(),
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

fn parse_capture_backends_args(args: Vec<String>) -> Result<CaptureBackendsCommand, CliError> {
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

fn default_capture_backends_output_for_format(value: &str) -> Result<PathBuf, CliError> {
    match value {
        "png" => Ok(PathBuf::from("screenshot.png")),
        "xwd" => Ok(PathBuf::from("screenshot.xwd")),
        _ => Err(CliError::Failure(format!(
            "--format must be png or xwd, got {value:?}"
        ))),
    }
}

fn parse_capture_backend_probe(value: &str) -> Result<CaptureBackendProbeDto, CliError> {
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

fn capture_backends_result(args: &CaptureBackendsArgs) -> CaptureBackendsResultDto {
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

fn capture_backend_dto(
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

fn zero_copy_backend_dto(
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

fn capture_backend_warnings(backends: &[ZeroCopyBackendDto]) -> Vec<String> {
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

fn capture_backend_probe_steps(probe: CaptureBackendProbeDto) -> Vec<CaptureBackendProbeDto> {
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

fn capture_backend_probe(
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

fn capture_backend_probe_file(output: &Path) -> CaptureBackendProbeResultDto {
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

fn capture_backend_probe_frame() -> CaptureBackendProbeResultDto {
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

fn capture_backend_probe_region(region: Rect) -> CaptureBackendProbeResultDto {
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

fn capture_backend_probe_dmabuf() -> CaptureBackendProbeResultDto {
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

fn capture_backend_probe_error(
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

fn capture_backend_probe_name(probe: CaptureBackendProbeDto) -> &'static str {
    match probe {
        CaptureBackendProbeDto::None => "none",
        CaptureBackendProbeDto::File => "file",
        CaptureBackendProbeDto::Frame => "frame",
        CaptureBackendProbeDto::Region => "region",
        CaptureBackendProbeDto::DmaBuf => "dmabuf",
        CaptureBackendProbeDto::All => "all",
    }
}

fn capture_frame_source_label(source: peekaboox_capture::CaptureFrameSource) -> &'static str {
    match source {
        peekaboox_capture::CaptureFrameSource::DirectStdout => "direct-stdout",
        peekaboox_capture::CaptureFrameSource::DmaBufZeroCopy => "dmabuf-zero-copy",
        peekaboox_capture::CaptureFrameSource::FileFallback => "file-fallback",
        peekaboox_capture::CaptureFrameSource::FullFrameCrop => "full-frame-crop",
    }
}

fn print_capture_backends_result(result: &CaptureBackendsResultDto, diagnose: bool) {
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

fn capture_session_display(value: &str) -> String {
    match value {
        "wayland" => "Wayland".to_owned(),
        "x11" => "X11".to_owned(),
        "unknown" => "Unknown".to_owned(),
        other => other.to_owned(),
    }
}

fn capture_availability_display(value: &str) -> String {
    match value {
        "available" => "Available".to_owned(),
        "missing_pipewire_session" => "MissingPipeWireSession".to_owned(),
        "unsupported_session" => "UnsupportedSession".to_owned(),
        other => other.to_owned(),
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

fn plugin_call(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let PluginCallCommand::Run(args) = parse_plugin_call_args(args)? else {
        print_plugin_call_usage();
        return Err(CliError::HelpRequested);
    };

    let result = if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::CallPluginTool {
                plugin_id: args.plugin_id.clone(),
                tool: args.tool.clone(),
                arguments: args.arguments.clone(),
                paths: args
                    .paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect(),
                timeout_ms: args.timeout_ms,
                max_output_bytes: args.max_output_bytes,
            },
        )?;
        let ApiResult::PluginToolExecution(result) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected plugin execution response".to_owned(),
            ));
        };
        result
    } else {
        let discovery = peekaboox_plugins::discover_plugins(&args.paths);
        if !discovery.errors.is_empty() {
            return Err(CliError::Failure(format!(
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
            .find(|plugin| plugin.manifest.id == args.plugin_id)
            .ok_or_else(|| CliError::Failure(format!("unknown plugin: {}", args.plugin_id)))?;
        let policy = peekaboox_plugins::PluginExecutionPolicy {
            timeout: std::time::Duration::from_millis(args.timeout_ms),
            max_output_bytes: args.max_output_bytes,
            ..Default::default()
        };
        plugin_execution_dto(
            peekaboox_plugins::execute_plugin_tool(
                plugin,
                &args.tool,
                args.arguments.clone(),
                &policy,
            )
            .map_err(CliError::Failure)?,
        )
    };

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&result)
                .map_err(|error| CliError::Failure(error.to_string()))?
        );
    } else {
        print_plugin_execution_result(&result);
    }
    Ok(())
}

fn parse_plugin_call_args(args: Vec<String>) -> Result<PluginCallCommand, CliError> {
    let mut paths = Vec::new();
    let mut arguments = serde_json::json!({});
    let mut timeout_ms = 10_000;
    let mut max_output_bytes = 1_048_576;
    let mut json = false;
    let mut positional = Vec::new();
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
            "--arguments-json" | "--args-json" | "--args" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --arguments-json".to_owned(),
                    ));
                };
                arguments = serde_json::from_str(value).map_err(|error| {
                    CliError::Failure(format!("invalid arguments JSON: {error}"))
                })?;
            }
            "--timeout-ms" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --timeout-ms".to_owned(),
                    ));
                };
                timeout_ms = parse_u64("--timeout-ms", value)?;
            }
            "--max-output-bytes" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --max-output-bytes".to_owned(),
                    ));
                };
                max_output_bytes = parse_usize("--max-output-bytes", value)?;
            }
            "--json" => json = true,
            "--help" | "-h" => return Ok(PluginCallCommand::Help),
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown plugin-call argument: {value}"
                )));
            }
            value => positional.push(value.to_owned()),
        }
        index += 1;
    }

    if positional.len() != 2 {
        return Err(CliError::Failure(
            "plugin-call requires <plugin-id> and <tool>".to_owned(),
        ));
    }
    Ok(PluginCallCommand::Run(PluginCallArgs {
        plugin_id: positional.remove(0),
        tool: positional.remove(0),
        arguments,
        paths,
        timeout_ms,
        max_output_bytes,
        json,
    }))
}

fn plugin_execution_dto(
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

fn print_plugin_execution_result(result: &PluginToolExecutionResultDto) {
    println!(
        "plugin_tool plugin_id={} tool={} ok={} exit_code={}",
        result.plugin_id, result.tool, result.ok, result.exit_code
    );
    if let Some(value) = &result.result {
        println!("result={value}");
    }
    if let Some(error) = &result.error {
        println!("error={}", error.replace('\n', " "));
    }
    if !result.stdout.trim().is_empty() {
        println!("stdout={}", result.stdout.trim().replace('\n', "\\n"));
    }
    if !result.stderr.trim().is_empty() {
        println!("stderr={}", result.stderr.trim().replace('\n', "\\n"));
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
    let WindowsCommand::Run(args) = parse_windows_args(args)? else {
        print_windows_usage();
        return Err(CliError::HelpRequested);
    };
    let query = window_query_from_args(&args);

    if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::ListWindows {
                id: args.id.clone(),
                app: args.app.clone(),
                title: args.title.clone(),
                title_regex: args.title_regex.clone(),
                focused: args.focused,
                limit: args.limit,
                sort: Some(args.sort.name().to_owned()),
                backend: Some(args.backend.name().to_owned()),
                diagnose: args.diagnose,
            },
        )?;
        let ApiResult::ListWindows(metadata) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected windows response".to_owned(),
            ));
        };
        if args.json {
            print_json_pretty(&metadata)?;
        } else {
            print_window_dto_table(metadata, args.diagnose);
        }
        return Ok(());
    }

    let metadata = peekaboox_windows::list_windows_with_query(query)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    let metadata = window_list_dto(metadata);

    if args.json {
        print_json_pretty(&metadata)?;
        return Ok(());
    }

    for warning in &metadata.warnings {
        eprintln!("warning: {warning}");
    }

    if metadata.windows.is_empty() {
        println!("no windows found via {}", metadata.backend_name);
        if args.diagnose {
            print_window_backend_reports(&metadata.backend_reports);
        }
        return Ok(());
    }

    println!(
        "{:<14} {:<7} {:<10} {:<11} {:<11} {:<18} TITLE",
        "ID", "FOCUS", "STATE", "POSITION", "SIZE", "APP"
    );

    for window in metadata.windows {
        print_window_dto(window);
    }

    if args.diagnose {
        print_window_backend_reports(&metadata.backend_reports);
    }

    Ok(())
}

fn window_query_from_args(args: &WindowsArgs) -> peekaboox_windows::WindowQuery {
    peekaboox_windows::WindowQuery {
        id: args.id.clone(),
        app: args.app.clone(),
        title: args.title.clone(),
        title_regex: args.title_regex.clone(),
        focused_only: args.focused,
        limit: args.limit,
        sort: args.sort,
        backend: args.backend,
        diagnose: args.diagnose,
    }
}

fn print_window_dto_table(metadata: WindowListResultDto, diagnose: bool) {
    for warning in &metadata.warnings {
        eprintln!("warning: {warning}");
    }

    if metadata.windows.is_empty() {
        println!("no windows found via {}", metadata.backend_name);
        if diagnose {
            print_window_backend_reports(&metadata.backend_reports);
        }
        return;
    }

    println!(
        "{:<14} {:<7} {:<10} {:<11} {:<11} {:<18} TITLE",
        "ID", "FOCUS", "STATE", "POSITION", "SIZE", "APP"
    );

    for window in metadata.windows {
        print_window_dto(window);
    }

    if diagnose {
        print_window_backend_reports(&metadata.backend_reports);
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

fn print_window_backend_reports(reports: &[WindowBackendReportDto]) {
    if reports.is_empty() {
        return;
    }

    println!();
    println!(
        "{:<24} {:<10} {:<5} {:<7} {:<8} ERROR",
        "BACKEND", "KIND", "RAW", "MATCH", "SELECTED"
    );

    for report in reports {
        println!(
            "{:<24} {:<10} {:<5} {:<7} {:<8} {}",
            report.backend_name,
            report.backend_kind,
            report.raw_window_count,
            report.matched_window_count,
            if report.selected { "yes" } else { "no" },
            report.error.as_deref().unwrap_or("-")
        );
    }
}

fn window_list_dto(metadata: peekaboox_windows::WindowListMetadata) -> WindowListResultDto {
    WindowListResultDto {
        backend_name: metadata.backend_name,
        backend_kind: backend_kind_label(metadata.backend_kind),
        warnings: metadata.warnings,
        backend_reports: metadata
            .backend_reports
            .into_iter()
            .map(|report| WindowBackendReportDto {
                backend_name: report.backend_name,
                backend_kind: backend_kind_label(report.backend_kind),
                raw_window_count: report.raw_window_count,
                matched_window_count: report.matched_window_count,
                selected: report.selected,
                error: report.error,
            })
            .collect(),
        windows: metadata
            .windows
            .into_iter()
            .map(|window| WindowDto {
                id: window.id,
                title: window.title,
                app_id: window.app_id,
                bounds: window.bounds.into(),
                focused: window.focused,
                state: format!("{:?}", window.state).to_ascii_lowercase(),
            })
            .collect(),
    }
}

fn parse_windows_args(args: Vec<String>) -> Result<WindowsCommand, CliError> {
    let mut json = false;
    let mut id = None;
    let mut app = None;
    let mut title = None;
    let mut title_regex = None;
    let mut focused = false;
    let mut limit = None;
    let mut sort = peekaboox_windows::WindowSort::Backend;
    let mut backend = peekaboox_windows::WindowBackendSelection::Auto;
    let mut diagnose = false;
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--id" => id = Some(require_next_arg(&mut iter, "--id")?),
            "--app" => app = Some(require_next_arg(&mut iter, "--app")?),
            "--title" => title = Some(require_next_arg(&mut iter, "--title")?),
            "--title-regex" => title_regex = Some(require_next_arg(&mut iter, "--title-regex")?),
            "--focused" => focused = true,
            "--limit" => {
                let value = require_next_arg(&mut iter, "--limit")?;
                limit = Some(parse_positive_usize("--limit", &value)?);
            }
            "--sort" => {
                let value = require_next_arg(&mut iter, "--sort")?;
                sort = peekaboox_windows::WindowSort::from_name(&value).ok_or_else(|| {
                    CliError::Failure(format!(
                        "invalid windows sort: {value}; expected backend, focused, title, app, area, id, or state"
                    ))
                })?;
            }
            "--backend" => {
                let value = require_next_arg(&mut iter, "--backend")?;
                backend = peekaboox_windows::WindowBackendSelection::from_name(&value).ok_or_else(
                    || {
                        CliError::Failure(format!(
                            "invalid windows backend: {value}; expected auto, gnome, at-spi, or xdotool"
                        ))
                    },
                )?;
            }
            "--diagnose" => diagnose = true,
            "--help" | "-h" => return Ok(WindowsCommand::Help),
            unknown => {
                return Err(CliError::Failure(format!(
                    "unknown windows argument: {unknown}"
                )));
            }
        }
    }
    Ok(WindowsCommand::Run(WindowsArgs {
        json,
        id,
        app,
        title,
        title_regex,
        focused,
        limit,
        sort,
        backend,
        diagnose,
    }))
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
                app: args.app.clone(),
                window_title: args.window_title.clone(),
                window_id: args.window_id.clone(),
                vision_region: args.vision_region.map(RectDto::from),
                vision_edge_threshold: args.vision_edge_threshold,
                vision_min_width: args.vision_min_width,
                vision_min_height: args.vision_min_height,
                vision_min_component_pixels: args.vision_min_component_pixels,
                vision_max_elements: args.vision_max_elements,
                vision_merge_distance: args.vision_merge_distance,
            },
        )?;
        let ApiResult::FindElements(metadata) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected elements response".to_owned(),
            ));
        };
        if args.json {
            print_json_pretty(&limited_element_list_dto(metadata, args.limit))?;
        } else {
            print_element_dto_table(metadata, args.limit);
        }
        return Ok(());
    }

    let metadata = find_elements_metadata(&args)?;
    if args.json {
        print_json_pretty(&limited_element_list_dto(
            element_list_dto(metadata),
            args.limit,
        ))?;
    } else {
        print_element_table(metadata, args.limit);
    }

    Ok(())
}

fn find_elements_metadata(args: &ElementsArgs) -> Result<AccessibilityTreeMetadata, CliError> {
    let query = ElementQuery::parse(&args.selector)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    match peekaboox_accessibility::semantic_tree() {
        Ok(mut metadata) => {
            metadata.elements.retain(|element| {
                query.matches(element) && element_matches_cli_scope(element, args)
            });
            if !metadata.elements.is_empty() || !args.vision_fallback {
                return Ok(metadata);
            }

            let mut fallback = vision_fallback_metadata(&query, args)?;
            fallback
                .warnings
                .push("no accessibility elements matched; used vision fallback".to_owned());
            Ok(fallback)
        }
        Err(error) if args.vision_fallback => {
            let mut fallback = vision_fallback_metadata(&query, args)?;
            fallback.warnings.push(format!(
                "accessibility lookup failed: {error}; used vision fallback"
            ));
            Ok(fallback)
        }
        Err(error) => Err(CliError::Failure(error.to_string())),
    }
}

fn vision_fallback_metadata(
    query: &ElementQuery,
    args: &ElementsArgs,
) -> Result<AccessibilityTreeMetadata, CliError> {
    let screenshot = vision_fallback_temp_path();
    let capture_region = vision_capture_region_from_elements_args(args)?;
    capture_cli_to_file(&screenshot, capture_region)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    let options = element_vision_options_from_elements_args(args)?;
    let result = peekaboox_vision::detect_ui_elements_from_image_file(&screenshot, &options)
        .map_err(|error| CliError::Failure(error.to_string()));
    remove_temp_file(&screenshot, "vision fallback screenshot");

    let mut elements = result?;
    apply_elements_scope_metadata(&mut elements, args, capture_region);
    elements.retain(|element| query.matches(element));
    Ok(AccessibilityTreeMetadata {
        backend_name: "heuristic_vision".to_owned(),
        backend_kind: BackendKind::Vision,
        warnings: Vec::new(),
        elements,
    })
}

fn element_matches_cli_scope(element: &UiElement, args: &ElementsArgs) -> bool {
    args.window_id
        .as_deref()
        .is_none_or(|window_id| element.window_id.as_deref() == Some(window_id))
        && args.window_title.as_deref().is_none_or(|window_title| {
            element
                .window_title
                .as_deref()
                .is_some_and(|value| contains_case_insensitive(value, window_title))
        })
        && args.app.as_deref().is_none_or(|app| {
            element
                .app_id
                .as_deref()
                .is_some_and(|value| contains_case_insensitive(value, app))
        })
}

fn vision_capture_region_from_elements_args(args: &ElementsArgs) -> Result<Option<Rect>, CliError> {
    if args.vision_region.is_some() {
        return Ok(args.vision_region);
    }
    if args.window_id.is_none() && args.window_title.is_none() && args.app.is_none() {
        return Ok(None);
    }
    let window = resolve_window_for_ocr(
        args.window_id.as_deref(),
        args.window_title.as_deref(),
        args.app.as_deref(),
    )?;
    Ok(Some(window.bounds))
}

fn element_vision_options_from_elements_args(
    args: &ElementsArgs,
) -> Result<UiElementDetectionOptions, CliError> {
    let defaults = UiElementDetectionOptions::default();
    Ok(UiElementDetectionOptions {
        region: None,
        edge_threshold: args
            .vision_edge_threshold
            .unwrap_or(defaults.edge_threshold),
        min_width: args.vision_min_width.unwrap_or(defaults.min_width),
        min_height: args.vision_min_height.unwrap_or(defaults.min_height),
        min_component_pixels: args
            .vision_min_component_pixels
            .unwrap_or(defaults.min_component_pixels),
        max_elements: args
            .vision_max_elements
            .map(usize::try_from)
            .transpose()
            .map_err(|_| CliError::Failure("--vision-max-elements is too large".to_owned()))?
            .unwrap_or(defaults.max_elements),
        merge_distance: args
            .vision_merge_distance
            .unwrap_or(defaults.merge_distance),
        ..defaults
    })
}

fn apply_elements_scope_metadata(
    elements: &mut [UiElement],
    args: &ElementsArgs,
    capture_region: Option<Rect>,
) {
    for element in elements {
        if let Some(region) = capture_region {
            element.bounds.x += region.x;
            element.bounds.y += region.y;
            element.center = element.bounds.center();
        }
        element.window_id.clone_from(&args.window_id);
        element.window_title.clone_from(&args.window_title);
        element.app_id.clone_from(&args.app);
    }
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

fn element_list_dto(metadata: AccessibilityTreeMetadata) -> ElementListResultDto {
    ElementListResultDto {
        backend_name: metadata.backend_name,
        backend_kind: backend_kind_label(metadata.backend_kind),
        warnings: metadata.warnings,
        cache_hit: false,
        cache_age_ms: 0,
        vision_fallback_used: metadata.backend_kind == BackendKind::Vision,
        elements: metadata.elements.into_iter().map(element_dto).collect(),
    }
}

fn limited_element_list_dto(
    mut metadata: ElementListResultDto,
    limit: usize,
) -> ElementListResultDto {
    if limit > 0 && metadata.elements.len() > limit {
        metadata.elements.truncate(limit);
    }
    metadata
}

fn element_dto(element: UiElement) -> ElementDto {
    ElementDto {
        id: element.id,
        role: element.role,
        label: element.label,
        bounds: element.bounds.into(),
        center: element
            .center
            .or_else(|| element.bounds.center())
            .map(Into::into),
        confidence: element.confidence,
        states: element.states,
        window_id: element.window_id,
        window_title: element.window_title,
        app_id: element.app_id,
        parent_id: element.parent_id,
        child_ids: element.child_ids,
    }
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
    let mut app = None;
    let mut window_title = None;
    let mut window_id = None;
    let mut vision_region = None;
    let mut vision_edge_threshold = None;
    let mut vision_min_width = None;
    let mut vision_min_height = None;
    let mut vision_min_component_pixels = None;
    let mut vision_max_elements = None;
    let mut vision_merge_distance = None;
    let mut json = false;
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
            "--id" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --id".to_owned()));
                };
                selector_parts.push(format!("id={value}"));
            }
            "--role" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --role".to_owned()));
                };
                selector_parts.push(format!("role={value}"));
            }
            "--role-exact" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --role-exact".to_owned(),
                    ));
                };
                selector_parts.push(format!("role-exact={value}"));
            }
            "--role-regex" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --role-regex".to_owned(),
                    ));
                };
                selector_parts.push(format!("role-regex={value}"));
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
            "--label-exact" | "--text-exact" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(format!(
                        "missing value for {}",
                        args[index - 1]
                    )));
                };
                selector_parts.push(format!("label-exact={value}"));
            }
            "--label-regex" | "--text-regex" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(format!(
                        "missing value for {}",
                        args[index - 1]
                    )));
                };
                selector_parts.push(format!("label-regex={value}"));
            }
            "--state" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --state".to_owned()));
                };
                selector_parts.push(format!("state={value}"));
            }
            "--not-state" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --not-state".to_owned(),
                    ));
                };
                selector_parts.push(format!("not-state={value}"));
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
            "--within" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --within".to_owned()));
                };
                selector_parts.push(format!("within={value}"));
            }
            "--intersects" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --intersects".to_owned(),
                    ));
                };
                selector_parts.push(format!("intersects={value}"));
            }
            "--min-width" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --min-width".to_owned(),
                    ));
                };
                parse_positive_u32("--min-width", value)?;
                selector_parts.push(format!("min-width={value}"));
            }
            "--min-height" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --min-height".to_owned(),
                    ));
                };
                parse_positive_u32("--min-height", value)?;
                selector_parts.push(format!("min-height={value}"));
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
            "--app" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --app".to_owned()));
                };
                app = non_empty_cli_string(value);
            }
            "--window-title" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --window-title".to_owned(),
                    ));
                };
                window_title = non_empty_cli_string(value);
            }
            "--window-id" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --window-id".to_owned(),
                    ));
                };
                window_id = non_empty_cli_string(value);
            }
            "--vision-region" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --vision-region".to_owned(),
                    ));
                };
                vision_region = Some(parse_rect("--vision-region", value)?);
            }
            "--vision-threshold" | "--vision-edge-threshold" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --vision-threshold".to_owned(),
                    ));
                };
                let threshold = parse_u8("--vision-threshold", value)?;
                if threshold == 0 {
                    return Err(CliError::Failure(
                        "--vision-threshold must be greater than zero".to_owned(),
                    ));
                }
                vision_edge_threshold = Some(threshold);
            }
            "--vision-min-width" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --vision-min-width".to_owned(),
                    ));
                };
                vision_min_width = Some(parse_positive_u32("--vision-min-width", value)?);
            }
            "--vision-min-height" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --vision-min-height".to_owned(),
                    ));
                };
                vision_min_height = Some(parse_positive_u32("--vision-min-height", value)?);
            }
            "--vision-min-component-pixels" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --vision-min-component-pixels".to_owned(),
                    ));
                };
                vision_min_component_pixels =
                    Some(parse_positive_u32("--vision-min-component-pixels", value)?);
            }
            "--vision-max-elements" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --vision-max-elements".to_owned(),
                    ));
                };
                vision_max_elements = Some(parse_positive_u32("--vision-max-elements", value)?);
            }
            "--vision-merge-distance" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --vision-merge-distance".to_owned(),
                    ));
                };
                vision_merge_distance = Some(value.parse::<u32>().map_err(|_| {
                    CliError::Failure(format!(
                        "--vision-merge-distance must be an integer, got {value:?}"
                    ))
                })?);
            }
            "--json" => json = true,
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
    ElementQuery::parse(&selector).map_err(|error| CliError::Failure(error.to_string()))?;

    Ok(ElementsCommand::Run(ElementsArgs {
        selector,
        limit,
        vision_fallback,
        app,
        window_title,
        window_id,
        vision_region,
        vision_edge_threshold,
        vision_min_width,
        vision_min_height,
        vision_min_component_pixels,
        vision_max_elements,
        vision_merge_distance,
        json,
    }))
}

fn ocr(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let OcrCommand::Run(args) = parse_ocr_args(args)? else {
        print_ocr_usage();
        return Err(CliError::HelpRequested);
    };
    let args = *args;

    if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::Ocr {
                image_path: args.image.as_ref().map(|path| path.display().to_string()),
                region: args.region.map(RectDto::from),
                app: args.app.clone(),
                window_title: args.window_title.clone(),
                window_id: args.window_id.clone(),
                language: args.language.clone(),
                page_segmentation_mode: args.page_segmentation_mode,
                engine_mode: args.engine_mode,
                dpi: args.dpi,
                min_confidence: args.min_confidence,
                whitelist: args.whitelist.clone(),
                config: args
                    .config
                    .iter()
                    .map(|config| format!("{}={}", config.key, config.value))
                    .collect(),
                scale: args.preprocessing.scale,
                grayscale: args.preprocessing.grayscale,
                threshold: args.preprocessing.threshold,
                invert: args.preprocessing.invert,
                contrast: args.preprocessing.contrast,
                deskew: args.preprocessing.deskew,
            },
        )?;
        let ApiResult::Ocr(result) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected OCR response".to_owned(),
            ));
        };
        if args.json {
            print_json_pretty(&result)?;
        } else if args.words {
            print_ocr_dto_words(result);
        } else {
            print_ocr_dto_result(result);
        }
        return Ok(());
    }

    let backend = TesseractOcrBackend::new("tesseract", ocr_options(&args));
    if !backend.is_available() {
        return Err(CliError::Failure(
            "OCR backend tesseract is not available; install tesseract-ocr".to_owned(),
        ));
    }

    let result = if let Some(image) = args.image.as_ref() {
        peekaboox_vision::ocr_image_file_with_backend(&backend, image, args.region)
    } else {
        let region = ocr_capture_region_from_args(&args)?;
        match region {
            Some(region) => peekaboox_vision::ocr_region_with_backend(&backend, region),
            None => peekaboox_vision::ocr_screen_with_backend(&backend),
        }
    }
    .map_err(|error| CliError::Failure(error.to_string()))?;
    if args.json {
        print_json_pretty(&ocr_result_dto(result))?;
    } else if args.words {
        print_ocr_words(result);
    } else {
        print_ocr_result(result);
    }

    Ok(())
}

fn parse_ocr_args(args: Vec<String>) -> Result<OcrCommand, CliError> {
    let mut image = None;
    let mut region = None;
    let mut app = None;
    let mut window_title = None;
    let mut window_id = None;
    let mut language = None;
    let mut page_segmentation_mode = None;
    let mut engine_mode = None;
    let mut dpi = None;
    let mut min_confidence = None;
    let mut whitelist = None;
    let mut config = Vec::new();
    let mut preprocessing = OcrPreprocessingOptions::default();
    let mut json = false;
    let mut words = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--image" | "-i" => {
                image = Some(PathBuf::from(parse_next_string(
                    &args, &mut index, "--image",
                )?));
            }
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
            "--app" | "-a" => app = Some(parse_next_string(&args, &mut index, "--app")?),
            "--window-title" | "--title" => {
                window_title = Some(parse_next_string(&args, &mut index, "--window-title")?)
            }
            "--window-id" | "--window" => {
                window_id = Some(parse_next_string(&args, &mut index, "--window-id")?)
            }
            "--psm" | "--page-segmentation-mode" => {
                page_segmentation_mode = Some(parse_ocr_psm(&parse_next_string(
                    &args, &mut index, "--psm",
                )?)?);
            }
            "--oem" | "--engine-mode" => {
                engine_mode = Some(parse_ocr_oem(&parse_next_string(
                    &args, &mut index, "--oem",
                )?)?);
            }
            "--dpi" => {
                dpi = Some(parse_positive_u32(
                    "--dpi",
                    &parse_next_string(&args, &mut index, "--dpi")?,
                )?)
            }
            "--min-confidence" => {
                min_confidence = Some(parse_ocr_confidence(&parse_next_string(
                    &args,
                    &mut index,
                    "--min-confidence",
                )?)?);
            }
            "--whitelist" => {
                whitelist =
                    non_empty_cli_string(&parse_next_string(&args, &mut index, "--whitelist")?);
            }
            "--config" | "-c" => {
                config.push(parse_ocr_config(&parse_next_string(
                    &args, &mut index, "--config",
                )?)?);
            }
            "--scale" => {
                preprocessing.scale = Some(parse_ocr_scale(&parse_next_string(
                    &args, &mut index, "--scale",
                )?)?);
            }
            "--grayscale" | "--greyscale" => preprocessing.grayscale = true,
            "--threshold" => {
                preprocessing.threshold = Some(parse_u8(
                    "--threshold",
                    &parse_next_string(&args, &mut index, "--threshold")?,
                )?);
            }
            "--invert" => preprocessing.invert = true,
            "--contrast" => {
                preprocessing.contrast = Some(parse_ocr_contrast(&parse_next_string(
                    &args,
                    &mut index,
                    "--contrast",
                )?)?);
            }
            "--deskew" => preprocessing.deskew = true,
            "--words" => words = true,
            "--json" => json = true,
            "--help" | "-h" => return Ok(OcrCommand::Help),
            unknown => {
                return Err(CliError::Failure(format!(
                    "unknown ocr argument: {unknown}"
                )));
            }
        }

        index += 1;
    }

    Ok(OcrCommand::Run(Box::new(OcrArgs {
        image,
        region,
        app,
        window_title,
        window_id,
        language,
        page_segmentation_mode,
        engine_mode,
        dpi,
        min_confidence,
        whitelist,
        config,
        preprocessing,
        json,
        words,
    })))
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

fn print_ocr_words(result: OcrResult) {
    for warning in result.warnings {
        eprintln!("warning: {warning}");
    }

    if result.words.is_empty() {
        println!("no OCR words found via {}", result.backend_name);
        return;
    }

    println!("{:<5} {:<11} {:<11} TEXT", "CONF", "POSITION", "SIZE");
    for word in result.words {
        println!(
            "{:<5} {:<11} {:<11} {}",
            format!("{:.2}", word.element.confidence),
            format!("{},{}", word.element.bounds.x, word.element.bounds.y),
            format!(
                "{}x{}",
                word.element.bounds.width, word.element.bounds.height
            ),
            word.text
        );
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

fn print_ocr_dto_words(result: OcrResultDto) {
    for warning in result.warnings {
        eprintln!("warning: {warning}");
    }

    if result.words.is_empty() {
        println!("no OCR words found via {}", result.backend_name);
        return;
    }

    println!("{:<5} {:<11} {:<11} TEXT", "CONF", "POSITION", "SIZE");
    for word in result.words {
        println!(
            "{:<5} {:<11} {:<11} {}",
            format!("{:.2}", word.element.confidence),
            format!("{},{}", word.element.bounds.x, word.element.bounds.y),
            format!(
                "{}x{}",
                word.element.bounds.width, word.element.bounds.height
            ),
            word.text
        );
    }
}

fn ocr_result_dto(result: OcrResult) -> OcrResultDto {
    OcrResultDto {
        backend_name: result.backend_name,
        text: result.text,
        blocks: result
            .blocks
            .into_iter()
            .map(|block| OcrBlockDto {
                text: block.text,
                element: element_dto(block.element),
            })
            .collect(),
        words: result
            .words
            .into_iter()
            .map(|word| OcrBlockDto {
                text: word.text,
                element: element_dto(word.element),
            })
            .collect(),
        warnings: result.warnings,
    }
}

fn ocr_options(args: &OcrArgs) -> OcrOptions {
    let mut options = OcrOptions::default();
    if let Some(language) = args.language.clone() {
        options.language = Some(language);
    }
    if let Some(psm) = args.page_segmentation_mode {
        options.page_segmentation_mode = Some(psm);
    }
    if let Some(oem) = args.engine_mode {
        options.engine_mode = Some(oem);
    }
    if let Some(dpi) = args.dpi {
        options.dpi = Some(dpi);
    }
    if let Some(min_confidence) = args.min_confidence {
        options.min_confidence = min_confidence;
    }
    if let Some(whitelist) = args.whitelist.clone() {
        options.whitelist = Some(whitelist);
    }
    options.config = args.config.clone();
    options.preprocessing = args.preprocessing.clone();
    options
}

fn ocr_capture_region_from_args(args: &OcrArgs) -> Result<Option<Rect>, CliError> {
    if args.window_id.is_none() && args.window_title.is_none() && args.app.is_none() {
        return Ok(args.region);
    }
    let window = resolve_window_for_ocr(
        args.window_id.as_deref(),
        args.window_title.as_deref(),
        args.app.as_deref(),
    )?;
    if window.bounds.width == 0 || window.bounds.height == 0 {
        return Err(CliError::Failure(format!(
            "window {} has empty bounds",
            window.id
        )));
    }
    let region = match args.region {
        Some(region) => offset_region(window.bounds, region)?,
        None => window.bounds,
    };
    Ok(Some(region))
}

fn resolve_window_for_ocr(
    window_id: Option<&str>,
    window_title: Option<&str>,
    app: Option<&str>,
) -> Result<peekaboox_core::WindowInfo, CliError> {
    let metadata =
        peekaboox_windows::list_windows().map_err(|error| CliError::Failure(error.to_string()))?;
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
        return Err(CliError::Failure(
            "no window matched OCR window filters".to_owned(),
        ));
    }
    matches.sort_by_key(|window| !window.focused);
    Ok(matches.remove(0))
}

fn offset_region(origin: Rect, region: Rect) -> Result<Rect, CliError> {
    let x = i64::from(origin.x) + i64::from(region.x);
    let y = i64::from(origin.y) + i64::from(region.y);
    let x = i32::try_from(x)
        .map_err(|_| CliError::Failure("OCR region x coordinate overflows i32".to_owned()))?;
    let y = i32::try_from(y)
        .map_err(|_| CliError::Failure("OCR region y coordinate overflows i32".to_owned()))?;
    Ok(Rect::new(x, y, region.width, region.height))
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
                ignore_regions: args
                    .ignore_regions
                    .iter()
                    .copied()
                    .map(RectDto::from)
                    .collect(),
                per_channel_threshold: args.per_channel_threshold,
                max_changed_ratio: args.max_changed_ratio,
                max_changed_pixels: args.max_changed_pixels,
                max_mean_absolute_error: args.max_mean_absolute_error,
                max_channel_delta: args.max_channel_delta,
                size_policy: visual_size_policy_name(args.size_policy).to_owned(),
                alpha_mode: visual_alpha_mode_name(args.alpha_mode).to_owned(),
                diff_output: args
                    .diff_output
                    .as_ref()
                    .map(|path| path.display().to_string()),
            },
        )?;
        let ApiResult::VisualDiff(result) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected visual diff response".to_owned(),
            ));
        };
        if args.json {
            print_json_pretty(&result)?;
        } else {
            print_visual_diff_dto(&result);
        }
        if let Some(report) = &args.report {
            write_json_pretty_file(report, &result)?;
        }
        return if args.no_fail {
            Ok(())
        } else {
            visual_diff_exit_status(result.matches)
        };
    }

    let options = visual_compare_options(&args);
    let result = if let Some(diff_output) = &args.diff_output {
        peekaboox_vision::write_visual_diff_image_file(
            &args.expected,
            &args.actual,
            diff_output,
            &options,
        )
    } else {
        peekaboox_vision::compare_image_files(&args.expected, &args.actual, &options)
    }
    .map_err(|error| CliError::Failure(error.to_string()))?;
    let result_dto = visual_diff_dto(&result);
    if args.json {
        print_json_pretty(&result_dto)?;
    } else {
        print_visual_diff(&result);
    }
    if let Some(report) = &args.report {
        write_json_pretty_file(report, &result_dto)?;
    }
    if args.no_fail {
        Ok(())
    } else {
        visual_diff_exit_status(result.matches)
    }
}

fn parse_compare_args(args: Vec<String>) -> Result<CompareCommand, CliError> {
    let mut expected = None;
    let mut actual = None;
    let mut region = None;
    let mut ignore_regions = Vec::new();
    let mut per_channel_threshold = 0_u8;
    let mut max_changed_ratio = 0.0_f32;
    let mut max_changed_pixels = None;
    let mut max_mean_absolute_error = None;
    let mut max_channel_delta = None;
    let mut size_policy = VisualSizePolicy::Error;
    let mut alpha_mode = VisualAlphaMode::Ignore;
    let mut diff_output = None;
    let mut report = None;
    let mut no_fail = false;
    let mut json = false;
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
            "--ignore-region" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --ignore-region".to_owned(),
                    ));
                };
                ignore_regions.push(parse_rect("--ignore-region", value)?);
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
            "--max-changed-pixels" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --max-changed-pixels".to_owned(),
                    ));
                };
                max_changed_pixels = Some(parse_u64("--max-changed-pixels", value)?);
            }
            "--max-mae" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --max-mae".to_owned()));
                };
                max_mean_absolute_error = Some(parse_visual_mae("--max-mae", value)?);
            }
            "--max-channel-delta" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --max-channel-delta".to_owned(),
                    ));
                };
                max_channel_delta = Some(parse_u8("--max-channel-delta", value)?);
            }
            "--size-policy" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --size-policy".to_owned(),
                    ));
                };
                size_policy = parse_visual_size_policy(value)?;
            }
            "--alpha" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --alpha".to_owned()));
                };
                alpha_mode = parse_visual_alpha_mode(value)?;
            }
            "--diff-output" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --diff-output".to_owned(),
                    ));
                };
                diff_output = Some(PathBuf::from(value));
            }
            "--report" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --report".to_owned()));
                };
                report = Some(PathBuf::from(value));
            }
            "--no-fail" => no_fail = true,
            "--json" => json = true,
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
        ignore_regions,
        per_channel_threshold,
        max_changed_ratio,
        max_changed_pixels,
        max_mean_absolute_error,
        max_channel_delta,
        size_policy,
        alpha_mode,
        diff_output,
        report,
        no_fail,
        json,
    }))
}

fn visual_compare_options(args: &CompareArgs) -> VisualCompareOptions {
    VisualCompareOptions {
        region: args.region,
        ignore_regions: args.ignore_regions.clone(),
        per_channel_threshold: args.per_channel_threshold,
        max_changed_ratio: args.max_changed_ratio,
        max_changed_pixels: args.max_changed_pixels,
        max_mean_absolute_error: args.max_mean_absolute_error,
        max_channel_delta: args.max_channel_delta,
        size_policy: args.size_policy,
        alpha_mode: args.alpha_mode,
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

fn visual_diff_dto(result: &VisualDiffResult) -> VisualDiffDto {
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
                ignore_regions: args
                    .ignore_regions
                    .iter()
                    .copied()
                    .map(RectDto::from)
                    .collect(),
                per_channel_threshold: args.per_channel_threshold,
                stable_max_changed_ratio: args.stable_max_changed_ratio,
                stable_max_changed_pixels: args.stable_max_changed_pixels,
                stable_max_mean_absolute_error: args.stable_max_mean_absolute_error,
                stable_max_channel_delta: args.stable_max_channel_delta,
                loading_min_changed_ratio: args.loading_min_changed_ratio,
                loading_min_changed_pixels: args.loading_min_changed_pixels,
                required_stable_transitions: args.required_stable_transitions,
                size_policy: visual_size_policy_name(args.size_policy).to_owned(),
                alpha_mode: visual_alpha_mode_name(args.alpha_mode).to_owned(),
            },
        )?;
        let ApiResult::UiState(result) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected UI state response".to_owned(),
            ));
        };
        if args.json {
            print_json_pretty(&result)?;
        } else {
            print_ui_state_dto(&result);
        }
        return Ok(());
    }

    let options = ui_state_options(&args);
    let result = peekaboox_vision::detect_ui_state_from_image_files(&args.image_paths, &options)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    if args.json {
        print_json_pretty(&ui_state_dto(&result))?;
    } else {
        print_ui_state(&result);
    }
    Ok(())
}

fn parse_ui_state_args(args: Vec<String>) -> Result<UiStateCommand, CliError> {
    let mut image_paths = Vec::new();
    let mut region = None;
    let mut ignore_regions = Vec::new();
    let mut per_channel_threshold = UiStateOptions::default().per_channel_threshold;
    let mut stable_max_changed_ratio = UiStateOptions::default().stable_max_changed_ratio;
    let mut stable_max_changed_pixels = None;
    let mut stable_max_mean_absolute_error = None;
    let mut stable_max_channel_delta = None;
    let mut loading_min_changed_ratio = UiStateOptions::default().loading_min_changed_ratio;
    let mut loading_min_changed_pixels = None;
    let mut required_stable_transitions =
        u32::try_from(UiStateOptions::default().required_stable_transitions).unwrap_or(1);
    let mut size_policy = VisualSizePolicy::Error;
    let mut alpha_mode = VisualAlphaMode::Ignore;
    let mut json = false;
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
            "--ignore-region" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --ignore-region".to_owned(),
                    ));
                };
                ignore_regions.push(parse_rect("--ignore-region", value)?);
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
            "--stable-max-changed-pixels" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --stable-max-changed-pixels".to_owned(),
                    ));
                };
                stable_max_changed_pixels = Some(parse_u64("--stable-max-changed-pixels", value)?);
            }
            "--stable-max-mae" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --stable-max-mae".to_owned(),
                    ));
                };
                stable_max_mean_absolute_error = Some(parse_visual_mae("--stable-max-mae", value)?);
            }
            "--stable-max-channel-delta" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --stable-max-channel-delta".to_owned(),
                    ));
                };
                stable_max_channel_delta = Some(parse_u8("--stable-max-channel-delta", value)?);
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
            "--loading-min-changed-pixels" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --loading-min-changed-pixels".to_owned(),
                    ));
                };
                loading_min_changed_pixels =
                    Some(parse_u64("--loading-min-changed-pixels", value)?);
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
            "--size-policy" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --size-policy".to_owned(),
                    ));
                };
                size_policy = parse_visual_size_policy(value)?;
            }
            "--alpha" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --alpha".to_owned()));
                };
                alpha_mode = parse_visual_alpha_mode(value)?;
            }
            "--json" => json = true,
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
    if let (Some(stable_max), Some(loading_min)) =
        (stable_max_changed_pixels, loading_min_changed_pixels)
        && stable_max > loading_min
    {
        return Err(CliError::Failure(
            "--stable-max-changed-pixels must be less than or equal to --loading-min-changed-pixels"
                .to_owned(),
        ));
    }

    Ok(UiStateCommand::Run(UiStateArgs {
        image_paths,
        region,
        ignore_regions,
        per_channel_threshold,
        stable_max_changed_ratio,
        stable_max_changed_pixels,
        stable_max_mean_absolute_error,
        stable_max_channel_delta,
        loading_min_changed_ratio,
        loading_min_changed_pixels,
        required_stable_transitions,
        size_policy,
        alpha_mode,
        json,
    }))
}

fn ui_state_options(args: &UiStateArgs) -> UiStateOptions {
    UiStateOptions {
        region: args.region,
        ignore_regions: args.ignore_regions.clone(),
        per_channel_threshold: args.per_channel_threshold,
        stable_max_changed_ratio: args.stable_max_changed_ratio,
        stable_max_changed_pixels: args.stable_max_changed_pixels,
        stable_max_mean_absolute_error: args.stable_max_mean_absolute_error,
        stable_max_channel_delta: args.stable_max_channel_delta,
        loading_min_changed_ratio: args.loading_min_changed_ratio,
        loading_min_changed_pixels: args.loading_min_changed_pixels,
        required_stable_transitions: usize::try_from(args.required_stable_transitions).unwrap_or(1),
        size_policy: args.size_policy,
        alpha_mode: args.alpha_mode,
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

fn ui_state_dto(result: &UiStateResult) -> UiStateDto {
    UiStateDto {
        state: format!("{:?}", result.state).to_ascii_lowercase(),
        compared_transitions: u64::try_from(result.compared_transitions).unwrap_or(u64::MAX),
        stable_transitions: u64::try_from(result.stable_transitions).unwrap_or(u64::MAX),
        loading_transitions: u64::try_from(result.loading_transitions).unwrap_or(u64::MAX),
        trailing_stable_transitions: u64::try_from(result.trailing_stable_transitions)
            .unwrap_or(u64::MAX),
        latest_diff: visual_diff_dto(&result.latest_diff),
        max_changed_ratio: result.max_changed_ratio,
        mean_changed_ratio: result.mean_changed_ratio,
        changed_bounds: result.changed_bounds.map(Into::into),
    }
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
                ignore_regions: args
                    .ignore_regions
                    .iter()
                    .copied()
                    .map(RectDto::from)
                    .collect(),
                edge_threshold: args.edge_threshold,
                min_width: args.min_width,
                min_height: args.min_height,
                min_component_pixels: args.min_component_pixels,
                min_confidence: args.min_confidence,
                max_width: args.max_width,
                max_height: args.max_height,
                min_area: args.min_area,
                max_area: args.max_area,
                max_elements: args.max_elements,
                merge_distance: args.merge_distance,
                padding: args.padding,
                sort: ui_element_sort_name(args.sort).to_owned(),
                mask_output_path: args
                    .mask_output
                    .as_ref()
                    .map(|path| path.display().to_string()),
                overlay_output_path: args
                    .overlay_output
                    .as_ref()
                    .map(|path| path.display().to_string()),
            },
        )?;
        let ApiResult::DetectUiElements(metadata) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected vision elements response".to_owned(),
            ));
        };
        if args.json {
            print_json_pretty(&metadata)?;
        } else {
            print_element_dto_table(metadata, 0);
        }
        return Ok(());
    }

    let options = vision_element_options(&args)?;
    let elements = peekaboox_vision::detect_ui_elements_from_image_file_with_outputs(
        &args.image,
        &options,
        args.mask_output.as_deref(),
        args.overlay_output.as_deref(),
    )
    .map_err(|error| CliError::Failure(error.to_string()))?;
    let metadata = AccessibilityTreeMetadata {
        backend_name: "heuristic_vision".to_owned(),
        backend_kind: BackendKind::Vision,
        warnings: Vec::new(),
        elements,
    };
    if args.json {
        print_json_pretty(&element_list_dto(metadata))?;
    } else {
        print_element_table(metadata, 0);
    }
    Ok(())
}

fn parse_vision_elements_args(args: Vec<String>) -> Result<VisionElementsCommand, CliError> {
    let defaults = UiElementDetectionOptions::default();
    let mut image = None;
    let mut region = None;
    let mut ignore_regions = Vec::new();
    let mut edge_threshold = defaults.edge_threshold;
    let mut min_width = defaults.min_width;
    let mut min_height = defaults.min_height;
    let mut min_component_pixels = defaults.min_component_pixels;
    let mut min_confidence = defaults.min_confidence;
    let mut max_width = defaults.max_width;
    let mut max_height = defaults.max_height;
    let mut min_area = defaults.min_area;
    let mut max_area = defaults.max_area;
    let mut max_elements = u32::try_from(defaults.max_elements).unwrap_or(100);
    let mut merge_distance = defaults.merge_distance;
    let mut padding = defaults.padding;
    let mut sort = defaults.sort;
    let mut mask_output = None;
    let mut overlay_output = None;
    let mut json = false;
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
            "--ignore-region" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --ignore-region".to_owned(),
                    ));
                };
                ignore_regions.push(parse_rect("--ignore-region", value)?);
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
            "--min-confidence" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --min-confidence".to_owned(),
                    ));
                };
                min_confidence = Some(parse_unit_f32("--min-confidence", value)?);
            }
            "--max-width" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --max-width".to_owned(),
                    ));
                };
                max_width = Some(parse_positive_u32("--max-width", value)?);
            }
            "--max-height" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --max-height".to_owned(),
                    ));
                };
                max_height = Some(parse_positive_u32("--max-height", value)?);
            }
            "--min-area" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --min-area".to_owned()));
                };
                min_area = Some(parse_u64("--min-area", value)?);
                if min_area == Some(0) {
                    return Err(CliError::Failure(
                        "--min-area must be greater than zero".to_owned(),
                    ));
                }
            }
            "--max-area" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --max-area".to_owned()));
                };
                max_area = Some(parse_u64("--max-area", value)?);
                if max_area == Some(0) {
                    return Err(CliError::Failure(
                        "--max-area must be greater than zero".to_owned(),
                    ));
                }
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
            "--padding" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --padding".to_owned()));
                };
                padding = parse_u32("--padding", value)?;
            }
            "--sort" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --sort".to_owned()));
                };
                sort = parse_ui_element_sort(value)?;
            }
            "--mask-output" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --mask-output".to_owned(),
                    ));
                };
                mask_output = Some(PathBuf::from(value));
            }
            "--overlay-output" | "--debug-output" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --overlay-output".to_owned(),
                    ));
                };
                overlay_output = Some(PathBuf::from(value));
            }
            "--json" => json = true,
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
    if let Some(max_width) = max_width
        && min_width > max_width
    {
        return Err(CliError::Failure(
            "--min-width must be less than or equal to --max-width".to_owned(),
        ));
    }
    if let Some(max_height) = max_height
        && min_height > max_height
    {
        return Err(CliError::Failure(
            "--min-height must be less than or equal to --max-height".to_owned(),
        ));
    }
    if let (Some(min_area), Some(max_area)) = (min_area, max_area)
        && min_area > max_area
    {
        return Err(CliError::Failure(
            "--min-area must be less than or equal to --max-area".to_owned(),
        ));
    }

    Ok(VisionElementsCommand::Run(VisionElementsArgs {
        image,
        region,
        ignore_regions,
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
        sort,
        mask_output,
        overlay_output,
        json,
    }))
}

fn vision_element_options(
    args: &VisionElementsArgs,
) -> Result<UiElementDetectionOptions, CliError> {
    Ok(UiElementDetectionOptions {
        region: args.region,
        ignore_regions: args.ignore_regions.clone(),
        edge_threshold: args.edge_threshold,
        min_width: args.min_width,
        min_height: args.min_height,
        min_component_pixels: args.min_component_pixels,
        min_confidence: args.min_confidence,
        max_width: args.max_width,
        max_height: args.max_height,
        min_area: args.min_area,
        max_area: args.max_area,
        max_elements: usize::try_from(args.max_elements)
            .map_err(|_| CliError::Failure("--max-elements is too large".to_owned()))?,
        merge_distance: args.merge_distance,
        padding: args.padding,
        sort: args.sort,
    })
}

fn desktop(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let command = parse_desktop_args(args)?;
    if context.use_daemon {
        return desktop_daemon(command, context);
    }

    match command {
        DesktopCommand::Profiles(args) => {
            print_desktop_profiles(args)?;
            Ok(())
        }
        DesktopCommand::Focus(args) => {
            let result = peekaboox_desktop::focus_app(
                &args.app,
                &FocusOptions {
                    use_gnome_overview: args.use_gnome_overview,
                    launch_if_needed: args.launch_if_needed,
                    wait_after_focus_ms: args.wait_after_focus_ms,
                    overview_wait_ms: args.overview_wait_ms,
                    window_title: args.window_title,
                    window_id: args.window_id,
                    verify: args.verify,
                },
            )
            .map_err(|error| CliError::Failure(error.to_string()))?;
            if args.json {
                print_desktop_action_result_json(&result)?;
            } else {
                print_desktop_action_result(result);
            }
            Ok(())
        }
        DesktopCommand::Locate(args) => {
            let target = peekaboox_desktop::locate_target(
                &args.app,
                &args.target,
                &LocateOptions {
                    image: args.image,
                    prefer_accessibility: args.prefer_accessibility,
                    window_title: args.window_title,
                    window_id: args.window_id,
                },
            )
            .map_err(|error| CliError::Failure(error.to_string()))?;
            if args.json {
                print_desktop_locate_result_json(&target)?;
            } else {
                print_desktop_locate_result(target);
            }
            Ok(())
        }
        DesktopCommand::Click(args) => {
            let result = peekaboox_desktop::click_target(
                &args.app,
                &args.target,
                &DesktopClickOptions {
                    locate: LocateOptions {
                        image: args.image,
                        prefer_accessibility: args.prefer_accessibility,
                        window_title: args.window_title,
                        window_id: args.window_id,
                    },
                    button: args.button,
                    dry_run: args.dry_run,
                    verify: args.verify,
                },
            )
            .map_err(|error| CliError::Failure(error.to_string()))?;
            if args.json {
                print_desktop_action_result_json(&result)?;
            } else {
                print_desktop_action_result(result);
            }
            Ok(())
        }
        DesktopCommand::Drag(args) => {
            let result = peekaboox_desktop::drag_target(
                &args.app,
                &args.target,
                &DesktopDragOptions {
                    locate: LocateOptions {
                        image: args.image,
                        prefer_accessibility: args.prefer_accessibility,
                        window_title: args.window_title,
                        window_id: args.window_id,
                    },
                    from_ratio: args.from_ratio,
                    to_ratio: args.to_ratio,
                    button: args.button,
                    duration_ms: args.duration_ms,
                    dry_run: args.dry_run,
                    verify: args.verify,
                },
            )
            .map_err(|error| CliError::Failure(error.to_string()))?;
            if args.json {
                print_desktop_action_result_json(&result)?;
            } else {
                print_desktop_action_result(result);
            }
            Ok(())
        }
        DesktopCommand::TypeInto(args) => {
            let result = peekaboox_desktop::type_into_target(
                &args.app,
                &args.target,
                &args.text,
                &TypeIntoOptions {
                    locate: LocateOptions {
                        image: args.image,
                        prefer_accessibility: args.prefer_accessibility,
                        window_title: args.window_title,
                        window_id: args.window_id,
                    },
                    clear: args.clear,
                    dry_run: args.dry_run,
                    verify: args.verify,
                },
            )
            .map_err(|error| CliError::Failure(error.to_string()))?;
            if args.json {
                print_desktop_action_result_json(&result)?;
            } else {
                print_desktop_action_result(result);
            }
            Ok(())
        }
        DesktopCommand::Assert(args) => {
            let result = peekaboox_desktop::assert_target(
                &args.app,
                &args.target,
                &AssertOptions {
                    locate: LocateOptions {
                        image: args.image,
                        prefer_accessibility: args.prefer_accessibility,
                        window_title: args.window_title,
                        window_id: args.window_id,
                    },
                    assertion: args.assertion,
                },
            )
            .map_err(|error| CliError::Failure(error.to_string()))?;
            if args.json {
                print_desktop_action_result_json(&result)?;
            } else {
                print_desktop_action_result(result);
            }
            Ok(())
        }
        DesktopCommand::Help => {
            print_desktop_usage();
            Err(CliError::HelpRequested)
        }
    }
}

fn desktop_daemon(command: DesktopCommand, context: &CliContext) -> Result<(), CliError> {
    match command {
        DesktopCommand::Profiles(args) => {
            print_desktop_profiles(args)?;
            Ok(())
        }
        DesktopCommand::Focus(args) => {
            let result = daemon_request(
                context,
                ApiRequest::DesktopFocus {
                    app: args.app,
                    use_gnome_overview: args.use_gnome_overview,
                    launch_if_needed: args.launch_if_needed,
                    wait_after_focus_ms: args.wait_after_focus_ms,
                    overview_wait_ms: args.overview_wait_ms,
                    window_title: args.window_title,
                    window_id: args.window_id,
                    verify: args.verify,
                },
            )?;
            let ApiResult::DesktopAction(result) = result else {
                return Err(CliError::Failure(
                    "daemon returned unexpected desktop focus response".to_owned(),
                ));
            };
            if args.json {
                print_json_pretty(&result)?;
            } else {
                print_desktop_action_dto(result);
            }
            Ok(())
        }
        DesktopCommand::Locate(args) => {
            let result = daemon_request(
                context,
                ApiRequest::DesktopLocate {
                    app: args.app,
                    target: args.target,
                    image_path: args.image.map(path_to_cli_string),
                    prefer_accessibility: args.prefer_accessibility,
                    window_title: args.window_title,
                    window_id: args.window_id,
                },
            )?;
            let ApiResult::DesktopLocate(result) = result else {
                return Err(CliError::Failure(
                    "daemon returned unexpected desktop locate response".to_owned(),
                ));
            };
            if args.json {
                print_json_pretty(&result)?;
            } else {
                print_desktop_locate_dto(result);
            }
            Ok(())
        }
        DesktopCommand::Click(args) => {
            let result = daemon_request(
                context,
                ApiRequest::DesktopClick {
                    app: args.app,
                    target: args.target,
                    image_path: args.image.map(path_to_cli_string),
                    prefer_accessibility: args.prefer_accessibility,
                    window_title: args.window_title,
                    button: mouse_button_dto(args.button),
                    dry_run: args.dry_run,
                    window_id: args.window_id,
                    verify: args.verify,
                },
            )?;
            let ApiResult::DesktopAction(result) = result else {
                return Err(CliError::Failure(
                    "daemon returned unexpected desktop click response".to_owned(),
                ));
            };
            if args.json {
                print_json_pretty(&result)?;
            } else {
                print_desktop_action_dto(result);
            }
            Ok(())
        }
        DesktopCommand::Drag(args) => {
            let result = daemon_request(
                context,
                ApiRequest::DesktopDrag {
                    app: args.app,
                    target: args.target,
                    image_path: args.image.map(path_to_cli_string),
                    prefer_accessibility: args.prefer_accessibility,
                    window_title: args.window_title,
                    button: mouse_button_dto(args.button),
                    from_ratio_x: args.from_ratio.0,
                    from_ratio_y: args.from_ratio.1,
                    to_ratio_x: args.to_ratio.0,
                    to_ratio_y: args.to_ratio.1,
                    duration_ms: args.duration_ms,
                    dry_run: args.dry_run,
                    window_id: args.window_id,
                    verify: args.verify,
                },
            )?;
            let ApiResult::DesktopAction(result) = result else {
                return Err(CliError::Failure(
                    "daemon returned unexpected desktop drag response".to_owned(),
                ));
            };
            if args.json {
                print_json_pretty(&result)?;
            } else {
                print_desktop_action_dto(result);
            }
            Ok(())
        }
        DesktopCommand::TypeInto(args) => {
            let result = daemon_request(
                context,
                ApiRequest::DesktopTypeInto {
                    app: args.app,
                    target: args.target,
                    text: args.text,
                    image_path: args.image.map(path_to_cli_string),
                    prefer_accessibility: args.prefer_accessibility,
                    window_title: args.window_title,
                    clear: args.clear,
                    dry_run: args.dry_run,
                    window_id: args.window_id,
                    verify: args.verify,
                },
            )?;
            let ApiResult::DesktopAction(result) = result else {
                return Err(CliError::Failure(
                    "daemon returned unexpected desktop type-into response".to_owned(),
                ));
            };
            if args.json {
                print_json_pretty(&result)?;
            } else {
                print_desktop_action_dto(result);
            }
            Ok(())
        }
        DesktopCommand::Assert(args) => {
            let (assertion, expected_text) = desktop_assertion_dto(&args.assertion);
            let result = daemon_request(
                context,
                ApiRequest::DesktopAssert {
                    app: args.app,
                    target: args.target,
                    image_path: args.image.map(path_to_cli_string),
                    prefer_accessibility: args.prefer_accessibility,
                    window_title: args.window_title,
                    assertion,
                    expected_text,
                    window_id: args.window_id,
                },
            )?;
            let ApiResult::DesktopAction(result) = result else {
                return Err(CliError::Failure(
                    "daemon returned unexpected desktop assert response".to_owned(),
                ));
            };
            if args.json {
                print_json_pretty(&result)?;
            } else {
                print_desktop_action_dto(result);
            }
            Ok(())
        }
        DesktopCommand::Help => {
            print_desktop_usage();
            Err(CliError::HelpRequested)
        }
    }
}

fn print_desktop_action_result(result: peekaboox_desktop::DesktopActionResult) {
    println!(
        "{} {}: {} via {}",
        result.app, result.action, result.detail, result.backend_name
    );
    if let Some(detail) = result.verification_detail {
        eprintln!(
            "verification: {}{}",
            if result.verified { "passed: " } else { "" },
            detail
        );
    }
}

fn print_desktop_action_dto(result: DesktopActionResultDto) {
    println!(
        "{} {}: {} via {}",
        result.app, result.action, result.detail, result.backend_name
    );
    if let Some(detail) = result.verification_detail {
        eprintln!(
            "verification: {}{}",
            if result.verified { "passed: " } else { "" },
            detail
        );
    }
}

fn print_desktop_locate_result(target: peekaboox_desktop::ResolvedDesktopTarget) {
    println!(
        "{} {} {},{} rect={} via {}",
        target.app,
        target.target,
        target.point.x,
        target.point.y,
        target
            .rect
            .map(format_rect)
            .unwrap_or_else(|| "-".to_owned()),
        target.source.label()
    );
}

fn print_desktop_locate_dto(target: DesktopLocateResultDto) {
    println!(
        "{} {} {},{} rect={} via {}",
        target.app,
        target.target,
        target.point.x,
        target.point.y,
        target
            .rect
            .map(Rect::from)
            .map(format_rect)
            .unwrap_or_else(|| "-".to_owned()),
        target.source
    );
}

fn print_desktop_profiles(args: DesktopProfilesArgs) -> Result<(), CliError> {
    let profiles = if let Some(app) = args.app {
        vec![
            peekaboox_desktop::desktop_profile(&app)
                .map_err(|error| CliError::Failure(error.to_string()))?,
        ]
    } else {
        peekaboox_desktop::desktop_profiles()
    };
    if args.json {
        print_json_pretty(&serde_json::json!({
            "profiles": profiles.iter().map(desktop_profile_json).collect::<Vec<_>>(),
        }))
    } else {
        for profile in profiles {
            println!(
                "{} targets={} aliases={} desktop_ids={} commands={}",
                profile.id,
                profile.targets.join(","),
                profile.aliases.join(","),
                profile.desktop_ids.join(","),
                profile.commands.join(",")
            );
        }
        Ok(())
    }
}

fn desktop_profile_json(profile: &peekaboox_desktop::DesktopProfileInfo) -> serde_json::Value {
    serde_json::json!({
        "id": &profile.id,
        "aliases": &profile.aliases,
        "search_name": &profile.search_name,
        "desktop_ids": &profile.desktop_ids,
        "commands": &profile.commands,
        "targets": &profile.targets,
    })
}

fn print_desktop_action_result_json(
    result: &peekaboox_desktop::DesktopActionResult,
) -> Result<(), CliError> {
    print_json_pretty(&serde_json::json!({
        "app": &result.app,
        "action": &result.action,
        "detail": &result.detail,
        "backend_name": &result.backend_name,
        "verified": result.verified,
        "verification_detail": &result.verification_detail,
    }))
}

fn print_desktop_locate_result_json(
    target: &peekaboox_desktop::ResolvedDesktopTarget,
) -> Result<(), CliError> {
    print_json_pretty(&serde_json::json!({
        "app": &target.app,
        "target": &target.target,
        "point": {
            "x": target.point.x,
            "y": target.point.y,
        },
        "rect": target.rect.map(|rect| serde_json::json!({
            "x": rect.x,
            "y": rect.y,
            "width": rect.width,
            "height": rect.height,
        })),
        "source": target.source.label(),
    }))
}

fn parse_desktop_args(args: Vec<String>) -> Result<DesktopCommand, CliError> {
    let Some((command, rest)) = args.split_first() else {
        return Ok(DesktopCommand::Help);
    };

    match command.as_str() {
        "profiles" | "apps" => parse_desktop_profiles_args(rest.to_vec()),
        "focus" => parse_desktop_focus_args(rest.to_vec()),
        "locate" => parse_desktop_locate_args(rest.to_vec()),
        "click" => parse_desktop_click_args(rest.to_vec()),
        "drag" => parse_desktop_drag_args(rest.to_vec()),
        "type-into" | "type" => parse_desktop_type_into_args(rest.to_vec()),
        "assert" => parse_desktop_assert_args(rest.to_vec(), false),
        "assert-not" => parse_desktop_assert_args(rest.to_vec(), true),
        "--help" | "-h" | "help" => Ok(DesktopCommand::Help),
        unknown => Err(CliError::Failure(format!(
            "unknown desktop command: {unknown}"
        ))),
    }
}

fn parse_desktop_profiles_args(args: Vec<String>) -> Result<DesktopCommand, CliError> {
    let mut json = false;
    let mut app = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--json" => json = true,
            "--app" | "-a" => app = Some(parse_next_string(&args, &mut index, "--app")?),
            "--help" | "-h" => return Ok(DesktopCommand::Help),
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown desktop profiles argument: {value}"
                )));
            }
            value if app.is_none() => app = Some(value.to_owned()),
            value => {
                return Err(CliError::Failure(format!(
                    "unexpected desktop profiles argument: {value}"
                )));
            }
        }
        index += 1;
    }

    Ok(DesktopCommand::Profiles(DesktopProfilesArgs { json, app }))
}

fn parse_desktop_focus_args(args: Vec<String>) -> Result<DesktopCommand, CliError> {
    let mut app = None;
    let mut use_gnome_overview = true;
    let mut launch_if_needed = true;
    let mut wait_after_focus_ms = 1_000_u64;
    let mut overview_wait_ms = 800_u64;
    let mut window_title = None;
    let mut window_id = None;
    let mut verify = false;
    let mut json = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--app" | "-a" => app = Some(parse_next_string(&args, &mut index, "--app")?),
            "--window-title" | "--title" => {
                window_title = Some(parse_next_string(&args, &mut index, "--window-title")?)
            }
            "--window-id" => window_id = Some(parse_next_string(&args, &mut index, "--window-id")?),
            "--no-overview" => use_gnome_overview = false,
            "--no-launch" => launch_if_needed = false,
            "--verify" => verify = true,
            "--json" => json = true,
            "--wait-ms" => {
                wait_after_focus_ms = parse_u64(
                    "--wait-ms",
                    &parse_next_string(&args, &mut index, "--wait-ms")?,
                )?;
            }
            "--overview-wait-ms" => {
                overview_wait_ms = parse_u64(
                    "--overview-wait-ms",
                    &parse_next_string(&args, &mut index, "--overview-wait-ms")?,
                )?;
            }
            "--help" | "-h" => return Ok(DesktopCommand::Help),
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown desktop focus argument: {value}"
                )));
            }
            value if app.is_none() => app = Some(value.to_owned()),
            value => {
                return Err(CliError::Failure(format!(
                    "unexpected desktop focus argument: {value}"
                )));
            }
        }
        index += 1;
    }

    Ok(DesktopCommand::Focus(DesktopFocusArgs {
        app: app.ok_or_else(|| CliError::Failure("missing --app".to_owned()))?,
        use_gnome_overview,
        launch_if_needed,
        wait_after_focus_ms,
        overview_wait_ms,
        window_title,
        window_id,
        verify,
        json,
    }))
}

fn parse_desktop_locate_args(args: Vec<String>) -> Result<DesktopCommand, CliError> {
    let mut app = None;
    let mut target = None;
    let mut image = None;
    let mut prefer_accessibility = true;
    let mut window_title = None;
    let mut window_id = None;
    let mut json = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--app" | "-a" => app = Some(parse_next_string(&args, &mut index, "--app")?),
            "--target" | "-t" => target = Some(parse_next_string(&args, &mut index, "--target")?),
            "--window-title" | "--title" => {
                window_title = Some(parse_next_string(&args, &mut index, "--window-title")?)
            }
            "--window-id" => window_id = Some(parse_next_string(&args, &mut index, "--window-id")?),
            "--image" | "-i" => {
                image = Some(PathBuf::from(parse_next_string(
                    &args, &mut index, "--image",
                )?))
            }
            "--no-accessibility" => prefer_accessibility = false,
            "--json" => json = true,
            "--help" | "-h" => return Ok(DesktopCommand::Help),
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown desktop locate argument: {value}"
                )));
            }
            value if app.is_none() => app = Some(value.to_owned()),
            value if target.is_none() => target = Some(value.to_owned()),
            value => {
                return Err(CliError::Failure(format!(
                    "unexpected desktop locate argument: {value}"
                )));
            }
        }
        index += 1;
    }

    Ok(DesktopCommand::Locate(DesktopLocateArgs {
        app: app.ok_or_else(|| CliError::Failure("missing --app".to_owned()))?,
        target: target.ok_or_else(|| CliError::Failure("missing --target".to_owned()))?,
        image,
        prefer_accessibility,
        window_title,
        window_id,
        json,
    }))
}

fn parse_desktop_click_args(args: Vec<String>) -> Result<DesktopCommand, CliError> {
    let mut app = None;
    let mut target = None;
    let mut image = None;
    let mut prefer_accessibility = true;
    let mut window_title = None;
    let mut window_id = None;
    let mut button = MouseButton::Left;
    let mut dry_run = false;
    let mut verify = false;
    let mut json = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--app" | "-a" => app = Some(parse_next_string(&args, &mut index, "--app")?),
            "--target" | "-t" => target = Some(parse_next_string(&args, &mut index, "--target")?),
            "--window-title" | "--title" => {
                window_title = Some(parse_next_string(&args, &mut index, "--window-title")?)
            }
            "--window-id" => window_id = Some(parse_next_string(&args, &mut index, "--window-id")?),
            "--image" | "-i" => {
                image = Some(PathBuf::from(parse_next_string(
                    &args, &mut index, "--image",
                )?))
            }
            "--button" | "-b" => {
                button = parse_mouse_button(&parse_next_string(&args, &mut index, "--button")?)?
            }
            "--dry-run" => dry_run = true,
            "--verify" => verify = true,
            "--json" => json = true,
            "--no-accessibility" => prefer_accessibility = false,
            "--help" | "-h" => return Ok(DesktopCommand::Help),
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown desktop click argument: {value}"
                )));
            }
            value if app.is_none() => app = Some(value.to_owned()),
            value if target.is_none() => target = Some(value.to_owned()),
            value => {
                return Err(CliError::Failure(format!(
                    "unexpected desktop click argument: {value}"
                )));
            }
        }
        index += 1;
    }

    Ok(DesktopCommand::Click(DesktopClickArgs {
        app: app.ok_or_else(|| CliError::Failure("missing --app".to_owned()))?,
        target: target.ok_or_else(|| CliError::Failure("missing --target".to_owned()))?,
        image,
        prefer_accessibility,
        window_title,
        window_id,
        button,
        dry_run,
        verify,
        json,
    }))
}

fn parse_desktop_drag_args(args: Vec<String>) -> Result<DesktopCommand, CliError> {
    let mut app = None;
    let mut target = None;
    let mut image = None;
    let mut prefer_accessibility = true;
    let mut window_title = None;
    let mut window_id = None;
    let mut button = MouseButton::Left;
    let mut from_ratio = None;
    let mut to_ratio = None;
    let mut duration_ms = 250_u64;
    let mut dry_run = false;
    let mut verify = false;
    let mut json = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--app" | "-a" => app = Some(parse_next_string(&args, &mut index, "--app")?),
            "--target" | "-t" => target = Some(parse_next_string(&args, &mut index, "--target")?),
            "--window-title" | "--title" => {
                window_title = Some(parse_next_string(&args, &mut index, "--window-title")?)
            }
            "--window-id" => window_id = Some(parse_next_string(&args, &mut index, "--window-id")?),
            "--image" | "-i" => {
                image = Some(PathBuf::from(parse_next_string(
                    &args, &mut index, "--image",
                )?))
            }
            "--button" | "-b" => {
                button = parse_mouse_button(&parse_next_string(&args, &mut index, "--button")?)?
            }
            "--from-ratio" | "--from" => {
                from_ratio = Some(parse_ratio_pair(
                    "--from-ratio",
                    &parse_next_string(&args, &mut index, "--from-ratio")?,
                )?)
            }
            "--to-ratio" | "--to" => {
                to_ratio = Some(parse_ratio_pair(
                    "--to-ratio",
                    &parse_next_string(&args, &mut index, "--to-ratio")?,
                )?)
            }
            "--duration-ms" => {
                duration_ms = parse_u64(
                    "--duration-ms",
                    &parse_next_string(&args, &mut index, "--duration-ms")?,
                )?;
            }
            "--dry-run" => dry_run = true,
            "--verify" => verify = true,
            "--json" => json = true,
            "--no-accessibility" => prefer_accessibility = false,
            "--help" | "-h" => return Ok(DesktopCommand::Help),
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown desktop drag argument: {value}"
                )));
            }
            value if app.is_none() => app = Some(value.to_owned()),
            value if target.is_none() => target = Some(value.to_owned()),
            value => {
                return Err(CliError::Failure(format!(
                    "unexpected desktop drag argument: {value}"
                )));
            }
        }
        index += 1;
    }

    Ok(DesktopCommand::Drag(DesktopDragArgs {
        app: app.ok_or_else(|| CliError::Failure("missing --app".to_owned()))?,
        target: target.ok_or_else(|| CliError::Failure("missing --target".to_owned()))?,
        image,
        prefer_accessibility,
        window_title,
        window_id,
        button,
        from_ratio: from_ratio
            .ok_or_else(|| CliError::Failure("missing --from-ratio".to_owned()))?,
        to_ratio: to_ratio.ok_or_else(|| CliError::Failure("missing --to-ratio".to_owned()))?,
        duration_ms,
        dry_run,
        verify,
        json,
    }))
}

fn parse_desktop_type_into_args(args: Vec<String>) -> Result<DesktopCommand, CliError> {
    let mut app = None;
    let mut target = None;
    let mut image = None;
    let mut prefer_accessibility = true;
    let mut window_title = None;
    let mut window_id = None;
    let mut clear = false;
    let mut dry_run = false;
    let mut verify = false;
    let mut json = false;
    let mut text_parts = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--app" | "-a" => app = Some(parse_next_string(&args, &mut index, "--app")?),
            "--target" | "-t" => target = Some(parse_next_string(&args, &mut index, "--target")?),
            "--window-title" | "--title" => {
                window_title = Some(parse_next_string(&args, &mut index, "--window-title")?)
            }
            "--window-id" => window_id = Some(parse_next_string(&args, &mut index, "--window-id")?),
            "--image" | "-i" => {
                image = Some(PathBuf::from(parse_next_string(
                    &args, &mut index, "--image",
                )?))
            }
            "--clear" => clear = true,
            "--dry-run" => dry_run = true,
            "--verify" => verify = true,
            "--json" => json = true,
            "--no-accessibility" => prefer_accessibility = false,
            "--help" | "-h" => return Ok(DesktopCommand::Help),
            value if value.starts_with('-') && text_parts.is_empty() => {
                return Err(CliError::Failure(format!(
                    "unknown desktop type-into argument: {value}"
                )));
            }
            value if app.is_none() => app = Some(value.to_owned()),
            value if target.is_none() => target = Some(value.to_owned()),
            value => text_parts.push(value.to_owned()),
        }
        index += 1;
    }

    let text = text_parts.join(" ");
    if text.is_empty() {
        return Err(CliError::Failure("missing text to type".to_owned()));
    }

    Ok(DesktopCommand::TypeInto(DesktopTypeIntoArgs {
        app: app.ok_or_else(|| CliError::Failure("missing --app".to_owned()))?,
        target: target.ok_or_else(|| CliError::Failure("missing --target".to_owned()))?,
        text,
        image,
        prefer_accessibility,
        window_title,
        window_id,
        clear,
        dry_run,
        verify,
        json,
    }))
}

fn parse_desktop_assert_args(args: Vec<String>, negated: bool) -> Result<DesktopCommand, CliError> {
    let mut app = None;
    let mut target = None;
    let mut image = None;
    let mut prefer_accessibility = true;
    let mut window_title = None;
    let mut window_id = None;
    let mut assertion = None;
    let mut json = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--app" | "-a" => app = Some(parse_next_string(&args, &mut index, "--app")?),
            "--target" | "-t" => target = Some(parse_next_string(&args, &mut index, "--target")?),
            "--window-title" | "--title" => {
                window_title = Some(parse_next_string(&args, &mut index, "--window-title")?)
            }
            "--window-id" => window_id = Some(parse_next_string(&args, &mut index, "--window-id")?),
            "--image" | "-i" => {
                image = Some(PathBuf::from(parse_next_string(
                    &args, &mut index, "--image",
                )?))
            }
            "--present" => {
                assertion = Some(if negated {
                    DesktopAssertion::NotPresent
                } else {
                    DesktopAssertion::Present
                })
            }
            "--active" => {
                assertion = Some(if negated {
                    DesktopAssertion::NotActive
                } else {
                    DesktopAssertion::Active
                })
            }
            "--not-active" => {
                assertion = Some(if negated {
                    DesktopAssertion::Active
                } else {
                    DesktopAssertion::NotActive
                })
            }
            "--contains" => {
                let expected = parse_next_string(&args, &mut index, "--contains")?;
                assertion = Some(if negated {
                    DesktopAssertion::NotContains(expected)
                } else {
                    DesktopAssertion::Contains(expected)
                });
            }
            "--no-accessibility" => prefer_accessibility = false,
            "--json" => json = true,
            "--help" | "-h" => return Ok(DesktopCommand::Help),
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown desktop assert argument: {value}"
                )));
            }
            value if app.is_none() => app = Some(value.to_owned()),
            value if target.is_none() => target = Some(value.to_owned()),
            value => {
                return Err(CliError::Failure(format!(
                    "unexpected desktop assert argument: {value}"
                )));
            }
        }
        index += 1;
    }

    Ok(DesktopCommand::Assert(DesktopAssertArgs {
        app: app.ok_or_else(|| CliError::Failure("missing --app".to_owned()))?,
        target: target.ok_or_else(|| CliError::Failure("missing --target".to_owned()))?,
        image,
        prefer_accessibility,
        window_title,
        window_id,
        assertion: assertion.unwrap_or({
            if negated {
                DesktopAssertion::NotPresent
            } else {
                DesktopAssertion::Present
            }
        }),
        json,
    }))
}

fn capture_cli_to_file(
    output: impl AsRef<std::path::Path>,
    region: Option<Rect>,
) -> peekaboox_core::Result<peekaboox_capture::CaptureFileMetadata> {
    match region {
        Some(region) => peekaboox_capture::capture_region_to_file(region, output),
        None => peekaboox_capture::capture_screen_to_file(output),
    }
}

#[derive(Debug, Clone)]
struct CaptureTarget {
    capture_region: Option<Rect>,
    window: Option<WindowInfo>,
}

#[derive(Debug, Clone)]
struct CaptureCliExecutionResult {
    metadata: CaptureResultDto,
    stdout_bytes: Option<Vec<u8>>,
}

fn capture_cli_execute(
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
        let bytes_written = peekaboox_capture::write_frame_png(&frame_metadata.frame, &output_path)
            .map_err(|error| CliError::Failure(error.to_string()))?;
        (bytes_written, None, output_path.display().to_string())
    } else {
        let bytes = peekaboox_capture::encode_frame_png(&frame_metadata.frame)
            .map_err(|error| CliError::Failure(error.to_string()))?;
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

fn capture_target_from_args(args: &CaptureArgs) -> Result<CaptureTarget, CliError> {
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

fn resolve_capture_window(args: &CaptureArgs) -> Result<Option<WindowInfo>, CliError> {
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

fn offset_window_relative_region(origin: Rect, region: Rect) -> Result<Rect, CliError> {
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

fn ensure_capture_output_path(output: &Path, no_overwrite: bool) -> Result<PathBuf, CliError> {
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

fn unix_time_ms_u64() -> u64 {
    u64::try_from(monotonic_ms()).unwrap_or(u64::MAX)
}

fn parse_capture_output_format(value: &str) -> Result<CaptureOutputFormat, CliError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "png" => Ok(CaptureOutputFormat::Png),
        "xwd" => Ok(CaptureOutputFormat::Xwd),
        other => Err(CliError::Failure(format!(
            "invalid capture format {other:?}; expected png or xwd"
        ))),
    }
}

fn print_capture_result_dto(metadata: &CaptureResultDto) {
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

fn format_rect(rect: Rect) -> String {
    format!("{},{},{}x{}", rect.x, rect.y, rect.width, rect.height)
}

fn contains_case_insensitive(value: &str, needle: &str) -> bool {
    value
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
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
            let query = ElementQuery::parse(selector)
                .map_err(|error| CliError::Failure(error.to_string()))?;
            let args = default_elements_args_for_selector(selector);
            let elements = vision_fallback_metadata(&query, &args)?.elements;
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

fn default_elements_args_for_selector(selector: &str) -> ElementsArgs {
    ElementsArgs {
        selector: selector.to_owned(),
        limit: 50,
        vision_fallback: true,
        app: None,
        window_title: None,
        window_id: None,
        vision_region: None,
        vision_edge_threshold: None,
        vision_min_width: None,
        vision_min_height: None,
        vision_min_component_pixels: None,
        vision_max_elements: None,
        vision_merge_distance: None,
        json: false,
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
    paste: bool,
    preserve_clipboard: bool,
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

    run_text_input(args, context)
}

fn paste_text(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let TypeCommand::Run(mut args) = parse_type_args(args)? else {
        print_paste_usage();
        return Err(CliError::HelpRequested);
    };
    args.paste = true;

    run_text_input(args, context)
}

fn run_text_input(args: TypeArgs, context: &CliContext) -> Result<(), CliError> {
    let action = if args.paste {
        peekaboox_input::InputAction::PasteText {
            text: args.text.clone(),
            preserve_clipboard: args.preserve_clipboard,
        }
    } else {
        peekaboox_input::InputAction::TypeText(args.text.clone())
    };

    if context.use_daemon {
        let result = daemon_request(
            context,
            if args.paste {
                ApiRequest::PasteText {
                    text: args.text.clone(),
                    preserve_clipboard: args.preserve_clipboard,
                    dry_run: args.dry_run,
                }
            } else {
                ApiRequest::TypeText {
                    text: args.text.clone(),
                    dry_run: args.dry_run,
                }
            },
        )?;
        let metadata = match result {
            ApiResult::TypeText(metadata) if !args.paste => metadata,
            ApiResult::PasteText(metadata) if args.paste => metadata,
            _ => {
                return Err(CliError::Failure(
                    "daemon returned unexpected text input response".to_owned(),
                ));
            }
        };
        print_type_result(&args, metadata);
        return Ok(());
    }

    if args.dry_run {
        let backend = peekaboox_input::CommandInputBackend
            .detect_backend_for(&action)
            .map_err(|error| CliError::Failure(error.to_string()))?;
        print_type_result(
            &args,
            ActionResultDto {
                backend_name: backend.name().to_owned(),
                backend_kind: format!("{:?}", backend.backend_kind()).to_ascii_lowercase(),
            },
        );
        return Ok(());
    }

    let metadata = if args.paste {
        peekaboox_input::paste_text_with_options(args.text.clone(), args.preserve_clipboard)
    } else {
        peekaboox_input::type_text(args.text.clone())
    }
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
    match (args.dry_run, args.paste) {
        (true, true) => println!("would paste via {}", metadata.backend_name),
        (true, false) => println!("would type via {}", metadata.backend_name),
        (false, true) => println!("pasted text via {}", metadata.backend_name),
        (false, false) => println!("typed text via {}", metadata.backend_name),
    }
}

fn parse_type_args(args: Vec<String>) -> Result<TypeCommand, CliError> {
    let mut dry_run = false;
    let mut paste = false;
    let mut preserve_clipboard = false;
    let mut text_parts = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--dry-run" => dry_run = true,
            "--paste" => paste = true,
            "--preserve-clipboard" => preserve_clipboard = true,
            "--help" | "-h" => return Ok(TypeCommand::Help),
            value => text_parts.push(value.to_owned()),
        }

        index += 1;
    }

    let text = text_parts.join(" ");
    if text.is_empty() {
        return Err(CliError::Failure("missing text to type".to_owned()));
    }

    Ok(TypeCommand::Run(TypeArgs {
        text,
        dry_run,
        paste,
        preserve_clipboard,
    }))
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

fn parse_u64(name: &str, value: &str) -> Result<u64, CliError> {
    value
        .parse::<u64>()
        .map_err(|_| CliError::Failure(format!("{name} must be an integer, got {value:?}")))
}

fn parse_usize(name: &str, value: &str) -> Result<usize, CliError> {
    value
        .parse::<usize>()
        .map_err(|_| CliError::Failure(format!("{name} must be an integer, got {value:?}")))
}

fn parse_positive_usize(name: &str, value: &str) -> Result<usize, CliError> {
    let parsed = parse_usize(name, value)?;
    if parsed == 0 {
        return Err(CliError::Failure(format!(
            "{name} must be greater than zero"
        )));
    }

    Ok(parsed)
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

fn parse_ocr_psm(value: &str) -> Result<u8, CliError> {
    let parsed = parse_u8("--psm", value)?;
    if parsed > 13 {
        return Err(CliError::Failure(format!(
            "--psm must be between 0 and 13, got {value:?}"
        )));
    }
    Ok(parsed)
}

fn parse_ocr_oem(value: &str) -> Result<u8, CliError> {
    let parsed = parse_u8("--oem", value)?;
    if parsed > 3 {
        return Err(CliError::Failure(format!(
            "--oem must be between 0 and 3, got {value:?}"
        )));
    }
    Ok(parsed)
}

fn parse_ocr_confidence(value: &str) -> Result<f32, CliError> {
    parse_unit_f32("--min-confidence", value)
}

fn parse_ocr_scale(value: &str) -> Result<f32, CliError> {
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

fn parse_ocr_contrast(value: &str) -> Result<f32, CliError> {
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

fn parse_ocr_config(value: &str) -> Result<OcrConfig, CliError> {
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

fn parse_ui_element_sort(value: &str) -> Result<UiElementSort, CliError> {
    match value {
        "position" => Ok(UiElementSort::Position),
        "area" => Ok(UiElementSort::Area),
        "confidence" => Ok(UiElementSort::Confidence),
        _ => Err(CliError::Failure(format!(
            "--sort must be position, area, or confidence, got {value:?}"
        ))),
    }
}

fn ui_element_sort_name(sort: UiElementSort) -> &'static str {
    match sort {
        UiElementSort::Position => "position",
        UiElementSort::Area => "area",
        UiElementSort::Confidence => "confidence",
    }
}

fn parse_visual_mae(name: &str, value: &str) -> Result<f32, CliError> {
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

fn parse_visual_size_policy(value: &str) -> Result<VisualSizePolicy, CliError> {
    match value {
        "error" => Ok(VisualSizePolicy::Error),
        "common-region" => Ok(VisualSizePolicy::CommonRegion),
        "resize-actual" => Ok(VisualSizePolicy::ResizeActual),
        _ => Err(CliError::Failure(format!(
            "--size-policy must be error, common-region, or resize-actual, got {value:?}"
        ))),
    }
}

fn parse_visual_alpha_mode(value: &str) -> Result<VisualAlphaMode, CliError> {
    match value {
        "ignore" => Ok(VisualAlphaMode::Ignore),
        "compare" => Ok(VisualAlphaMode::Compare),
        _ => Err(CliError::Failure(format!(
            "--alpha must be ignore or compare, got {value:?}"
        ))),
    }
}

fn visual_size_policy_name(policy: VisualSizePolicy) -> &'static str {
    match policy {
        VisualSizePolicy::Error => "error",
        VisualSizePolicy::CommonRegion => "common-region",
        VisualSizePolicy::ResizeActual => "resize-actual",
    }
}

fn visual_alpha_mode_name(mode: VisualAlphaMode) -> &'static str {
    match mode {
        VisualAlphaMode::Ignore => "ignore",
        VisualAlphaMode::Compare => "compare",
    }
}

fn parse_ratio_pair(name: &str, value: &str) -> Result<(f32, f32), CliError> {
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

fn parse_next_string(args: &[String], index: &mut usize, name: &str) -> Result<String, CliError> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| CliError::Failure(format!("missing value for {name}")))
}

fn require_next_arg(
    args: &mut impl Iterator<Item = String>,
    name: &str,
) -> Result<String, CliError> {
    args.next()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CliError::Failure(format!("missing value for {name}")))
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

fn desktop_assertion_dto(assertion: &DesktopAssertion) -> (DesktopAssertionDto, Option<String>) {
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

fn path_to_cli_string(path: PathBuf) -> String {
    path.display().to_string()
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
        ApiResponse::Ok { result } => Ok(*result),
        ApiResponse::Error { message } => Err(CliError::Failure(message)),
    }
}

fn print_json_pretty(value: &impl serde::Serialize) -> Result<(), CliError> {
    println!(
        "{}",
        serde_json::to_string_pretty(value)
            .map_err(|error| CliError::Failure(error.to_string()))?
    );
    Ok(())
}

fn write_json_pretty_file(path: &Path, value: &impl serde::Serialize) -> Result<(), CliError> {
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

fn print_usage() {
    println!(
        "Usage: peekaboox [--daemon] [--socket <path>] <capture|capture-delta|capture-backends|capture-dmabuf|plugins|plugin-call|windows|elements|ocr|compare|state|vision-elements|desktop|doctor|click|move|drag|type|paste|hotkey>"
    );
    println!("Try:   peekaboox capture --output screenshot.png");
    println!("Try:   peekaboox --daemon capture-delta --stream agent-loop");
    println!("Try:   peekaboox capture-backends");
    println!("Try:   peekaboox capture-dmabuf");
    println!("Try:   peekaboox plugins --path examples/plugins");
    println!(
        "Try:   peekaboox plugin-call org.peekaboox.examples.system-info system_info.uname --path examples/plugins --json"
    );
    println!("Try:   peekaboox --daemon windows");
    println!("Try:   peekaboox windows");
    println!("Try:   peekaboox elements --role \"push button\" --state enabled");
    println!("Try:   peekaboox ocr --region 10,20,400,120 --language eng");
    println!("Try:   peekaboox compare before.png after.png --max-changed-ratio 0.01");
    println!("Try:   peekaboox state frame1.png frame2.png frame3.png");
    println!("Try:   peekaboox vision-elements screenshot.png --min-width 8");
    println!("Try:   peekaboox desktop focus --app telegram");
    println!("Try:   peekaboox desktop click --app telegram --target search-input");
    println!("Try:   peekaboox doctor --json");
    println!("Try:   peekaboox click --x 100 --y 200");
    println!("Try:   peekaboox click --text \"Submit\"");
    println!("Try:   peekaboox move --x 100 --y 200");
    println!("Try:   peekaboox drag --from 100,200 --to 300,240 --duration-ms 350");
    println!("Try:   peekaboox type \"Hello World\"");
    println!("Try:   peekaboox hotkey ctrl+s");
}

fn print_capture_usage() {
    println!(
        "Usage: peekaboox capture [--output <path>|--stdout] [--format png|xwd] [--region x,y,width,height] [--window-id <id>|--app <name>|--window-title <text>|--title-regex <regex>] [--json] [--include-semantic-tree] [--no-overwrite]"
    );
}

fn print_capture_delta_usage() {
    println!(
        "Usage: peekaboox --daemon capture-delta [--stream <id>] [--reset] [--region x,y,width,height | --window-id <id>] [--threshold <0-255>] [--low-bandwidth|--full-frame] [--json]"
    );
}

fn print_capture_backends_usage() {
    println!(
        "Usage: peekaboox [--daemon] capture-backends [--output <path>|--format png|xwd] [--region x,y,width,height] [--diagnose|--all] [--probe none|file|frame|region|dmabuf|all] [--json]"
    );
}

fn print_capture_dmabuf_usage() {
    println!("Usage: peekaboox capture-dmabuf [--import <compute|egl|egl-texture>]");
}

fn print_plugins_usage() {
    println!("Usage: peekaboox [--daemon] plugins [--path <plugin-dir-or-manifest>]... [--json]");
}

fn print_plugin_call_usage() {
    println!(
        "Usage: peekaboox [--daemon] plugin-call <plugin-id> <tool> [--arguments-json <json>] [--path <plugin-dir-or-manifest>]... [--timeout-ms <ms>] [--max-output-bytes <n>] [--json]"
    );
}

fn print_windows_usage() {
    println!(
        "Usage: peekaboox windows [--id <id>] [--app <app>] [--title <text>] [--title-regex <regex>] [--focused] [--limit <n>] [--sort backend|focused|title|app|area|id|state] [--backend auto|gnome|at-spi|xdotool] [--diagnose] [--json]"
    );
}

fn print_elements_usage() {
    println!(
        "Usage: peekaboox elements [<selector>|--selector <query>] [--id <id>] [--role <role>] [--role-exact <role>] [--role-regex <regex>] [--text <label>] [--text-exact <label>] [--text-regex <regex>] [--state <state>] [--not-state <state>] [--bounds x,y,w,h] [--contains x,y] [--within x,y,w,h] [--intersects x,y,w,h] [--min-width <px>] [--min-height <px>] [--min-confidence <float>] [--app <app>] [--window-title <text>] [--window-id <id>] [--limit <n>] [--vision-fallback] [--vision-region x,y,w,h] [--vision-threshold <1-255>] [--vision-min-width <px>] [--vision-min-height <px>] [--vision-min-component-pixels <px>] [--vision-max-elements <n>] [--vision-merge-distance <px>] [--json]"
    );
}

fn print_ocr_usage() {
    println!(
        "Usage: peekaboox ocr [--image <path> | --window-id <id> | --window-title <text> | --app <app>] [--region x,y,width,height] [--language <code>] [--psm <0-13>] [--oem <0-3>] [--dpi <n>] [--min-confidence <0..1>] [--whitelist <chars>] [--config key=value] [--scale <0.1..8>] [--grayscale] [--threshold <0-255>] [--invert] [--contrast <-255..255>] [--deskew] [--words] [--json]"
    );
}

fn print_compare_usage() {
    println!(
        "Usage: peekaboox compare [--expected <path>] [--actual <path>] [--region x,y,width,height] [--ignore-region x,y,width,height]... [--threshold 0..255] [--max-changed-ratio 0.0..1.0] [--max-changed-pixels <n>] [--max-mae 0..255] [--max-channel-delta 0..255] [--size-policy error|common-region|resize-actual] [--alpha ignore|compare] [--diff-output <path>] [--report <path>] [--no-fail] [--json]"
    );
    println!("       peekaboox compare <expected-path> <actual-path>");
}

fn print_ui_state_usage() {
    println!(
        "Usage: peekaboox state [--image <path>]... [--region x,y,width,height] [--ignore-region x,y,width,height]... [--threshold 0..255] [--stable-max-changed-ratio 0.0..1.0] [--stable-max-changed-pixels <n>] [--stable-max-mae 0..255] [--stable-max-channel-delta 0..255] [--loading-min-changed-ratio 0.0..1.0] [--loading-min-changed-pixels <n>] [--required-stable-transitions <n>] [--size-policy error|common-region|resize-actual] [--alpha ignore|compare] [--json]"
    );
    println!("       peekaboox state <image-path> <image-path> [more-image-paths...]");
}

fn print_vision_elements_usage() {
    println!(
        "Usage: peekaboox vision-elements [--image <path>] [--region x,y,width,height] [--ignore-region x,y,width,height]... [--threshold 1..255] [--min-width <pixels>] [--max-width <pixels>] [--min-height <pixels>] [--max-height <pixels>] [--min-component-pixels <pixels>] [--min-confidence 0.0..1.0] [--min-area <pixels>] [--max-area <pixels>] [--max-elements <n>] [--merge-distance <pixels>] [--padding <pixels>] [--sort position|area|confidence] [--mask-output <path>] [--overlay-output <path>] [--json]"
    );
    println!("       peekaboox vision-elements <image-path>");
}

fn print_desktop_usage() {
    println!("Usage: peekaboox desktop profiles [--app <app>] [--json]");
    println!(
        "Usage: peekaboox desktop focus --app <app> [--window-id <id>|--window-title <text>] [--verify] [--json] [--no-overview] [--no-launch] [--wait-ms <ms>] [--overview-wait-ms <ms>]"
    );
    println!(
        "Usage: peekaboox desktop locate --app <app> --target <target> [--window-id <id>|--window-title <text>] [--image <path>] [--json] [--no-accessibility]"
    );
    println!(
        "Usage: peekaboox desktop click --app <app> --target <target> [--window-id <id>|--window-title <text>] [--button left|middle|right] [--image <path>] [--dry-run] [--verify] [--json] [--no-accessibility]"
    );
    println!(
        "Usage: peekaboox desktop drag --app <app> --target <target> --from-ratio <x,y> --to-ratio <x,y> [--window-id <id>|--window-title <text>] [--duration-ms <ms>] [--button left|middle|right] [--image <path>] [--dry-run] [--verify] [--json] [--no-accessibility]"
    );
    println!(
        "Usage: peekaboox desktop type-into --app <app> --target <target> [--window-id <id>|--window-title <text>] [--clear] [--image <path>] [--dry-run] [--verify] [--json] <text>"
    );
    println!(
        "Usage: peekaboox desktop assert --app <app> --target <target> [--window-id <id>|--window-title <text>] [--present|--active|--not-active|--contains <text>] [--image <path>] [--json]"
    );
    println!(
        "       peekaboox desktop assert-not --app <app> --target <target> [--window-title <text>] [--present|--active|--contains <text>]"
    );
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
    println!("Usage: peekaboox type [--dry-run] [--paste] [--preserve-clipboard] <text>");
}

fn print_paste_usage() {
    println!("Usage: peekaboox paste [--dry-run] [--preserve-clipboard] <text>");
}

fn print_hotkey_usage() {
    println!("Usage: peekaboox hotkey [--dry-run] <key-or-combo> [more-keys]");
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use peekaboox_input::MouseButton;
    use peekaboox_ipc::CaptureBackendProbeDto;

    use super::{
        CaptureArgs, CaptureBackendsArgs, CaptureBackendsCommand, CaptureCommand, CaptureDeltaArgs,
        CaptureDeltaCommand, CaptureDmaBufArgs, CaptureDmaBufCommand, CaptureDmaBufImportTarget,
        CaptureOutputFormat, CliContext, CliError, ClickArgs, ClickCommand, ClickTarget,
        CompareArgs, CompareCommand, DesktopAssertArgs, DesktopClickArgs, DesktopCommand,
        DesktopDragArgs, DesktopFocusArgs, DesktopLocateArgs, DesktopProfilesArgs,
        DesktopTypeIntoArgs, DragArgs, DragCommand, ElementsArgs, ElementsCommand, GlobalArgs,
        HotkeyArgs, HotkeyCommand, MoveArgs, MoveCommand, OcrArgs, OcrCommand, PluginsArgs,
        PluginsCommand, TypeArgs, TypeCommand, UiStateArgs, UiStateCommand, VisionElementsArgs,
        VisionElementsCommand, WindowsArgs, WindowsCommand, parse_capture_args,
        parse_capture_backends_args, parse_capture_delta_args, parse_capture_dmabuf_args,
        parse_click_args, parse_compare_args, parse_desktop_args, parse_drag_args,
        parse_elements_args, parse_global_args, parse_hotkey_args, parse_move_args, parse_ocr_args,
        parse_plugins_args, parse_type_args, parse_ui_state_args, parse_vision_elements_args,
        parse_windows_args,
    };
    use peekaboox_core::{Point, Rect};
    use peekaboox_desktop::DesktopAssertion;
    use peekaboox_vision::{
        OcrConfig, OcrPreprocessingOptions, UiElementSort, VisualAlphaMode, VisualSizePolicy,
    };

    #[test]
    fn capture_defaults_to_screenshot_png() {
        let args = parse_capture_args(vec![]).unwrap();

        assert_eq!(
            args,
            CaptureCommand::Run(CaptureArgs {
                output: PathBuf::from("screenshot.png"),
                region: None,
                window_id: None,
                app: None,
                window_title: None,
                title_regex: None,
                format: CaptureOutputFormat::Png,
                json: false,
                stdout: false,
                no_overwrite: false,
                include_semantic_tree: false,
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
                output: PathBuf::from("tmp/screenshot.png"),
                region: None,
                window_id: None,
                app: None,
                window_title: None,
                title_regex: None,
                format: CaptureOutputFormat::Png,
                json: false,
                stdout: false,
                no_overwrite: false,
                include_semantic_tree: false,
            })
        );
    }

    #[test]
    fn capture_accepts_region_and_window_id_targets() {
        let region = parse_capture_args(vec![
            "--output".to_owned(),
            "tmp/region.png".to_owned(),
            "--region".to_owned(),
            "10,20,100,40".to_owned(),
        ])
        .unwrap();
        let window =
            parse_capture_args(vec!["--window-id".to_owned(), "window-1".to_owned()]).unwrap();

        assert_eq!(
            region,
            CaptureCommand::Run(CaptureArgs {
                output: PathBuf::from("tmp/region.png"),
                region: Some(Rect::new(10, 20, 100, 40)),
                window_id: None,
                app: None,
                window_title: None,
                title_regex: None,
                format: CaptureOutputFormat::Png,
                json: false,
                stdout: false,
                no_overwrite: false,
                include_semantic_tree: false,
            })
        );
        assert_eq!(
            window,
            CaptureCommand::Run(CaptureArgs {
                output: PathBuf::from("screenshot.png"),
                region: None,
                window_id: Some("window-1".to_owned()),
                app: None,
                window_title: None,
                title_regex: None,
                format: CaptureOutputFormat::Png,
                json: false,
                stdout: false,
                no_overwrite: false,
                include_semantic_tree: false,
            })
        );
    }

    #[test]
    fn capture_accepts_window_relative_region_and_filters() {
        let command = parse_capture_args(vec![
            "--region".to_owned(),
            "10,20,100,40".to_owned(),
            "--window-id".to_owned(),
            "window-1".to_owned(),
            "--app".to_owned(),
            "calculator".to_owned(),
            "--window-title".to_owned(),
            "Calculator".to_owned(),
            "--title-regex".to_owned(),
            "Calc.*".to_owned(),
            "--json".to_owned(),
            "--include-semantic-tree".to_owned(),
            "--no-overwrite".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            CaptureCommand::Run(CaptureArgs {
                output: PathBuf::from("screenshot.png"),
                region: Some(Rect::new(10, 20, 100, 40)),
                window_id: Some("window-1".to_owned()),
                app: Some("calculator".to_owned()),
                window_title: Some("Calculator".to_owned()),
                title_regex: Some("Calc.*".to_owned()),
                format: CaptureOutputFormat::Png,
                json: true,
                stdout: false,
                no_overwrite: true,
                include_semantic_tree: true,
            })
        );
    }

    #[test]
    fn capture_accepts_stdout_and_xwd_format() {
        let stdout = parse_capture_args(vec!["--stdout".to_owned()]).unwrap();
        let xwd = parse_capture_args(vec!["--format".to_owned(), "xwd".to_owned()]).unwrap();

        assert_eq!(
            stdout,
            CaptureCommand::Run(CaptureArgs {
                output: PathBuf::from("screenshot.png"),
                region: None,
                window_id: None,
                app: None,
                window_title: None,
                title_regex: None,
                format: CaptureOutputFormat::Png,
                json: false,
                stdout: true,
                no_overwrite: false,
                include_semantic_tree: false,
            })
        );
        assert_eq!(
            xwd,
            CaptureCommand::Run(CaptureArgs {
                output: PathBuf::from("screenshot.xwd"),
                region: None,
                window_id: None,
                app: None,
                window_title: None,
                title_regex: None,
                format: CaptureOutputFormat::Xwd,
                json: false,
                stdout: false,
                no_overwrite: false,
                include_semantic_tree: false,
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
                window_id: None,
                per_channel_threshold: 3,
                low_bandwidth: false,
                json: false,
            })
        );
    }

    #[test]
    fn capture_delta_accepts_json_output() {
        let args = parse_capture_delta_args(vec!["--json".to_owned()]).unwrap();

        assert_eq!(
            args,
            CaptureDeltaCommand::Run(CaptureDeltaArgs {
                stream_id: None,
                reset: false,
                region: None,
                window_id: None,
                per_channel_threshold: 0,
                low_bandwidth: true,
                json: true,
            })
        );
    }

    #[test]
    fn capture_delta_accepts_window_id_target() {
        let args = parse_capture_delta_args(vec![
            "--stream".to_owned(),
            "agent-loop".to_owned(),
            "--window-id".to_owned(),
            "window-1".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            args,
            CaptureDeltaCommand::Run(CaptureDeltaArgs {
                stream_id: Some("agent-loop".to_owned()),
                reset: false,
                region: None,
                window_id: Some("window-1".to_owned()),
                per_channel_threshold: 0,
                low_bandwidth: true,
                json: false,
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

        assert_eq!(
            command,
            CaptureBackendsCommand::Run(CaptureBackendsArgs {
                output: PathBuf::from("screenshot.png"),
                region: None,
                diagnose: false,
                json: false,
                probe: CaptureBackendProbeDto::None,
            })
        );
    }

    #[test]
    fn capture_backends_accepts_diagnostics_json_output_format_region_and_probe() {
        let command = parse_capture_backends_args(vec![
            "--format".to_owned(),
            "xwd".to_owned(),
            "--region".to_owned(),
            "0,0,320,180".to_owned(),
            "--probe".to_owned(),
            "all".to_owned(),
            "--diagnose".to_owned(),
            "--json".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            CaptureBackendsCommand::Run(CaptureBackendsArgs {
                output: PathBuf::from("screenshot.xwd"),
                region: Some(Rect::new(0, 0, 320, 180)),
                diagnose: true,
                json: true,
                probe: CaptureBackendProbeDto::All,
            })
        );
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

        assert_eq!(
            command,
            WindowsCommand::Run(WindowsArgs {
                json: false,
                id: None,
                app: None,
                title: None,
                title_regex: None,
                focused: false,
                limit: None,
                sort: peekaboox_windows::WindowSort::Backend,
                backend: peekaboox_windows::WindowBackendSelection::Auto,
                diagnose: false,
            })
        );
    }

    #[test]
    fn windows_accepts_json() {
        let command = parse_windows_args(vec!["--json".to_owned()]).unwrap();

        assert_eq!(
            command,
            WindowsCommand::Run(WindowsArgs {
                json: true,
                id: None,
                app: None,
                title: None,
                title_regex: None,
                focused: false,
                limit: None,
                sort: peekaboox_windows::WindowSort::Backend,
                backend: peekaboox_windows::WindowBackendSelection::Auto,
                diagnose: false,
            })
        );
    }

    #[test]
    fn windows_help_is_not_a_failure() {
        let command = parse_windows_args(vec!["--help".to_owned()]).unwrap();

        assert_eq!(command, WindowsCommand::Help);
    }

    #[test]
    fn windows_accepts_filters_sort_backend_and_diagnose() {
        let command = parse_windows_args(vec![
            "--focused".to_owned(),
            "--app".to_owned(),
            "Calculator".to_owned(),
            "--title-regex".to_owned(),
            "Calc.*".to_owned(),
            "--id".to_owned(),
            "42".to_owned(),
            "--limit".to_owned(),
            "1".to_owned(),
            "--sort".to_owned(),
            "focused".to_owned(),
            "--backend".to_owned(),
            "xdotool".to_owned(),
            "--diagnose".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            WindowsCommand::Run(WindowsArgs {
                json: false,
                id: Some("42".to_owned()),
                app: Some("Calculator".to_owned()),
                title: None,
                title_regex: Some("Calc.*".to_owned()),
                focused: true,
                limit: Some(1),
                sort: peekaboox_windows::WindowSort::Focused,
                backend: peekaboox_windows::WindowBackendSelection::Xdotool,
                diagnose: true,
            })
        );
    }

    #[test]
    fn elements_defaults_to_all_with_limit() {
        let command = parse_elements_args(vec![]).unwrap();

        assert_eq!(
            command,
            ElementsCommand::Run(ElementsArgs {
                selector: String::new(),
                limit: 50,
                vision_fallback: false,
                app: None,
                window_title: None,
                window_id: None,
                vision_region: None,
                vision_edge_threshold: None,
                vision_min_width: None,
                vision_min_height: None,
                vision_min_component_pixels: None,
                vision_max_elements: None,
                vision_merge_distance: None,
                json: false
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
                vision_fallback: false,
                app: None,
                window_title: None,
                window_id: None,
                vision_region: None,
                vision_edge_threshold: None,
                vision_min_width: None,
                vision_min_height: None,
                vision_min_component_pixels: None,
                vision_max_elements: None,
                vision_merge_distance: None,
                json: false
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
                vision_fallback: true,
                app: None,
                window_title: None,
                window_id: None,
                vision_region: None,
                vision_edge_threshold: None,
                vision_min_width: None,
                vision_min_height: None,
                vision_min_component_pixels: None,
                vision_max_elements: None,
                vision_merge_distance: None,
                json: false
            })
        );
    }

    #[test]
    fn elements_accepts_scope_and_vision_options() {
        let command = parse_elements_args(vec![
            "--selector".to_owned(),
            "label-regex=^Save,not-state=disabled,min-width=40".to_owned(),
            "--app".to_owned(),
            "text-editor".to_owned(),
            "--window-title".to_owned(),
            "Draft".to_owned(),
            "--window-id".to_owned(),
            "window-1".to_owned(),
            "--vision-region".to_owned(),
            "10,20,300,200".to_owned(),
            "--vision-threshold".to_owned(),
            "24".to_owned(),
            "--vision-min-width".to_owned(),
            "8".to_owned(),
            "--vision-max-elements".to_owned(),
            "25".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            ElementsCommand::Run(ElementsArgs {
                selector: "label-regex=^Save,not-state=disabled,min-width=40".to_owned(),
                limit: 50,
                vision_fallback: false,
                app: Some("text-editor".to_owned()),
                window_title: Some("Draft".to_owned()),
                window_id: Some("window-1".to_owned()),
                vision_region: Some(Rect::new(10, 20, 300, 200)),
                vision_edge_threshold: Some(24),
                vision_min_width: Some(8),
                vision_min_height: None,
                vision_min_component_pixels: None,
                vision_max_elements: Some(25),
                vision_merge_distance: None,
                json: false
            })
        );
    }

    #[test]
    fn elements_rejects_invalid_selector_values() {
        let error = parse_elements_args(vec!["--bounds".to_owned(), "bad".to_owned()]).unwrap_err();

        assert!(matches!(error, CliError::Failure(message) if message.contains("bounds")));
    }

    #[test]
    fn ocr_accepts_region_and_language() {
        let command = parse_ocr_args(vec![
            "--image".to_owned(),
            "tests/fixtures/ocr/sample.png".to_owned(),
            "--region".to_owned(),
            "10,20,300,80".to_owned(),
            "--language".to_owned(),
            "eng".to_owned(),
            "--psm".to_owned(),
            "6".to_owned(),
            "--oem".to_owned(),
            "1".to_owned(),
            "--dpi".to_owned(),
            "300".to_owned(),
            "--min-confidence".to_owned(),
            "0.5".to_owned(),
            "--whitelist".to_owned(),
            "ABC123".to_owned(),
            "--config".to_owned(),
            "preserve_interword_spaces=1".to_owned(),
            "--scale".to_owned(),
            "2".to_owned(),
            "--grayscale".to_owned(),
            "--threshold".to_owned(),
            "180".to_owned(),
            "--invert".to_owned(),
            "--contrast".to_owned(),
            "10".to_owned(),
            "--deskew".to_owned(),
            "--words".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            OcrCommand::Run(Box::new(OcrArgs {
                image: Some(PathBuf::from("tests/fixtures/ocr/sample.png")),
                region: Some(Rect::new(10, 20, 300, 80)),
                app: None,
                window_title: None,
                window_id: None,
                language: Some("eng".to_owned()),
                page_segmentation_mode: Some(6),
                engine_mode: Some(1),
                dpi: Some(300),
                min_confidence: Some(0.5),
                whitelist: Some("ABC123".to_owned()),
                config: vec![OcrConfig {
                    key: "preserve_interword_spaces".to_owned(),
                    value: "1".to_owned()
                }],
                preprocessing: OcrPreprocessingOptions {
                    scale: Some(2.0),
                    grayscale: true,
                    threshold: Some(180),
                    invert: true,
                    contrast: Some(10.0),
                    deskew: true
                },
                json: false,
                words: true
            }))
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
                ignore_regions: Vec::new(),
                per_channel_threshold: 4,
                max_changed_ratio: 0.01,
                max_changed_pixels: None,
                max_mean_absolute_error: None,
                max_channel_delta: None,
                size_policy: VisualSizePolicy::Error,
                alpha_mode: VisualAlphaMode::Ignore,
                diff_output: None,
                report: None,
                no_fail: false,
                json: false
            })
        );
    }

    #[test]
    fn compare_accepts_visual_regression_options() {
        let command = parse_compare_args(vec![
            "--expected".to_owned(),
            "before.png".to_owned(),
            "--actual".to_owned(),
            "after.png".to_owned(),
            "--ignore-region".to_owned(),
            "1,2,3,4".to_owned(),
            "--ignore-region".to_owned(),
            "5,6,7,8".to_owned(),
            "--max-changed-pixels".to_owned(),
            "12".to_owned(),
            "--max-mae".to_owned(),
            "3.5".to_owned(),
            "--max-channel-delta".to_owned(),
            "9".to_owned(),
            "--size-policy".to_owned(),
            "common-region".to_owned(),
            "--alpha".to_owned(),
            "compare".to_owned(),
            "--diff-output".to_owned(),
            "diff.png".to_owned(),
            "--report".to_owned(),
            "report.json".to_owned(),
            "--no-fail".to_owned(),
            "--json".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            CompareCommand::Run(CompareArgs {
                expected: PathBuf::from("before.png"),
                actual: PathBuf::from("after.png"),
                region: None,
                ignore_regions: vec![Rect::new(1, 2, 3, 4), Rect::new(5, 6, 7, 8)],
                per_channel_threshold: 0,
                max_changed_ratio: 0.0,
                max_changed_pixels: Some(12),
                max_mean_absolute_error: Some(3.5),
                max_channel_delta: Some(9),
                size_policy: VisualSizePolicy::CommonRegion,
                alpha_mode: VisualAlphaMode::Compare,
                diff_output: Some(PathBuf::from("diff.png")),
                report: Some(PathBuf::from("report.json")),
                no_fail: true,
                json: true
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
            "--ignore-region".to_owned(),
            "11,22,33,44".to_owned(),
            "--stable-max-changed-pixels".to_owned(),
            "9".to_owned(),
            "--stable-max-mae".to_owned(),
            "1.5".to_owned(),
            "--stable-max-channel-delta".to_owned(),
            "12".to_owned(),
            "--loading-min-changed-pixels".to_owned(),
            "10".to_owned(),
            "--size-policy".to_owned(),
            "resize-actual".to_owned(),
            "--alpha".to_owned(),
            "compare".to_owned(),
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
                ignore_regions: vec![Rect::new(11, 22, 33, 44)],
                per_channel_threshold: 4,
                stable_max_changed_ratio: 0.002,
                stable_max_changed_pixels: Some(9),
                stable_max_mean_absolute_error: Some(1.5),
                stable_max_channel_delta: Some(12),
                loading_min_changed_ratio: 0.03,
                loading_min_changed_pixels: Some(10),
                required_stable_transitions: 2,
                size_policy: VisualSizePolicy::ResizeActual,
                alpha_mode: VisualAlphaMode::Compare,
                json: false
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
    fn ui_state_rejects_inverted_absolute_thresholds() {
        let error = parse_ui_state_args(vec![
            "first.png".to_owned(),
            "second.png".to_owned(),
            "--stable-max-changed-pixels".to_owned(),
            "10".to_owned(),
            "--loading-min-changed-pixels".to_owned(),
            "2".to_owned(),
        ])
        .unwrap_err();

        assert_eq!(
            error,
            CliError::Failure(
                "--stable-max-changed-pixels must be less than or equal to --loading-min-changed-pixels"
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
            "--ignore-region".to_owned(),
            "10,20,30,40".to_owned(),
            "--min-confidence".to_owned(),
            "0.72".to_owned(),
            "--max-width".to_owned(),
            "200".to_owned(),
            "--max-height".to_owned(),
            "100".to_owned(),
            "--min-area".to_owned(),
            "63".to_owned(),
            "--max-area".to_owned(),
            "2000".to_owned(),
            "--padding".to_owned(),
            "4".to_owned(),
            "--sort".to_owned(),
            "confidence".to_owned(),
            "--mask-output".to_owned(),
            "mask.png".to_owned(),
            "--overlay-output".to_owned(),
            "overlay.png".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            VisionElementsCommand::Run(VisionElementsArgs {
                image: PathBuf::from("screen.png"),
                region: Some(Rect::new(10, 20, 300, 80)),
                ignore_regions: vec![Rect::new(10, 20, 30, 40)],
                edge_threshold: 32,
                min_width: 9,
                min_height: 7,
                min_component_pixels: 20,
                min_confidence: Some(0.72),
                max_width: Some(200),
                max_height: Some(100),
                min_area: Some(63),
                max_area: Some(2000),
                max_elements: 12,
                merge_distance: 3,
                padding: 4,
                sort: UiElementSort::Confidence,
                mask_output: Some(PathBuf::from("mask.png")),
                overlay_output: Some(PathBuf::from("overlay.png")),
                json: false
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
    fn desktop_profiles_accepts_no_arguments() {
        let command = parse_desktop_args(vec!["profiles".to_owned()]).unwrap();

        assert_eq!(
            command,
            DesktopCommand::Profiles(DesktopProfilesArgs {
                json: false,
                app: None
            })
        );
    }

    #[test]
    fn desktop_profiles_accepts_json_and_app_filter() {
        let command = parse_desktop_args(vec![
            "profiles".to_owned(),
            "--json".to_owned(),
            "--app".to_owned(),
            "telegram".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            DesktopCommand::Profiles(DesktopProfilesArgs {
                json: true,
                app: Some("telegram".to_owned())
            })
        );
    }

    #[test]
    fn desktop_focus_accepts_app_and_wait_options() {
        let command = parse_desktop_args(vec![
            "focus".to_owned(),
            "--app".to_owned(),
            "telegram".to_owned(),
            "--no-overview".to_owned(),
            "--wait-ms".to_owned(),
            "250".to_owned(),
            "--overview-wait-ms".to_owned(),
            "125".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            DesktopCommand::Focus(DesktopFocusArgs {
                app: "telegram".to_owned(),
                use_gnome_overview: false,
                launch_if_needed: true,
                wait_after_focus_ms: 250,
                overview_wait_ms: 125,
                window_title: None,
                window_id: None,
                verify: false,
                json: false
            })
        );
    }

    #[test]
    fn desktop_focus_accepts_window_title_filter() {
        let command = parse_desktop_args(vec![
            "focus".to_owned(),
            "--app".to_owned(),
            "text-editor".to_owned(),
            "--window-title".to_owned(),
            "peekaboox-draft.txt".to_owned(),
            "--no-launch".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            DesktopCommand::Focus(DesktopFocusArgs {
                app: "text-editor".to_owned(),
                use_gnome_overview: true,
                launch_if_needed: false,
                wait_after_focus_ms: 1_000,
                overview_wait_ms: 800,
                window_title: Some("peekaboox-draft.txt".to_owned()),
                window_id: None,
                verify: false,
                json: false
            })
        );
    }

    #[test]
    fn desktop_focus_accepts_window_id_verify_and_json() {
        let command = parse_desktop_args(vec![
            "focus".to_owned(),
            "--app".to_owned(),
            "telegram".to_owned(),
            "--window-id".to_owned(),
            "123".to_owned(),
            "--verify".to_owned(),
            "--json".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            DesktopCommand::Focus(DesktopFocusArgs {
                app: "telegram".to_owned(),
                use_gnome_overview: true,
                launch_if_needed: true,
                wait_after_focus_ms: 1_000,
                overview_wait_ms: 800,
                window_title: None,
                window_id: Some("123".to_owned()),
                verify: true,
                json: true
            })
        );
    }

    #[test]
    fn desktop_locate_accepts_positional_app_and_target() {
        let command = parse_desktop_args(vec![
            "locate".to_owned(),
            "telegram".to_owned(),
            "send-button".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            DesktopCommand::Locate(DesktopLocateArgs {
                app: "telegram".to_owned(),
                target: "send-button".to_owned(),
                image: None,
                prefer_accessibility: true,
                window_title: None,
                window_id: None,
                json: false
            })
        );
    }

    #[test]
    fn desktop_click_accepts_button_dry_run_and_image() {
        let command = parse_desktop_args(vec![
            "click".to_owned(),
            "--app".to_owned(),
            "telegram".to_owned(),
            "--target".to_owned(),
            "search-input".to_owned(),
            "--button".to_owned(),
            "right".to_owned(),
            "--image".to_owned(),
            "screen.png".to_owned(),
            "--dry-run".to_owned(),
            "--no-accessibility".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            DesktopCommand::Click(DesktopClickArgs {
                app: "telegram".to_owned(),
                target: "search-input".to_owned(),
                image: Some(PathBuf::from("screen.png")),
                prefer_accessibility: false,
                window_title: None,
                window_id: None,
                button: MouseButton::Right,
                dry_run: true,
                verify: false,
                json: false
            })
        );
    }

    #[test]
    fn desktop_drag_accepts_ratio_endpoints() {
        let command = parse_desktop_args(vec![
            "drag".to_owned(),
            "--app".to_owned(),
            "drawing".to_owned(),
            "--target".to_owned(),
            "canvas".to_owned(),
            "--from-ratio".to_owned(),
            "0.2,0.3".to_owned(),
            "--to-ratio".to_owned(),
            "0.8,0.7".to_owned(),
            "--duration-ms".to_owned(),
            "400".to_owned(),
            "--dry-run".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            DesktopCommand::Drag(DesktopDragArgs {
                app: "drawing".to_owned(),
                target: "canvas".to_owned(),
                image: None,
                prefer_accessibility: true,
                window_title: None,
                window_id: None,
                button: MouseButton::Left,
                from_ratio: (0.2, 0.3),
                to_ratio: (0.8, 0.7),
                duration_ms: 400,
                dry_run: true,
                verify: false,
                json: false
            })
        );
    }

    #[test]
    fn desktop_type_into_joins_text() {
        let command = parse_desktop_args(vec![
            "type-into".to_owned(),
            "telegram".to_owned(),
            "message-input".to_owned(),
            "--clear".to_owned(),
            "PeekabooX".to_owned(),
            "Example".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            DesktopCommand::TypeInto(DesktopTypeIntoArgs {
                app: "telegram".to_owned(),
                target: "message-input".to_owned(),
                text: "PeekabooX Example".to_owned(),
                image: None,
                prefer_accessibility: true,
                window_title: None,
                window_id: None,
                clear: true,
                dry_run: false,
                verify: false,
                json: false
            })
        );
    }

    #[test]
    fn desktop_assert_not_active_maps_to_not_active_guard() {
        let command = parse_desktop_args(vec![
            "assert".to_owned(),
            "--app".to_owned(),
            "telegram".to_owned(),
            "--target".to_owned(),
            "send-button".to_owned(),
            "--not-active".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            DesktopCommand::Assert(DesktopAssertArgs {
                app: "telegram".to_owned(),
                target: "send-button".to_owned(),
                image: None,
                prefer_accessibility: true,
                window_title: None,
                window_id: None,
                assertion: DesktopAssertion::NotActive,
                json: false
            })
        );
    }

    #[test]
    fn desktop_assert_not_negates_contains() {
        let command = parse_desktop_args(vec![
            "assert-not".to_owned(),
            "telegram".to_owned(),
            "header".to_owned(),
            "--contains".to_owned(),
            "Alerts".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            DesktopCommand::Assert(DesktopAssertArgs {
                app: "telegram".to_owned(),
                target: "header".to_owned(),
                image: None,
                prefer_accessibility: true,
                window_title: None,
                window_id: None,
                assertion: DesktopAssertion::NotContains("Alerts".to_owned()),
                json: false
            })
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
                dry_run: false,
                paste: false,
                preserve_clipboard: false
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
                dry_run: true,
                paste: false,
                preserve_clipboard: false
            })
        );
    }

    #[test]
    fn type_accepts_paste_flag() {
        let command = parse_type_args(vec!["--paste".to_owned(), "hello".to_owned()]).unwrap();

        assert_eq!(
            command,
            TypeCommand::Run(TypeArgs {
                text: "hello".to_owned(),
                dry_run: false,
                paste: true,
                preserve_clipboard: false
            })
        );
    }

    #[test]
    fn type_accepts_preserve_clipboard_for_paste() {
        let command = parse_type_args(vec![
            "--paste".to_owned(),
            "--preserve-clipboard".to_owned(),
            "hello".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            TypeCommand::Run(TypeArgs {
                text: "hello".to_owned(),
                dry_run: false,
                paste: true,
                preserve_clipboard: true
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
