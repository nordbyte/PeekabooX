use super::*;

pub(super) fn grpc_click(
    request: proto::ClickRequest,
    config: &ServerConfig,
    accessibility_cache: &SharedAccessibilityCache,
    list_windows: WindowListProvider,
) -> Result<proto::ActionResponse, Status> {
    if request.window_selector.is_some() {
        return Err(Status::unimplemented(
            "window selector clicks require the window focus phase",
        ));
    }

    let options =
        click_options_from_fields(request.bounds_policy.as_deref(), request.backend.as_deref())
            .map_err(Status::invalid_argument)?;
    let button = proto_mouse_button(request.button)?;
    if !request.dry_run {
        ensure_input_allowed(config).map_err(Status::permission_denied)?;
    }
    let resolved = resolve_grpc_click_target(&request, config, accessibility_cache, list_windows)?;
    let position = peekaboox_input::resolve_move_position(resolved.position, options.bounds_policy)
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
    let action = peekaboox_input::InputAction::Click { position, button };

    let metadata = if request.dry_run {
        let backend = peekaboox_input::CommandInputBackend
            .detect_backend_for_with_selection(&action, options.backend)
            .map_err(|error| Status::failed_precondition(error.to_string()))?;
        peekaboox_input::InputExecutionMetadata {
            backend_name: backend.name().to_owned(),
            backend_kind: backend.backend_kind(),
            action,
        }
    } else {
        let restore_position = if request.restore {
            Some(
                peekaboox_input::current_mouse_position()
                    .map_err(|error| Status::failed_precondition(error.to_string()))?,
            )
        } else {
            None
        };
        let metadata = peekaboox_input::click_with_options(position, button, options)
            .map_err(|error| Status::internal(error.to_string()))?;
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
            .map_err(|error| Status::internal(error.to_string()))?;
        }
        metadata
    };
    let backend_kind = backend_kind_name(metadata.backend_kind);
    let backend_name = metadata.backend_name;
    let action = if request.dry_run {
        "would click"
    } else {
        "clicked"
    };
    let restore_suffix = if request.restore && !request.dry_run {
        " and restored"
    } else {
        ""
    };

    Ok(proto::ActionResponse {
        ok: true,
        message: format!(
            "{action} {} ({},{}) using {}/{}{restore_suffix}",
            resolved.description, position.x, position.y, backend_name, backend_kind
        ),
        backend_name: Some(backend_name),
        backend_kind: Some(backend_kind),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedClickTarget {
    pub(super) position: Point,
    pub(super) description: String,
}

pub(super) fn resolve_grpc_click_target(
    request: &proto::ClickRequest,
    config: &ServerConfig,
    accessibility_cache: &SharedAccessibilityCache,
    list_windows: WindowListProvider,
) -> Result<ResolvedClickTarget, Status> {
    let has_scope = request.region.is_some()
        || request.ratio_x.is_some()
        || request.ratio_y.is_some()
        || request.window_id.is_some()
        || request.app.is_some()
        || request.window_title.is_some()
        || request.title_regex.is_some();
    let target_count = usize::from(request.coordinates.is_some())
        + usize::from(request.semantic_selector.is_some())
        + usize::from(has_scope);
    if target_count != 1 {
        return Err(Status::invalid_argument(
            "provide exactly one click target: coordinates, semantic_selector, or ratio/region/window scope",
        ));
    }

    if let Some(coordinates) = request.coordinates.as_ref() {
        return Ok(ResolvedClickTarget {
            position: Point::new(coordinates.x, coordinates.y),
            description: format!("{},{}", coordinates.x, coordinates.y),
        });
    }

    if let Some(selector) = request.semantic_selector.as_ref() {
        if selector.trim().is_empty() {
            return Err(Status::invalid_argument(
                "semantic_selector must not be empty",
            ));
        }
        let target = resolve_click_target_with_optional_vision_fallback(
            selector,
            request.vision_fallback || config.vision_fallback,
            accessibility_cache,
        )?;
        let label = target
            .element
            .label
            .as_deref()
            .unwrap_or(target.element.role.as_str())
            .to_owned();
        return Ok(ResolvedClickTarget {
            position: target.position,
            description: format!(
                "selector {selector:?} at {},{} ({label})",
                target.position.x, target.position.y
            ),
        });
    }

    let ratio = (
        request.ratio_x.unwrap_or(0.5),
        request.ratio_y.unwrap_or(0.5),
    );
    validate_ratio_status("ratio_x", ratio.0)?;
    validate_ratio_status("ratio_y", ratio.1)?;
    let scope = resolve_grpc_click_scope(request, list_windows)?;
    Ok(ResolvedClickTarget {
        position: point_from_ratio_status(scope, ratio)?,
        description: format!(
            "ratio {:.3},{:.3} in {}",
            ratio.0,
            ratio.1,
            format_rect(scope)
        ),
    })
}

pub(super) fn resolve_grpc_click_scope(
    request: &proto::ClickRequest,
    list_windows: WindowListProvider,
) -> Result<Rect, Status> {
    let region = request.region.map(rect_from_proto);
    let window = resolve_grpc_click_window(request, list_windows)?;
    match (window, region) {
        (Some(window), Some(region)) => {
            offset_window_relative_capture_region(window.bounds, region)
                .map_err(Status::invalid_argument)
        }
        (Some(window), None) => Ok(window.bounds),
        (None, Some(region)) => Ok(region),
        (None, None) => {
            let (width, height) = peekaboox_input::screen_size().ok_or_else(|| {
                Status::failed_precondition(
                    "click ratio without region/window requires a detectable screen size",
                )
            })?;
            Ok(Rect::new(
                0,
                0,
                u32::try_from(width).map_err(|_| Status::internal("screen width overflows u32"))?,
                u32::try_from(height)
                    .map_err(|_| Status::internal("screen height overflows u32"))?,
            ))
        }
    }
}

pub(super) fn resolve_grpc_click_window(
    request: &proto::ClickRequest,
    list_windows: WindowListProvider,
) -> Result<Option<WindowInfo>, Status> {
    let id = clean_optional_string(request.window_id.clone());
    let app = clean_optional_string(request.app.clone());
    let title = clean_optional_string(request.window_title.clone());
    let title_regex = clean_optional_string(request.title_regex.clone());
    if id.is_none() && app.is_none() && title.is_none() && title_regex.is_none() {
        return Ok(None);
    }

    let query = window_query_from_fields(WindowQueryFields {
        id,
        app,
        title,
        title_regex,
        focused: false,
        limit: Some(1),
        sort: Some("focused".to_owned()),
        backend: None,
        diagnose: false,
    })
    .map_err(Status::invalid_argument)?;
    let metadata = list_windows(query).map_err(|error| Status::internal(error.to_string()))?;
    let window = metadata
        .windows
        .into_iter()
        .next()
        .ok_or_else(|| Status::not_found("no window matched click filters"))?;
    if window.bounds.width == 0 || window.bounds.height == 0 {
        return Err(Status::failed_precondition(format!(
            "window {} has empty bounds",
            window.id
        )));
    }
    Ok(Some(window))
}

pub(super) fn grpc_move_mouse(
    request: proto::MoveMouseRequest,
    config: &ServerConfig,
    list_windows: WindowListProvider,
) -> Result<proto::ActionResponse, Status> {
    let options = move_options_from_fields(
        request.duration_ms,
        request.steps,
        request.bounds_policy.as_deref(),
        request.backend.as_deref(),
    )
    .map_err(Status::invalid_argument)?;
    let resolved = resolve_grpc_move_target(&request, list_windows)?;
    let position = peekaboox_input::resolve_move_position(resolved.position, options.bounds_policy)
        .map_err(|error| Status::invalid_argument(error.to_string()))?;

    let metadata = if request.dry_run {
        let action = peekaboox_input::InputAction::MoveMouse(position);
        let backend = peekaboox_input::CommandInputBackend
            .detect_backend_for_with_selection(&action, options.backend)
            .map_err(|error| Status::failed_precondition(error.to_string()))?;
        peekaboox_input::InputExecutionMetadata {
            backend_name: backend.name().to_owned(),
            backend_kind: backend.backend_kind(),
            action,
        }
    } else {
        ensure_input_allowed(config).map_err(Status::permission_denied)?;
        let restore_position = if request.restore {
            Some(
                peekaboox_input::current_mouse_position()
                    .map_err(|error| Status::failed_precondition(error.to_string()))?,
            )
        } else {
            None
        };
        let metadata = peekaboox_input::move_mouse_with_options(position, options)
            .map_err(|error| Status::internal(error.to_string()))?;
        if let Some(position) = restore_position {
            peekaboox_input::move_mouse_with_options(position, options)
                .map_err(|error| Status::internal(error.to_string()))?;
        }
        metadata
    };
    let backend_kind = backend_kind_name(metadata.backend_kind);
    let backend_name = metadata.backend_name;
    let action = if request.dry_run {
        "would move"
    } else {
        "moved"
    };
    let restore_suffix = if request.restore && !request.dry_run {
        " and restored"
    } else {
        ""
    };

    Ok(proto::ActionResponse {
        ok: true,
        message: format!(
            "{action} mouse to {} ({},{}) using {}/{}{restore_suffix}",
            resolved.description, position.x, position.y, backend_name, backend_kind
        ),
        backend_name: Some(backend_name),
        backend_kind: Some(backend_kind),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedMoveTarget {
    pub(super) position: Point,
    pub(super) description: String,
}

pub(super) fn resolve_grpc_move_target(
    request: &proto::MoveMouseRequest,
    list_windows: WindowListProvider,
) -> Result<ResolvedMoveTarget, Status> {
    let has_scope = request.region.is_some()
        || request.ratio_x.is_some()
        || request.ratio_y.is_some()
        || request.window_id.is_some()
        || request.app.is_some()
        || request.window_title.is_some()
        || request.title_regex.is_some();
    let target_count = usize::from(request.coordinates.is_some())
        + usize::from(request.relative.is_some())
        + usize::from(has_scope);
    if target_count != 1 {
        return Err(Status::invalid_argument(
            "provide exactly one move target: coordinates, relative, or ratio/region/window scope",
        ));
    }

    if let Some(coordinates) = request.coordinates.as_ref() {
        return Ok(ResolvedMoveTarget {
            position: Point::new(coordinates.x, coordinates.y),
            description: format!("{},{}", coordinates.x, coordinates.y),
        });
    }

    if let Some(relative) = request.relative.as_ref() {
        let current = peekaboox_input::current_mouse_position()
            .map_err(|error| Status::failed_precondition(error.to_string()))?;
        let x = current
            .x
            .checked_add(relative.x)
            .ok_or_else(|| Status::invalid_argument("relative move x coordinate overflows i32"))?;
        let y = current
            .y
            .checked_add(relative.y)
            .ok_or_else(|| Status::invalid_argument("relative move y coordinate overflows i32"))?;
        return Ok(ResolvedMoveTarget {
            position: Point::new(x, y),
            description: format!(
                "relative {},{} from {},{}",
                relative.x, relative.y, current.x, current.y
            ),
        });
    }

    let ratio = (
        request.ratio_x.unwrap_or(0.5),
        request.ratio_y.unwrap_or(0.5),
    );
    validate_ratio_status("ratio_x", ratio.0)?;
    validate_ratio_status("ratio_y", ratio.1)?;
    let scope = resolve_grpc_move_scope(request, list_windows)?;
    Ok(ResolvedMoveTarget {
        position: point_from_ratio_status(scope, ratio)?,
        description: format!(
            "ratio {:.3},{:.3} in {}",
            ratio.0,
            ratio.1,
            format_rect(scope)
        ),
    })
}

pub(super) fn resolve_grpc_move_scope(
    request: &proto::MoveMouseRequest,
    list_windows: WindowListProvider,
) -> Result<Rect, Status> {
    let region = request.region.map(rect_from_proto);
    let window = resolve_grpc_move_window(request, list_windows)?;
    match (window, region) {
        (Some(window), Some(region)) => {
            offset_window_relative_capture_region(window.bounds, region)
                .map_err(Status::invalid_argument)
        }
        (Some(window), None) => Ok(window.bounds),
        (None, Some(region)) => Ok(region),
        (None, None) => {
            let (width, height) = peekaboox_input::screen_size().ok_or_else(|| {
                Status::failed_precondition(
                    "move ratio without region/window requires a detectable screen size",
                )
            })?;
            Ok(Rect::new(
                0,
                0,
                u32::try_from(width).map_err(|_| Status::internal("screen width overflows u32"))?,
                u32::try_from(height)
                    .map_err(|_| Status::internal("screen height overflows u32"))?,
            ))
        }
    }
}

pub(super) fn resolve_grpc_move_window(
    request: &proto::MoveMouseRequest,
    list_windows: WindowListProvider,
) -> Result<Option<WindowInfo>, Status> {
    let id = clean_optional_string(request.window_id.clone());
    let app = clean_optional_string(request.app.clone());
    let title = clean_optional_string(request.window_title.clone());
    let title_regex = clean_optional_string(request.title_regex.clone());
    if id.is_none() && app.is_none() && title.is_none() && title_regex.is_none() {
        return Ok(None);
    }

    let query = window_query_from_fields(WindowQueryFields {
        id,
        app,
        title,
        title_regex,
        focused: false,
        limit: Some(1),
        sort: Some("focused".to_owned()),
        backend: None,
        diagnose: false,
    })
    .map_err(Status::invalid_argument)?;
    let metadata = list_windows(query).map_err(|error| Status::internal(error.to_string()))?;
    let window = metadata
        .windows
        .into_iter()
        .next()
        .ok_or_else(|| Status::not_found("no window matched move filters"))?;
    if window.bounds.width == 0 || window.bounds.height == 0 {
        return Err(Status::failed_precondition(format!(
            "window {} has empty bounds",
            window.id
        )));
    }
    Ok(Some(window))
}

pub(super) fn point_from_ratio_status(rect: Rect, ratio: (f32, f32)) -> Result<Point, Status> {
    let x = f64::from(rect.x) + (f64::from(rect.width.saturating_sub(1)) * f64::from(ratio.0));
    let y = f64::from(rect.y) + (f64::from(rect.height.saturating_sub(1)) * f64::from(ratio.1));
    Ok(Point::new(
        i32::try_from(x.round() as i64)
            .map_err(|_| Status::invalid_argument("move ratio x coordinate overflows i32"))?,
        i32::try_from(y.round() as i64)
            .map_err(|_| Status::invalid_argument("move ratio y coordinate overflows i32"))?,
    ))
}

pub(super) fn grpc_drag(
    request: proto::DragRequest,
    config: &ServerConfig,
    list_windows: WindowListProvider,
) -> Result<proto::ActionResponse, Status> {
    let options = drag_options_from_fields(
        request.duration_ms.map(u64::from),
        request.steps,
        request.bounds_policy.as_deref(),
        request.backend.as_deref(),
    )
    .map_err(Status::invalid_argument)?;
    let resolved = resolve_grpc_drag_target(&request, list_windows)?;
    let button = proto_mouse_button(request.button)?;
    let from = peekaboox_input::resolve_move_position(resolved.from, options.bounds_policy)
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
    let to = peekaboox_input::resolve_move_position(resolved.to, options.bounds_policy)
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
    let action = peekaboox_input::InputAction::Drag {
        from,
        to,
        button,
        duration_ms: options.duration_ms,
    };
    let metadata = if request.dry_run {
        let backend = peekaboox_input::CommandInputBackend
            .detect_backend_for_with_selection(&action, options.backend)
            .map_err(|error| Status::failed_precondition(error.to_string()))?;
        peekaboox_input::InputExecutionMetadata {
            backend_name: backend.name().to_owned(),
            backend_kind: backend.backend_kind(),
            action,
        }
    } else {
        ensure_input_allowed(config).map_err(Status::permission_denied)?;
        let restore_position = if request.restore {
            Some(
                peekaboox_input::current_mouse_position()
                    .map_err(|error| Status::failed_precondition(error.to_string()))?,
            )
        } else {
            None
        };
        let metadata = peekaboox_input::drag_with_options(from, to, button, options)
            .map_err(|error| Status::internal(error.to_string()))?;
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
            .map_err(|error| Status::internal(error.to_string()))?;
        }
        metadata
    };
    let backend_kind = backend_kind_name(metadata.backend_kind);
    let backend_name = metadata.backend_name;
    let action = if request.dry_run {
        "would drag"
    } else {
        "dragged"
    };
    let restore_suffix = if request.restore && !request.dry_run {
        " and restored"
    } else {
        ""
    };

    Ok(proto::ActionResponse {
        ok: true,
        message: format!(
            "{action} from {},{} ({}) to {},{} ({}) using {}/{}{restore_suffix}",
            from.x,
            from.y,
            resolved.from_description,
            to.x,
            to.y,
            resolved.to_description,
            backend_name,
            backend_kind
        ),
        backend_name: Some(backend_name),
        backend_kind: Some(backend_kind),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedDragTarget {
    pub(super) from: Point,
    pub(super) to: Point,
    pub(super) from_description: String,
    pub(super) to_description: String,
}

pub(super) fn resolve_grpc_drag_target(
    request: &proto::DragRequest,
    list_windows: WindowListProvider,
) -> Result<ResolvedDragTarget, Status> {
    let has_scope = request.region.is_some()
        || request.window_id.is_some()
        || request.app.is_some()
        || request.window_title.is_some()
        || request.title_regex.is_some();
    let has_from_ratio = request.from_ratio_x.is_some() || request.from_ratio_y.is_some();
    let has_to_ratio = request.to_ratio_x.is_some() || request.to_ratio_y.is_some();
    if has_scope && !has_from_ratio && !has_to_ratio {
        return Err(Status::invalid_argument(
            "drag region/window scope requires from_ratio or to_ratio",
        ));
    }

    let from_count = usize::from(request.from.is_some())
        + usize::from(request.from_current)
        + usize::from(has_from_ratio);
    if from_count != 1 {
        return Err(Status::invalid_argument(
            "provide exactly one drag from endpoint: from, from_current, or from_ratio",
        ));
    }

    let to_count = usize::from(request.to.is_some()) + usize::from(has_to_ratio);
    if to_count != 1 {
        return Err(Status::invalid_argument(
            "provide exactly one drag to endpoint: to or to_ratio",
        ));
    }

    let (from, from_description) = if let Some(from) = request.from.as_ref() {
        (Point::new(from.x, from.y), format!("{},{}", from.x, from.y))
    } else if request.from_current {
        let position = peekaboox_input::current_mouse_position()
            .map_err(|error| Status::failed_precondition(error.to_string()))?;
        (
            position,
            format!("current cursor at {},{}", position.x, position.y),
        )
    } else {
        let ratio = (
            request.from_ratio_x.unwrap_or(0.5),
            request.from_ratio_y.unwrap_or(0.5),
        );
        validate_ratio_status("from_ratio_x", ratio.0)?;
        validate_ratio_status("from_ratio_y", ratio.1)?;
        let scope = resolve_grpc_drag_scope(request, list_windows)?;
        (
            point_from_ratio_status(scope, ratio)?,
            format!(
                "from-ratio {:.3},{:.3} in {}",
                ratio.0,
                ratio.1,
                format_rect(scope)
            ),
        )
    };

    let (to, to_description) = if let Some(to) = request.to.as_ref() {
        (Point::new(to.x, to.y), format!("{},{}", to.x, to.y))
    } else {
        let ratio = (
            request.to_ratio_x.unwrap_or(0.5),
            request.to_ratio_y.unwrap_or(0.5),
        );
        validate_ratio_status("to_ratio_x", ratio.0)?;
        validate_ratio_status("to_ratio_y", ratio.1)?;
        let scope = resolve_grpc_drag_scope(request, list_windows)?;
        (
            point_from_ratio_status(scope, ratio)?,
            format!(
                "to-ratio {:.3},{:.3} in {}",
                ratio.0,
                ratio.1,
                format_rect(scope)
            ),
        )
    };

    Ok(ResolvedDragTarget {
        from,
        to,
        from_description,
        to_description,
    })
}

pub(super) fn resolve_grpc_drag_scope(
    request: &proto::DragRequest,
    list_windows: WindowListProvider,
) -> Result<Rect, Status> {
    let region = request.region.map(rect_from_proto);
    let window = resolve_grpc_drag_window(request, list_windows)?;
    match (window, region) {
        (Some(window), Some(region)) => {
            offset_window_relative_capture_region(window.bounds, region)
                .map_err(Status::invalid_argument)
        }
        (Some(window), None) => Ok(window.bounds),
        (None, Some(region)) => Ok(region),
        (None, None) => {
            let (width, height) = peekaboox_input::screen_size().ok_or_else(|| {
                Status::failed_precondition(
                    "drag ratio without region/window requires a detectable screen size",
                )
            })?;
            Ok(Rect::new(
                0,
                0,
                u32::try_from(width).map_err(|_| Status::internal("screen width overflows u32"))?,
                u32::try_from(height)
                    .map_err(|_| Status::internal("screen height overflows u32"))?,
            ))
        }
    }
}

pub(super) fn resolve_grpc_drag_window(
    request: &proto::DragRequest,
    list_windows: WindowListProvider,
) -> Result<Option<WindowInfo>, Status> {
    let id = clean_optional_string(request.window_id.clone());
    let app = clean_optional_string(request.app.clone());
    let title = clean_optional_string(request.window_title.clone());
    let title_regex = clean_optional_string(request.title_regex.clone());
    if id.is_none() && app.is_none() && title.is_none() && title_regex.is_none() {
        return Ok(None);
    }

    let query = window_query_from_fields(WindowQueryFields {
        id,
        app,
        title,
        title_regex,
        focused: false,
        limit: Some(1),
        sort: Some("focused".to_owned()),
        backend: None,
        diagnose: false,
    })
    .map_err(Status::invalid_argument)?;
    let metadata = list_windows(query).map_err(|error| Status::internal(error.to_string()))?;
    let window = metadata
        .windows
        .into_iter()
        .next()
        .ok_or_else(|| Status::not_found("no window matched drag filters"))?;
    if window.bounds.width == 0 || window.bounds.height == 0 {
        return Err(Status::failed_precondition(format!(
            "window {} has empty bounds",
            window.id
        )));
    }
    Ok(Some(window))
}

pub(super) fn resolve_click_target_with_optional_vision_fallback(
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

pub(super) fn resolve_vision_click_target(
    selector: &str,
) -> std::result::Result<peekaboox_accessibility::ResolvedClickTarget, String> {
    let query = ElementQuery::parse(selector).map_err(|error| error.to_string())?;
    let options = ElementLookupOptions::default();
    let elements = vision_fallback_elements(&query, &options)?.elements;
    peekaboox_accessibility::resolve_click_target_from_tree(selector, &elements)
        .map_err(|error| error.to_string())
}

pub(super) fn semantic_click_status(error: peekaboox_core::PeekabooXError) -> Status {
    let message = error.to_string();
    if message.contains("no clickable accessibility element matched") {
        Status::not_found(message)
    } else {
        Status::internal(message)
    }
}

pub(super) fn grpc_type_text(
    request: proto::TypeTextRequest,
    config: &ServerConfig,
) -> Result<proto::ActionResponse, Status> {
    let options = type_options_from_fields(
        request.typing_speed_chars_per_second,
        request.delay_ms,
        request.key_delay_ms,
        request.backend.as_deref(),
    )
    .map_err(Status::invalid_argument)?;
    let action = peekaboox_input::InputAction::TypeText(request.text.clone());
    let metadata = if request.dry_run {
        let backend = peekaboox_input::CommandInputBackend
            .detect_backend_for_with_selection(&action, options.backend)
            .map_err(|error| Status::failed_precondition(error.to_string()))?;
        peekaboox_input::InputExecutionMetadata {
            backend_name: backend.name().to_owned(),
            backend_kind: backend.backend_kind(),
            action,
        }
    } else {
        ensure_input_allowed(config).map_err(Status::permission_denied)?;
        peekaboox_input::type_text_with_options(request.text, options)
            .map_err(|error| Status::internal(error.to_string()))?
    };
    let backend_kind = backend_kind_name(metadata.backend_kind);
    let backend_name = metadata.backend_name;
    let action = if request.dry_run {
        "would type text"
    } else {
        "typed text"
    };

    Ok(proto::ActionResponse {
        ok: true,
        message: format!("{action} using {backend_name}/{backend_kind}"),
        backend_name: Some(backend_name),
        backend_kind: Some(backend_kind),
    })
}

pub(super) fn grpc_paste_text(
    request: proto::PasteTextRequest,
    config: &ServerConfig,
) -> Result<proto::ActionResponse, Status> {
    let options = paste_options_from_fields(
        request.preserve_clipboard,
        request.clipboard_backend.as_deref(),
        request.hotkey_backend.as_deref(),
        request.delay_ms,
        request.restore_delay_ms,
        request.restore_policy.as_deref(),
    )
    .map_err(Status::invalid_argument)?;
    let action = peekaboox_input::InputAction::PasteText {
        text: request.text.clone(),
        preserve_clipboard: request.preserve_clipboard,
    };
    let metadata = if request.dry_run {
        let backend = peekaboox_input::CommandInputBackend
            .detect_paste_backend_for_options(options)
            .map_err(|error| Status::failed_precondition(error.to_string()))?;
        peekaboox_input::InputExecutionMetadata {
            backend_name: backend.name(),
            backend_kind: backend.backend_kind(),
            action,
        }
    } else {
        ensure_input_allowed(config).map_err(Status::permission_denied)?;
        peekaboox_input::paste_text_with_options(request.text, options)
            .map_err(|error| Status::internal(error.to_string()))?
    };
    let backend_kind = backend_kind_name(metadata.backend_kind);
    let backend_name = metadata.backend_name;
    let action = if request.dry_run {
        "would paste text"
    } else {
        "pasted text"
    };

    Ok(proto::ActionResponse {
        ok: true,
        message: format!("{action} using {backend_name}/{backend_kind}"),
        backend_name: Some(backend_name),
        backend_kind: Some(backend_kind),
    })
}

pub(super) fn grpc_hotkey(
    request: proto::HotkeyRequest,
    config: &ServerConfig,
) -> Result<proto::ActionResponse, Status> {
    validate_hotkey_keys(&request.keys)?;
    let keys = peekaboox_input::normalize_hotkey_keys(&request.keys)
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
    let options = hotkey_options_from_fields(
        request.backend.as_deref(),
        request.delay_ms,
        request.key_delay_ms,
        request.repeat,
        request.interval_ms,
        request.release_before,
        request.release_after,
    )
    .map_err(Status::invalid_argument)?;
    let action = peekaboox_input::InputAction::Hotkey(keys.clone());
    let metadata = if request.dry_run {
        let backend = peekaboox_input::CommandInputBackend
            .detect_backend_for_with_selection(&action, options.backend)
            .map_err(|error| Status::failed_precondition(error.to_string()))?;
        peekaboox_input::InputExecutionMetadata {
            backend_name: backend.name().to_owned(),
            backend_kind: backend.backend_kind(),
            action,
        }
    } else {
        ensure_input_allowed(config).map_err(Status::permission_denied)?;
        peekaboox_input::hotkey_with_options(keys, options)
            .map_err(|error| Status::internal(error.to_string()))?
    };
    let backend_kind = backend_kind_name(metadata.backend_kind);
    let backend_name = metadata.backend_name;
    let action = if request.dry_run {
        "would press hotkey"
    } else {
        "pressed hotkey"
    };

    Ok(proto::ActionResponse {
        ok: true,
        message: format!("{action} using {backend_name}/{backend_kind}"),
        backend_name: Some(backend_name),
        backend_kind: Some(backend_kind),
    })
}
