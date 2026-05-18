use std::collections::HashMap;
use std::ffi::CString;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use dbus::blocking::Connection;
use dbus::message::MatchRule;
use dbus::{Message, MessageType};
use peekaboox_accessibility::{AccessibilityTreeMetadata, ElementQuery};
use peekaboox_core::{BackendKind, CaptureFrame, PixelFormat, Point, Rect, UiElement, WindowInfo};
use peekaboox_desktop::{
    AssertOptions as DesktopAssertOptions, ClickOptions as DesktopClickOptions, DesktopAssertion,
    DesktopDragOptions, DesktopProfileQuery, FocusOptions as DesktopFocusOptions,
    LocateOptions as DesktopLocateOptions, TypeIntoOptions as DesktopTypeIntoOptions,
};
use peekaboox_input::{EMERGENCY_STOP_HOTKEY_LABEL, EmergencyHotkeyState, MouseButton};
use peekaboox_ipc::proto::{
    self, capture_target,
    peekaboo_x_server::{PeekabooX, PeekabooXServer},
};
use peekaboox_ipc::{
    API_VERSION, ActionResultDto, ApiRequest, ApiResponseEnvelope, ApiResult, CaptureBackendDto,
    CaptureBackendProbeDto, CaptureBackendProbeResultDto, CaptureBackendsResultDto,
    CaptureDeltaResultDto, CaptureResultDto, DesktopActionResultDto, DesktopAssertionDto,
    DesktopLocateResultDto, DesktopProfileAvailabilityDto, DesktopProfileCommandDto,
    DesktopProfileDto, DesktopProfileTargetDto, DesktopProfilesResultDto, DmaBufImportTargetDto,
    DmaBufProbeResultDto, ElementDto, ElementListResultDto, MouseButtonDto, OcrBlockDto,
    OcrResultDto, PluginDiscoveryErrorDto, PluginDto, PluginListResultDto, PluginToolDto,
    PluginToolExecutionResultDto, PointDto, RectDto, UiStateDto, VisualDiffDto,
    WindowBackendReportDto, WindowDto, WindowListResultDto, ZeroCopyBackendDto, decode_request,
    default_socket_path, encode_response,
};
use peekaboox_vision::{
    IncrementalCaptureDelta, IncrementalCaptureOptions, OcrConfig, OcrOptions,
    OcrPreprocessingOptions, OcrResult, TesseractOcrBackend, UiElementDetectionOptions,
    UiElementSort, UiStateKind, UiStateOptions, UiStateResult, VisualAlphaMode,
    VisualCompareOptions, VisualDiffResult, VisualSizePolicy,
};
use serde_json::json;
use signal_hook::consts::signal::{SIGINT, SIGTERM};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Request, Response, Status};

mod capture;
mod dispatch;
mod dto;
mod events;
mod grpc_service;
mod input;
mod policy;
mod runtime;
mod state;
mod vision;
mod windows;

use capture::*;
use dispatch::*;
use dto::*;
use events::*;
use grpc_service::*;
use input::*;
use policy::*;
use runtime::*;
use state::*;
use vision::*;
use windows::*;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_GRPC_ADDR: &str = "127.0.0.1:47777";
const DEFAULT_ACCESSIBILITY_CACHE_TTL_MS: u64 = 500;
const MAX_INCREMENTAL_CAPTURE_STREAMS: usize = 64;
const MAX_CAPTURE_STREAM_ID_LEN: usize = 128;
const VISION_UI_BACKEND_NAME: &str = "heuristic_vision";
const VISION_UI_BACKEND_KIND: &str = "vision";
const ATSPI_EVENT_REGISTRY_DESTINATION: &str = "org.a11y.atspi.Registry";
const ATSPI_EVENT_REGISTRY_PATH: &str = "/org/a11y/atspi/registry";
const ATSPI_EVENT_REGISTRY_INTERFACE: &str = "org.a11y.atspi.Registry";
const ATSPI_EVENT_REGISTRATIONS: &[&str] = &[
    "focus:",
    "window:",
    "object:children-changed",
    "object:property-change",
    "object:state-changed",
    "object:text-changed",
    "object:visible-data-changed",
];
const ATSPI_EVENT_INTERFACES: &[&str] = &[
    "org.a11y.atspi.Event.Focus",
    "org.a11y.atspi.Event.Object",
    "org.a11y.atspi.Event.Window",
];
const INPUT_EVENT_DIR: &str = "/dev/input";
static TEMP_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

fn main() {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => {}
        Err(error) => {
            eprintln!("peekabooxd failed: {error}");
            std::process::exit(1);
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let command = parse_args(args)?;

    match command {
        DaemonCommand::Version => {
            println!("peekabooxd {VERSION}");
            Ok(())
        }
        DaemonCommand::Run { config } => run_server(config),
        DaemonCommand::Help => {
            print_usage();
            Ok(())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DaemonCommand {
    Version,
    Run { config: ServerConfig },
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DaemonPolicyProfile {
    Observe,
    Assist,
    Operator,
}

impl DaemonPolicyProfile {
    fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "observe" | "locked" | "read-only" | "readonly" => Ok(Self::Observe),
            "assist" | "inspect" => Ok(Self::Assist),
            "operator" | "trusted" | "full" => Ok(Self::Operator),
            unknown => Err(format!(
                "unknown daemon profile {unknown:?}; expected one of: observe, assist, operator"
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Observe => "observe",
            Self::Assist => "assist",
            Self::Operator => "operator",
        }
    }

    fn allow_input(self) -> bool {
        matches!(self, Self::Operator)
    }

    fn allow_plugins(self) -> bool {
        matches!(self, Self::Operator)
    }

    fn vision_fallback(self) -> bool {
        matches!(self, Self::Assist | Self::Operator)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SandboxProfile {
    Off,
    Basic,
    Strict,
}

impl SandboxProfile {
    fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "off" | "none" | "disabled" => Ok(Self::Off),
            "basic" | "no-new-privileges" => Ok(Self::Basic),
            "strict" | "namespace" | "namespaces" => Ok(Self::Strict),
            unknown => Err(format!(
                "unknown sandbox profile {unknown:?}; expected one of: off, basic, strict"
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Basic => "basic",
            Self::Strict => "strict",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ServerConfig {
    socket: PathBuf,
    once: bool,
    audit_log: PathBuf,
    policy_profile: DaemonPolicyProfile,
    sandbox_profile: SandboxProfile,
    allow_input: bool,
    allow_plugins: bool,
    vision_fallback: bool,
    grpc_addr: Option<SocketAddr>,
    grpc_token: Option<String>,
    accessibility_cache_ttl: Duration,
    accessibility_events: bool,
    emergency_hotkey: bool,
    plugin_paths: Vec<PathBuf>,
}

fn parse_args(args: Vec<String>) -> Result<DaemonCommand, String> {
    if args.is_empty() {
        return Ok(DaemonCommand::Run {
            config: default_server_config()?,
        });
    }

    match args[0].as_str() {
        "--version" | "-V" => Ok(DaemonCommand::Version),
        "--help" | "-h" => Ok(DaemonCommand::Help),
        "run" => parse_run_args(&args[1..]),
        command => Err(format!("unknown peekabooxd command: {command}")),
    }
}

fn parse_run_args(args: &[String]) -> Result<DaemonCommand, String> {
    let mut config = default_server_config()?;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--socket" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("missing value for --socket".to_owned());
                };
                config.socket = PathBuf::from(value);
            }
            "--audit-log" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("missing value for --audit-log".to_owned());
                };
                config.audit_log = PathBuf::from(value);
            }
            "--profile" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("missing value for --profile".to_owned());
                };
                apply_daemon_policy_profile(&mut config, DaemonPolicyProfile::parse(value)?);
            }
            "--sandbox" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("missing value for --sandbox".to_owned());
                };
                config.sandbox_profile = SandboxProfile::parse(value)?;
            }
            "--allow-input" => config.allow_input = true,
            "--allow-plugins" | "--allow-plugin-exec" => config.allow_plugins = true,
            "--vision-fallback" => config.vision_fallback = true,
            "--grpc-addr" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("missing value for --grpc-addr".to_owned());
                };
                config.grpc_addr = Some(
                    value
                        .parse()
                        .map_err(|error| format!("invalid --grpc-addr {value:?}: {error}"))?,
                );
            }
            "--grpc-token" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("missing value for --grpc-token".to_owned());
                };
                let value = value.trim();
                if value.is_empty() {
                    return Err("--grpc-token must not be empty".to_owned());
                }
                config.grpc_token = Some(value.to_owned());
            }
            "--no-grpc" => config.grpc_addr = None,
            "--accessibility-cache-ttl-ms" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("missing value for --accessibility-cache-ttl-ms".to_owned());
                };
                config.accessibility_cache_ttl =
                    Duration::from_millis(value.parse().map_err(|error| {
                        format!("invalid --accessibility-cache-ttl-ms {value:?}: {error}")
                    })?);
            }
            "--no-accessibility-events" => config.accessibility_events = false,
            "--no-emergency-hotkey" => config.emergency_hotkey = false,
            "--plugin-path" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("missing value for --plugin-path".to_owned());
                };
                config.plugin_paths.push(PathBuf::from(value));
            }
            "--once" => config.once = true,
            "--help" | "-h" => return Ok(DaemonCommand::Help),
            unknown => return Err(format!("unknown run argument: {unknown}")),
        }

        index += 1;
    }

    Ok(DaemonCommand::Run { config })
}

fn default_server_config() -> Result<ServerConfig, String> {
    let profile = daemon_policy_profile_from_env()?;
    let sandbox_profile = sandbox_profile_from_env()?;
    let mut config = server_config_for_profile(profile);
    config.sandbox_profile = sandbox_profile;
    config.allow_input = config.allow_input || input_allowed_from_env();
    config.allow_plugins = config.allow_plugins || plugin_execution_allowed_from_env();
    config.vision_fallback = config.vision_fallback || vision_fallback_from_env();
    Ok(config)
}

fn server_config_for_profile(policy_profile: DaemonPolicyProfile) -> ServerConfig {
    ServerConfig {
        socket: default_socket_path(),
        once: false,
        audit_log: default_audit_log_path(),
        policy_profile,
        sandbox_profile: SandboxProfile::Off,
        allow_input: policy_profile.allow_input(),
        allow_plugins: policy_profile.allow_plugins(),
        vision_fallback: policy_profile.vision_fallback(),
        grpc_addr: Some(default_grpc_addr()),
        grpc_token: grpc_token_from_env(),
        accessibility_cache_ttl: default_accessibility_cache_ttl(),
        accessibility_events: true,
        emergency_hotkey: emergency_hotkey_enabled_from_env(),
        plugin_paths: Vec::new(),
    }
}

fn apply_daemon_policy_profile(config: &mut ServerConfig, policy_profile: DaemonPolicyProfile) {
    config.policy_profile = policy_profile;
    config.allow_input = policy_profile.allow_input();
    config.allow_plugins = policy_profile.allow_plugins();
    config.vision_fallback = policy_profile.vision_fallback();
}

fn apply_sandbox_profile(profile: SandboxProfile) -> Result<Vec<&'static str>, String> {
    let mut applied = Vec::new();
    match profile {
        SandboxProfile::Off => Ok(applied),
        SandboxProfile::Basic => {
            apply_no_new_privileges()?;
            applied.push("no_new_privileges");
            disable_process_dumping()?;
            applied.push("non_dumpable");
            set_private_file_creation_mask();
            applied.push("umask_077");
            Ok(applied)
        }
        SandboxProfile::Strict => {
            apply_no_new_privileges()?;
            applied.push("no_new_privileges");
            disable_process_dumping()?;
            applied.push("non_dumpable");
            set_private_file_creation_mask();
            applied.push("umask_077");
            unshare_user_namespace()?;
            applied.push("user_namespace");
            write_user_namespace_id_maps()?;
            applied.push("user_namespace_id_map");
            unshare_flags(
                libc::CLONE_NEWNS | libc::CLONE_NEWIPC,
                "mount/ipc namespaces",
            )?;
            applied.push("mount_namespace");
            applied.push("ipc_namespace");
            make_root_mounts_private()?;
            applied.push("private_mount_propagation");
            Ok(applied)
        }
    }
}

fn apply_no_new_privileges() -> Result<(), String> {
    let result = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
    if result == -1 {
        return Err(format!(
            "failed to enable no_new_privileges: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

fn disable_process_dumping() -> Result<(), String> {
    let result = unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) };
    if result == -1 {
        return Err(format!(
            "failed to disable process dumping: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

fn set_private_file_creation_mask() {
    unsafe {
        libc::umask(0o077);
    }
}

fn unshare_user_namespace() -> Result<(), String> {
    unshare_flags(libc::CLONE_NEWUSER, "user namespace")
}

fn unshare_flags(flags: libc::c_int, label: &str) -> Result<(), String> {
    let result = unsafe { libc::unshare(flags) };
    if result == -1 {
        return Err(format!(
            "failed to unshare {label}: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

fn write_user_namespace_id_maps() -> Result<(), String> {
    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };
    write_optional_proc_file("/proc/self/setgroups", "deny\n")?;
    fs::write("/proc/self/uid_map", format!("0 {uid} 1\n"))
        .map_err(|error| format!("failed to write /proc/self/uid_map: {error}"))?;
    fs::write("/proc/self/gid_map", format!("0 {gid} 1\n"))
        .map_err(|error| format!("failed to write /proc/self/gid_map: {error}"))?;
    Ok(())
}

fn write_optional_proc_file(path: &str, contents: &str) -> Result<(), String> {
    match fs::write(path, contents) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("failed to write {path}: {error}")),
    }
}

fn make_root_mounts_private() -> Result<(), String> {
    let target = CString::new("/").expect("literal mount target is valid");
    let result = unsafe {
        libc::mount(
            std::ptr::null(),
            target.as_ptr(),
            std::ptr::null(),
            (libc::MS_REC | libc::MS_PRIVATE) as libc::c_ulong,
            std::ptr::null(),
        )
    };
    if result == -1 {
        return Err(format!(
            "failed to make root mount propagation private: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

fn run_server(config: ServerConfig) -> Result<(), String> {
    let audit = Arc::new(Mutex::new(AuditLogger::new(config.audit_log.clone())?));
    let sandbox_steps = match apply_sandbox_profile(config.sandbox_profile) {
        Ok(steps) => {
            audit_write(
                &audit,
                "sandbox_applied",
                None,
                "ok",
                None,
                json!({
                    "sandbox_profile": config.sandbox_profile.as_str(),
                    "steps": steps
                }),
            );
            steps
        }
        Err(error) => {
            audit_write(
                &audit,
                "sandbox_applied",
                None,
                "error",
                Some(&error),
                json!({ "sandbox_profile": config.sandbox_profile.as_str() }),
            );
            return Err(error);
        }
    };
    prepare_socket_path(&config.socket)?;
    let listener = UnixListener::bind(&config.socket)
        .map_err(|error| format!("failed to bind {}: {error}", config.socket.display()))?;
    let _socket_guard = SocketGuard::new(config.socket.clone())?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("failed to configure nonblocking socket: {error}"))?;
    let shutdown = install_shutdown_handler()?;
    let accessibility_cache = Arc::new(Mutex::new(AccessibilityCache::new(
        config.accessibility_cache_ttl,
    )));
    let incremental_capture_state = Arc::new(Mutex::new(IncrementalCaptureState::default()));
    let accessibility_events_handle = spawn_accessibility_event_listener(
        &config,
        Arc::clone(&accessibility_cache),
        Arc::clone(&audit),
        Arc::clone(&shutdown),
    );
    let emergency_hotkey_handle =
        spawn_emergency_hotkey_listener(&config, Arc::clone(&audit), Arc::clone(&shutdown));
    let grpc_handle = spawn_grpc_server(
        &config,
        Arc::clone(&audit),
        Arc::clone(&accessibility_cache),
        Arc::clone(&incremental_capture_state),
        Arc::clone(&shutdown),
    )?;

    println!("peekabooxd listening on {}", config.socket.display());
    audit_write(
        &audit,
        "daemon_started",
        None,
        "ok",
        None,
        json!({
            "socket": config.socket.display().to_string(),
            "audit_log": config.audit_log.display().to_string(),
            "policy_profile": config.policy_profile.as_str(),
            "sandbox_profile": config.sandbox_profile.as_str(),
            "sandbox_steps": sandbox_steps,
            "allow_input": config.allow_input,
            "allow_plugins": config.allow_plugins,
            "vision_fallback": config.vision_fallback,
            "once": config.once,
            "grpc_addr": config.grpc_addr.map(|addr| addr.to_string()),
            "grpc_auth": config.grpc_token.as_ref().map(|_| "token").unwrap_or("loopback-only"),
            "accessibility_cache_ttl_ms": config.accessibility_cache_ttl.as_millis(),
            "accessibility_events": config.accessibility_events,
            "emergency_hotkey": config.emergency_hotkey,
            "emergency_hotkey_label": EMERGENCY_STOP_HOTKEY_LABEL
        }),
    );

    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _addr)) => {
                handle_stream(
                    stream,
                    &config,
                    &audit,
                    &accessibility_cache,
                    &incremental_capture_state,
                );
                if config.once {
                    break;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(error) => {
                eprintln!("peekabooxd connection failed: {error}");
                audit_write(
                    &audit,
                    "connection",
                    None,
                    "error",
                    Some(&error.to_string()),
                    json!({}),
                );
            }
        }
    }

    shutdown.store(true, Ordering::Relaxed);
    if let Some(handle) = grpc_handle
        && let Err(error) = handle.join()
    {
        eprintln!("peekabooxd grpc thread failed to join: {error:?}");
    }
    if let Some(handle) = accessibility_events_handle
        && let Err(error) = handle.join()
    {
        eprintln!("peekabooxd accessibility event thread failed to join: {error:?}");
    }
    if let Some(handle) = emergency_hotkey_handle
        && let Err(error) = handle.join()
    {
        eprintln!("peekabooxd emergency hotkey thread failed to join: {error:?}");
    }

    perform_emergency_stop(&audit, "daemon_shutdown");
    audit_write(&audit, "daemon_stopped", None, "ok", None, json!({}));
    Ok(())
}

type SharedAudit = Arc<Mutex<AuditLogger>>;
type SharedAccessibilityCache = Arc<Mutex<AccessibilityCache>>;
type SharedIncrementalCaptureState = Arc<Mutex<IncrementalCaptureState>>;
type WindowListProvider = fn(
    peekaboox_windows::WindowQuery,
) -> peekaboox_core::Result<peekaboox_windows::WindowListMetadata>;

#[cfg(test)]
mod tests;
