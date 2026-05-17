use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

mod capture;
mod common;
mod desktop;
mod doctor;
mod input;
mod legacy;
mod plugins;
mod usage;
mod vision;

use capture::*;
use common::*;
use desktop::*;
use input::*;
use legacy::*;
use plugins::*;
use usage::*;
use vision::*;

use peekaboox_accessibility::{AccessibilityTreeMetadata, ElementQuery};
use peekaboox_core::{BackendKind, Point, Rect, UiElement, WindowInfo};
use peekaboox_desktop::{
    AssertOptions, ClickOptions as DesktopClickOptions, DesktopAssertion, DesktopDragOptions,
    DesktopProfileQuery, FocusOptions, LocateOptions, TypeIntoOptions,
};
use peekaboox_input::MouseButton;
use peekaboox_ipc::{
    ActionResultDto, ApiRequest, ApiResponse, ApiResult, CaptureBackendDto, CaptureBackendProbeDto,
    CaptureBackendProbeResultDto, CaptureBackendsResultDto, CaptureDeltaResultDto,
    CaptureResultDto, DesktopActionResultDto, DesktopAssertionDto, DesktopLocateResultDto,
    DesktopProfileDto, DesktopProfilesResultDto, DmaBufImportTargetDto, DmaBufProbeResultDto,
    ElementDto, ElementListResultDto, MouseButtonDto, OcrBlockDto, OcrResultDto,
    PluginDiscoveryErrorDto, PluginDto, PluginListResultDto, PluginToolDto,
    PluginToolExecutionResultDto, RectDto, UiStateDto, VisualDiffDto, WindowBackendReportDto,
    WindowDto, WindowListResultDto, ZeroCopyBackendDto, default_socket_path, send_request,
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
        Some("image") => match capture(args.collect(), &global.context) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("image failed: {error}");
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
        Some("window") => match window_command(args.collect()) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("window failed: {error}");
                std::process::exit(1);
            }
        },
        Some("app") => match app_command(args.collect()) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("app failed: {error}");
                std::process::exit(1);
            }
        },
        Some("launcher") | Some("dock") => match launcher_command(args.collect()) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("launcher failed: {error}");
                std::process::exit(1);
            }
        },
        Some("workspace") | Some("workspaces") => match workspace_command(args.collect()) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("workspace failed: {error}");
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
        Some("see") | Some("observe") => match see(args.collect(), &global.context) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("see failed: {error}");
                std::process::exit(1);
            }
        },
        Some("set-value") => match set_value_command(args.collect()) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("set-value failed: {error}");
                std::process::exit(1);
            }
        },
        Some("perform-action") => match perform_action_command(args.collect()) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("perform-action failed: {error}");
                std::process::exit(1);
            }
        },
        Some("dialog") => match dialog_command(args.collect(), &global.context) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("dialog failed: {error}");
                std::process::exit(1);
            }
        },
        Some("menu") | Some("menubar") | Some("status-items") | Some("status-item") => {
            match menu_command(args.collect(), &global.context) {
                Ok(()) => {}
                Err(CliError::HelpRequested) => {}
                Err(CliError::Failure(error)) => {
                    eprintln!("menu failed: {error}");
                    std::process::exit(1);
                }
            }
        }
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
        Some("diagnose") => match doctor::diagnose(args.collect()) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("diagnose failed: {error}");
                std::process::exit(1);
            }
        },
        Some("agent") => match agent_command(args.collect(), &global.context) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("agent failed: {error}");
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
        Some("swipe") => match swipe(args.collect(), &global.context) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("swipe failed: {error}");
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
        Some("press") => match press(args.collect(), &global.context) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("press failed: {error}");
                std::process::exit(1);
            }
        },
        Some("scroll") => match scroll(args.collect()) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("scroll failed: {error}");
                std::process::exit(1);
            }
        },
        Some("tools") => match tools_command(args.collect()) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("tools failed: {error}");
                std::process::exit(1);
            }
        },
        Some("completions") | Some("completion") => match completions_command(args.collect()) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("completions failed: {error}");
                std::process::exit(1);
            }
        },
        Some("config") => match config_command(args.collect()) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("config failed: {error}");
                std::process::exit(1);
            }
        },
        Some("permissions") | Some("permission") => match permissions_command(args.collect()) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("permissions failed: {error}");
                std::process::exit(1);
            }
        },
        Some("clean") => match clean_command(args.collect()) {
            Ok(()) => {}
            Err(CliError::HelpRequested) => {}
            Err(CliError::Failure(error)) => {
                eprintln!("clean failed: {error}");
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
enum CliError {
    HelpRequested,
    Failure(String),
}

#[cfg(test)]
mod tests;
