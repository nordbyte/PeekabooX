use std::collections::HashMap;
use std::ffi::CString;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::os::unix::fs::{FileTypeExt, OpenOptionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
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
    DesktopDragOptions, FocusOptions as DesktopFocusOptions, LocateOptions as DesktopLocateOptions,
    TypeIntoOptions as DesktopTypeIntoOptions,
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
    DesktopLocateResultDto, DmaBufImportTargetDto, DmaBufProbeResultDto, ElementDto,
    ElementListResultDto, MouseButtonDto, OcrBlockDto, OcrResultDto, PluginDiscoveryErrorDto,
    PluginDto, PluginListResultDto, PluginToolDto, PluginToolExecutionResultDto, PointDto, RectDto,
    UiStateDto, VisualDiffDto, WindowBackendReportDto, WindowDto, WindowListResultDto,
    ZeroCopyBackendDto, decode_request, default_socket_path, encode_response,
};
use peekaboox_vision::{
    IncrementalCaptureDelta, IncrementalCaptureOptions, OcrConfig, OcrOptions,
    OcrPreprocessingOptions, OcrResult, TesseractOcrBackend, UiElementDetectionOptions,
    UiStateKind, UiStateOptions, UiStateResult, VisualCompareOptions, VisualDiffResult,
};
use serde_json::json;
use signal_hook::consts::signal::{SIGINT, SIGTERM};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Request, Response, Status};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_GRPC_ADDR: &str = "127.0.0.1:47777";
const DEFAULT_ACCESSIBILITY_CACHE_TTL_MS: u64 = 500;
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
    vision_fallback: bool,
    grpc_addr: Option<SocketAddr>,
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
        vision_fallback: policy_profile.vision_fallback(),
        grpc_addr: Some(default_grpc_addr()),
        accessibility_cache_ttl: default_accessibility_cache_ttl(),
        accessibility_events: true,
        emergency_hotkey: emergency_hotkey_enabled_from_env(),
        plugin_paths: Vec::new(),
    }
}

fn apply_daemon_policy_profile(config: &mut ServerConfig, policy_profile: DaemonPolicyProfile) {
    config.policy_profile = policy_profile;
    config.allow_input = policy_profile.allow_input();
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
    let _socket_guard = SocketGuard {
        path: config.socket.clone(),
    };
    let listener = UnixListener::bind(&config.socket)
        .map_err(|error| format!("failed to bind {}: {error}", config.socket.display()))?;
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
            "vision_fallback": config.vision_fallback,
            "once": config.once,
            "grpc_addr": config.grpc_addr.map(|addr| addr.to_string()),
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

#[derive(Debug, Default)]
struct IncrementalCaptureState {
    streams: HashMap<String, IncrementalCaptureStream>,
}

#[derive(Debug)]
struct IncrementalCaptureStream {
    sequence: u64,
    frame: CaptureFrame,
}

#[derive(Debug)]
struct CapturedFrame {
    frame: CaptureFrame,
    backend_name: String,
    backend_kind: BackendKind,
    captured_at_unix_ms: u64,
}

#[derive(Debug)]
struct CaptureDeltaData {
    stream_id: String,
    delta: IncrementalCaptureDelta,
    low_bandwidth: bool,
    capture_region: Option<Rect>,
    backend_name: String,
    backend_kind: BackendKind,
    captured_at_unix_ms: u64,
}

#[derive(Debug)]
struct AccessibilityCache {
    ttl: Duration,
    snapshot: Option<AccessibilityCacheSnapshot>,
}

#[derive(Debug, Clone)]
struct AccessibilityCacheSnapshot {
    loaded_at: Instant,
    metadata: AccessibilityTreeMetadata,
}

#[derive(Debug, Clone)]
struct CachedAccessibilityTree {
    metadata: AccessibilityTreeMetadata,
    cache_hit: bool,
    age_ms: u128,
}

#[derive(Debug, Clone)]
struct ElementLookupResult {
    backend_name: String,
    backend_kind: String,
    warnings: Vec<String>,
    elements: Vec<UiElement>,
    cache_hit: bool,
    cache_age_ms: u128,
    vision_fallback_used: bool,
}

#[derive(Debug, Clone, Default)]
struct ElementLookupScope {
    app: Option<String>,
    window_title: Option<String>,
    window_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct ElementVisionFallbackConfig {
    region: Option<Rect>,
    options: UiElementDetectionOptions,
}

#[derive(Debug, Clone, Default)]
struct ElementLookupOptions {
    scope: ElementLookupScope,
    vision: ElementVisionFallbackConfig,
}

impl AccessibilityCache {
    fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            snapshot: None,
        }
    }

    fn fresh(&self) -> Option<CachedAccessibilityTree> {
        let snapshot = self.snapshot.as_ref()?;
        let age = snapshot.loaded_at.elapsed();
        if age > self.ttl {
            return None;
        }

        Some(CachedAccessibilityTree {
            metadata: snapshot.metadata.clone(),
            cache_hit: true,
            age_ms: age.as_millis(),
        })
    }

    fn store(&mut self, metadata: AccessibilityTreeMetadata) -> CachedAccessibilityTree {
        self.snapshot = Some(AccessibilityCacheSnapshot {
            loaded_at: Instant::now(),
            metadata: metadata.clone(),
        });

        CachedAccessibilityTree {
            metadata,
            cache_hit: false,
            age_ms: 0,
        }
    }

    fn invalidate(&mut self) -> bool {
        self.snapshot.take().is_some()
    }
}

fn cached_accessibility_tree(
    cache: &SharedAccessibilityCache,
) -> Result<CachedAccessibilityTree, String> {
    {
        let cache = cache
            .lock()
            .map_err(|_| "failed to lock accessibility cache".to_owned())?;
        if let Some(snapshot) = cache.fresh() {
            return Ok(snapshot);
        }
    }

    let metadata = peekaboox_accessibility::semantic_tree().map_err(|error| error.to_string())?;
    let mut cache = cache
        .lock()
        .map_err(|_| "failed to lock accessibility cache".to_owned())?;
    Ok(cache.store(metadata))
}

fn find_elements_with_optional_vision_fallback(
    selector: &str,
    use_vision_fallback: bool,
    options: &ElementLookupOptions,
    accessibility_cache: &SharedAccessibilityCache,
) -> Result<ElementLookupResult, String> {
    element_lookup_with_optional_vision_fallback(
        selector,
        use_vision_fallback,
        options,
        cached_accessibility_tree(accessibility_cache),
        vision_fallback_elements,
    )
}

fn element_lookup_with_optional_vision_fallback(
    selector: &str,
    use_vision_fallback: bool,
    options: &ElementLookupOptions,
    accessibility_result: Result<CachedAccessibilityTree, String>,
    fallback_elements: impl FnOnce(
        &ElementQuery,
        &ElementLookupOptions,
    ) -> Result<ElementLookupResult, String>,
) -> Result<ElementLookupResult, String> {
    let query = ElementQuery::parse(selector).map_err(|error| error.to_string())?;
    match accessibility_result {
        Ok(tree) => {
            let mut metadata = tree.metadata;
            metadata.elements.retain(|element| {
                query.matches(element) && element_matches_scope(element, &options.scope)
            });
            if !metadata.elements.is_empty() || !use_vision_fallback {
                return Ok(ElementLookupResult {
                    backend_name: metadata.backend_name,
                    backend_kind: backend_kind_name(metadata.backend_kind),
                    warnings: metadata.warnings,
                    elements: metadata.elements,
                    cache_hit: tree.cache_hit,
                    cache_age_ms: tree.age_ms,
                    vision_fallback_used: false,
                });
            }

            let mut fallback = fallback_elements(&query, options)?;
            fallback
                .warnings
                .push("no accessibility elements matched; used vision fallback".to_owned());
            Ok(fallback)
        }
        Err(error) if use_vision_fallback => {
            let mut fallback = fallback_elements(&query, options)?;
            fallback.warnings.push(format!(
                "accessibility lookup failed: {error}; used vision fallback"
            ));
            Ok(fallback)
        }
        Err(error) => Err(error),
    }
}

fn vision_fallback_elements(
    query: &ElementQuery,
    options: &ElementLookupOptions,
) -> Result<ElementLookupResult, String> {
    let screenshot = vision_fallback_temp_path();
    let capture_region = element_vision_capture_region(options)?;
    capture_to_file(&screenshot, capture_region).map_err(|error| error.to_string())?;
    let result =
        peekaboox_vision::detect_ui_elements_from_image_file(&screenshot, &options.vision.options)
            .map_err(|error| error.to_string());
    remove_best_effort(&screenshot, "vision fallback screenshot");

    let mut elements = result?;
    apply_element_scope_metadata(&mut elements, &options.scope, capture_region);
    elements.retain(|element| query.matches(element));
    Ok(ElementLookupResult {
        backend_name: VISION_UI_BACKEND_NAME.to_owned(),
        backend_kind: VISION_UI_BACKEND_KIND.to_owned(),
        warnings: Vec::new(),
        elements,
        cache_hit: false,
        cache_age_ms: 0,
        vision_fallback_used: true,
    })
}

fn element_matches_scope(element: &UiElement, scope: &ElementLookupScope) -> bool {
    scope
        .window_id
        .as_deref()
        .is_none_or(|window_id| element.window_id.as_deref() == Some(window_id))
        && scope.window_title.as_deref().is_none_or(|window_title| {
            element
                .window_title
                .as_deref()
                .is_some_and(|value| contains_case_insensitive(value, window_title))
        })
        && scope.app.as_deref().is_none_or(|app| {
            element
                .app_id
                .as_deref()
                .is_some_and(|value| contains_case_insensitive(value, app))
        })
}

fn element_vision_capture_region(options: &ElementLookupOptions) -> Result<Option<Rect>, String> {
    if options.vision.region.is_some() {
        return Ok(options.vision.region);
    }
    if options.scope.window_id.is_none()
        && options.scope.window_title.is_none()
        && options.scope.app.is_none()
    {
        return Ok(None);
    }
    resolve_ocr_window_region(
        None,
        options.scope.window_id.as_deref(),
        options.scope.window_title.as_deref(),
        options.scope.app.as_deref(),
    )
    .map(Some)
    .map_err(|error| error.to_string())
}

fn apply_element_scope_metadata(
    elements: &mut [UiElement],
    scope: &ElementLookupScope,
    capture_region: Option<Rect>,
) {
    for element in elements {
        if let Some(region) = capture_region {
            element.bounds.x += region.x;
            element.bounds.y += region.y;
            element.center = element.bounds.center();
        }
        element.window_id.clone_from(&scope.window_id);
        element.window_title.clone_from(&scope.window_title);
        element.app_id.clone_from(&scope.app);
    }
}

fn contains_case_insensitive(value: &str, needle: &str) -> bool {
    value
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

fn vision_fallback_temp_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "peekaboox-vision-fallback-{}-{}.png",
        std::process::id(),
        unix_time_ms()
    ))
}

fn remove_best_effort(path: &PathBuf, description: &str) {
    if let Err(error) = fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        eprintln!("failed to remove {description} {}: {error}", path.display());
    }
}

fn invalidate_accessibility_cache(cache: &SharedAccessibilityCache) -> bool {
    match cache.lock() {
        Ok(mut cache) => cache.invalidate(),
        Err(_) => false,
    }
}

fn spawn_accessibility_event_listener(
    config: &ServerConfig,
    accessibility_cache: SharedAccessibilityCache,
    audit: SharedAudit,
    shutdown: Arc<AtomicBool>,
) -> Option<std::thread::JoinHandle<()>> {
    if !config.accessibility_events {
        audit_write(
            &audit,
            "accessibility_events_disabled",
            Some(API_VERSION),
            "ok",
            None,
            json!({}),
        );
        return None;
    }

    Some(std::thread::spawn(
        move || match run_accessibility_event_listener(
            Arc::clone(&accessibility_cache),
            Arc::clone(&audit),
            shutdown,
        ) {
            Ok(()) => audit_write(
                &audit,
                "accessibility_events_stopped",
                Some(API_VERSION),
                "ok",
                None,
                json!({}),
            ),
            Err(error) => audit_write(
                &audit,
                "accessibility_events",
                Some(API_VERSION),
                "error",
                Some(&error),
                json!({}),
            ),
        },
    ))
}

fn run_accessibility_event_listener(
    accessibility_cache: SharedAccessibilityCache,
    audit: SharedAudit,
    shutdown: Arc<AtomicBool>,
) -> Result<(), String> {
    let address =
        peekaboox_accessibility::atspi_bus_address().map_err(|error| error.to_string())?;
    let connection = Connection::new_address(&address)
        .map_err(|error| format!("failed to connect to AT-SPI event bus: {error}"))?;
    register_atspi_events(&connection, &audit);
    subscribe_atspi_event_interfaces(&connection, accessibility_cache, Arc::clone(&audit))?;

    audit_write(
        &audit,
        "accessibility_events_started",
        Some(API_VERSION),
        "ok",
        None,
        json!({
            "interfaces": ATSPI_EVENT_INTERFACES,
            "registrations": ATSPI_EVENT_REGISTRATIONS
        }),
    );

    while !shutdown.load(Ordering::Relaxed) {
        connection
            .process(Duration::from_millis(250))
            .map_err(|error| format!("AT-SPI event processing failed: {error}"))?;
    }

    Ok(())
}

fn register_atspi_events(connection: &Connection, audit: &SharedAudit) {
    let proxy = connection.with_proxy(
        ATSPI_EVENT_REGISTRY_DESTINATION,
        ATSPI_EVENT_REGISTRY_PATH,
        Duration::from_secs(2),
    );

    for event_name in ATSPI_EVENT_REGISTRATIONS {
        let result: Result<(), dbus::Error> = proxy.method_call(
            ATSPI_EVENT_REGISTRY_INTERFACE,
            "RegisterEvent",
            (*event_name, Vec::<String>::new(), ""),
        );
        if let Err(error) = result {
            audit_write(
                audit,
                "accessibility_event_registration",
                Some(API_VERSION),
                "error",
                Some(&error.to_string()),
                json!({ "event": event_name }),
            );
        }
    }
}

fn subscribe_atspi_event_interfaces(
    connection: &Connection,
    accessibility_cache: SharedAccessibilityCache,
    audit: SharedAudit,
) -> Result<(), String> {
    connection.set_signal_match_mode(true);
    for interface in ATSPI_EVENT_INTERFACES {
        let rule = atspi_event_match_rule(interface);
        let cache = Arc::clone(&accessibility_cache);
        let audit = Arc::clone(&audit);
        connection
            .add_match(rule, move |_: (), _, message| {
                let reason = atspi_event_reason(message);
                if invalidate_accessibility_cache(&cache) {
                    audit_write(
                        &audit,
                        "accessibility_cache_invalidated",
                        Some(API_VERSION),
                        "ok",
                        None,
                        json!({ "reason": reason }),
                    );
                }
                true
            })
            .map_err(|error| format!("failed to subscribe to AT-SPI events: {error}"))?;
    }

    Ok(())
}

fn atspi_event_match_rule(interface: &'static str) -> MatchRule<'static> {
    let mut rule = MatchRule::new();
    rule.msg_type = Some(MessageType::Signal);
    rule.interface = Some(interface.into());
    rule
}

fn atspi_event_reason(message: &Message) -> String {
    let interface = message
        .interface()
        .map(|interface| interface.to_string())
        .unwrap_or_else(|| "unknown-interface".to_owned());
    let member = message
        .member()
        .map(|member| member.to_string())
        .unwrap_or_else(|| "unknown-member".to_owned());

    format!("{interface}.{member}")
}

#[derive(Debug)]
struct EmergencyHotkeyDevice {
    path: PathBuf,
    file: File,
}

fn spawn_emergency_hotkey_listener(
    config: &ServerConfig,
    audit: SharedAudit,
    shutdown: Arc<AtomicBool>,
) -> Option<std::thread::JoinHandle<()>> {
    if !config.emergency_hotkey {
        audit_write(
            &audit,
            "emergency_hotkey_disabled",
            Some(API_VERSION),
            "ok",
            None,
            emergency_hotkey_details(),
        );
        return None;
    }

    Some(std::thread::spawn(
        move || match run_emergency_hotkey_listener(Arc::clone(&audit), shutdown) {
            Ok(()) => audit_write(
                &audit,
                "emergency_hotkey_stopped",
                Some(API_VERSION),
                "ok",
                None,
                emergency_hotkey_details(),
            ),
            Err(error) => audit_write(
                &audit,
                "emergency_hotkey",
                Some(API_VERSION),
                "error",
                Some(&error),
                emergency_hotkey_details(),
            ),
        },
    ))
}

fn run_emergency_hotkey_listener(
    audit: SharedAudit,
    shutdown: Arc<AtomicBool>,
) -> Result<(), String> {
    let mut devices = open_emergency_hotkey_devices(INPUT_EVENT_DIR)?;
    if devices.is_empty() {
        return Err(format!(
            "no readable {INPUT_EVENT_DIR}/event* devices; {EMERGENCY_STOP_HOTKEY_LABEL} requires Linux input device read access"
        ));
    }

    audit_write(
        &audit,
        "emergency_hotkey_started",
        Some(API_VERSION),
        "ok",
        None,
        json!({
            "hotkey": EMERGENCY_STOP_HOTKEY_LABEL,
            "devices": devices.iter().map(|device| device.path.display().to_string()).collect::<Vec<_>>()
        }),
    );

    let mut state = EmergencyHotkeyState::default();
    while !shutdown.load(Ordering::Relaxed) {
        let mut index = 0;
        while index < devices.len() {
            match read_emergency_hotkey_device(&mut devices[index], &mut state) {
                Ok(true) => {
                    shutdown.store(true, Ordering::Relaxed);
                    perform_emergency_stop(&audit, "emergency_hotkey_triggered");
                    return Ok(());
                }
                Ok(false) => index += 1,
                Err(error) => {
                    let device = devices.remove(index);
                    audit_write(
                        &audit,
                        "emergency_hotkey_device",
                        Some(API_VERSION),
                        "error",
                        Some(&error),
                        json!({
                            "hotkey": EMERGENCY_STOP_HOTKEY_LABEL,
                            "device": device.path.display().to_string()
                        }),
                    );
                }
            }
        }

        if devices.is_empty() {
            return Err("all emergency hotkey input devices became unavailable".to_owned());
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    Ok(())
}

fn open_emergency_hotkey_devices(
    input_dir: impl AsRef<Path>,
) -> Result<Vec<EmergencyHotkeyDevice>, String> {
    let entries = fs::read_dir(input_dir.as_ref()).map_err(|error| {
        format!(
            "failed to read input device directory {}: {error}",
            input_dir.as_ref().display()
        )
    })?;
    let mut devices = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with("event") {
            continue;
        }
        match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&path)
        {
            Ok(file) => devices.push(EmergencyHotkeyDevice { path, file }),
            Err(error) => {
                eprintln!(
                    "failed to open emergency hotkey device {}: {error}",
                    path.display()
                );
            }
        }
    }

    Ok(devices)
}

fn read_emergency_hotkey_device(
    device: &mut EmergencyHotkeyDevice,
    state: &mut EmergencyHotkeyState,
) -> Result<bool, String> {
    let event_size = linux_input_event_size();
    let mut buffer = vec![0_u8; event_size * 32];

    loop {
        match device.file.read(&mut buffer) {
            Ok(0) => return Ok(false),
            Ok(bytes_read) => {
                for chunk in buffer[..bytes_read].chunks_exact(event_size) {
                    let Some((event_type, key_code, value)) = parse_linux_input_event(chunk) else {
                        continue;
                    };
                    if state.update_linux_key_event(event_type, key_code, value) {
                        return Ok(true);
                    }
                }
                if bytes_read < buffer.len() {
                    return Ok(false);
                }
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(false),
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => {
                return Err(format!("failed to read {}: {error}", device.path.display()));
            }
        }
    }
}

fn linux_input_event_size() -> usize {
    std::mem::size_of::<libc::timeval>() + 8
}

fn parse_linux_input_event(bytes: &[u8]) -> Option<(u16, u16, i32)> {
    let time_size = std::mem::size_of::<libc::timeval>();
    if bytes.len() < time_size + 8 {
        return None;
    }

    let event_type = u16::from_ne_bytes(bytes[time_size..time_size + 2].try_into().ok()?);
    let key_code = u16::from_ne_bytes(bytes[time_size + 2..time_size + 4].try_into().ok()?);
    let value = i32::from_ne_bytes(bytes[time_size + 4..time_size + 8].try_into().ok()?);
    Some((event_type, key_code, value))
}

fn perform_emergency_stop(audit: &SharedAudit, event: &str) {
    match peekaboox_input::emergency_stop() {
        Ok(()) => audit_write(
            audit,
            event,
            Some(API_VERSION),
            "ok",
            None,
            emergency_hotkey_details(),
        ),
        Err(error) => audit_write(
            audit,
            event,
            Some(API_VERSION),
            "error",
            Some(&error.to_string()),
            emergency_hotkey_details(),
        ),
    }
}

fn emergency_hotkey_details() -> serde_json::Value {
    json!({ "hotkey": EMERGENCY_STOP_HOTKEY_LABEL })
}

fn spawn_grpc_server(
    config: &ServerConfig,
    audit: SharedAudit,
    accessibility_cache: SharedAccessibilityCache,
    incremental_capture_state: SharedIncrementalCaptureState,
    shutdown: Arc<AtomicBool>,
) -> Result<Option<std::thread::JoinHandle<()>>, String> {
    let Some(addr) = config.grpc_addr else {
        return Ok(None);
    };

    let listener = TcpListener::bind(addr)
        .map_err(|error| format!("failed to bind gRPC listener at {addr}: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("failed to configure gRPC listener at {addr}: {error}"))?;
    let local_addr = listener
        .local_addr()
        .map_err(|error| format!("failed to inspect gRPC listener address: {error}"))?;
    let service = GrpcPeekabooXService {
        config: config.clone(),
        audit: Arc::clone(&audit),
        accessibility_cache,
        incremental_capture_state,
        list_windows: peekaboox_windows::list_windows_with_query,
    };
    let audit_for_thread = Arc::clone(&audit);

    println!("peekabooxd grpc listening on {local_addr}");
    audit_write(
        &audit,
        "grpc_started",
        Some(API_VERSION),
        "ok",
        None,
        json!({ "addr": local_addr.to_string() }),
    );

    let handle = std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                let message = format!("failed to start gRPC runtime: {error}");
                eprintln!("{message}");
                audit_write(
                    &audit_for_thread,
                    "grpc_server",
                    Some(API_VERSION),
                    "error",
                    Some(&message),
                    json!({ "addr": local_addr.to_string() }),
                );
                return;
            }
        };

        let result = runtime.block_on(async move {
            let listener = tokio::net::TcpListener::from_std(listener)
                .map_err(|error| format!("failed to adopt gRPC listener: {error}"))?;
            let incoming = TcpListenerStream::new(listener);
            tonic::transport::Server::builder()
                .add_service(PeekabooXServer::new(service))
                .serve_with_incoming_shutdown(incoming, async move {
                    while !shutdown.load(Ordering::Relaxed) {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                })
                .await
                .map_err(|error| format!("gRPC server stopped with error: {error}"))
        });

        if let Err(error) = result {
            eprintln!("{error}");
            audit_write(
                &audit_for_thread,
                "grpc_server",
                Some(API_VERSION),
                "error",
                Some(&error),
                json!({ "addr": local_addr.to_string() }),
            );
        }
    });

    Ok(Some(handle))
}

fn handle_stream(
    mut stream: UnixStream,
    config: &ServerConfig,
    audit: &SharedAudit,
    accessibility_cache: &SharedAccessibilityCache,
    incremental_capture_state: &SharedIncrementalCaptureState,
) {
    let mut payload = String::new();
    let response = match stream.read_to_string(&mut payload) {
        Ok(_) => match decode_request(&payload) {
            Ok(envelope) => handle_request(
                envelope.version,
                envelope.request,
                config,
                audit,
                accessibility_cache,
                incremental_capture_state,
            ),
            Err(error) => {
                audit_write(
                    audit,
                    "invalid_request",
                    None,
                    "error",
                    Some(&error.to_string()),
                    json!({ "bytes": payload.len() }),
                );
                ApiResponseEnvelope::error(format!("invalid request: {error}"))
            }
        },
        Err(error) => {
            audit_write(
                audit,
                "read_request",
                None,
                "error",
                Some(&error.to_string()),
                json!({}),
            );
            ApiResponseEnvelope::error(format!("failed to read request: {error}"))
        }
    };

    match encode_response(&response) {
        Ok(payload) => {
            if let Err(error) = stream.write_all(&payload) {
                eprintln!("peekabooxd response write failed: {error}");
            }
        }
        Err(error) => eprintln!("peekabooxd response encoding failed: {error}"),
    }
}

fn handle_request(
    version: String,
    request: ApiRequest,
    config: &ServerConfig,
    audit: &SharedAudit,
    accessibility_cache: &SharedAccessibilityCache,
    incremental_capture_state: &SharedIncrementalCaptureState,
) -> ApiResponseEnvelope {
    let method = request_method(&request);
    let details = audit_details(&request);

    if version != API_VERSION {
        let message = format!("unsupported API version {version:?}; expected {API_VERSION}");
        audit_write(
            audit,
            method,
            Some(&version),
            "error",
            Some(&message),
            details,
        );
        return ApiResponseEnvelope::error(message);
    }

    match dispatch_request(
        request,
        config,
        accessibility_cache,
        incremental_capture_state,
    ) {
        Ok(result) => {
            audit_write(audit, method, Some(&version), "ok", None, details);
            ApiResponseEnvelope::ok(result)
        }
        Err(error) => {
            audit_write(
                audit,
                method,
                Some(&version),
                "error",
                Some(&error),
                details,
            );
            ApiResponseEnvelope::error(error)
        }
    }
}

fn dispatch_request(
    request: ApiRequest,
    config: &ServerConfig,
    accessibility_cache: &SharedAccessibilityCache,
    incremental_capture_state: &SharedIncrementalCaptureState,
) -> Result<ApiResult, String> {
    match request {
        ApiRequest::Ping => Ok(ApiResult::Pong),
        ApiRequest::Capture {
            output,
            region,
            window_id,
        } => {
            let capture_region =
                capture_region_from_request(region.map(Rect::from), window_id.as_deref())?;
            let metadata =
                capture_to_file(output, capture_region).map_err(|error| error.to_string())?;
            Ok(ApiResult::Capture(CaptureResultDto {
                output_path: metadata.output_path.display().to_string(),
                backend_name: metadata.backend_name,
                backend_kind: backend_kind_name(metadata.backend_kind),
                bytes_written: metadata.bytes_written,
            }))
        }
        ApiRequest::CaptureDelta {
            stream_id,
            reset,
            region,
            window_id,
            per_channel_threshold,
            low_bandwidth,
        } => {
            let capture_region =
                capture_region_from_request(region.map(Rect::from), window_id.as_deref())?;
            let data = capture_delta_data(
                stream_id.as_deref(),
                reset,
                capture_region,
                per_channel_threshold,
                low_bandwidth,
                incremental_capture_state,
            )?;
            Ok(ApiResult::CaptureDelta(capture_delta_dto(&data)))
        }
        ApiRequest::CaptureBackends {
            output,
            region,
            diagnose,
            probe,
        } => Ok(ApiResult::CaptureBackends(capture_backends_result(
            &PathBuf::from(output),
            region.map(Rect::from),
            diagnose,
            probe,
        ))),
        ApiRequest::ProbeDmaBuf { import_target } => {
            Ok(ApiResult::DmaBufProbe(probe_dmabuf_import(import_target)?))
        }
        ApiRequest::ListPlugins { paths } => {
            let paths = if paths.is_empty() {
                config.plugin_paths.clone()
            } else {
                paths.into_iter().map(PathBuf::from).collect()
            };
            Ok(ApiResult::Plugins(plugin_list_dto(
                peekaboox_plugins::discover_plugins(&paths),
            )))
        }
        ApiRequest::CallPluginTool {
            plugin_id,
            tool,
            arguments,
            paths,
            timeout_ms,
            max_output_bytes,
        } => {
            let paths = if paths.is_empty() {
                config.plugin_paths.clone()
            } else {
                paths.into_iter().map(PathBuf::from).collect()
            };
            let discovery = peekaboox_plugins::discover_plugins(&paths);
            if !discovery.errors.is_empty() {
                return Err(format!(
                    "plugin discovery failed: {}",
                    discovery
                        .errors
                        .iter()
                        .map(|error| format!("{}: {}", error.path.display(), error.message))
                        .collect::<Vec<_>>()
                        .join("; ")
                ));
            }
            let plugin = discovery
                .plugins
                .iter()
                .find(|plugin| plugin.manifest.id == plugin_id)
                .ok_or_else(|| format!("unknown plugin: {plugin_id}"))?;
            let arguments = if arguments.is_null() {
                serde_json::json!({})
            } else {
                arguments
            };
            let policy = peekaboox_plugins::PluginExecutionPolicy {
                timeout: Duration::from_millis(timeout_ms),
                max_output_bytes,
                ..Default::default()
            };
            let result = peekaboox_plugins::execute_plugin_tool(plugin, &tool, arguments, &policy)?;
            Ok(ApiResult::PluginToolExecution(plugin_execution_dto(result)))
        }
        ApiRequest::Click {
            x,
            y,
            button,
            dry_run,
        } => {
            let action = peekaboox_input::InputAction::Click {
                position: Point::new(x, y),
                button: mouse_button(button),
            };
            let metadata = if dry_run {
                let backend = peekaboox_input::CommandInputBackend
                    .detect_backend_for(&action)
                    .map_err(|error| error.to_string())?;
                ActionResultDto {
                    backend_name: backend.name().to_owned(),
                    backend_kind: backend_kind_name(backend.backend_kind()),
                }
            } else {
                ensure_input_allowed(config)?;
                let metadata = peekaboox_input::click(Point::new(x, y), mouse_button(button))
                    .map_err(|error| error.to_string())?;
                ActionResultDto {
                    backend_name: metadata.backend_name,
                    backend_kind: backend_kind_name(metadata.backend_kind),
                }
            };
            Ok(ApiResult::Click(metadata))
        }
        ApiRequest::MoveMouse { x, y, dry_run } => {
            let action = peekaboox_input::InputAction::MoveMouse(Point::new(x, y));
            let metadata = if dry_run {
                let backend = peekaboox_input::CommandInputBackend
                    .detect_backend_for(&action)
                    .map_err(|error| error.to_string())?;
                detected_input_backend_dto(backend)
            } else {
                ensure_input_allowed(config)?;
                let metadata = peekaboox_input::move_mouse(Point::new(x, y))
                    .map_err(|error| error.to_string())?;
                input_metadata_dto(metadata)
            };
            Ok(ApiResult::MoveMouse(metadata))
        }
        ApiRequest::Drag {
            from_x,
            from_y,
            to_x,
            to_y,
            button,
            duration_ms,
            dry_run,
        } => {
            let button = mouse_button(button);
            let action = peekaboox_input::InputAction::Drag {
                from: Point::new(from_x, from_y),
                to: Point::new(to_x, to_y),
                button,
                duration_ms: u64::from(duration_ms),
            };
            let metadata = if dry_run {
                let backend = peekaboox_input::CommandInputBackend
                    .detect_backend_for(&action)
                    .map_err(|error| error.to_string())?;
                detected_input_backend_dto(backend)
            } else {
                ensure_input_allowed(config)?;
                let metadata = peekaboox_input::drag(
                    Point::new(from_x, from_y),
                    Point::new(to_x, to_y),
                    button,
                    u64::from(duration_ms),
                )
                .map_err(|error| error.to_string())?;
                input_metadata_dto(metadata)
            };
            Ok(ApiResult::Drag(metadata))
        }
        ApiRequest::TypeText { text, dry_run } => {
            let action = peekaboox_input::InputAction::TypeText(text.clone());
            let metadata = if dry_run {
                let backend = peekaboox_input::CommandInputBackend
                    .detect_backend_for(&action)
                    .map_err(|error| error.to_string())?;
                detected_input_backend_dto(backend)
            } else {
                ensure_input_allowed(config)?;
                let metadata =
                    peekaboox_input::type_text(text).map_err(|error| error.to_string())?;
                input_metadata_dto(metadata)
            };
            Ok(ApiResult::TypeText(metadata))
        }
        ApiRequest::PasteText {
            text,
            preserve_clipboard,
            dry_run,
        } => {
            let action = peekaboox_input::InputAction::PasteText {
                text: text.clone(),
                preserve_clipboard,
            };
            let metadata = if dry_run {
                let backend = peekaboox_input::CommandInputBackend
                    .detect_backend_for(&action)
                    .map_err(|error| error.to_string())?;
                detected_input_backend_dto(backend)
            } else {
                ensure_input_allowed(config)?;
                let metadata = peekaboox_input::paste_text_with_options(text, preserve_clipboard)
                    .map_err(|error| error.to_string())?;
                input_metadata_dto(metadata)
            };
            Ok(ApiResult::PasteText(metadata))
        }
        ApiRequest::Hotkey { keys, dry_run } => {
            validate_hotkey_keys(&keys).map_err(|status| status.message().to_owned())?;
            let action = peekaboox_input::InputAction::Hotkey(keys.clone());
            let metadata = if dry_run {
                let backend = peekaboox_input::CommandInputBackend
                    .detect_backend_for(&action)
                    .map_err(|error| error.to_string())?;
                detected_input_backend_dto(backend)
            } else {
                ensure_input_allowed(config)?;
                let metadata = peekaboox_input::hotkey(keys).map_err(|error| error.to_string())?;
                input_metadata_dto(metadata)
            };
            Ok(ApiResult::Hotkey(metadata))
        }
        ApiRequest::ListWindows {
            id,
            app,
            title,
            title_regex,
            focused,
            limit,
            sort,
            backend,
            diagnose,
        } => {
            let query = window_query_from_fields(WindowQueryFields {
                id,
                app,
                title,
                title_regex,
                focused,
                limit,
                sort,
                backend,
                diagnose,
            })?;
            let metadata = peekaboox_windows::list_windows_with_query(query)
                .map_err(|error| error.to_string())?;
            Ok(ApiResult::ListWindows(window_list_result_dto(metadata)))
        }
        ApiRequest::FindElements {
            selector,
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
        } => {
            let options = element_lookup_options_from_request(
                app,
                window_title,
                window_id,
                vision_region.map(Rect::from),
                vision_edge_threshold.map(u32::from),
                vision_min_width,
                vision_min_height,
                vision_min_component_pixels,
                vision_max_elements,
                vision_merge_distance,
            )
            .map_err(|error| error.to_string())?;
            let result = find_elements_with_optional_vision_fallback(
                &selector,
                vision_fallback || config.vision_fallback,
                &options,
                accessibility_cache,
            )?;
            Ok(ApiResult::FindElements(element_lookup_dto(&result)))
        }
        ApiRequest::Ocr {
            image_path,
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
            scale,
            grayscale,
            threshold,
            invert,
            contrast,
            deskew,
        } => {
            let result = run_ocr(OcrRunRequest {
                image_path,
                region: region.map(Rect::from),
                app,
                window_title,
                window_id,
                options: ocr_options(OcrOptionInput {
                    language,
                    page_segmentation_mode,
                    engine_mode,
                    dpi,
                    min_confidence,
                    whitelist,
                    config,
                    scale,
                    grayscale,
                    threshold,
                    invert,
                    contrast,
                    deskew,
                })
                .map_err(|error| error.to_string())?,
            })
            .map_err(|error| error.to_string())?;
            Ok(ApiResult::Ocr(ocr_result_dto(&result)))
        }
        ApiRequest::CompareImages {
            expected_path,
            actual_path,
            region,
            per_channel_threshold,
            max_changed_ratio,
        } => {
            let options = visual_compare_options(
                region.map(Rect::from),
                u32::from(per_channel_threshold),
                Some(max_changed_ratio),
            )
            .map_err(|status| status.message().to_owned())?;
            let result =
                peekaboox_vision::compare_image_files(&expected_path, &actual_path, &options)
                    .map_err(|error| error.to_string())?;
            Ok(ApiResult::VisualDiff(visual_diff_dto(&result)))
        }
        ApiRequest::DetectUiState {
            image_paths,
            region,
            per_channel_threshold,
            stable_max_changed_ratio,
            loading_min_changed_ratio,
            required_stable_transitions,
        } => {
            let options = ui_state_options(
                region.map(Rect::from),
                Some(u32::from(per_channel_threshold)),
                Some(stable_max_changed_ratio),
                Some(loading_min_changed_ratio),
                Some(required_stable_transitions),
            )
            .map_err(|status| status.message().to_owned())?;
            let paths = image_paths.iter().map(PathBuf::from).collect::<Vec<_>>();
            let result = peekaboox_vision::detect_ui_state_from_image_files(&paths, &options)
                .map_err(|error| error.to_string())?;
            Ok(ApiResult::UiState(ui_state_dto(&result)))
        }
        ApiRequest::DetectUiElements {
            image_path,
            region,
            edge_threshold,
            min_width,
            min_height,
            min_component_pixels,
            max_elements,
            merge_distance,
        } => {
            let options = ui_element_detection_options(
                region.map(Rect::from),
                Some(u32::from(edge_threshold)),
                Some(min_width),
                Some(min_height),
                Some(min_component_pixels),
                Some(max_elements),
                Some(merge_distance),
            )
            .map_err(|status| status.message().to_owned())?;
            let elements =
                peekaboox_vision::detect_ui_elements_from_image_file(&image_path, &options)
                    .map_err(|error| error.to_string())?;
            Ok(ApiResult::DetectUiElements(ui_element_list_dto(&elements)))
        }
        ApiRequest::DesktopFocus {
            app,
            use_gnome_overview,
            launch_if_needed,
            wait_after_focus_ms,
            overview_wait_ms,
            window_title,
            window_id,
            verify,
        } => {
            ensure_input_allowed(config)?;
            let result = peekaboox_desktop::focus_app(
                &app,
                &DesktopFocusOptions {
                    use_gnome_overview,
                    launch_if_needed,
                    wait_after_focus_ms,
                    overview_wait_ms,
                    window_title,
                    window_id,
                    verify,
                },
            )
            .map_err(|error| error.to_string())?;
            Ok(ApiResult::DesktopAction(desktop_action_dto(result)))
        }
        ApiRequest::DesktopLocate {
            app,
            target,
            image_path,
            prefer_accessibility,
            window_title,
            window_id,
        } => {
            let result = peekaboox_desktop::locate_target(
                &app,
                &target,
                &DesktopLocateOptions {
                    image: image_path.map(PathBuf::from),
                    prefer_accessibility,
                    window_title,
                    window_id,
                },
            )
            .map_err(|error| error.to_string())?;
            Ok(ApiResult::DesktopLocate(desktop_locate_dto(result)))
        }
        ApiRequest::DesktopClick {
            app,
            target,
            image_path,
            prefer_accessibility,
            window_title,
            button,
            dry_run,
            window_id,
            verify,
        } => {
            if !dry_run {
                ensure_input_allowed(config)?;
            }
            let result = peekaboox_desktop::click_target(
                &app,
                &target,
                &DesktopClickOptions {
                    locate: DesktopLocateOptions {
                        image: image_path.map(PathBuf::from),
                        prefer_accessibility,
                        window_title,
                        window_id,
                    },
                    button: mouse_button(button),
                    dry_run,
                    verify,
                },
            )
            .map_err(|error| error.to_string())?;
            Ok(ApiResult::DesktopAction(desktop_action_dto(result)))
        }
        ApiRequest::DesktopDrag {
            app,
            target,
            image_path,
            prefer_accessibility,
            window_title,
            button,
            from_ratio_x,
            from_ratio_y,
            to_ratio_x,
            to_ratio_y,
            duration_ms,
            dry_run,
            window_id,
            verify,
        } => {
            if !dry_run {
                ensure_input_allowed(config)?;
            }
            validate_ratio("from_ratio_x", from_ratio_x)?;
            validate_ratio("from_ratio_y", from_ratio_y)?;
            validate_ratio("to_ratio_x", to_ratio_x)?;
            validate_ratio("to_ratio_y", to_ratio_y)?;
            let result = peekaboox_desktop::drag_target(
                &app,
                &target,
                &DesktopDragOptions {
                    locate: DesktopLocateOptions {
                        image: image_path.map(PathBuf::from),
                        prefer_accessibility,
                        window_title,
                        window_id,
                    },
                    from_ratio: (from_ratio_x, from_ratio_y),
                    to_ratio: (to_ratio_x, to_ratio_y),
                    button: mouse_button(button),
                    duration_ms,
                    dry_run,
                    verify,
                },
            )
            .map_err(|error| error.to_string())?;
            Ok(ApiResult::DesktopAction(desktop_action_dto(result)))
        }
        ApiRequest::DesktopTypeInto {
            app,
            target,
            text,
            image_path,
            prefer_accessibility,
            window_title,
            clear,
            dry_run,
            window_id,
            verify,
        } => {
            if !dry_run {
                ensure_input_allowed(config)?;
            }
            let result = peekaboox_desktop::type_into_target(
                &app,
                &target,
                &text,
                &DesktopTypeIntoOptions {
                    locate: DesktopLocateOptions {
                        image: image_path.map(PathBuf::from),
                        prefer_accessibility,
                        window_title,
                        window_id,
                    },
                    clear,
                    dry_run,
                    verify,
                },
            )
            .map_err(|error| error.to_string())?;
            Ok(ApiResult::DesktopAction(desktop_action_dto(result)))
        }
        ApiRequest::DesktopAssert {
            app,
            target,
            image_path,
            prefer_accessibility,
            window_title,
            assertion,
            expected_text,
            window_id,
        } => {
            let result = peekaboox_desktop::assert_target(
                &app,
                &target,
                &DesktopAssertOptions {
                    locate: DesktopLocateOptions {
                        image: image_path.map(PathBuf::from),
                        prefer_accessibility,
                        window_title,
                        window_id,
                    },
                    assertion: desktop_assertion(assertion, expected_text)?,
                },
            )
            .map_err(|error| error.to_string())?;
            Ok(ApiResult::DesktopAction(desktop_action_dto(result)))
        }
    }
}

#[derive(Clone)]
struct GrpcPeekabooXService {
    config: ServerConfig,
    audit: SharedAudit,
    accessibility_cache: SharedAccessibilityCache,
    incremental_capture_state: SharedIncrementalCaptureState,
    list_windows: WindowListProvider,
}

#[tonic::async_trait]
impl PeekabooX for GrpcPeekabooXService {
    async fn capture_screen(
        &self,
        request: Request<proto::CaptureScreenRequest>,
    ) -> Result<Response<proto::CaptureScreenResponse>, Status> {
        let request = request.into_inner();
        audit_write(
            &self.audit,
            "grpc.capture_screen",
            Some(API_VERSION),
            "started",
            None,
            json!({
                "include_semantic_tree": request.include_semantic_tree,
                "target": capture_target_name(request.target.as_ref())
            }),
        );

        match capture_screen_response(
            request.target,
            request.include_semantic_tree,
            &self.accessibility_cache,
        ) {
            Ok(response) => {
                audit_write(
                    &self.audit,
                    "grpc.capture_screen",
                    Some(API_VERSION),
                    "ok",
                    None,
                    json!({ "bytes": response.image.len() }),
                );
                Ok(Response::new(response))
            }
            Err(error) => {
                let status = Status::internal(error);
                audit_write(
                    &self.audit,
                    "grpc.capture_screen",
                    Some(API_VERSION),
                    "error",
                    Some(status.message()),
                    json!({}),
                );
                Err(status)
            }
        }
    }

    async fn capture_delta(
        &self,
        request: Request<proto::CaptureDeltaRequest>,
    ) -> Result<Response<proto::CaptureDeltaResponse>, Status> {
        let request = request.into_inner();
        let details = json!({
            "stream_id": normalized_capture_stream_id(request.stream_id.as_str()),
            "reset": request.reset,
            "target": capture_target_name(request.target.as_ref()),
            "has_region": request.region.is_some(),
            "per_channel_threshold": request.per_channel_threshold,
            "low_bandwidth": request.low_bandwidth.unwrap_or(true)
        });
        let result = grpc_capture_delta(request, &self.incremental_capture_state);
        audit_grpc_result(&self.audit, "grpc.capture_delta", &result, details);
        result.map(Response::new)
    }

    async fn capture_backends(
        &self,
        request: Request<proto::CaptureBackendsRequest>,
    ) -> Result<Response<proto::CaptureBackendsResponse>, Status> {
        let request = request.into_inner();
        let details = json!({
            "output": request.output.as_str(),
            "has_region": request.region.is_some(),
            "diagnose": request.diagnose,
            "probe": request.probe,
        });
        let result = grpc_capture_backends(request);
        audit_grpc_result(&self.audit, "grpc.capture_backends", &result, details);
        result.map(Response::new)
    }

    async fn click(
        &self,
        request: Request<proto::ClickRequest>,
    ) -> Result<Response<proto::ActionResponse>, Status> {
        let request = request.into_inner();
        let effective_vision_fallback = request.vision_fallback || self.config.vision_fallback;
        let details = json!({
            "has_coordinates": request.coordinates.is_some(),
            "has_semantic_selector": request.semantic_selector.is_some(),
            "has_window_selector": request.window_selector.is_some(),
            "vision_fallback": effective_vision_fallback,
            "request_vision_fallback": request.vision_fallback,
            "daemon_vision_fallback": self.config.vision_fallback
        });

        let result = grpc_click(request, &self.config, &self.accessibility_cache);
        audit_grpc_result(&self.audit, "grpc.click", &result, details);
        result.map(Response::new)
    }

    async fn move_mouse(
        &self,
        request: Request<proto::MoveMouseRequest>,
    ) -> Result<Response<proto::ActionResponse>, Status> {
        let request = request.into_inner();
        let details = json!({
            "has_coordinates": request.coordinates.is_some()
        });

        let result = grpc_move_mouse(request, &self.config);
        audit_grpc_result(&self.audit, "grpc.move_mouse", &result, details);
        result.map(Response::new)
    }

    async fn drag(
        &self,
        request: Request<proto::DragRequest>,
    ) -> Result<Response<proto::ActionResponse>, Status> {
        let request = request.into_inner();
        let details = json!({
            "has_from": request.from.is_some(),
            "has_to": request.to.is_some(),
            "button": request.button,
            "duration_ms": request.duration_ms
        });

        let result = grpc_drag(request, &self.config);
        audit_grpc_result(&self.audit, "grpc.drag", &result, details);
        result.map(Response::new)
    }

    async fn type_text(
        &self,
        request: Request<proto::TypeTextRequest>,
    ) -> Result<Response<proto::ActionResponse>, Status> {
        let request = request.into_inner();
        let details = json!({
            "text_length": request.text.chars().count(),
            "typing_speed_chars_per_second": request.typing_speed_chars_per_second
        });

        let result = grpc_type_text(request, &self.config);
        audit_grpc_result(&self.audit, "grpc.type_text", &result, details);
        result.map(Response::new)
    }

    async fn paste_text(
        &self,
        request: Request<proto::PasteTextRequest>,
    ) -> Result<Response<proto::ActionResponse>, Status> {
        let request = request.into_inner();
        let details = json!({
            "text_length": request.text.chars().count(),
            "preserve_clipboard": request.preserve_clipboard
        });

        let result = grpc_paste_text(request, &self.config);
        audit_grpc_result(&self.audit, "grpc.paste_text", &result, details);
        result.map(Response::new)
    }

    async fn hotkey(
        &self,
        request: Request<proto::HotkeyRequest>,
    ) -> Result<Response<proto::ActionResponse>, Status> {
        let request = request.into_inner();
        let details = json!({
            "key_count": request.keys.len()
        });

        let result = grpc_hotkey(request, &self.config);
        audit_grpc_result(&self.audit, "grpc.hotkey", &result, details);
        result.map(Response::new)
    }

    async fn find_element(
        &self,
        request: Request<proto::FindElementRequest>,
    ) -> Result<Response<proto::FindElementResponse>, Status> {
        let request = request.into_inner();
        if request.selector.trim().is_empty() {
            let status = Status::invalid_argument("selector must not be empty");
            audit_write(
                &self.audit,
                "grpc.find_element",
                Some(API_VERSION),
                "error",
                Some(status.message()),
                json!({ "selector_length": 0 }),
            );
            return Err(status);
        }

        let selector_length = request.selector.chars().count();
        let vision_fallback = request.vision_fallback || self.config.vision_fallback;
        let options = match element_lookup_options_from_request(
            request.app.clone(),
            request.window_title.clone(),
            request.window_id.clone(),
            request.vision_region.map(rect_from_proto),
            request.vision_edge_threshold,
            request.vision_min_width,
            request.vision_min_height,
            request.vision_min_component_pixels,
            request.vision_max_elements,
            request.vision_merge_distance,
        ) {
            Ok(options) => options,
            Err(error) => {
                let status = Status::invalid_argument(error);
                audit_write(
                    &self.audit,
                    "grpc.find_element",
                    Some(API_VERSION),
                    "error",
                    Some(status.message()),
                    json!({ "selector_length": selector_length, "vision_fallback": vision_fallback }),
                );
                return Err(status);
            }
        };
        let result = grpc_find_element(
            &request.selector,
            vision_fallback,
            &options,
            &self.accessibility_cache,
        );
        match &result {
            Ok(result) => audit_write(
                &self.audit,
                "grpc.find_element",
                Some(API_VERSION),
                "ok",
                None,
                json!({
                    "selector_length": selector_length,
                    "elements": result.response.elements.len(),
                    "accessibility_cache_hit": result.cache_hit,
                    "accessibility_cache_age_ms": result.cache_age_ms,
                    "vision_fallback": vision_fallback,
                    "vision_fallback_used": result.vision_fallback_used,
                    "has_app": request.app.as_deref().is_some_and(|value| !value.trim().is_empty()),
                    "has_window_title": request.window_title.as_deref().is_some_and(|value| !value.trim().is_empty()),
                    "has_window_id": request.window_id.as_deref().is_some_and(|value| !value.trim().is_empty())
                }),
            ),
            Err(status) => audit_write(
                &self.audit,
                "grpc.find_element",
                Some(API_VERSION),
                "error",
                Some(status.message()),
                json!({ "selector_length": selector_length, "vision_fallback": vision_fallback }),
            ),
        }
        result.map(|result| Response::new(result.response))
    }

    async fn list_windows(
        &self,
        request: Request<proto::ListWindowsRequest>,
    ) -> Result<Response<proto::ListWindowsResponse>, Status> {
        let request = request.into_inner();
        let audit_details = grpc_list_windows_audit_details(&request);
        let result = grpc_list_windows(self.list_windows, request);
        audit_grpc_result(&self.audit, "grpc.list_windows", &result, audit_details);
        result.map(Response::new)
    }

    async fn get_desktop_state(
        &self,
        _request: Request<proto::GetDesktopStateRequest>,
    ) -> Result<Response<proto::DesktopState>, Status> {
        let result = grpc_desktop_state(&self.accessibility_cache, self.list_windows);
        audit_grpc_result(&self.audit, "grpc.get_desktop_state", &result, json!({}));
        result.map(Response::new)
    }

    async fn ocr_screen(
        &self,
        request: Request<proto::OcrScreenRequest>,
    ) -> Result<Response<proto::OcrResponse>, Status> {
        let request = request.into_inner();
        let details = json!({
            "has_region": request.region.is_some(),
            "has_language": request.language.as_deref().is_some_and(|language| !language.trim().is_empty()),
            "has_image_path": request.image_path.as_deref().is_some_and(|path| !path.trim().is_empty()),
            "has_window_id": request.window_id.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "has_window_title": request.window_title.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "has_app": request.app.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "has_preprocessing": request.scale.is_some()
                || request.grayscale.unwrap_or(false)
                || request.threshold.is_some()
                || request.invert.unwrap_or(false)
                || request.contrast.is_some()
                || request.deskew.unwrap_or(false)
        });
        let result = grpc_ocr_screen(request);
        audit_grpc_result(&self.audit, "grpc.ocr_screen", &result, details);
        result.map(Response::new)
    }

    async fn compare_images(
        &self,
        request: Request<proto::CompareImagesRequest>,
    ) -> Result<Response<proto::VisualDiffResponse>, Status> {
        let request = request.into_inner();
        let details = json!({
            "expected_bytes": request.expected_image.len(),
            "actual_bytes": request.actual_image.len(),
            "has_region": request.region.is_some(),
            "per_channel_threshold": request.per_channel_threshold,
            "max_changed_ratio": request.max_changed_ratio
        });
        let result = grpc_compare_images(request);
        audit_grpc_result(&self.audit, "grpc.compare_images", &result, details);
        result.map(Response::new)
    }

    async fn detect_ui_state(
        &self,
        request: Request<proto::DetectUiStateRequest>,
    ) -> Result<Response<proto::UiStateResponse>, Status> {
        let request = request.into_inner();
        let details = json!({
            "images": request.images.len(),
            "total_image_bytes": request.images.iter().map(Vec::len).sum::<usize>(),
            "has_region": request.region.is_some(),
            "per_channel_threshold": request.per_channel_threshold,
            "stable_max_changed_ratio": request.stable_max_changed_ratio,
            "loading_min_changed_ratio": request.loading_min_changed_ratio,
            "required_stable_transitions": request.required_stable_transitions
        });
        let result = grpc_detect_ui_state(request);
        audit_grpc_result(&self.audit, "grpc.detect_ui_state", &result, details);
        result.map(Response::new)
    }

    async fn detect_ui_elements(
        &self,
        request: Request<proto::DetectUiElementsRequest>,
    ) -> Result<Response<proto::DetectUiElementsResponse>, Status> {
        let request = request.into_inner();
        let details = json!({
            "image_bytes": request.image.len(),
            "has_region": request.region.is_some(),
            "edge_threshold": request.edge_threshold,
            "min_width": request.min_width,
            "min_height": request.min_height,
            "min_component_pixels": request.min_component_pixels,
            "max_elements": request.max_elements,
            "merge_distance": request.merge_distance
        });
        let result = grpc_detect_ui_elements(request);
        audit_grpc_result(&self.audit, "grpc.detect_ui_elements", &result, details);
        result.map(Response::new)
    }

    async fn probe_dma_buf(
        &self,
        request: Request<proto::ProbeDmaBufRequest>,
    ) -> Result<Response<proto::DmaBufProbeResponse>, Status> {
        let request = request.into_inner();
        let details = json!({ "import_target": request.import_target });
        let result = grpc_probe_dmabuf(request);
        audit_grpc_result(&self.audit, "grpc.probe_dmabuf", &result, details);
        result.map(Response::new)
    }

    async fn list_plugins(
        &self,
        request: Request<proto::ListPluginsRequest>,
    ) -> Result<Response<proto::PluginListResponse>, Status> {
        let request = request.into_inner();
        let details = json!({ "path_count": request.paths.len() });
        let result = grpc_list_plugins(request, &self.config);
        audit_grpc_result(&self.audit, "grpc.list_plugins", &result, details);
        result.map(Response::new)
    }

    async fn call_plugin_tool(
        &self,
        request: Request<proto::CallPluginToolRequest>,
    ) -> Result<Response<proto::PluginToolExecutionResponse>, Status> {
        let request = request.into_inner();
        let details = json!({
            "plugin_id": request.plugin_id.as_str(),
            "tool": request.tool.as_str(),
            "arguments_bytes": request.arguments_json.len(),
            "path_count": request.paths.len(),
            "timeout_ms": request.timeout_ms,
            "max_output_bytes": request.max_output_bytes
        });
        let result = grpc_call_plugin_tool(request, &self.config);
        audit_grpc_result(&self.audit, "grpc.call_plugin_tool", &result, details);
        result.map(Response::new)
    }

    async fn desktop_focus(
        &self,
        request: Request<proto::DesktopFocusRequest>,
    ) -> Result<Response<proto::DesktopActionResponse>, Status> {
        let request = request.into_inner();
        let details = json!({
            "app": request.app.as_str(),
            "use_gnome_overview": request.use_gnome_overview.unwrap_or(true),
            "launch_if_needed": request.launch_if_needed.unwrap_or(true),
            "has_window_title": request.window_title.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "has_window_id": request.window_id.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "verify": request.verify
        });
        let result = grpc_desktop_focus(request, &self.config);
        audit_grpc_result(&self.audit, "grpc.desktop_focus", &result, details);
        result.map(Response::new)
    }

    async fn desktop_locate(
        &self,
        request: Request<proto::DesktopLocateRequest>,
    ) -> Result<Response<proto::DesktopLocateResponse>, Status> {
        let request = request.into_inner();
        let details = json!({
            "app": request.app.as_str(),
            "target": request.target.as_str(),
            "has_image_path": request.image_path.is_some(),
            "prefer_accessibility": request.prefer_accessibility.unwrap_or(true),
            "has_window_title": request.window_title.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "has_window_id": request.window_id.as_deref().is_some_and(|value| !value.trim().is_empty())
        });
        let result = grpc_desktop_locate(request);
        audit_grpc_result(&self.audit, "grpc.desktop_locate", &result, details);
        result.map(Response::new)
    }

    async fn desktop_click(
        &self,
        request: Request<proto::DesktopClickRequest>,
    ) -> Result<Response<proto::DesktopActionResponse>, Status> {
        let request = request.into_inner();
        let details = json!({
            "app": request.app.as_str(),
            "target": request.target.as_str(),
            "dry_run": request.dry_run,
            "verify": request.verify,
            "has_image_path": request.image_path.is_some(),
            "prefer_accessibility": request.prefer_accessibility.unwrap_or(true),
            "has_window_id": request.window_id.as_deref().is_some_and(|value| !value.trim().is_empty())
        });
        let result = grpc_desktop_click(request, &self.config);
        audit_grpc_result(&self.audit, "grpc.desktop_click", &result, details);
        result.map(Response::new)
    }

    async fn desktop_drag(
        &self,
        request: Request<proto::DesktopDragRequest>,
    ) -> Result<Response<proto::DesktopActionResponse>, Status> {
        let request = request.into_inner();
        let details = json!({
            "app": request.app.as_str(),
            "target": request.target.as_str(),
            "dry_run": request.dry_run,
            "duration_ms": request.duration_ms.unwrap_or(250),
            "verify": request.verify,
            "has_window_id": request.window_id.as_deref().is_some_and(|value| !value.trim().is_empty())
        });
        let result = grpc_desktop_drag(request, &self.config);
        audit_grpc_result(&self.audit, "grpc.desktop_drag", &result, details);
        result.map(Response::new)
    }

    async fn desktop_type_into(
        &self,
        request: Request<proto::DesktopTypeIntoRequest>,
    ) -> Result<Response<proto::DesktopActionResponse>, Status> {
        let request = request.into_inner();
        let details = json!({
            "app": request.app.as_str(),
            "target": request.target.as_str(),
            "text_length": request.text.chars().count(),
            "clear": request.clear,
            "dry_run": request.dry_run,
            "verify": request.verify,
            "has_window_id": request.window_id.as_deref().is_some_and(|value| !value.trim().is_empty())
        });
        let result = grpc_desktop_type_into(request, &self.config);
        audit_grpc_result(&self.audit, "grpc.desktop_type_into", &result, details);
        result.map(Response::new)
    }

    async fn desktop_assert(
        &self,
        request: Request<proto::DesktopAssertRequest>,
    ) -> Result<Response<proto::DesktopActionResponse>, Status> {
        let request = request.into_inner();
        let details = json!({
            "app": request.app.as_str(),
            "target": request.target.as_str(),
            "assertion": request.assertion,
            "has_expected_text": request.expected_text.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "has_window_id": request.window_id.as_deref().is_some_and(|value| !value.trim().is_empty())
        });
        let result = grpc_desktop_assert(request);
        audit_grpc_result(&self.audit, "grpc.desktop_assert", &result, details);
        result.map(Response::new)
    }
}

fn capture_screen_response(
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

fn capture_screen_region(target: Option<proto::CaptureTarget>) -> Result<Option<Rect>, String> {
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

fn capture_delta_data(
    stream_id: Option<&str>,
    reset: bool,
    capture_region: Option<Rect>,
    per_channel_threshold: u8,
    low_bandwidth: bool,
    incremental_capture_state: &SharedIncrementalCaptureState,
) -> Result<CaptureDeltaData, String> {
    let stream_id = normalized_capture_stream_id(stream_id.unwrap_or_default());
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
    state.streams.insert(
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

fn grpc_capture_delta(
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

fn grpc_capture_backends(
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
fn probe_dmabuf_import(
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
fn probe_dmabuf_import(
    _import_target: DmaBufImportTargetDto,
) -> Result<DmaBufProbeResultDto, String> {
    Err(
        "DMA-BUF probing requires building peekabooxd with the `pipewire-backend` feature"
            .to_owned(),
    )
}

#[cfg(feature = "pipewire-backend")]
fn probe_compute_dmabuf_import(
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
fn probe_egl_dmabuf_import(
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
fn probe_egl_dmabuf_import(
    _descriptor: &peekaboox_capture::DmaBufFrameDescriptor,
    _import_target: DmaBufImportTargetDto,
    _stream_node_id: u32,
    _pipewire_serial: Option<u64>,
) -> Result<DmaBufProbeResultDto, String> {
    Err("EGL DMA-BUF probing requires building peekabooxd with `egl-backend`".to_owned())
}

#[cfg(all(feature = "pipewire-backend", feature = "egl-backend"))]
fn probe_egl_texture_dmabuf_import(
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
fn probe_egl_texture_dmabuf_import(
    _descriptor: &peekaboox_capture::DmaBufFrameDescriptor,
    _import_target: DmaBufImportTargetDto,
    _stream_node_id: u32,
    _pipewire_serial: Option<u64>,
) -> Result<DmaBufProbeResultDto, String> {
    Err("EGL texture DMA-BUF probing requires building peekabooxd with `egl-backend`".to_owned())
}

#[cfg(feature = "pipewire-backend")]
struct DmaBufProbeMetadata {
    import_target: DmaBufImportTargetDto,
    backend_name: String,
    stream_node_id: u32,
    pipewire_serial: Option<u64>,
    egl_version: Option<String>,
    egl_modifiers: Option<bool>,
    texture_id: Option<u32>,
}

#[cfg(feature = "pipewire-backend")]
fn dmabuf_probe_result(
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
fn egl_version_string(version: (i32, i32)) -> String {
    format!("{}.{}", version.0, version.1)
}

fn capture_current_frame(region: Option<Rect>) -> Result<CapturedFrame, String> {
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

fn capture_to_file(
    output: impl AsRef<Path>,
    region: Option<Rect>,
) -> Result<peekaboox_capture::CaptureFileMetadata, peekaboox_core::PeekabooXError> {
    match region {
        Some(region) => peekaboox_capture::capture_region_to_file(region, output),
        None => peekaboox_capture::capture_screen_to_file(output),
    }
}

fn capture_backends_result(
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

fn capture_backend_dto(
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
        backend_kind: backend_kind_name(capability.backend_kind),
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

fn capture_backend_probe_file(output: &Path) -> CaptureBackendProbeResultDto {
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

fn capture_backend_probe_frame() -> CaptureBackendProbeResultDto {
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

fn capture_backend_probe_region(region: Rect) -> CaptureBackendProbeResultDto {
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

fn capture_backend_probe_dmabuf() -> CaptureBackendProbeResultDto {
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

fn format_rect(rect: Rect) -> String {
    format!("{},{},{}x{}", rect.x, rect.y, rect.width, rect.height)
}

fn capture_region_from_request(
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

fn capture_delta_region(
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

fn normalized_capture_stream_id(stream_id: &str) -> String {
    let stream_id = stream_id.trim();
    if stream_id.is_empty() {
        "default".to_owned()
    } else {
        stream_id.to_owned()
    }
}

fn grpc_click(
    request: proto::ClickRequest,
    config: &ServerConfig,
    accessibility_cache: &SharedAccessibilityCache,
) -> Result<proto::ActionResponse, Status> {
    if request.window_selector.is_some() {
        return Err(Status::unimplemented(
            "window selector clicks require the window focus phase",
        ));
    }

    if request.coordinates.is_some() && request.semantic_selector.is_some() {
        return Err(Status::invalid_argument(
            "provide either coordinates or semantic_selector, not both",
        ));
    }

    ensure_input_allowed(config).map_err(Status::permission_denied)?;

    let (position, target_description) = if let Some(selector) = request.semantic_selector {
        if selector.trim().is_empty() {
            return Err(Status::invalid_argument(
                "semantic_selector must not be empty",
            ));
        }
        let target = resolve_click_target_with_optional_vision_fallback(
            &selector,
            request.vision_fallback || config.vision_fallback,
            accessibility_cache,
        )?;
        let label = target
            .element
            .label
            .as_deref()
            .unwrap_or(target.element.role.as_str())
            .to_owned();
        (
            target.position,
            format!(
                "selector {selector:?} at {},{} ({label})",
                target.position.x, target.position.y
            ),
        )
    } else {
        let Some(coordinates) = request.coordinates else {
            return Err(Status::invalid_argument(
                "coordinates or semantic_selector are required",
            ));
        };
        (
            Point::new(coordinates.x, coordinates.y),
            format!("{},{}", coordinates.x, coordinates.y),
        )
    };
    let metadata = peekaboox_input::click(position, MouseButton::Left)
        .map_err(|error| Status::internal(error.to_string()))?;
    let backend_kind = backend_kind_name(metadata.backend_kind);
    let backend_name = metadata.backend_name;

    Ok(proto::ActionResponse {
        ok: true,
        message: format!(
            "clicked {target_description} using {}/{}",
            backend_name, backend_kind
        ),
        backend_name: Some(backend_name),
        backend_kind: Some(backend_kind),
    })
}

fn grpc_move_mouse(
    request: proto::MoveMouseRequest,
    config: &ServerConfig,
) -> Result<proto::ActionResponse, Status> {
    ensure_input_allowed(config).map_err(Status::permission_denied)?;
    let coordinates = request
        .coordinates
        .ok_or_else(|| Status::invalid_argument("coordinates are required"))?;
    let position = Point::new(coordinates.x, coordinates.y);
    let metadata = peekaboox_input::move_mouse(position)
        .map_err(|error| Status::internal(error.to_string()))?;
    let backend_kind = backend_kind_name(metadata.backend_kind);
    let backend_name = metadata.backend_name;

    Ok(proto::ActionResponse {
        ok: true,
        message: format!(
            "moved mouse to {},{} using {}/{}",
            position.x, position.y, backend_name, backend_kind
        ),
        backend_name: Some(backend_name),
        backend_kind: Some(backend_kind),
    })
}

fn grpc_drag(
    request: proto::DragRequest,
    config: &ServerConfig,
) -> Result<proto::ActionResponse, Status> {
    ensure_input_allowed(config).map_err(Status::permission_denied)?;
    let from = request
        .from
        .ok_or_else(|| Status::invalid_argument("from coordinates are required"))?;
    let to = request
        .to
        .ok_or_else(|| Status::invalid_argument("to coordinates are required"))?;
    let button = proto_mouse_button(request.button)?;
    let duration_ms = u64::from(request.duration_ms.unwrap_or(250));
    let from = Point::new(from.x, from.y);
    let to = Point::new(to.x, to.y);
    let metadata = peekaboox_input::drag(from, to, button, duration_ms)
        .map_err(|error| Status::internal(error.to_string()))?;
    let backend_kind = backend_kind_name(metadata.backend_kind);
    let backend_name = metadata.backend_name;

    Ok(proto::ActionResponse {
        ok: true,
        message: format!(
            "dragged from {},{} to {},{} using {}/{}",
            from.x, from.y, to.x, to.y, backend_name, backend_kind
        ),
        backend_name: Some(backend_name),
        backend_kind: Some(backend_kind),
    })
}

fn resolve_click_target_with_optional_vision_fallback(
    selector: &str,
    use_vision_fallback: bool,
    accessibility_cache: &SharedAccessibilityCache,
) -> Result<peekaboox_accessibility::ResolvedClickTarget, Status> {
    match cached_accessibility_tree(accessibility_cache) {
        Ok(tree) => match peekaboox_accessibility::resolve_click_target_from_tree(
            selector,
            &tree.metadata.elements,
        ) {
            Ok(target) => Ok(target),
            Err(error) if use_vision_fallback => resolve_vision_click_target(selector)
                .map_err(|fallback_error| {
                    Status::not_found(format!(
                        "{}; vision fallback also failed: {fallback_error}",
                        error
                    ))
                }),
            Err(error) => Err(semantic_click_status(error)),
        },
        Err(error) if use_vision_fallback => resolve_vision_click_target(selector)
            .map_err(|fallback_error| {
                Status::internal(format!(
                    "accessibility lookup failed: {error}; vision fallback also failed: {fallback_error}"
                ))
            }),
        Err(error) => Err(Status::internal(error)),
    }
}

fn resolve_vision_click_target(
    selector: &str,
) -> std::result::Result<peekaboox_accessibility::ResolvedClickTarget, String> {
    let query = ElementQuery::parse(selector).map_err(|error| error.to_string())?;
    let options = ElementLookupOptions::default();
    let elements = vision_fallback_elements(&query, &options)?.elements;
    peekaboox_accessibility::resolve_click_target_from_tree(selector, &elements)
        .map_err(|error| error.to_string())
}

fn semantic_click_status(error: peekaboox_core::PeekabooXError) -> Status {
    let message = error.to_string();
    if message.contains("no clickable accessibility element matched") {
        Status::not_found(message)
    } else {
        Status::internal(message)
    }
}

fn grpc_type_text(
    request: proto::TypeTextRequest,
    config: &ServerConfig,
) -> Result<proto::ActionResponse, Status> {
    ensure_input_allowed(config).map_err(Status::permission_denied)?;
    let metadata = peekaboox_input::type_text(request.text)
        .map_err(|error| Status::internal(error.to_string()))?;
    let backend_kind = backend_kind_name(metadata.backend_kind);
    let backend_name = metadata.backend_name;

    Ok(proto::ActionResponse {
        ok: true,
        message: format!("typed text using {backend_name}/{backend_kind}"),
        backend_name: Some(backend_name),
        backend_kind: Some(backend_kind),
    })
}

fn grpc_paste_text(
    request: proto::PasteTextRequest,
    config: &ServerConfig,
) -> Result<proto::ActionResponse, Status> {
    ensure_input_allowed(config).map_err(Status::permission_denied)?;
    let metadata =
        peekaboox_input::paste_text_with_options(request.text, request.preserve_clipboard)
            .map_err(|error| Status::internal(error.to_string()))?;
    let backend_kind = backend_kind_name(metadata.backend_kind);
    let backend_name = metadata.backend_name;

    Ok(proto::ActionResponse {
        ok: true,
        message: format!("pasted text using {backend_name}/{backend_kind}"),
        backend_name: Some(backend_name),
        backend_kind: Some(backend_kind),
    })
}

fn grpc_hotkey(
    request: proto::HotkeyRequest,
    config: &ServerConfig,
) -> Result<proto::ActionResponse, Status> {
    ensure_input_allowed(config).map_err(Status::permission_denied)?;
    validate_hotkey_keys(&request.keys)?;
    let metadata = peekaboox_input::hotkey(request.keys)
        .map_err(|error| Status::internal(error.to_string()))?;
    let backend_kind = backend_kind_name(metadata.backend_kind);
    let backend_name = metadata.backend_name;

    Ok(proto::ActionResponse {
        ok: true,
        message: format!("pressed hotkey using {backend_name}/{backend_kind}"),
        backend_name: Some(backend_name),
        backend_kind: Some(backend_kind),
    })
}

fn grpc_list_windows_audit_details(request: &proto::ListWindowsRequest) -> serde_json::Value {
    json!({
        "id": request.id.as_deref(),
        "app": request.app.as_deref(),
        "title": request.title.as_deref(),
        "title_regex": request.title_regex.as_deref(),
        "focused": request.focused,
        "limit": request.limit,
        "sort": request.sort.as_deref(),
        "backend": request.backend.as_deref(),
        "diagnose": request.diagnose,
    })
}

fn window_query_from_proto(
    request: proto::ListWindowsRequest,
) -> Result<peekaboox_windows::WindowQuery, Status> {
    window_query_from_fields(WindowQueryFields {
        id: request.id,
        app: request.app,
        title: request.title,
        title_regex: request.title_regex,
        focused: request.focused,
        limit: request.limit.map(|value| value as usize),
        sort: request.sort,
        backend: request.backend,
        diagnose: request.diagnose,
    })
    .map_err(Status::invalid_argument)
}

struct WindowQueryFields {
    id: Option<String>,
    app: Option<String>,
    title: Option<String>,
    title_regex: Option<String>,
    focused: bool,
    limit: Option<usize>,
    sort: Option<String>,
    backend: Option<String>,
    diagnose: bool,
}

fn window_query_from_fields(
    fields: WindowQueryFields,
) -> Result<peekaboox_windows::WindowQuery, String> {
    let sort = match clean_optional_string(fields.sort) {
        Some(value) => peekaboox_windows::WindowSort::from_name(&value)
            .ok_or_else(|| format!("invalid windows sort: {value}"))?,
        None => peekaboox_windows::WindowSort::Backend,
    };
    let backend = match clean_optional_string(fields.backend) {
        Some(value) => peekaboox_windows::WindowBackendSelection::from_name(&value)
            .ok_or_else(|| format!("invalid windows backend: {value}"))?,
        None => peekaboox_windows::WindowBackendSelection::Auto,
    };

    if fields.limit == Some(0) {
        return Err("windows limit must be greater than zero".to_owned());
    }

    Ok(peekaboox_windows::WindowQuery {
        id: clean_optional_string(fields.id),
        app: clean_optional_string(fields.app),
        title: clean_optional_string(fields.title),
        title_regex: clean_optional_string(fields.title_regex),
        focused_only: fields.focused,
        limit: fields.limit,
        sort,
        backend,
        diagnose: fields.diagnose,
    })
}

fn clean_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn window_list_result_dto(metadata: peekaboox_windows::WindowListMetadata) -> WindowListResultDto {
    WindowListResultDto {
        backend_name: metadata.backend_name,
        backend_kind: backend_kind_name(metadata.backend_kind),
        warnings: metadata.warnings,
        backend_reports: metadata
            .backend_reports
            .into_iter()
            .map(|report| WindowBackendReportDto {
                backend_name: report.backend_name,
                backend_kind: backend_kind_name(report.backend_kind),
                raw_window_count: report.raw_window_count,
                matched_window_count: report.matched_window_count,
                selected: report.selected,
                error: report.error,
            })
            .collect(),
        windows: metadata.windows.iter().map(WindowDto::from).collect(),
    }
}

fn grpc_list_windows(
    list_windows: WindowListProvider,
    request: proto::ListWindowsRequest,
) -> Result<proto::ListWindowsResponse, Status> {
    let query = window_query_from_proto(request)?;
    let metadata = list_windows(query).map_err(|error| Status::internal(error.to_string()))?;
    Ok(proto::ListWindowsResponse {
        windows: metadata.windows.iter().map(proto_window_info).collect(),
        backend_name: metadata.backend_name,
        backend_kind: backend_kind_name(metadata.backend_kind),
        warnings: metadata.warnings,
        backend_reports: metadata
            .backend_reports
            .iter()
            .map(proto_window_backend_report)
            .collect(),
    })
}

struct GrpcFindElementResult {
    response: proto::FindElementResponse,
    cache_hit: bool,
    cache_age_ms: u128,
    vision_fallback_used: bool,
}

fn grpc_find_element(
    selector: &str,
    use_vision_fallback: bool,
    options: &ElementLookupOptions,
    accessibility_cache: &SharedAccessibilityCache,
) -> Result<GrpcFindElementResult, Status> {
    let result = find_elements_with_optional_vision_fallback(
        selector,
        use_vision_fallback,
        options,
        accessibility_cache,
    )
    .map_err(|error| Status::internal(error.to_string()))?;
    let elements = result.elements.iter().map(proto_ui_element).collect();

    Ok(GrpcFindElementResult {
        response: proto::FindElementResponse {
            elements,
            backend_name: result.backend_name,
            backend_kind: result.backend_kind,
            warnings: result.warnings,
            cache_hit: result.cache_hit,
            cache_age_ms: u64::try_from(result.cache_age_ms).unwrap_or(u64::MAX),
            vision_fallback_used: result.vision_fallback_used,
        },
        cache_hit: result.cache_hit,
        cache_age_ms: result.cache_age_ms,
        vision_fallback_used: result.vision_fallback_used,
    })
}

fn grpc_desktop_state(
    accessibility_cache: &SharedAccessibilityCache,
    list_windows: WindowListProvider,
) -> Result<proto::DesktopState, Status> {
    let metadata = list_windows(peekaboox_windows::WindowQuery::default())
        .map_err(|error| Status::internal(error.to_string()))?;
    let active_window = metadata
        .windows
        .iter()
        .find(|window| window.focused)
        .map(proto_window_info);
    let windows = metadata.windows.iter().map(proto_window_info).collect();
    let elements = cached_accessibility_tree(accessibility_cache)
        .map(|tree| {
            tree.metadata
                .elements
                .iter()
                .map(proto_ui_element)
                .collect()
        })
        .unwrap_or_default();

    Ok(proto::DesktopState {
        active_window,
        windows,
        elements,
    })
}

fn grpc_ocr_screen(request: proto::OcrScreenRequest) -> Result<proto::OcrResponse, Status> {
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

fn grpc_compare_images(
    request: proto::CompareImagesRequest,
) -> Result<proto::VisualDiffResponse, Status> {
    if request.expected_image.is_empty() || request.actual_image.is_empty() {
        return Err(Status::invalid_argument(
            "expected_image and actual_image must not be empty",
        ));
    }

    let options = visual_compare_options(
        request.region.map(rect_from_proto),
        request.per_channel_threshold.unwrap_or_default(),
        request.max_changed_ratio,
    )?;
    let result = peekaboox_vision::compare_image_bytes(
        &request.expected_image,
        &request.actual_image,
        &options,
    )
    .map_err(|error| Status::invalid_argument(error.to_string()))?;

    Ok(proto_visual_diff_response(&result))
}

fn grpc_detect_ui_state(
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

    let options = ui_state_options(
        request.region.map(rect_from_proto),
        request.per_channel_threshold,
        request.stable_max_changed_ratio,
        request.loading_min_changed_ratio,
        request.required_stable_transitions,
    )?;
    let result = peekaboox_vision::detect_ui_state_from_image_bytes(&request.images, &options)
        .map_err(|error| Status::invalid_argument(error.to_string()))?;

    Ok(proto_ui_state_response(&result))
}

fn grpc_detect_ui_elements(
    request: proto::DetectUiElementsRequest,
) -> Result<proto::DetectUiElementsResponse, Status> {
    if request.image.is_empty() {
        return Err(Status::invalid_argument("image must not be empty"));
    }

    let options = ui_element_detection_options(
        request.region.map(rect_from_proto),
        request.edge_threshold,
        request.min_width,
        request.min_height,
        request.min_component_pixels,
        request.max_elements,
        request.merge_distance,
    )?;
    let elements = peekaboox_vision::detect_ui_elements_from_image_bytes(&request.image, &options)
        .map_err(|error| Status::invalid_argument(error.to_string()))?;

    Ok(proto_detect_ui_elements_response(&elements))
}

fn grpc_probe_dmabuf(
    request: proto::ProbeDmaBufRequest,
) -> Result<proto::DmaBufProbeResponse, Status> {
    let target = proto_dmabuf_import_target(request.import_target)?;
    let result = probe_dmabuf_import(target).map_err(Status::internal)?;
    Ok(proto_dmabuf_probe_response(result))
}

fn grpc_list_plugins(
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

fn grpc_call_plugin_tool(
    request: proto::CallPluginToolRequest,
    config: &ServerConfig,
) -> Result<proto::PluginToolExecutionResponse, Status> {
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
    let paths = if request.paths.is_empty() {
        config.plugin_paths.clone()
    } else {
        request.paths.into_iter().map(PathBuf::from).collect()
    };
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

fn grpc_desktop_focus(
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

fn grpc_desktop_locate(
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

fn grpc_desktop_click(
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

fn grpc_desktop_drag(
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

fn grpc_desktop_type_into(
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

fn grpc_desktop_assert(
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

fn visual_compare_options(
    region: Option<Rect>,
    per_channel_threshold: u32,
    max_changed_ratio: Option<f32>,
) -> Result<VisualCompareOptions, Status> {
    let per_channel_threshold = u8::try_from(per_channel_threshold)
        .map_err(|_| Status::invalid_argument("per_channel_threshold must be between 0 and 255"))?;
    let max_changed_ratio = max_changed_ratio.unwrap_or_default();
    if !max_changed_ratio.is_finite() || !(0.0..=1.0).contains(&max_changed_ratio) {
        return Err(Status::invalid_argument(
            "max_changed_ratio must be between 0.0 and 1.0",
        ));
    }

    Ok(VisualCompareOptions {
        region,
        per_channel_threshold,
        max_changed_ratio,
    })
}

fn ui_state_options(
    region: Option<Rect>,
    per_channel_threshold: Option<u32>,
    stable_max_changed_ratio: Option<f32>,
    loading_min_changed_ratio: Option<f32>,
    required_stable_transitions: Option<u32>,
) -> Result<UiStateOptions, Status> {
    let mut options = UiStateOptions {
        region,
        ..UiStateOptions::default()
    };
    if let Some(per_channel_threshold) = per_channel_threshold {
        options.per_channel_threshold = u8::try_from(per_channel_threshold).map_err(|_| {
            Status::invalid_argument("per_channel_threshold must be between 0 and 255")
        })?;
    }
    if let Some(stable_max_changed_ratio) = stable_max_changed_ratio {
        options.stable_max_changed_ratio = stable_max_changed_ratio;
    }
    if let Some(loading_min_changed_ratio) = loading_min_changed_ratio {
        options.loading_min_changed_ratio = loading_min_changed_ratio;
    }
    if let Some(required_stable_transitions) = required_stable_transitions {
        options.required_stable_transitions = usize::try_from(required_stable_transitions)
            .map_err(|_| Status::invalid_argument("required_stable_transitions is too large"))?;
    }

    Ok(options)
}

fn ui_element_detection_options(
    region: Option<Rect>,
    edge_threshold: Option<u32>,
    min_width: Option<u32>,
    min_height: Option<u32>,
    min_component_pixels: Option<u32>,
    max_elements: Option<u32>,
    merge_distance: Option<u32>,
) -> Result<UiElementDetectionOptions, Status> {
    let mut options = UiElementDetectionOptions {
        region,
        ..UiElementDetectionOptions::default()
    };
    if let Some(edge_threshold) = edge_threshold {
        options.edge_threshold = u8::try_from(edge_threshold)
            .map_err(|_| Status::invalid_argument("edge_threshold must be between 0 and 255"))?;
    }
    if let Some(min_width) = min_width {
        options.min_width = min_width;
    }
    if let Some(min_height) = min_height {
        options.min_height = min_height;
    }
    if let Some(min_component_pixels) = min_component_pixels {
        options.min_component_pixels = min_component_pixels;
    }
    if let Some(max_elements) = max_elements {
        options.max_elements = usize::try_from(max_elements)
            .map_err(|_| Status::invalid_argument("max_elements is too large"))?;
    }
    if let Some(merge_distance) = merge_distance {
        options.merge_distance = merge_distance;
    }

    Ok(options)
}

#[allow(clippy::too_many_arguments)]
fn element_lookup_options_from_request(
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

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[derive(Debug, Clone, PartialEq)]
struct OcrRunRequest {
    image_path: Option<String>,
    region: Option<Rect>,
    app: Option<String>,
    window_title: Option<String>,
    window_id: Option<String>,
    options: OcrOptions,
}

#[derive(Debug, Clone, PartialEq)]
struct OcrOptionInput {
    language: Option<String>,
    page_segmentation_mode: Option<u8>,
    engine_mode: Option<u8>,
    dpi: Option<u32>,
    min_confidence: Option<f32>,
    whitelist: Option<String>,
    config: Vec<String>,
    scale: Option<f32>,
    grayscale: bool,
    threshold: Option<u8>,
    invert: bool,
    contrast: Option<f32>,
    deskew: bool,
}

fn run_ocr(
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

fn ocr_options(
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

fn parse_ocr_config(entry: &str) -> std::result::Result<OcrConfig, peekaboox_core::PeekabooXError> {
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

fn resolve_ocr_window_region(
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

fn offset_ocr_region(
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

fn ocr_status(error: peekaboox_core::PeekabooXError) -> Status {
    let message = error.to_string();
    if message.contains("not available") {
        Status::failed_precondition(message)
    } else {
        Status::internal(message)
    }
}

fn capture_target_name(target: Option<&proto::CaptureTarget>) -> &'static str {
    match target.and_then(|target| target.target.as_ref()) {
        None => "full_screen_default",
        Some(capture_target::Target::FullScreen(true)) => "full_screen",
        Some(capture_target::Target::FullScreen(false)) => "full_screen_false",
        Some(capture_target::Target::Region(_)) => "region",
        Some(capture_target::Target::WindowId(_)) => "window",
    }
}

fn proto_window_info(window: &WindowInfo) -> proto::WindowInfo {
    proto::WindowInfo {
        id: window.id.clone(),
        title: window.title.clone(),
        app_id: window.app_id.clone(),
        bounds: Some(proto_rect(window.bounds)),
        focused: window.focused,
        state: format!("{:?}", window.state).to_ascii_lowercase(),
    }
}

fn proto_window_backend_report(
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

fn proto_ui_element(element: &UiElement) -> proto::UiElement {
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

fn proto_detect_ui_elements_response(elements: &[UiElement]) -> proto::DetectUiElementsResponse {
    proto::DetectUiElementsResponse {
        backend_name: VISION_UI_BACKEND_NAME.to_owned(),
        backend_kind: VISION_UI_BACKEND_KIND.to_owned(),
        warnings: Vec::new(),
        elements: elements.iter().map(proto_ui_element).collect(),
    }
}

fn ui_element_list_dto(elements: &[UiElement]) -> ElementListResultDto {
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

fn element_lookup_dto(result: &ElementLookupResult) -> ElementListResultDto {
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

fn proto_ocr_response(result: &OcrResult) -> proto::OcrResponse {
    proto::OcrResponse {
        backend_name: result.backend_name.clone(),
        text: result.text.clone(),
        blocks: result.blocks.iter().map(proto_ocr_block).collect(),
        warnings: result.warnings.clone(),
        words: result.words.iter().map(proto_ocr_block).collect(),
    }
}

fn proto_ocr_block(block: &peekaboox_vision::OcrText) -> proto::OcrBlock {
    proto::OcrBlock {
        text: block.text.clone(),
        element: Some(proto_ui_element(&block.element)),
    }
}

fn ocr_result_dto(result: &OcrResult) -> OcrResultDto {
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

fn proto_capture_delta_response(data: &CaptureDeltaData) -> proto::CaptureDeltaResponse {
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

fn capture_delta_dto(data: &CaptureDeltaData) -> CaptureDeltaResultDto {
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

fn proto_capture_backends_response(
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

fn proto_capture_backend(backend: CaptureBackendDto) -> proto::CaptureBackend {
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

fn proto_zero_copy_backend(backend: ZeroCopyBackendDto) -> proto::ZeroCopyBackend {
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

fn proto_capture_backend_probe_result(
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

fn proto_plugin_list_response(
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

fn proto_plugin(plugin: &peekaboox_plugins::PluginDescriptor) -> proto::Plugin {
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

fn proto_plugin_execution_response(
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

fn proto_desktop_action_response(
    result: peekaboox_desktop::DesktopActionResult,
) -> proto::DesktopActionResponse {
    proto::DesktopActionResponse {
        app: result.app,
        action: result.action,
        detail: result.detail,
        backend_name: result.backend_name,
        verified: result.verified,
        verification_detail: result.verification_detail,
    }
}

fn proto_desktop_locate_response(
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

fn proto_desktop_assertion(
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

fn capture_backend_probe_from_proto(value: i32) -> Result<CaptureBackendProbeDto, Status> {
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

fn proto_dmabuf_import_target(value: i32) -> Result<DmaBufImportTargetDto, Status> {
    match value {
        0 | 1 => Ok(DmaBufImportTargetDto::Compute),
        2 => Ok(DmaBufImportTargetDto::Egl),
        3 => Ok(DmaBufImportTargetDto::EglTexture),
        _ => Err(Status::invalid_argument("unknown import_target")),
    }
}

fn proto_dmabuf_import_target_value(value: DmaBufImportTargetDto) -> i32 {
    match value {
        DmaBufImportTargetDto::Compute => 1,
        DmaBufImportTargetDto::Egl => 2,
        DmaBufImportTargetDto::EglTexture => 3,
    }
}

fn proto_dmabuf_probe_response(result: DmaBufProbeResultDto) -> proto::DmaBufProbeResponse {
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

fn capture_delta_metadata(data: &CaptureDeltaData) -> proto::CaptureMetadata {
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

fn proto_pixel_format(format: PixelFormat) -> i32 {
    match format {
        PixelFormat::Rgb8 => proto::PixelFormat::Rgb8 as i32,
        PixelFormat::Rgba8 => proto::PixelFormat::Rgba8 as i32,
        PixelFormat::Bgra8 => proto::PixelFormat::Bgra8 as i32,
    }
}

fn pixel_format_name(format: PixelFormat) -> &'static str {
    match format {
        PixelFormat::Rgb8 => "rgb8",
        PixelFormat::Rgba8 => "rgba8",
        PixelFormat::Bgra8 => "bgra8",
    }
}

fn proto_visual_diff_response(result: &VisualDiffResult) -> proto::VisualDiffResponse {
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

fn proto_ui_state_response(result: &UiStateResult) -> proto::UiStateResponse {
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

fn ui_state_dto(result: &UiStateResult) -> UiStateDto {
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

fn proto_ui_state_kind(kind: UiStateKind) -> i32 {
    match kind {
        UiStateKind::Stable => 1,
        UiStateKind::Loading => 2,
        UiStateKind::Changing => 3,
    }
}

fn ui_state_name(kind: UiStateKind) -> &'static str {
    match kind {
        UiStateKind::Stable => "stable",
        UiStateKind::Loading => "loading",
        UiStateKind::Changing => "changing",
    }
}

fn proto_rect(rect: Rect) -> proto::Rect {
    proto::Rect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}

fn rect_dto_to_proto(rect: RectDto) -> proto::Rect {
    proto::Rect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}

fn proto_point(point: Point) -> proto::Point {
    proto::Point {
        x: point.x,
        y: point.y,
    }
}

fn rect_from_proto(rect: proto::Rect) -> Rect {
    Rect::new(rect.x, rect.y, rect.width, rect.height)
}

fn audit_grpc_result<T>(
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

fn ensure_input_allowed(config: &ServerConfig) -> Result<(), String> {
    if config.allow_input {
        return Ok(());
    }

    Err(
        "permission denied: non-dry-run input actions require peekabooxd --profile operator, --allow-input, or PEEKABOOX_ALLOW_INPUT=1"
            .to_owned(),
    )
}

fn prepare_socket_path(socket: &PathBuf) -> Result<(), String> {
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

struct SocketGuard {
    path: PathBuf,
}

impl Drop for SocketGuard {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_file(&self.path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!("failed to remove socket {}: {error}", self.path.display());
        }
    }
}

fn mouse_button(button: MouseButtonDto) -> MouseButton {
    match button {
        MouseButtonDto::Left => MouseButton::Left,
        MouseButtonDto::Middle => MouseButton::Middle,
        MouseButtonDto::Right => MouseButton::Right,
    }
}

fn proto_mouse_button(button: Option<i32>) -> Result<MouseButton, Status> {
    match proto::MouseButton::try_from(button.unwrap_or(proto::MouseButton::Left as i32)) {
        Ok(proto::MouseButton::Unspecified) | Ok(proto::MouseButton::Left) => Ok(MouseButton::Left),
        Ok(proto::MouseButton::Middle) => Ok(MouseButton::Middle),
        Ok(proto::MouseButton::Right) => Ok(MouseButton::Right),
        Err(_) => Err(Status::invalid_argument("unknown mouse button")),
    }
}

fn input_metadata_dto(metadata: peekaboox_input::InputExecutionMetadata) -> ActionResultDto {
    ActionResultDto {
        backend_name: metadata.backend_name,
        backend_kind: backend_kind_name(metadata.backend_kind),
    }
}

fn detected_input_backend_dto(backend: peekaboox_input::DetectedInputBackend) -> ActionResultDto {
    ActionResultDto {
        backend_name: backend.name().to_owned(),
        backend_kind: backend_kind_name(backend.backend_kind()),
    }
}

fn desktop_action_dto(result: peekaboox_desktop::DesktopActionResult) -> DesktopActionResultDto {
    DesktopActionResultDto {
        app: result.app,
        action: result.action,
        detail: result.detail,
        backend_name: result.backend_name,
        verified: result.verified,
        verification_detail: result.verification_detail,
    }
}

fn desktop_locate_dto(result: peekaboox_desktop::ResolvedDesktopTarget) -> DesktopLocateResultDto {
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

fn desktop_assertion(
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

fn required_expected_text(
    assertion: &str,
    expected_text: Option<String>,
) -> Result<String, String> {
    expected_text
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("desktop assertion {assertion} requires expected_text"))
}

fn validate_ratio(name: &str, value: f32) -> Result<(), String> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(format!("{name} must be between 0.0 and 1.0"))
    }
}

fn validate_ratio_status(name: &str, value: f32) -> Result<(), Status> {
    validate_ratio(name, value).map_err(Status::invalid_argument)
}

fn validate_hotkey_keys(keys: &[String]) -> Result<(), Status> {
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

fn backend_kind_name(kind: BackendKind) -> String {
    format!("{kind:?}").to_ascii_lowercase()
}

fn print_usage() {
    println!(
        "Usage: peekabooxd run [--profile <observe|assist|operator>] [--sandbox <off|basic|strict>] [--socket <path>] [--audit-log <path>] [--grpc-addr <addr>] [--no-grpc] [--accessibility-cache-ttl-ms <ms>] [--allow-input] [--vision-fallback] [--no-accessibility-events] [--no-emergency-hotkey] [--once]"
    );
}

fn default_grpc_addr() -> SocketAddr {
    DEFAULT_GRPC_ADDR
        .parse()
        .expect("default gRPC address must be valid")
}

fn default_accessibility_cache_ttl() -> Duration {
    Duration::from_millis(DEFAULT_ACCESSIBILITY_CACHE_TTL_MS)
}

fn install_shutdown_handler() -> Result<Arc<AtomicBool>, String> {
    let shutdown = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(SIGINT, Arc::clone(&shutdown))
        .map_err(|error| format!("failed to register SIGINT handler: {error}"))?;
    signal_hook::flag::register(SIGTERM, Arc::clone(&shutdown))
        .map_err(|error| format!("failed to register SIGTERM handler: {error}"))?;
    Ok(shutdown)
}

fn input_allowed_from_env() -> bool {
    std::env::var("PEEKABOOX_ALLOW_INPUT")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn vision_fallback_from_env() -> bool {
    std::env::var("PEEKABOOX_VISION_FALLBACK")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn emergency_hotkey_enabled_from_env() -> bool {
    std::env::var("PEEKABOOX_EMERGENCY_HOTKEY")
        .map(|value| !matches!(value.as_str(), "0" | "false" | "FALSE" | "no" | "NO"))
        .unwrap_or(true)
}

fn daemon_policy_profile_from_env() -> Result<DaemonPolicyProfile, String> {
    std::env::var("PEEKABOOX_DAEMON_PROFILE")
        .map(|value| DaemonPolicyProfile::parse(&value))
        .unwrap_or(Ok(DaemonPolicyProfile::Observe))
}

fn sandbox_profile_from_env() -> Result<SandboxProfile, String> {
    std::env::var("PEEKABOOX_DAEMON_SANDBOX")
        .map(|value| SandboxProfile::parse(&value))
        .unwrap_or(Ok(SandboxProfile::Off))
}

fn default_audit_log_path() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".local/state"))
        })
        .unwrap_or_else(std::env::temp_dir)
        .join("peekaboox/audit.jsonl")
}

fn audit_write(
    audit: &SharedAudit,
    event: &str,
    version: Option<&str>,
    status: &str,
    error: Option<&str>,
    details: serde_json::Value,
) {
    match audit.lock() {
        Ok(mut logger) => logger.write(event, version, status, error, details),
        Err(_) => eprintln!("failed to lock audit log for event {event}"),
    }
}

struct AuditLogger {
    path: PathBuf,
}

impl AuditLogger {
    fn new(path: PathBuf) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create audit log directory: {error}"))?;
        }

        Ok(Self { path })
    }

    fn write(
        &mut self,
        event: &str,
        version: Option<&str>,
        status: &str,
        error: Option<&str>,
        details: serde_json::Value,
    ) {
        let record = json!({
            "ts_unix_ms": unix_time_ms(),
            "event": event,
            "version": version,
            "status": status,
            "error": error,
            "pid": std::process::id(),
            "details": details
        });

        if let Err(write_error) = self.write_record(&record) {
            eprintln!(
                "failed to write audit log {}: {write_error}",
                self.path.display()
            );
        }
    }

    fn write_record(&self, record: &serde_json::Value) -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        serde_json::to_writer(&mut file, record)?;
        file.write_all(b"\n")?;
        Ok(())
    }
}

fn unix_time_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

fn unix_time_ms_u64() -> u64 {
    unix_time_ms().min(u128::from(u64::MAX)) as u64
}

fn request_method(request: &ApiRequest) -> &'static str {
    match request {
        ApiRequest::Ping => "ping",
        ApiRequest::Capture { .. } => "capture",
        ApiRequest::CaptureDelta { .. } => "capture_delta",
        ApiRequest::CaptureBackends { .. } => "capture_backends",
        ApiRequest::ProbeDmaBuf { .. } => "probe_dmabuf",
        ApiRequest::ListPlugins { .. } => "list_plugins",
        ApiRequest::CallPluginTool { .. } => "call_plugin_tool",
        ApiRequest::Click { .. } => "click",
        ApiRequest::MoveMouse { .. } => "move_mouse",
        ApiRequest::Drag { .. } => "drag",
        ApiRequest::TypeText { .. } => "type_text",
        ApiRequest::PasteText { .. } => "paste_text",
        ApiRequest::Hotkey { .. } => "hotkey",
        ApiRequest::ListWindows { .. } => "list_windows",
        ApiRequest::FindElements { .. } => "find_elements",
        ApiRequest::Ocr { .. } => "ocr",
        ApiRequest::CompareImages { .. } => "compare_images",
        ApiRequest::DetectUiState { .. } => "detect_ui_state",
        ApiRequest::DetectUiElements { .. } => "detect_ui_elements",
        ApiRequest::DesktopFocus { .. } => "desktop_focus",
        ApiRequest::DesktopLocate { .. } => "desktop_locate",
        ApiRequest::DesktopClick { .. } => "desktop_click",
        ApiRequest::DesktopDrag { .. } => "desktop_drag",
        ApiRequest::DesktopTypeInto { .. } => "desktop_type_into",
        ApiRequest::DesktopAssert { .. } => "desktop_assert",
    }
}

fn audit_details(request: &ApiRequest) -> serde_json::Value {
    match request {
        ApiRequest::Ping => json!({}),
        ApiRequest::Capture {
            output,
            region,
            window_id,
        } => json!({
            "output": output,
            "has_region": region.is_some(),
            "has_window_id": window_id.as_deref().is_some_and(|value| !value.trim().is_empty())
        }),
        ApiRequest::CaptureDelta {
            stream_id,
            reset,
            region,
            window_id,
            per_channel_threshold,
            low_bandwidth,
        } => json!({
            "stream_id": stream_id.as_deref().map(normalized_capture_stream_id).unwrap_or_else(|| normalized_capture_stream_id("")),
            "reset": reset,
            "has_region": region.is_some(),
            "has_window_id": window_id.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "per_channel_threshold": per_channel_threshold,
            "low_bandwidth": low_bandwidth
        }),
        ApiRequest::CaptureBackends {
            output,
            region,
            diagnose,
            probe,
        } => json!({
            "output": output,
            "has_region": region.is_some(),
            "diagnose": diagnose,
            "probe": format!("{probe:?}").to_ascii_lowercase()
        }),
        ApiRequest::ProbeDmaBuf { import_target } => json!({
            "import_target": format!("{import_target:?}").to_ascii_lowercase()
        }),
        ApiRequest::ListPlugins { paths } => json!({
            "path_count": paths.len()
        }),
        ApiRequest::CallPluginTool {
            plugin_id,
            tool,
            arguments,
            paths,
            timeout_ms,
            max_output_bytes,
        } => json!({
            "plugin_id": plugin_id,
            "tool": tool,
            "argument_keys": arguments.as_object().map(|object| object.len()).unwrap_or_default(),
            "path_count": paths.len(),
            "timeout_ms": timeout_ms,
            "max_output_bytes": max_output_bytes
        }),
        ApiRequest::Click {
            x,
            y,
            button,
            dry_run,
        } => json!({
            "x": x,
            "y": y,
            "button": format!("{button:?}").to_ascii_lowercase(),
            "dry_run": dry_run
        }),
        ApiRequest::MoveMouse { x, y, dry_run } => json!({
            "x": x,
            "y": y,
            "dry_run": dry_run
        }),
        ApiRequest::Drag {
            from_x,
            from_y,
            to_x,
            to_y,
            button,
            duration_ms,
            dry_run,
        } => json!({
            "from_x": from_x,
            "from_y": from_y,
            "to_x": to_x,
            "to_y": to_y,
            "button": format!("{button:?}").to_ascii_lowercase(),
            "duration_ms": duration_ms,
            "dry_run": dry_run
        }),
        ApiRequest::TypeText { text, dry_run } => {
            json!({ "text_length": text.chars().count(), "dry_run": dry_run })
        }
        ApiRequest::PasteText {
            text,
            preserve_clipboard,
            dry_run,
        } => {
            json!({
                "text_length": text.chars().count(),
                "preserve_clipboard": preserve_clipboard,
                "dry_run": dry_run
            })
        }
        ApiRequest::Hotkey { keys, dry_run } => {
            json!({ "key_count": keys.len(), "dry_run": dry_run })
        }
        ApiRequest::ListWindows {
            id,
            app,
            title,
            title_regex,
            focused,
            limit,
            sort,
            backend,
            diagnose,
        } => json!({
            "id": id.as_deref(),
            "app": app.as_deref(),
            "title": title.as_deref(),
            "title_regex": title_regex.as_deref(),
            "focused": focused,
            "limit": limit,
            "sort": sort.as_deref(),
            "backend": backend.as_deref(),
            "diagnose": diagnose
        }),
        ApiRequest::FindElements {
            selector,
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
        } => {
            json!({
                "selector_length": selector.chars().count(),
                "vision_fallback": vision_fallback,
                "has_app": app.as_deref().is_some_and(|value| !value.trim().is_empty()),
                "has_window_title": window_title.as_deref().is_some_and(|value| !value.trim().is_empty()),
                "has_window_id": window_id.as_deref().is_some_and(|value| !value.trim().is_empty()),
                "has_vision_region": vision_region.is_some(),
                "has_vision_edge_threshold": vision_edge_threshold.is_some(),
                "has_vision_min_width": vision_min_width.is_some(),
                "has_vision_min_height": vision_min_height.is_some(),
                "has_vision_min_component_pixels": vision_min_component_pixels.is_some(),
                "has_vision_max_elements": vision_max_elements.is_some(),
                "has_vision_merge_distance": vision_merge_distance.is_some()
            })
        }
        ApiRequest::Ocr {
            image_path,
            region,
            app,
            window_title,
            window_id,
            language,
            scale,
            grayscale,
            threshold,
            invert,
            contrast,
            deskew,
            ..
        } => {
            json!({
                "has_image_path": image_path.as_deref().is_some_and(|path| !path.trim().is_empty()),
                "has_region": region.is_some(),
                "has_app": app.as_deref().is_some_and(|value| !value.trim().is_empty()),
                "has_window_title": window_title.as_deref().is_some_and(|value| !value.trim().is_empty()),
                "has_window_id": window_id.as_deref().is_some_and(|value| !value.trim().is_empty()),
                "has_language": language.as_deref().is_some_and(|language| !language.trim().is_empty()),
                "has_preprocessing": scale.is_some()
                    || *grayscale
                    || threshold.is_some()
                    || *invert
                    || contrast.is_some()
                    || *deskew
            })
        }
        ApiRequest::CompareImages {
            expected_path,
            actual_path,
            region,
            per_channel_threshold,
            max_changed_ratio,
        } => {
            json!({
                "expected_path": expected_path,
                "actual_path": actual_path,
                "has_region": region.is_some(),
                "per_channel_threshold": per_channel_threshold,
                "max_changed_ratio": max_changed_ratio
            })
        }
        ApiRequest::DetectUiState {
            image_paths,
            region,
            per_channel_threshold,
            stable_max_changed_ratio,
            loading_min_changed_ratio,
            required_stable_transitions,
        } => {
            json!({
                "image_paths": image_paths,
                "image_count": image_paths.len(),
                "has_region": region.is_some(),
                "per_channel_threshold": per_channel_threshold,
                "stable_max_changed_ratio": stable_max_changed_ratio,
                "loading_min_changed_ratio": loading_min_changed_ratio,
                "required_stable_transitions": required_stable_transitions
            })
        }
        ApiRequest::DetectUiElements {
            image_path,
            region,
            edge_threshold,
            min_width,
            min_height,
            min_component_pixels,
            max_elements,
            merge_distance,
        } => {
            json!({
                "image_path": image_path,
                "has_region": region.is_some(),
                "edge_threshold": edge_threshold,
                "min_width": min_width,
                "min_height": min_height,
                "min_component_pixels": min_component_pixels,
                "max_elements": max_elements,
                "merge_distance": merge_distance
            })
        }
        ApiRequest::DesktopFocus {
            app,
            use_gnome_overview,
            launch_if_needed,
            wait_after_focus_ms,
            overview_wait_ms,
            window_title,
            window_id,
            verify,
        } => json!({
            "app": app,
            "use_gnome_overview": use_gnome_overview,
            "launch_if_needed": launch_if_needed,
            "wait_after_focus_ms": wait_after_focus_ms,
            "overview_wait_ms": overview_wait_ms,
            "has_window_title": window_title.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "has_window_id": window_id.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "verify": verify
        }),
        ApiRequest::DesktopLocate {
            app,
            target,
            image_path,
            prefer_accessibility,
            window_title,
            window_id,
        } => json!({
            "app": app,
            "target": target,
            "has_image_path": image_path.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "prefer_accessibility": prefer_accessibility,
            "has_window_title": window_title.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "has_window_id": window_id.as_deref().is_some_and(|value| !value.trim().is_empty())
        }),
        ApiRequest::DesktopClick {
            app,
            target,
            image_path,
            prefer_accessibility,
            window_title,
            button,
            dry_run,
            window_id,
            verify,
        } => json!({
            "app": app,
            "target": target,
            "has_image_path": image_path.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "prefer_accessibility": prefer_accessibility,
            "has_window_title": window_title.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "has_window_id": window_id.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "button": format!("{button:?}").to_ascii_lowercase(),
            "dry_run": dry_run,
            "verify": verify
        }),
        ApiRequest::DesktopDrag {
            app,
            target,
            image_path,
            prefer_accessibility,
            window_title,
            button,
            from_ratio_x,
            from_ratio_y,
            to_ratio_x,
            to_ratio_y,
            duration_ms,
            dry_run,
            window_id,
            verify,
        } => json!({
            "app": app,
            "target": target,
            "has_image_path": image_path.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "prefer_accessibility": prefer_accessibility,
            "has_window_title": window_title.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "has_window_id": window_id.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "button": format!("{button:?}").to_ascii_lowercase(),
            "from_ratio_x": from_ratio_x,
            "from_ratio_y": from_ratio_y,
            "to_ratio_x": to_ratio_x,
            "to_ratio_y": to_ratio_y,
            "duration_ms": duration_ms,
            "dry_run": dry_run,
            "verify": verify
        }),
        ApiRequest::DesktopTypeInto {
            app,
            target,
            text,
            image_path,
            prefer_accessibility,
            window_title,
            clear,
            dry_run,
            window_id,
            verify,
        } => json!({
            "app": app,
            "target": target,
            "text_length": text.chars().count(),
            "has_image_path": image_path.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "prefer_accessibility": prefer_accessibility,
            "has_window_title": window_title.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "has_window_id": window_id.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "clear": clear,
            "dry_run": dry_run,
            "verify": verify
        }),
        ApiRequest::DesktopAssert {
            app,
            target,
            image_path,
            prefer_accessibility,
            window_title,
            assertion,
            expected_text,
            window_id,
        } => json!({
            "app": app,
            "target": target,
            "has_image_path": image_path.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "prefer_accessibility": prefer_accessibility,
            "has_window_title": window_title.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "has_window_id": window_id.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "assertion": format!("{assertion:?}").to_ascii_lowercase(),
            "has_expected_text": expected_text.as_deref().is_some_and(|value| !value.trim().is_empty())
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use super::{
        AccessibilityCache, AccessibilityCacheSnapshot, CachedAccessibilityTree, CaptureDeltaData,
        DaemonCommand, DaemonPolicyProfile, ElementLookupOptions, ElementLookupResult,
        GrpcPeekabooXService, IncrementalCaptureState, SandboxProfile, ServerConfig,
        VISION_UI_BACKEND_KIND, VISION_UI_BACKEND_NAME, audit_details, capture_delta_dto,
        default_accessibility_cache_ttl, default_audit_log_path, default_grpc_addr,
        dispatch_request, element_lookup_with_optional_vision_fallback, emergency_hotkey_details,
        emergency_hotkey_enabled_from_env, ensure_input_allowed, input_allowed_from_env,
        linux_input_event_size, ocr_result_dto, parse_args, parse_linux_input_event,
        proto_capture_backends_response, proto_capture_delta_response,
        proto_detect_ui_elements_response, proto_ocr_response, proto_ui_state_response,
        proto_visual_diff_response, sandbox_profile_from_env, server_config_for_profile,
        ui_element_list_dto, ui_state_dto, vision_fallback_from_env, visual_diff_dto,
    };
    use peekaboox_accessibility::AccessibilityTreeMetadata;
    use peekaboox_core::{BackendKind, PixelFormat, Rect, UiElement, WindowInfo, WindowState};
    use peekaboox_ipc::{
        API_VERSION, ApiRequest, ApiResult, CaptureBackendDto, CaptureBackendProbeResultDto,
        CaptureBackendsResultDto, RectDto, ZeroCopyBackendDto,
        proto::{
            self,
            peekaboo_x_client::PeekabooXClient,
            peekaboo_x_server::{PeekabooX, PeekabooXServer},
        },
    };
    use std::sync::{Arc, Mutex};
    use tokio_stream::wrappers::TcpListenerStream;

    #[test]
    fn default_command_runs_daemon() {
        let command = parse_args(vec![]).unwrap();

        assert!(matches!(
            command,
            DaemonCommand::Run {
                config: ServerConfig { once: false, .. }
            }
        ));
    }

    #[test]
    fn parses_run_options() {
        let command = parse_args(vec![
            "run".to_owned(),
            "--socket".to_owned(),
            "/tmp/peekaboox-test.sock".to_owned(),
            "--audit-log".to_owned(),
            "/tmp/peekaboox-audit.jsonl".to_owned(),
            "--sandbox".to_owned(),
            "basic".to_owned(),
            "--grpc-addr".to_owned(),
            "127.0.0.1:47778".to_owned(),
            "--accessibility-cache-ttl-ms".to_owned(),
            "250".to_owned(),
            "--no-accessibility-events".to_owned(),
            "--no-emergency-hotkey".to_owned(),
            "--allow-input".to_owned(),
            "--vision-fallback".to_owned(),
            "--once".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            DaemonCommand::Run {
                config: ServerConfig {
                    socket: PathBuf::from("/tmp/peekaboox-test.sock"),
                    once: true,
                    audit_log: PathBuf::from("/tmp/peekaboox-audit.jsonl"),
                    policy_profile: DaemonPolicyProfile::Observe,
                    sandbox_profile: SandboxProfile::Basic,
                    allow_input: true,
                    vision_fallback: true,
                    grpc_addr: Some("127.0.0.1:47778".parse().unwrap()),
                    accessibility_cache_ttl: Duration::from_millis(250),
                    accessibility_events: false,
                    emergency_hotkey: false,
                    plugin_paths: Vec::new()
                }
            }
        );
    }

    #[test]
    fn parses_no_grpc_option() {
        let command = parse_args(vec!["run".to_owned(), "--no-grpc".to_owned()]).unwrap();

        assert!(matches!(
            command,
            DaemonCommand::Run {
                config: ServerConfig {
                    grpc_addr: None,
                    ..
                }
            }
        ));
    }

    #[test]
    fn parses_run_plugin_path_option() {
        let command = parse_args(vec![
            "run".to_owned(),
            "--plugin-path".to_owned(),
            "examples/plugins".to_owned(),
        ])
        .unwrap();

        assert!(matches!(
            command,
            DaemonCommand::Run {
                config: ServerConfig {
                    plugin_paths,
                    ..
                }
            } if plugin_paths == vec![PathBuf::from("examples/plugins")]
        ));
    }

    #[test]
    fn parses_run_policy_profile() {
        let command = parse_args(vec![
            "run".to_owned(),
            "--profile".to_owned(),
            "operator".to_owned(),
        ])
        .unwrap();

        assert!(matches!(
            command,
            DaemonCommand::Run {
                config: ServerConfig {
                    policy_profile: DaemonPolicyProfile::Operator,
                    allow_input: true,
                    vision_fallback: true,
                    ..
                }
            }
        ));
    }

    #[test]
    fn parses_run_sandbox_profile() {
        let command = parse_args(vec![
            "run".to_owned(),
            "--sandbox".to_owned(),
            "strict".to_owned(),
        ])
        .unwrap();

        assert!(matches!(
            command,
            DaemonCommand::Run {
                config: ServerConfig {
                    sandbox_profile: SandboxProfile::Strict,
                    ..
                }
            }
        ));
    }

    #[test]
    fn daemon_policy_profiles_apply_daemon_gates() {
        let observe = server_config_for_profile(DaemonPolicyProfile::Observe);
        let assist = server_config_for_profile(DaemonPolicyProfile::Assist);
        let operator = server_config_for_profile(DaemonPolicyProfile::Operator);

        assert!(!observe.allow_input);
        assert!(!observe.vision_fallback);
        assert!(!assist.allow_input);
        assert!(assist.vision_fallback);
        assert!(operator.allow_input);
        assert!(operator.vision_fallback);
    }

    #[test]
    fn audit_type_text_does_not_log_secret_text() {
        let details = audit_details(&ApiRequest::TypeText {
            text: "secret".to_owned(),
            dry_run: false,
        });

        assert_eq!(details["text_length"], 6);
        assert!(details.get("text").is_none());
    }

    #[test]
    fn default_audit_log_has_jsonl_name() {
        assert!(default_audit_log_path().ends_with("audit.jsonl"));
    }

    #[test]
    fn env_permission_helper_defaults_to_false() {
        let _ = input_allowed_from_env();
        let _ = vision_fallback_from_env();
        let _ = emergency_hotkey_enabled_from_env();
        let _ = sandbox_profile_from_env();
    }

    #[test]
    fn emergency_hotkey_details_names_default_hotkey() {
        let details = emergency_hotkey_details();

        assert_eq!(details["hotkey"], "CTRL+ALT+ESC");
    }

    #[test]
    fn linux_input_event_parser_reads_key_events() {
        let mut bytes = vec![0_u8; linux_input_event_size()];
        let offset = std::mem::size_of::<libc::timeval>();
        bytes[offset..offset + 2].copy_from_slice(&peekaboox_input::LINUX_EV_KEY.to_ne_bytes());
        bytes[offset + 2..offset + 4]
            .copy_from_slice(&peekaboox_input::LINUX_KEY_ESC.to_ne_bytes());
        bytes[offset + 4..offset + 8].copy_from_slice(&1_i32.to_ne_bytes());

        assert_eq!(
            parse_linux_input_event(&bytes),
            Some((
                peekaboox_input::LINUX_EV_KEY,
                peekaboox_input::LINUX_KEY_ESC,
                1
            ))
        );
    }

    #[test]
    fn input_permission_gate_denies_by_default() {
        let config = ServerConfig {
            socket: PathBuf::from("/tmp/peekaboox-test.sock"),
            once: true,
            audit_log: PathBuf::from("/tmp/peekaboox-audit.jsonl"),
            policy_profile: DaemonPolicyProfile::Observe,
            sandbox_profile: SandboxProfile::Off,
            allow_input: false,
            vision_fallback: false,
            grpc_addr: Some(default_grpc_addr()),
            accessibility_cache_ttl: default_accessibility_cache_ttl(),
            accessibility_events: true,
            emergency_hotkey: true,
            plugin_paths: Vec::new(),
        };

        let error = ensure_input_allowed(&config).unwrap_err();

        assert!(error.contains("--allow-input"));
    }

    #[test]
    fn ocr_result_maps_to_proto_and_json_dto() {
        let result = sample_ocr_result();

        let proto = proto_ocr_response(&result);
        assert_eq!(proto.backend_name, "tesseract");
        assert_eq!(proto.text, "Submit");
        assert_eq!(proto.blocks[0].element.as_ref().unwrap().role, "text");

        let dto = ocr_result_dto(&result);
        assert_eq!(dto.backend_name, "tesseract");
        assert_eq!(dto.blocks[0].element.bounds.x, 10);
        assert_eq!(dto.blocks[0].element.label.as_deref(), Some("Submit"));
    }

    #[test]
    fn detected_ui_elements_map_to_proto_and_json_dto() {
        let elements = vec![UiElement {
            id: "vision:0:10:20:100:40".to_owned(),
            role: "visual-region".to_owned(),
            label: None,
            bounds: Rect::new(10, 20, 100, 40),
            center: Rect::new(10, 20, 100, 40).center(),
            confidence: 0.86,
            states: vec!["visible".to_owned()],
            window_id: None,
            window_title: None,
            app_id: None,
            parent_id: None,
            child_ids: Vec::new(),
        }];

        let proto = proto_detect_ui_elements_response(&elements);
        assert_eq!(proto.backend_name, VISION_UI_BACKEND_NAME);
        assert_eq!(proto.backend_kind, VISION_UI_BACKEND_KIND);
        assert_eq!(proto.elements[0].role, "visual-region");
        assert_eq!(proto.elements[0].bounds.as_ref().unwrap().width, 100);

        let dto = ui_element_list_dto(&elements);
        assert_eq!(dto.backend_name, VISION_UI_BACKEND_NAME);
        assert_eq!(dto.backend_kind, VISION_UI_BACKEND_KIND);
        assert_eq!(dto.elements[0].bounds.x, 10);
        assert_eq!(dto.elements[0].states, vec!["visible".to_owned()]);
    }

    #[test]
    fn visual_diff_maps_to_proto_and_json_dto() {
        let result = sample_visual_diff_result();

        let proto = proto_visual_diff_response(&result);
        assert_eq!(proto.compared_pixels, 12);
        assert_eq!(proto.changed_pixels, 2);
        assert_eq!(proto.max_channel_delta, 255);
        assert_eq!(proto.changed_bounds.as_ref().unwrap().x, 1);
        assert!(!proto.matches);

        let dto = visual_diff_dto(&result);
        assert_eq!(
            dto.compared_region,
            peekaboox_ipc::RectDto::from(Rect::new(0, 0, 4, 3))
        );
        assert_eq!(
            dto.changed_bounds,
            Some(peekaboox_ipc::RectDto::from(Rect::new(1, 1, 2, 1)))
        );
        assert!(!dto.matches);
    }

    #[test]
    fn capture_delta_maps_to_proto_and_json_dto() {
        let data = sample_capture_delta_data();

        let proto = proto_capture_delta_response(&data);
        assert_eq!(proto.stream_id, "agent-loop");
        assert_eq!(proto.sequence, 3);
        assert!(proto.low_bandwidth);
        assert!(!proto.full_frame);
        assert_eq!(proto.pixel_format, proto::PixelFormat::Rgba8 as i32);
        assert_eq!(proto.capture_region.as_ref().unwrap().x, 10);
        assert_eq!(proto.changed_bounds.as_ref().unwrap().x, 1);
        assert_eq!(proto.patch, b"abc");
        assert_eq!(proto.metadata.as_ref().unwrap().backend, "fake/portal");

        let dto = capture_delta_dto(&data);
        assert_eq!(dto.stream_id, "agent-loop");
        assert!(dto.low_bandwidth);
        assert_eq!(dto.pixel_format, "rgba8");
        assert_eq!(dto.capture_region.unwrap().height, 120);
        assert_eq!(dto.changed_bounds.unwrap().width, 2);
        assert_eq!(dto.patch_base64, "YWJj");
        assert_eq!(dto.backend_kind, "portal");
    }

    #[test]
    fn capture_backends_maps_to_proto_response() {
        let response = proto_capture_backends_response(CaptureBackendsResultDto {
            session_type: "wayland".to_owned(),
            desktop: Some("GNOME".to_owned()),
            pipewire_session_available: true,
            pipewire_backend_feature_enabled: true,
            egl_backend_feature_enabled: false,
            output_path: "screen.png".to_owned(),
            region: Some(RectDto {
                x: 1,
                y: 2,
                width: 3,
                height: 4,
            }),
            image_backends: vec![CaptureBackendDto {
                name: "portal".to_owned(),
                backend_kind: "wayland".to_owned(),
                command: None,
                available: true,
                supports_output: true,
                supports_file_capture: true,
                supports_stdout_capture: true,
                supports_stdout_region_capture: true,
                selected: true,
                reason: None,
            }],
            zero_copy_backends: vec![ZeroCopyBackendDto {
                name: "pipewire".to_owned(),
                backend_kind: "wayland".to_owned(),
                transport: "dmabuf".to_owned(),
                availability: "available".to_owned(),
                selected: true,
                pipewire_backend_feature_enabled: true,
                egl_backend_feature_enabled: false,
                reason: None,
            }],
            probes: vec![CaptureBackendProbeResultDto {
                probe: "region".to_owned(),
                ok: true,
                backend_name: Some("portal".to_owned()),
                backend_kind: Some("wayland".to_owned()),
                detail: "captured 3x4".to_owned(),
                output_path: None,
                bytes_written: None,
                width: Some(3),
                height: Some(4),
            }],
            warnings: vec!["diagnostic".to_owned()],
        });

        assert_eq!(response.session_type, "wayland");
        assert_eq!(response.desktop.as_deref(), Some("GNOME"));
        assert_eq!(response.region.as_ref().unwrap().width, 3);
        assert_eq!(response.image_backends[0].name, "portal");
        assert!(response.zero_copy_backends[0].selected);
        assert_eq!(response.probes[0].probe, "region");
        assert_eq!(response.probes[0].width, Some(3));
        assert_eq!(response.warnings[0], "diagnostic");
    }

    #[test]
    fn ui_state_maps_to_proto_and_json_dto() {
        let result = sample_ui_state_result();

        let proto = proto_ui_state_response(&result);
        assert_eq!(proto.state, 2);
        assert_eq!(proto.compared_transitions, 2);
        assert_eq!(proto.loading_transitions, 1);
        assert_eq!(proto.latest_diff.as_ref().unwrap().changed_pixels, 2);
        assert_eq!(proto.changed_bounds.as_ref().unwrap().width, 2);

        let dto = ui_state_dto(&result);
        assert_eq!(dto.state, "loading");
        assert_eq!(dto.compared_transitions, 2);
        assert_eq!(dto.latest_diff.changed_pixels, 2);
        assert_eq!(
            dto.changed_bounds,
            Some(peekaboox_ipc::RectDto::from(Rect::new(1, 1, 2, 1)))
        );
    }

    #[test]
    fn accessibility_cache_returns_fresh_snapshot() {
        let mut cache = AccessibilityCache::new(Duration::from_secs(60));

        let stored = cache.store(sample_accessibility_metadata("Submit"));
        let fresh = cache.fresh().unwrap();

        assert!(!stored.cache_hit);
        assert!(fresh.cache_hit);
        assert_eq!(fresh.metadata.elements[0].label.as_deref(), Some("Submit"));
    }

    #[test]
    fn accessibility_cache_expires_old_snapshot() {
        let cache = AccessibilityCache {
            ttl: Duration::from_millis(1),
            snapshot: Some(AccessibilityCacheSnapshot {
                loaded_at: Instant::now() - Duration::from_secs(1),
                metadata: sample_accessibility_metadata("Old"),
            }),
        };

        assert!(cache.fresh().is_none());
    }

    #[test]
    fn accessibility_cache_invalidation_clears_snapshot() {
        let mut cache = AccessibilityCache::new(Duration::from_secs(60));
        cache.store(sample_accessibility_metadata("Submit"));

        assert!(cache.invalidate());
        assert!(cache.fresh().is_none());
        assert!(!cache.invalidate());
    }

    #[test]
    fn dispatch_find_elements_uses_cached_selector_query() {
        let cache = test_accessibility_cache();
        cache
            .lock()
            .unwrap()
            .store(sample_accessibility_metadata("Submit"));
        let config = ServerConfig {
            socket: PathBuf::from("/tmp/peekaboox-test.sock"),
            once: true,
            audit_log: PathBuf::from("/tmp/peekaboox-audit.jsonl"),
            policy_profile: DaemonPolicyProfile::Observe,
            sandbox_profile: SandboxProfile::Off,
            allow_input: false,
            vision_fallback: false,
            grpc_addr: None,
            accessibility_cache_ttl: default_accessibility_cache_ttl(),
            accessibility_events: true,
            emergency_hotkey: true,
            plugin_paths: Vec::new(),
        };

        let result = dispatch_request(
            ApiRequest::FindElements {
                selector: "state=enabled,contains=20,30,confidence>=0.9".to_owned(),
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
            },
            &config,
            &cache,
            &Arc::new(Mutex::new(IncrementalCaptureState::default())),
        )
        .unwrap();

        let ApiResult::FindElements(metadata) = result else {
            panic!("expected find_elements result");
        };
        assert_eq!(metadata.backend_kind, "atspi");
        assert_eq!(metadata.elements.len(), 1);
        assert_eq!(metadata.elements[0].label.as_deref(), Some("Submit"));
        assert_eq!(metadata.elements[0].states, vec!["enabled".to_owned()]);
    }

    #[test]
    fn dispatch_list_plugins_uses_configured_plugin_paths() {
        let root = std::env::temp_dir().join(format!(
            "peekaboox-plugin-daemon-test-{}",
            std::process::id()
        ));
        let plugin_dir = root.join("demo");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join(peekaboox_plugins::PLUGIN_MANIFEST_FILE),
            serde_json::json!({
                "schema_version": peekaboox_plugins::PLUGIN_SDK_VERSION,
                "id": "daemon.demo",
                "name": "Daemon Demo",
                "version": "1.0.0",
                "tools": [{"name": "daemon.inspect", "description": "Inspect daemon state"}]
            })
            .to_string(),
        )
        .unwrap();
        let config = ServerConfig {
            socket: PathBuf::from("/tmp/peekaboox-test.sock"),
            once: true,
            audit_log: PathBuf::from("/tmp/peekaboox-audit.jsonl"),
            policy_profile: DaemonPolicyProfile::Observe,
            sandbox_profile: SandboxProfile::Off,
            allow_input: false,
            vision_fallback: false,
            grpc_addr: None,
            accessibility_cache_ttl: default_accessibility_cache_ttl(),
            accessibility_events: true,
            emergency_hotkey: true,
            plugin_paths: vec![root.clone()],
        };

        let result = dispatch_request(
            ApiRequest::ListPlugins { paths: Vec::new() },
            &config,
            &test_accessibility_cache(),
            &test_incremental_capture_state(),
        )
        .unwrap();

        let ApiResult::Plugins(plugins) = result else {
            panic!("expected plugins result");
        };
        assert_eq!(plugins.errors, Vec::new());
        assert_eq!(plugins.plugins[0].id, "daemon.demo");
        assert_eq!(plugins.plugins[0].tools[0].name, "daemon.inspect");
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn element_lookup_does_not_call_vision_fallback_when_disabled() {
        let mut fallback_called = false;
        let result = element_lookup_with_optional_vision_fallback(
            "role=visual-region",
            false,
            &ElementLookupOptions::default(),
            Ok(CachedAccessibilityTree {
                metadata: sample_accessibility_metadata("Submit"),
                cache_hit: true,
                age_ms: 12,
            }),
            |_, _| {
                fallback_called = true;
                Err("fallback should not be called".to_owned())
            },
        )
        .unwrap();

        assert!(!fallback_called);
        assert_eq!(result.backend_kind, "atspi");
        assert!(result.elements.is_empty());
        assert!(result.cache_hit);
        assert_eq!(result.cache_age_ms, 12);
        assert!(!result.vision_fallback_used);
    }

    #[test]
    fn element_lookup_uses_fixture_vision_fallback_after_accessibility_miss() {
        let result = element_lookup_with_optional_vision_fallback(
            "role=visual-region,contains=24,7",
            true,
            &ElementLookupOptions::default(),
            Ok(CachedAccessibilityTree {
                metadata: sample_accessibility_metadata("Submit"),
                cache_hit: true,
                age_ms: 12,
            }),
            fixture_vision_fallback,
        )
        .unwrap();

        assert_eq!(result.backend_name, VISION_UI_BACKEND_NAME);
        assert_eq!(result.backend_kind, VISION_UI_BACKEND_KIND);
        assert_eq!(result.elements.len(), 1);
        assert_eq!(result.elements[0].bounds, Rect::new(21, 4, 8, 8));
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("used vision fallback"));
        assert!(!result.cache_hit);
        assert_eq!(result.cache_age_ms, 0);
        assert!(result.vision_fallback_used);
    }

    #[tokio::test]
    async fn grpc_list_windows_responds() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let audit_path = std::env::temp_dir().join(format!(
            "peekaboox-test-audit-{}-{}.jsonl",
            std::process::id(),
            super::unix_time_ms()
        ));
        let service = GrpcPeekabooXService {
            config: ServerConfig {
                socket: PathBuf::from("/tmp/peekaboox-test.sock"),
                once: true,
                audit_log: audit_path.clone(),
                policy_profile: DaemonPolicyProfile::Observe,
                sandbox_profile: SandboxProfile::Off,
                allow_input: false,
                vision_fallback: false,
                grpc_addr: None,
                accessibility_cache_ttl: default_accessibility_cache_ttl(),
                accessibility_events: true,
                emergency_hotkey: true,
                plugin_paths: Vec::new(),
            },
            audit: Arc::new(Mutex::new(
                super::AuditLogger::new(audit_path.clone()).unwrap(),
            )),
            accessibility_cache: test_accessibility_cache(),
            incremental_capture_state: test_incremental_capture_state(),
            list_windows: test_list_windows,
        };
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        let server = tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(PeekabooXServer::new(service))
                .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async {
                    let _ = shutdown_rx.await;
                })
                .await
                .unwrap();
        });

        let mut client = PeekabooXClient::connect(format!("http://{addr}"))
            .await
            .unwrap();
        let response = client
            .list_windows(proto::ListWindowsRequest {
                focused: true,
                sort: Some("focused".to_owned()),
                diagnose: true,
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();

        assert!(
            response
                .windows
                .iter()
                .all(|window| window.bounds.is_some())
        );
        assert_eq!(response.backend_name, "test");
        assert_eq!(response.backend_kind, "x11");
        assert_eq!(response.backend_reports.len(), 1);
        assert!(response.backend_reports[0].selected);
        shutdown_tx.send(()).unwrap();
        server.await.unwrap();
        let _ = std::fs::remove_file(audit_path);
    }

    #[tokio::test]
    async fn grpc_click_is_permission_gated() {
        let audit_path = std::env::temp_dir().join(format!(
            "peekaboox-test-audit-{}-{}-click.jsonl",
            std::process::id(),
            super::unix_time_ms()
        ));
        let service = GrpcPeekabooXService {
            config: ServerConfig {
                socket: PathBuf::from("/tmp/peekaboox-test.sock"),
                once: true,
                audit_log: audit_path.clone(),
                policy_profile: DaemonPolicyProfile::Observe,
                sandbox_profile: SandboxProfile::Off,
                allow_input: false,
                vision_fallback: false,
                grpc_addr: None,
                accessibility_cache_ttl: default_accessibility_cache_ttl(),
                accessibility_events: true,
                emergency_hotkey: true,
                plugin_paths: Vec::new(),
            },
            audit: Arc::new(Mutex::new(
                super::AuditLogger::new(audit_path.clone()).unwrap(),
            )),
            accessibility_cache: test_accessibility_cache(),
            incremental_capture_state: test_incremental_capture_state(),
            list_windows: test_list_windows,
        };

        let error = service
            .click(tonic::Request::new(proto::ClickRequest {
                coordinates: Some(proto::Point { x: 1, y: 2 }),
                semantic_selector: None,
                window_selector: None,
                vision_fallback: false,
            }))
            .await
            .unwrap_err();

        assert_eq!(error.code(), tonic::Code::PermissionDenied);
        let audit_log = std::fs::read_to_string(&audit_path).unwrap();
        assert!(audit_log.contains(API_VERSION));
        let _ = std::fs::remove_file(audit_path);
    }

    #[tokio::test]
    async fn grpc_semantic_click_is_permission_gated() {
        let audit_path = std::env::temp_dir().join(format!(
            "peekaboox-test-audit-{}-{}-semantic-click.jsonl",
            std::process::id(),
            super::unix_time_ms()
        ));
        let service = GrpcPeekabooXService {
            config: ServerConfig {
                socket: PathBuf::from("/tmp/peekaboox-test.sock"),
                once: true,
                audit_log: audit_path.clone(),
                policy_profile: DaemonPolicyProfile::Observe,
                sandbox_profile: SandboxProfile::Off,
                allow_input: false,
                vision_fallback: false,
                grpc_addr: None,
                accessibility_cache_ttl: default_accessibility_cache_ttl(),
                accessibility_events: true,
                emergency_hotkey: true,
                plugin_paths: Vec::new(),
            },
            audit: Arc::new(Mutex::new(
                super::AuditLogger::new(audit_path.clone()).unwrap(),
            )),
            accessibility_cache: test_accessibility_cache(),
            incremental_capture_state: test_incremental_capture_state(),
            list_windows: test_list_windows,
        };

        let error = service
            .click(tonic::Request::new(proto::ClickRequest {
                coordinates: None,
                semantic_selector: Some("role=push button,label=Submit".to_owned()),
                window_selector: None,
                vision_fallback: false,
            }))
            .await
            .unwrap_err();

        assert_eq!(error.code(), tonic::Code::PermissionDenied);
        let audit_log = std::fs::read_to_string(&audit_path).unwrap();
        assert!(audit_log.contains("grpc.click"));
        let _ = std::fs::remove_file(audit_path);
    }

    fn test_accessibility_cache() -> super::SharedAccessibilityCache {
        Arc::new(Mutex::new(AccessibilityCache::new(
            default_accessibility_cache_ttl(),
        )))
    }

    fn test_incremental_capture_state() -> super::SharedIncrementalCaptureState {
        Arc::new(Mutex::new(IncrementalCaptureState::default()))
    }

    fn test_list_windows(
        query: peekaboox_windows::WindowQuery,
    ) -> peekaboox_core::Result<peekaboox_windows::WindowListMetadata> {
        assert!(query.focused_only || query == peekaboox_windows::WindowQuery::default());
        Ok(peekaboox_windows::WindowListMetadata {
            backend_name: "test".to_owned(),
            backend_kind: BackendKind::X11,
            windows: vec![WindowInfo {
                id: "window-1".to_owned(),
                title: "PeekabooX Test".to_owned(),
                app_id: Some("peekaboox-test".to_owned()),
                bounds: Rect::new(10, 20, 800, 600),
                focused: true,
                state: WindowState::Normal,
            }],
            warnings: Vec::new(),
            backend_reports: vec![peekaboox_windows::WindowBackendReport {
                backend_name: "test".to_owned(),
                backend_kind: BackendKind::X11,
                raw_window_count: 1,
                matched_window_count: 1,
                selected: true,
                error: None,
            }],
        })
    }

    fn sample_accessibility_metadata(label: &str) -> AccessibilityTreeMetadata {
        AccessibilityTreeMetadata {
            backend_name: "test".to_owned(),
            backend_kind: BackendKind::AtSpi,
            warnings: Vec::new(),
            elements: vec![UiElement {
                id: "element-1".to_owned(),
                role: "push button".to_owned(),
                label: Some(label.to_owned()),
                bounds: Rect::new(10, 20, 100, 40),
                center: Rect::new(10, 20, 100, 40).center(),
                confidence: 1.0,
                states: vec!["enabled".to_owned()],
                window_id: Some("window-1".to_owned()),
                window_title: Some("PeekabooX Test".to_owned()),
                app_id: Some("peekaboox-test".to_owned()),
                parent_id: None,
                child_ids: Vec::new(),
            }],
        }
    }

    fn fixture_vision_fallback(
        query: &peekaboox_accessibility::ElementQuery,
        _options: &ElementLookupOptions,
    ) -> Result<ElementLookupResult, String> {
        let mut elements = peekaboox_vision::detect_ui_elements_from_image_file(
            vision_fixture_path("ui_controls.pbm"),
            &peekaboox_vision::UiElementDetectionOptions::default(),
        )
        .map_err(|error| error.to_string())?;
        elements.retain(|element| query.matches(element));

        Ok(ElementLookupResult {
            backend_name: VISION_UI_BACKEND_NAME.to_owned(),
            backend_kind: VISION_UI_BACKEND_KIND.to_owned(),
            warnings: Vec::new(),
            elements,
            cache_hit: false,
            cache_age_ms: 0,
            vision_fallback_used: true,
        })
    }

    fn vision_fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/vision")
            .join(name)
    }

    fn sample_ocr_result() -> peekaboox_vision::OcrResult {
        peekaboox_vision::OcrResult {
            backend_name: "tesseract".to_owned(),
            text: "Submit".to_owned(),
            blocks: vec![peekaboox_vision::OcrText {
                text: "Submit".to_owned(),
                element: UiElement {
                    id: "ocr:10:20:100:40".to_owned(),
                    role: "text".to_owned(),
                    label: Some("Submit".to_owned()),
                    bounds: Rect::new(10, 20, 100, 40),
                    center: Rect::new(10, 20, 100, 40).center(),
                    confidence: 0.95,
                    states: Vec::new(),
                    window_id: None,
                    window_title: None,
                    app_id: None,
                    parent_id: None,
                    child_ids: Vec::new(),
                },
            }],
            words: vec![peekaboox_vision::OcrText {
                text: "Submit".to_owned(),
                element: UiElement {
                    id: "ocr-word:10:20:100:40".to_owned(),
                    role: "word".to_owned(),
                    label: Some("Submit".to_owned()),
                    bounds: Rect::new(10, 20, 100, 40),
                    center: Rect::new(10, 20, 100, 40).center(),
                    confidence: 0.95,
                    states: Vec::new(),
                    window_id: None,
                    window_title: None,
                    app_id: None,
                    parent_id: None,
                    child_ids: Vec::new(),
                },
            }],
            warnings: Vec::new(),
        }
    }

    fn sample_visual_diff_result() -> peekaboox_vision::VisualDiffResult {
        peekaboox_vision::VisualDiffResult {
            compared_region: Rect::new(0, 0, 4, 3),
            compared_pixels: 12,
            changed_pixels: 2,
            changed_ratio: 2.0 / 12.0,
            mean_absolute_error: 12.5,
            max_channel_delta: 255,
            changed_bounds: Some(Rect::new(1, 1, 2, 1)),
            matches: false,
        }
    }

    fn sample_capture_delta_data() -> CaptureDeltaData {
        CaptureDeltaData {
            stream_id: "agent-loop".to_owned(),
            delta: peekaboox_vision::IncrementalCaptureDelta {
                sequence: 3,
                frame_width: 4,
                frame_height: 3,
                format: PixelFormat::Rgba8,
                full_frame: false,
                changed_bounds: Some(Rect::new(1, 1, 2, 1)),
                changed_pixels: 2,
                changed_ratio: 2.0 / 12.0,
                patch_stride: 8,
                patch_data: b"abc".to_vec(),
            },
            low_bandwidth: true,
            capture_region: Some(Rect::new(10, 20, 300, 120)),
            backend_name: "fake".to_owned(),
            backend_kind: BackendKind::Portal,
            captured_at_unix_ms: 123,
        }
    }

    fn sample_ui_state_result() -> peekaboox_vision::UiStateResult {
        peekaboox_vision::UiStateResult {
            state: peekaboox_vision::UiStateKind::Loading,
            compared_transitions: 2,
            stable_transitions: 1,
            loading_transitions: 1,
            trailing_stable_transitions: 0,
            latest_diff: sample_visual_diff_result(),
            max_changed_ratio: 2.0 / 12.0,
            mean_changed_ratio: 1.0 / 12.0,
            changed_bounds: Some(Rect::new(1, 1, 2, 1)),
        }
    }
}
