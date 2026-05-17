use super::*;

pub(super) fn spawn_grpc_server(
    config: &ServerConfig,
    audit: SharedAudit,
    accessibility_cache: SharedAccessibilityCache,
    incremental_capture_state: SharedIncrementalCaptureState,
    shutdown: Arc<AtomicBool>,
) -> Result<Option<std::thread::JoinHandle<()>>, String> {
    let Some(addr) = config.grpc_addr else {
        return Ok(None);
    };
    if !addr.ip().is_loopback() && config.grpc_token.is_none() {
        return Err(
            "refusing to expose unauthenticated gRPC on a non-loopback address; set --grpc-token or PEEKABOOX_GRPC_TOKEN"
                .to_owned(),
        );
    }

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
    let grpc_token = config.grpc_token.clone();

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
            let shutdown_future = async move {
                while !shutdown.load(Ordering::Relaxed) {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            };
            let mut builder = tonic::transport::Server::builder();
            if let Some(expected_token) = grpc_token {
                builder
                    .add_service(PeekabooXServer::with_interceptor(
                        service,
                        move |request: Request<()>| {
                            if grpc_request_has_token(&request, &expected_token) {
                                Ok(request)
                            } else {
                                Err(Status::unauthenticated(
                                    "missing or invalid PeekabooX gRPC token",
                                ))
                            }
                        },
                    ))
                    .serve_with_incoming_shutdown(incoming, shutdown_future)
                    .await
                    .map_err(|error| format!("gRPC server stopped with error: {error}"))
            } else {
                builder
                    .add_service(PeekabooXServer::new(service))
                    .serve_with_incoming_shutdown(incoming, shutdown_future)
                    .await
                    .map_err(|error| format!("gRPC server stopped with error: {error}"))
            }
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

pub(super) fn grpc_request_has_token(request: &Request<()>, expected_token: &str) -> bool {
    let metadata = request.metadata();
    metadata
        .get("x-peekaboox-token")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == expected_token)
        || metadata
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .is_some_and(|value| value == expected_token)
}

pub(super) fn handle_stream(
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

pub(super) fn handle_request(
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

pub(super) fn dispatch_request(
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
            app,
            window_title,
            title_regex,
            format,
            no_overwrite,
            include_semantic_tree,
        } => Ok(ApiResult::Capture(capture_to_file_response(
            &output,
            region.map(Rect::from),
            window_id.as_deref(),
            app.as_deref(),
            window_title.as_deref(),
            title_regex.as_deref(),
            format.as_deref(),
            no_overwrite,
            include_semantic_tree,
            accessibility_cache,
        )?)),
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
            ensure_plugin_execution_allowed(config)?;
            if !paths.is_empty() {
                return Err(
                    "permission denied: daemon plugin execution only uses plugin paths configured at daemon startup"
                        .to_owned(),
                );
            }
            let paths = config.plugin_paths.clone();
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
            bounds_policy,
            backend,
            restore,
        } => {
            let options =
                click_options_from_fields(Some(bounds_policy.as_str()), Some(backend.as_str()))
                    .map_err(|error| error.to_string())?;
            let position =
                peekaboox_input::resolve_move_position(Point::new(x, y), options.bounds_policy)
                    .map_err(|error| error.to_string())?;
            let action = peekaboox_input::InputAction::Click {
                position,
                button: mouse_button(button),
            };
            let metadata = if dry_run {
                let backend = peekaboox_input::CommandInputBackend
                    .detect_backend_for_with_selection(&action, options.backend)
                    .map_err(|error| error.to_string())?;
                detected_input_backend_dto(backend)
            } else {
                ensure_input_allowed(config)?;
                let restore_position = if restore {
                    Some(peekaboox_input::current_mouse_position().map_err(|error| {
                        format!("failed to query cursor before click restore: {error}")
                    })?)
                } else {
                    None
                };
                let metadata =
                    peekaboox_input::click_with_options(position, mouse_button(button), options)
                        .map_err(|error| error.to_string())?;
                if let Some(position) = restore_position {
                    peekaboox_input::move_mouse_with_options(
                        position,
                        peekaboox_input::MoveMouseOptions {
                            duration_ms: 0,
                            steps: None,
                            bounds_policy: options.bounds_policy,
                            backend: options.backend,
                        },
                    )
                    .map_err(|error| error.to_string())?;
                }
                input_metadata_dto(metadata)
            };
            Ok(ApiResult::Click(metadata))
        }
        ApiRequest::MoveMouse {
            x,
            y,
            dry_run,
            duration_ms,
            steps,
            bounds_policy,
            backend,
            restore,
        } => {
            let options = move_options_from_fields(
                Some(duration_ms),
                steps,
                Some(bounds_policy.as_str()),
                Some(backend.as_str()),
            )
            .map_err(|error| error.to_string())?;
            let position =
                peekaboox_input::resolve_move_position(Point::new(x, y), options.bounds_policy)
                    .map_err(|error| error.to_string())?;
            let action = peekaboox_input::InputAction::MoveMouse(position);
            let metadata = if dry_run {
                let backend = peekaboox_input::CommandInputBackend
                    .detect_backend_for_with_selection(&action, options.backend)
                    .map_err(|error| error.to_string())?;
                detected_input_backend_dto(backend)
            } else {
                ensure_input_allowed(config)?;
                let restore_position = if restore {
                    Some(
                        peekaboox_input::current_mouse_position()
                            .map_err(|error| error.to_string())?,
                    )
                } else {
                    None
                };
                let metadata = peekaboox_input::move_mouse_with_options(Point::new(x, y), options)
                    .map_err(|error| error.to_string())?;
                if let Some(position) = restore_position {
                    peekaboox_input::move_mouse_with_options(position, options)
                        .map_err(|error| error.to_string())?;
                }
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
            steps,
            bounds_policy,
            backend,
            restore,
            dry_run,
        } => {
            let button = mouse_button(button);
            let options = drag_options_from_fields(
                Some(u64::from(duration_ms)),
                steps,
                Some(bounds_policy.as_str()),
                Some(backend.as_str()),
            )?;
            let from = peekaboox_input::resolve_move_position(
                Point::new(from_x, from_y),
                options.bounds_policy,
            )
            .map_err(|error| error.to_string())?;
            let to = peekaboox_input::resolve_move_position(
                Point::new(to_x, to_y),
                options.bounds_policy,
            )
            .map_err(|error| error.to_string())?;
            let action = peekaboox_input::InputAction::Drag {
                from,
                to,
                button,
                duration_ms: options.duration_ms,
            };
            let metadata = if dry_run {
                let backend = peekaboox_input::CommandInputBackend
                    .detect_backend_for_with_selection(&action, options.backend)
                    .map_err(|error| error.to_string())?;
                detected_input_backend_dto(backend)
            } else {
                ensure_input_allowed(config)?;
                let restore_position = if restore {
                    Some(peekaboox_input::current_mouse_position().map_err(|error| {
                        format!("failed to query cursor before drag restore: {error}")
                    })?)
                } else {
                    None
                };
                let metadata = peekaboox_input::drag_with_options(from, to, button, options)
                    .map_err(|error| error.to_string())?;
                if let Some(position) = restore_position {
                    peekaboox_input::move_mouse_with_options(
                        position,
                        peekaboox_input::MoveMouseOptions {
                            duration_ms: options.duration_ms,
                            steps: options.steps,
                            bounds_policy: options.bounds_policy,
                            backend: options.backend,
                        },
                    )
                    .map_err(|error| error.to_string())?;
                }
                input_metadata_dto(metadata)
            };
            Ok(ApiResult::Drag(metadata))
        }
        ApiRequest::TypeText {
            text,
            dry_run,
            typing_speed_chars_per_second,
            delay_ms,
            key_delay_ms,
            backend,
        } => {
            let action = peekaboox_input::InputAction::TypeText(text.clone());
            let options = type_options_from_fields(
                typing_speed_chars_per_second,
                delay_ms,
                key_delay_ms,
                Some(backend.as_str()),
            )?;
            let metadata = if dry_run {
                let backend = peekaboox_input::CommandInputBackend
                    .detect_backend_for_with_selection(&action, options.backend)
                    .map_err(|error| error.to_string())?;
                detected_input_backend_dto(backend)
            } else {
                ensure_input_allowed(config)?;
                let metadata = peekaboox_input::type_text_with_options(text, options)
                    .map_err(|error| error.to_string())?;
                input_metadata_dto(metadata)
            };
            Ok(ApiResult::TypeText(metadata))
        }
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
            let options = paste_options_from_fields(
                preserve_clipboard,
                Some(clipboard_backend.as_str()),
                Some(hotkey_backend.as_str()),
                delay_ms,
                restore_delay_ms,
                Some(restore_policy.as_str()),
            )?;
            let metadata = if dry_run {
                let backend = peekaboox_input::CommandInputBackend
                    .detect_paste_backend_for_options(options)
                    .map_err(|error| error.to_string())?;
                detected_paste_backend_dto(backend)
            } else {
                ensure_input_allowed(config)?;
                let metadata = peekaboox_input::paste_text_with_options(text, options)
                    .map_err(|error| error.to_string())?;
                input_metadata_dto(metadata)
            };
            Ok(ApiResult::PasteText(metadata))
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
            validate_hotkey_keys(&keys).map_err(|status| status.message().to_owned())?;
            let keys =
                peekaboox_input::normalize_hotkey_keys(&keys).map_err(|error| error.to_string())?;
            let options = hotkey_options_from_fields(
                Some(backend.as_str()),
                delay_ms,
                key_delay_ms,
                repeat,
                interval_ms,
                release_before,
                release_after,
            )?;
            let action = peekaboox_input::InputAction::Hotkey(keys.clone());
            let metadata = if dry_run {
                let backend = peekaboox_input::CommandInputBackend
                    .detect_backend_for_with_selection(&action, options.backend)
                    .map_err(|error| error.to_string())?;
                detected_input_backend_dto(backend)
            } else {
                ensure_input_allowed(config)?;
                let metadata = peekaboox_input::hotkey_with_options(keys, options)
                    .map_err(|error| error.to_string())?;
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
            let options = visual_compare_options(VisualCompareRequestOptions {
                region: region.map(Rect::from),
                ignore_regions: ignore_regions.into_iter().map(Rect::from).collect(),
                per_channel_threshold: u32::from(per_channel_threshold),
                max_changed_ratio: Some(max_changed_ratio),
                max_changed_pixels,
                max_mean_absolute_error,
                max_channel_delta: max_channel_delta.map(u32::from),
                size_policy: Some(size_policy.as_str()),
                alpha_mode: Some(alpha_mode.as_str()),
            })
            .map_err(|status| status.message().to_owned())?;
            let result = if let Some(diff_output) = diff_output {
                peekaboox_vision::write_visual_diff_image_file(
                    &expected_path,
                    &actual_path,
                    diff_output,
                    &options,
                )
            } else {
                peekaboox_vision::compare_image_files(&expected_path, &actual_path, &options)
            }
            .map_err(|error| error.to_string())?;
            Ok(ApiResult::VisualDiff(visual_diff_dto(&result)))
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
            let options = ui_state_options(UiStateRequestOptions {
                region: region.map(Rect::from),
                ignore_regions: ignore_regions.into_iter().map(Rect::from).collect(),
                per_channel_threshold: Some(u32::from(per_channel_threshold)),
                stable_max_changed_ratio: Some(stable_max_changed_ratio),
                stable_max_changed_pixels,
                stable_max_mean_absolute_error,
                stable_max_channel_delta: stable_max_channel_delta.map(u32::from),
                loading_min_changed_ratio: Some(loading_min_changed_ratio),
                loading_min_changed_pixels,
                required_stable_transitions: Some(required_stable_transitions),
                size_policy: Some(size_policy.as_str()),
                alpha_mode: Some(alpha_mode.as_str()),
            })
            .map_err(|status| status.message().to_owned())?;
            let paths = image_paths.iter().map(PathBuf::from).collect::<Vec<_>>();
            let result = peekaboox_vision::detect_ui_state_from_image_files(&paths, &options)
                .map_err(|error| error.to_string())?;
            Ok(ApiResult::UiState(ui_state_dto(&result)))
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
            let options = ui_element_detection_options(UiElementDetectionRequestOptions {
                region: region.map(Rect::from),
                ignore_regions: ignore_regions.into_iter().map(Rect::from).collect(),
                edge_threshold: Some(u32::from(edge_threshold)),
                min_width: Some(min_width),
                min_height: Some(min_height),
                min_component_pixels: Some(min_component_pixels),
                min_confidence,
                max_width,
                max_height,
                min_area,
                max_area,
                max_elements: Some(max_elements),
                merge_distance: Some(merge_distance),
                padding: Some(padding),
                sort: Some(sort.as_str()),
            })
            .map_err(|status| status.message().to_owned())?;
            let elements = peekaboox_vision::detect_ui_elements_from_image_file_with_outputs(
                &image_path,
                &options,
                mask_output_path.as_deref().map(Path::new),
                overlay_output_path.as_deref().map(Path::new),
            )
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
        ApiRequest::DesktopProfiles {
            app,
            target,
            command,
            desktop_id,
            supports,
            check,
            installed,
            available,
        } => {
            let result = peekaboox_desktop::desktop_profiles_with_query(&DesktopProfileQuery {
                app,
                target,
                command,
                desktop_id,
                supports,
                check_availability: check,
                installed_only: installed,
                available_only: available,
            })
            .map_err(|error| error.to_string())?;
            Ok(ApiResult::DesktopProfiles(desktop_profiles_dto(result)))
        }
    }
}
