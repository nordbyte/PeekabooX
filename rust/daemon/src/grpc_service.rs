use super::*;

#[derive(Clone)]
pub(super) struct GrpcPeekabooXService {
    pub(super) config: ServerConfig,
    pub(super) audit: SharedAudit,
    pub(super) accessibility_cache: SharedAccessibilityCache,
    pub(super) incremental_capture_state: SharedIncrementalCaptureState,
    pub(super) list_windows: WindowListProvider,
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
            "has_region": request.region.is_some(),
            "has_ratio": request.ratio_x.is_some() || request.ratio_y.is_some(),
            "has_window_filter": request.window_id.is_some()
                || request.app.is_some()
                || request.window_title.is_some()
                || request.title_regex.is_some(),
            "vision_fallback": effective_vision_fallback,
            "request_vision_fallback": request.vision_fallback,
            "daemon_vision_fallback": self.config.vision_fallback,
            "button": request.button,
            "dry_run": request.dry_run,
            "bounds_policy": request.bounds_policy.as_deref(),
            "backend": request.backend.as_deref(),
            "restore": request.restore
        });

        let result = grpc_click(
            request,
            &self.config,
            &self.accessibility_cache,
            self.list_windows,
        );
        audit_grpc_result(&self.audit, "grpc.click", &result, details);
        result.map(Response::new)
    }

    async fn move_mouse(
        &self,
        request: Request<proto::MoveMouseRequest>,
    ) -> Result<Response<proto::ActionResponse>, Status> {
        let request = request.into_inner();
        let details = json!({
            "has_coordinates": request.coordinates.is_some(),
            "has_relative": request.relative.is_some(),
            "has_region": request.region.is_some(),
            "has_ratio": request.ratio_x.is_some() || request.ratio_y.is_some(),
            "has_window_filter": request.window_id.is_some()
                || request.app.is_some()
                || request.window_title.is_some()
                || request.title_regex.is_some(),
            "dry_run": request.dry_run,
            "duration_ms": request.duration_ms,
            "steps": request.steps,
            "bounds_policy": request.bounds_policy.as_deref(),
            "backend": request.backend.as_deref(),
            "restore": request.restore
        });

        let result = grpc_move_mouse(request, &self.config, self.list_windows);
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
            "from_current": request.from_current,
            "has_region": request.region.is_some(),
            "has_from_ratio": request.from_ratio_x.is_some() || request.from_ratio_y.is_some(),
            "has_to_ratio": request.to_ratio_x.is_some() || request.to_ratio_y.is_some(),
            "has_window_filter": request.window_id.is_some()
                || request.app.is_some()
                || request.window_title.is_some()
                || request.title_regex.is_some(),
            "button": request.button,
            "dry_run": request.dry_run,
            "duration_ms": request.duration_ms,
            "steps": request.steps,
            "bounds_policy": request.bounds_policy.as_deref(),
            "backend": request.backend.as_deref(),
            "restore": request.restore
        });

        let result = grpc_drag(request, &self.config, self.list_windows);
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
            "typing_speed_chars_per_second": request.typing_speed_chars_per_second,
            "dry_run": request.dry_run,
            "backend": request.backend.as_deref(),
            "delay_ms": request.delay_ms,
            "key_delay_ms": request.key_delay_ms
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
            "preserve_clipboard": request.preserve_clipboard,
            "dry_run": request.dry_run,
            "clipboard_backend": request.clipboard_backend.as_deref(),
            "hotkey_backend": request.hotkey_backend.as_deref(),
            "delay_ms": request.delay_ms,
            "restore_delay_ms": request.restore_delay_ms,
            "restore_policy": request.restore_policy.as_deref()
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
            "key_count": request.keys.len(),
            "dry_run": request.dry_run,
            "backend": request.backend.as_deref(),
            "delay_ms": request.delay_ms,
            "key_delay_ms": request.key_delay_ms,
            "repeat": request.repeat,
            "interval_ms": request.interval_ms,
            "release_before": request.release_before,
            "release_after": request.release_after
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
            "ignore_region_count": request.ignore_regions.len(),
            "edge_threshold": request.edge_threshold,
            "min_width": request.min_width,
            "min_height": request.min_height,
            "min_component_pixels": request.min_component_pixels,
            "min_confidence": request.min_confidence,
            "max_width": request.max_width,
            "max_height": request.max_height,
            "min_area": request.min_area,
            "max_area": request.max_area,
            "max_elements": request.max_elements,
            "merge_distance": request.merge_distance,
            "padding": request.padding,
            "sort": request.sort,
            "has_mask_output": request.mask_output_path.is_some(),
            "has_overlay_output": request.overlay_output_path.is_some()
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

    async fn desktop_profiles(
        &self,
        request: Request<proto::DesktopProfilesRequest>,
    ) -> Result<Response<proto::DesktopProfilesResponse>, Status> {
        let request = request.into_inner();
        let details = json!({
            "app": request.app.as_deref(),
            "target": request.target.as_deref(),
            "command": request.command.as_deref(),
            "desktop_id": request.desktop_id.as_deref(),
            "supports": request.supports.as_deref(),
            "check": request.check,
            "installed": request.installed,
            "available": request.available
        });
        let result = grpc_desktop_profiles(request);
        audit_grpc_result(&self.audit, "grpc.desktop_profiles", &result, details);
        result.map(Response::new)
    }
}
