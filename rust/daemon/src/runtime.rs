use super::*;

pub(super) fn print_usage() {
    println!(
        "Usage: peekabooxd run [--profile <observe|assist|operator>] [--sandbox <off|basic|strict>] [--socket <path>] [--audit-log <path>] [--grpc-addr <addr>] [--no-grpc] [--accessibility-cache-ttl-ms <ms>] [--allow-input] [--allow-plugins] [--vision-fallback] [--no-accessibility-events] [--no-emergency-hotkey] [--once]"
    );
}

pub(super) fn default_grpc_addr() -> SocketAddr {
    DEFAULT_GRPC_ADDR
        .parse()
        .expect("default gRPC address must be valid")
}

pub(super) fn default_accessibility_cache_ttl() -> Duration {
    Duration::from_millis(DEFAULT_ACCESSIBILITY_CACHE_TTL_MS)
}

pub(super) fn install_shutdown_handler() -> Result<Arc<AtomicBool>, String> {
    let shutdown = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(SIGINT, Arc::clone(&shutdown))
        .map_err(|error| format!("failed to register SIGINT handler: {error}"))?;
    signal_hook::flag::register(SIGTERM, Arc::clone(&shutdown))
        .map_err(|error| format!("failed to register SIGTERM handler: {error}"))?;
    Ok(shutdown)
}

pub(super) fn input_allowed_from_env() -> bool {
    std::env::var("PEEKABOOX_ALLOW_INPUT")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

pub(super) fn vision_fallback_from_env() -> bool {
    std::env::var("PEEKABOOX_VISION_FALLBACK")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

pub(super) fn plugin_execution_allowed_from_env() -> bool {
    std::env::var("PEEKABOOX_ALLOW_PLUGINS")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

pub(super) fn grpc_token_from_env() -> Option<String> {
    std::env::var("PEEKABOOX_GRPC_TOKEN")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

pub(super) fn emergency_hotkey_enabled_from_env() -> bool {
    std::env::var("PEEKABOOX_EMERGENCY_HOTKEY")
        .map(|value| !matches!(value.as_str(), "0" | "false" | "FALSE" | "no" | "NO"))
        .unwrap_or(true)
}

pub(super) fn daemon_policy_profile_from_env() -> Result<DaemonPolicyProfile, String> {
    std::env::var("PEEKABOOX_DAEMON_PROFILE")
        .map(|value| DaemonPolicyProfile::parse(&value))
        .unwrap_or(Ok(DaemonPolicyProfile::Observe))
}

pub(super) fn sandbox_profile_from_env() -> Result<SandboxProfile, String> {
    std::env::var("PEEKABOOX_DAEMON_SANDBOX")
        .map(|value| SandboxProfile::parse(&value))
        .unwrap_or(Ok(SandboxProfile::Off))
}

pub(super) fn default_audit_log_path() -> PathBuf {
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

pub(super) fn audit_write(
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

pub(super) struct AuditLogger {
    pub(super) path: PathBuf,
}

impl AuditLogger {
    pub(super) fn new(path: PathBuf) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create audit log directory: {error}"))?;
        }

        Ok(Self { path })
    }

    pub(super) fn write(
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

pub(super) fn unix_time_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

pub(super) fn unix_time_ms_u64() -> u64 {
    unix_time_ms().min(u128::from(u64::MAX)) as u64
}

pub(super) fn request_method(request: &ApiRequest) -> &'static str {
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
        ApiRequest::DesktopProfiles { .. } => "desktop_profiles",
    }
}

pub(super) fn audit_details(request: &ApiRequest) -> serde_json::Value {
    match request {
        ApiRequest::Ping => json!({}),
        ApiRequest::Capture {
            output,
            region,
            window_id,
            app,
            window_title,
            title_regex,
            format,
            no_overwrite,
            include_semantic_tree,
        } => json!({
            "output": output,
            "has_region": region.is_some(),
            "has_window_id": window_id.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "has_app": app.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "has_window_title": window_title.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "has_title_regex": title_regex.as_deref().is_some_and(|value| !value.trim().is_empty()),
            "format": format.as_deref(),
            "no_overwrite": no_overwrite,
            "include_semantic_tree": include_semantic_tree
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
            require_trusted,
            trust_policy,
        } => json!({
            "plugin_id": plugin_id,
            "tool": tool,
            "argument_keys": arguments.as_object().map(|object| object.len()).unwrap_or_default(),
            "path_count": paths.len(),
            "timeout_ms": timeout_ms,
            "max_output_bytes": max_output_bytes,
            "require_trusted": require_trusted,
            "has_trust_policy": trust_policy.is_some()
        }),
        ApiRequest::Click {
            x,
            y,
            button,
            dry_run,
            bounds_policy,
            backend,
            restore,
        } => json!({
            "x": x,
            "y": y,
            "button": format!("{button:?}").to_ascii_lowercase(),
            "dry_run": dry_run,
            "bounds_policy": bounds_policy,
            "backend": backend,
            "restore": restore
        }),
        ApiRequest::MoveMouse {
            x,
            y,
            dry_run,
            duration_ms,
            steps,
            bounds_policy,
            backend,
            restore,
        } => json!({
            "x": x,
            "y": y,
            "dry_run": dry_run,
            "duration_ms": duration_ms,
            "steps": steps,
            "bounds_policy": bounds_policy,
            "backend": backend,
            "restore": restore
        }),
        ApiRequest::Drag {
            from_x,
            from_y,
            to_x,
            to_y,
            button,
            duration_ms,
            steps,
            bounds_policy,
            backend,
            restore,
            dry_run,
        } => json!({
            "from_x": from_x,
            "from_y": from_y,
            "to_x": to_x,
            "to_y": to_y,
            "button": format!("{button:?}").to_ascii_lowercase(),
            "duration_ms": duration_ms,
            "steps": steps,
            "bounds_policy": bounds_policy,
            "backend": backend,
            "restore": restore,
            "dry_run": dry_run
        }),
        ApiRequest::TypeText {
            text,
            dry_run,
            typing_speed_chars_per_second,
            delay_ms,
            key_delay_ms,
            backend,
        } => json!({
            "text_length": text.chars().count(),
            "dry_run": dry_run,
            "typing_speed_chars_per_second": typing_speed_chars_per_second,
            "delay_ms": delay_ms,
            "key_delay_ms": key_delay_ms,
            "backend": backend
        }),
        ApiRequest::PasteText {
            text,
            preserve_clipboard,
            dry_run,
            clipboard_backend,
            hotkey_backend,
            delay_ms,
            restore_delay_ms,
            restore_policy,
        } => {
            json!({
                "text_length": text.chars().count(),
                "preserve_clipboard": preserve_clipboard,
                "dry_run": dry_run,
                "clipboard_backend": clipboard_backend,
                "hotkey_backend": hotkey_backend,
                "delay_ms": delay_ms,
                "restore_delay_ms": restore_delay_ms,
                "restore_policy": restore_policy
            })
        }
        ApiRequest::Hotkey {
            keys,
            dry_run,
            backend,
            delay_ms,
            key_delay_ms,
            repeat,
            interval_ms,
            release_before,
            release_after,
        } => {
            json!({
                "key_count": keys.len(),
                "dry_run": dry_run,
                "backend": backend,
                "delay_ms": delay_ms,
                "key_delay_ms": key_delay_ms,
                "repeat": repeat,
                "interval_ms": interval_ms,
                "release_before": release_before,
                "release_after": release_after
            })
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
            ignore_regions,
            per_channel_threshold,
            max_changed_ratio,
            max_changed_pixels,
            max_mean_absolute_error,
            max_channel_delta,
            size_policy,
            alpha_mode,
            diff_output,
        } => {
            json!({
                "expected_path": expected_path,
                "actual_path": actual_path,
                "has_region": region.is_some(),
                "ignore_region_count": ignore_regions.len(),
                "per_channel_threshold": per_channel_threshold,
                "max_changed_ratio": max_changed_ratio,
                "max_changed_pixels": max_changed_pixels,
                "max_mean_absolute_error": max_mean_absolute_error,
                "max_channel_delta": max_channel_delta,
                "size_policy": size_policy,
                "alpha_mode": alpha_mode,
                "has_diff_output": diff_output.is_some()
            })
        }
        ApiRequest::DetectUiState {
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
        } => {
            json!({
                "image_paths": image_paths,
                "image_count": image_paths.len(),
                "has_region": region.is_some(),
                "ignore_region_count": ignore_regions.len(),
                "per_channel_threshold": per_channel_threshold,
                "stable_max_changed_ratio": stable_max_changed_ratio,
                "stable_max_changed_pixels": stable_max_changed_pixels,
                "stable_max_mean_absolute_error": stable_max_mean_absolute_error,
                "stable_max_channel_delta": stable_max_channel_delta,
                "loading_min_changed_ratio": loading_min_changed_ratio,
                "loading_min_changed_pixels": loading_min_changed_pixels,
                "required_stable_transitions": required_stable_transitions,
                "size_policy": size_policy,
                "alpha_mode": alpha_mode
            })
        }
        ApiRequest::DetectUiElements {
            image_path,
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
            mask_output_path,
            overlay_output_path,
        } => {
            json!({
                "image_path": image_path,
                "has_region": region.is_some(),
                "ignore_region_count": ignore_regions.len(),
                "edge_threshold": edge_threshold,
                "min_width": min_width,
                "min_height": min_height,
                "min_component_pixels": min_component_pixels,
                "min_confidence": min_confidence,
                "max_width": max_width,
                "max_height": max_height,
                "min_area": min_area,
                "max_area": max_area,
                "max_elements": max_elements,
                "merge_distance": merge_distance,
                "padding": padding,
                "sort": sort,
                "has_mask_output": mask_output_path.is_some(),
                "has_overlay_output": overlay_output_path.is_some()
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
        ApiRequest::DesktopProfiles {
            app,
            target,
            command,
            desktop_id,
            supports,
            check,
            installed,
            available,
        } => json!({
            "app": app,
            "target": target,
            "command": command,
            "desktop_id": desktop_id,
            "supports": supports,
            "check": check,
            "installed": installed,
            "available": available
        }),
    }
}
