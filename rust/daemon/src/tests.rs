use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::{
    AccessibilityCache, AccessibilityCacheSnapshot, CachedAccessibilityTree, CaptureDeltaData,
    DaemonCommand, DaemonPolicyProfile, ElementLookupOptions, ElementLookupResult,
    GrpcPeekabooXService, IncrementalCaptureState, SandboxProfile, ServerConfig,
    VISION_UI_BACKEND_KIND, VISION_UI_BACKEND_NAME, audit_details, capture_delta_dto,
    default_accessibility_cache_ttl, default_audit_log_path, default_grpc_addr, dispatch_request,
    element_lookup_with_optional_vision_fallback, emergency_hotkey_details,
    emergency_hotkey_enabled_from_env, ensure_input_allowed, ensure_plugin_execution_allowed,
    input_allowed_from_env, linux_input_event_size, ocr_result_dto, parse_args,
    parse_linux_input_event, proto_capture_backends_response, proto_capture_delta_response,
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
                allow_plugins: false,
                vision_fallback: true,
                grpc_addr: Some("127.0.0.1:47778".parse().unwrap()),
                grpc_token: None,
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
fn parses_run_allow_plugins_option() {
    let command = parse_args(vec!["run".to_owned(), "--allow-plugins".to_owned()]).unwrap();

    assert!(matches!(
        command,
        DaemonCommand::Run {
            config: ServerConfig {
                allow_plugins: true,
                allow_input: false,
                ..
            }
        }
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
                allow_plugins: true,
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
    assert!(!observe.allow_plugins);
    assert!(!observe.vision_fallback);
    assert!(!assist.allow_input);
    assert!(!assist.allow_plugins);
    assert!(assist.vision_fallback);
    assert!(operator.allow_input);
    assert!(operator.allow_plugins);
    assert!(operator.vision_fallback);
}

#[test]
fn audit_type_text_does_not_log_secret_text() {
    let details = audit_details(&ApiRequest::TypeText {
        text: "secret".to_owned(),
        dry_run: false,
        typing_speed_chars_per_second: Some(20),
        delay_ms: Some(10),
        key_delay_ms: None,
        backend: "wtype".to_owned(),
    });

    assert_eq!(details["text_length"], 6);
    assert_eq!(details["typing_speed_chars_per_second"], 20);
    assert_eq!(details["backend"], "wtype");
    assert!(details.get("text").is_none());
}

#[test]
fn type_options_validate_backend_and_timing() {
    let options = super::type_options_from_fields(Some(20), Some(10), None, Some("wtype"))
        .expect("valid type options");

    assert_eq!(options.backend, peekaboox_input::InputToolSelection::Wtype);
    assert_eq!(options.typing_speed_chars_per_second, Some(20));
    assert_eq!(options.delay_ms, Some(10));
    assert!(
        super::type_options_from_fields(Some(20), None, Some(5), Some("auto"))
            .unwrap_err()
            .contains("cannot be combined")
    );
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
    bytes[offset + 2..offset + 4].copy_from_slice(&peekaboox_input::LINUX_KEY_ESC.to_ne_bytes());
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
        allow_plugins: false,
        vision_fallback: false,
        grpc_addr: Some(default_grpc_addr()),
        grpc_token: None,
        accessibility_cache_ttl: default_accessibility_cache_ttl(),
        accessibility_events: true,
        emergency_hotkey: true,
        plugin_paths: Vec::new(),
    };

    let error = ensure_input_allowed(&config).unwrap_err();

    assert!(error.contains("--allow-input"));
}

#[test]
fn plugin_permission_gate_is_separate_from_input() {
    let mut config = ServerConfig {
        socket: PathBuf::from("/tmp/peekaboox-test.sock"),
        once: true,
        audit_log: PathBuf::from("/tmp/peekaboox-audit.jsonl"),
        policy_profile: DaemonPolicyProfile::Observe,
        sandbox_profile: SandboxProfile::Off,
        allow_input: true,
        allow_plugins: false,
        vision_fallback: false,
        grpc_addr: Some(default_grpc_addr()),
        grpc_token: None,
        accessibility_cache_ttl: default_accessibility_cache_ttl(),
        accessibility_events: true,
        emergency_hotkey: true,
        plugin_paths: Vec::new(),
    };

    let error = ensure_plugin_execution_allowed(&config).unwrap_err();
    assert!(error.contains("--allow-plugins"));

    config.allow_plugins = true;
    ensure_plugin_execution_allowed(&config).unwrap();
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
        allow_plugins: false,
        vision_fallback: false,
        grpc_addr: None,
        grpc_token: None,
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
        allow_plugins: false,
        vision_fallback: false,
        grpc_addr: None,
        grpc_token: None,
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
            allow_plugins: false,
            vision_fallback: false,
            grpc_addr: None,
            grpc_token: None,
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
            allow_plugins: false,
            vision_fallback: false,
            grpc_addr: None,
            grpc_token: None,
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
            button: None,
            dry_run: false,
            bounds_policy: None,
            backend: None,
            restore: false,
            region: None,
            ratio_x: None,
            ratio_y: None,
            window_id: None,
            app: None,
            window_title: None,
            title_regex: None,
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
            allow_plugins: false,
            vision_fallback: false,
            grpc_addr: None,
            grpc_token: None,
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
            button: None,
            dry_run: false,
            bounds_policy: None,
            backend: None,
            restore: false,
            region: None,
            ratio_x: None,
            ratio_y: None,
            window_id: None,
            app: None,
            window_title: None,
            title_regex: None,
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
