use super::*;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ClickArgs {
    pub(super) target: ClickTarget,
    pub(super) button: MouseButton,
    pub(super) dry_run: bool,
    pub(super) json: bool,
    pub(super) vision_fallback: bool,
    pub(super) bounds_policy: peekaboox_input::MoveBoundsPolicy,
    pub(super) backend: peekaboox_input::InputToolSelection,
    pub(super) restore: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum ClickTarget {
    Coordinates(Point),
    SemanticSelector(String),
    ScopeRatio {
        ratio: (f32, f32),
        region: Option<Rect>,
        window_id: Option<String>,
        app: Option<String>,
        window_title: Option<String>,
        title_regex: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedClickTarget {
    pub(super) position: Point,
    pub(super) description: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum ClickCommand {
    Run(ClickArgs),
    Help,
}

pub(super) fn click(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let ClickCommand::Run(args) = parse_click_args(args)? else {
        print_click_usage();
        return Err(CliError::HelpRequested);
    };

    let target = resolve_click_target(&args)?;
    let options = click_options_from_args(&args);
    let effective_position =
        peekaboox_input::resolve_move_position(target.position, args.bounds_policy)
            .map_err(|error| CliError::Failure(error.to_string()))?;
    let target = ResolvedClickTarget {
        description: target.description,
        position: effective_position,
    };
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
                bounds_policy: args.bounds_policy.name().to_owned(),
                backend: args.backend.name().to_owned(),
                restore: args.restore,
            },
        )?;
        let ApiResult::Click(metadata) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected click response".to_owned(),
            ));
        };
        print_click_result(&args, &target, metadata, None);
        return Ok(());
    }

    if args.dry_run {
        let backend = peekaboox_input::CommandInputBackend
            .detect_backend_for_with_selection(&action, args.backend)
            .map_err(|error| CliError::Failure(error.to_string()))?;
        print_click_result(
            &args,
            &target,
            ActionResultDto {
                backend_name: backend.name().to_owned(),
                backend_kind: format!("{:?}", backend.backend_kind()).to_ascii_lowercase(),
            },
            None,
        );
        return Ok(());
    }

    let restore_position = if args.restore {
        Some(
            peekaboox_input::current_mouse_position()
                .map_err(|error| CliError::Failure(error.to_string()))?,
        )
    } else {
        None
    };
    let metadata = peekaboox_input::click_with_options(target.position, args.button, options)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    if let Some(position) = restore_position {
        let restore_options = peekaboox_input::MoveMouseOptions {
            duration_ms: 0,
            steps: None,
            bounds_policy: args.bounds_policy,
            backend: args.backend,
        };
        peekaboox_input::move_mouse_with_options(position, restore_options)
            .map_err(|error| CliError::Failure(error.to_string()))?;
    }
    print_click_result(
        &args,
        &target,
        input_metadata_dto(metadata),
        restore_position,
    );

    Ok(())
}

pub(super) fn resolve_click_target(args: &ClickArgs) -> Result<ResolvedClickTarget, CliError> {
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
        ClickTarget::ScopeRatio {
            ratio,
            region,
            window_id,
            app,
            window_title,
            title_regex,
        } => {
            let scope = resolve_move_scope(
                *region,
                window_id.as_deref(),
                app.as_deref(),
                window_title.as_deref(),
                title_regex.as_deref(),
            )?;
            let position = point_from_ratio(scope, *ratio)?;
            Ok(ResolvedClickTarget {
                position,
                description: format!(
                    "ratio {:.3},{:.3} in {}",
                    ratio.0,
                    ratio.1,
                    format_rect(scope)
                ),
            })
        }
    }
}

pub(super) fn resolve_semantic_click_target(
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

pub(super) fn default_elements_args_for_selector(selector: &str) -> ElementsArgs {
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

pub(super) fn click_options_from_args(args: &ClickArgs) -> peekaboox_input::ClickMouseOptions {
    peekaboox_input::ClickMouseOptions {
        bounds_policy: args.bounds_policy,
        backend: args.backend,
    }
}

pub(super) fn print_click_result(
    args: &ClickArgs,
    target: &ResolvedClickTarget,
    metadata: ActionResultDto,
    restored_to: Option<Point>,
) {
    if args.json {
        let _ = print_json_pretty(&serde_json::json!({
            "ok": true,
            "dry_run": args.dry_run,
            "target": {
                "x": target.position.x,
                "y": target.position.y,
                "description": target.description,
            },
            "button": mouse_button_label(args.button),
            "backend_name": metadata.backend_name,
            "backend_kind": metadata.backend_kind,
            "requested_backend": args.backend.name(),
            "bounds_policy": args.bounds_policy.name(),
            "restore": args.restore,
            "restored_to": restored_to.map(|point| serde_json::json!({
                "x": point.x,
                "y": point.y,
            })),
        }));
        return;
    }

    if args.dry_run {
        println!(
            "would click {} via {}",
            target.description, metadata.backend_name
        );
    } else {
        if let Some(restored_to) = restored_to {
            println!(
                "clicked {} with {:?} via {} and restored to {},{}",
                target.description,
                args.button,
                metadata.backend_name,
                restored_to.x,
                restored_to.y
            );
        } else {
            println!(
                "clicked {} with {:?} via {}",
                target.description, args.button, metadata.backend_name
            );
        }
    }
}

pub(super) fn parse_click_args(args: Vec<String>) -> Result<ClickCommand, CliError> {
    let mut x = None;
    let mut y = None;
    let mut to = None;
    let mut selector = None;
    let mut ratio = None;
    let mut region = None;
    let mut window_id = None;
    let mut app = None;
    let mut window_title = None;
    let mut title_regex = None;
    let mut button = MouseButton::Left;
    let mut dry_run = false;
    let mut json = false;
    let mut vision_fallback = false;
    let mut bounds_policy = peekaboox_input::MoveBoundsPolicy::Allow;
    let mut backend = peekaboox_input::InputToolSelection::Auto;
    let mut restore = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--x" => {
                let value = parse_next_string(&args, &mut index, "--x")?;
                x = Some(parse_i32("--x", &value)?);
            }
            "--y" => {
                let value = parse_next_string(&args, &mut index, "--y")?;
                y = Some(parse_i32("--y", &value)?);
            }
            "--to" => {
                let value = parse_next_string(&args, &mut index, "--to")?;
                to = Some(parse_point("--to", &value)?);
            }
            "--selector" | "--text" => {
                let key = args[index].clone();
                selector = Some(parse_next_string(&args, &mut index, &key)?);
            }
            "--ratio" => {
                let value = parse_next_string(&args, &mut index, "--ratio")?;
                ratio = Some(parse_ratio_pair("--ratio", &value)?);
            }
            "--region" | "-r" => {
                let value = parse_next_string(&args, &mut index, "--region")?;
                region = Some(parse_rect("--region", &value)?);
            }
            "--window-id" => {
                window_id = Some(parse_next_string(&args, &mut index, "--window-id")?);
            }
            "--app" | "-a" => app = Some(parse_next_string(&args, &mut index, "--app")?),
            "--window-title" => {
                window_title = Some(parse_next_string(&args, &mut index, "--window-title")?)
            }
            "--title-regex" => {
                title_regex = Some(parse_next_string(&args, &mut index, "--title-regex")?)
            }
            "--button" | "-b" => {
                let value = parse_next_string(&args, &mut index, "--button")?;
                button = parse_mouse_button(&value)?;
            }
            "--dry-run" => dry_run = true,
            "--json" => json = true,
            "--vision-fallback" => vision_fallback = true,
            "--bounds" => {
                let value = parse_next_string(&args, &mut index, "--bounds")?;
                bounds_policy = parse_move_bounds_policy(&value)?;
            }
            "--clamp" => bounds_policy = peekaboox_input::MoveBoundsPolicy::Clamp,
            "--fail-out-of-bounds" => bounds_policy = peekaboox_input::MoveBoundsPolicy::Fail,
            "--backend" => {
                let value = parse_next_string(&args, &mut index, "--backend")?;
                backend = parse_input_backend_selection(&value)?;
            }
            "--restore" => restore = true,
            "--help" | "-h" => return Ok(ClickCommand::Help),
            unknown => {
                return Err(CliError::Failure(format!(
                    "unknown click argument: {unknown}"
                )));
            }
        }

        index += 1;
    }

    let absolute = match (x, y) {
        (Some(x), Some(y)) => Some(Point::new(x, y)),
        (Some(_), None) => return Err(CliError::Failure("missing required --y".to_owned())),
        (None, Some(_)) => return Err(CliError::Failure("missing required --x".to_owned())),
        (None, None) => None,
    };

    let has_scope = ratio.is_some()
        || region.is_some()
        || window_id.is_some()
        || app.is_some()
        || window_title.is_some()
        || title_regex.is_some();
    let scope_target = if has_scope {
        Some(ClickTarget::ScopeRatio {
            ratio: ratio.unwrap_or((0.5, 0.5)),
            region,
            window_id,
            app,
            window_title,
            title_regex,
        })
    } else {
        None
    };

    let mut targets = Vec::new();
    if let Some(position) = absolute {
        targets.push(ClickTarget::Coordinates(position));
    }
    if let Some(position) = to {
        targets.push(ClickTarget::Coordinates(position));
    }
    if let Some(selector) = selector {
        targets.push(ClickTarget::SemanticSelector(selector));
    }
    if let Some(scope_target) = scope_target {
        targets.push(scope_target);
    }

    if targets.len() > 1 {
        return Err(CliError::Failure(
            "provide exactly one click target".to_owned(),
        ));
    }

    let target = targets.into_iter().next().ok_or_else(|| {
        CliError::Failure(
            "missing click target; provide --x/--y, --to, --selector/--text, --ratio, or a region/window scope"
                .to_owned(),
        )
    })?;

    Ok(ClickCommand::Run(ClickArgs {
        target,
        button,
        dry_run,
        json,
        vision_fallback,
        bounds_policy,
        backend,
        restore,
    }))
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct MoveArgs {
    pub(super) target: MoveTarget,
    pub(super) dry_run: bool,
    pub(super) json: bool,
    pub(super) duration_ms: u64,
    pub(super) steps: Option<u32>,
    pub(super) bounds_policy: peekaboox_input::MoveBoundsPolicy,
    pub(super) backend: peekaboox_input::InputToolSelection,
    pub(super) restore: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum MoveTarget {
    Position(Point),
    Relative(Point),
    ScopeRatio {
        ratio: (f32, f32),
        region: Option<Rect>,
        window_id: Option<String>,
        app: Option<String>,
        window_title: Option<String>,
        title_regex: Option<String>,
    },
    CurrentPosition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedMoveTarget {
    pub(super) position: Point,
    pub(super) description: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum MoveCommand {
    Run(MoveArgs),
    Help,
}

pub(super) fn move_mouse(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let MoveCommand::Run(args) = parse_move_args(args)? else {
        print_move_usage();
        return Err(CliError::HelpRequested);
    };

    if matches!(args.target, MoveTarget::CurrentPosition) {
        let position = peekaboox_input::current_mouse_position()
            .map_err(|error| CliError::Failure(error.to_string()))?;
        print_current_position(&args, position);
        return Ok(());
    }

    let target = resolve_move_target(&args)?;
    let options = move_options_from_args(&args);
    let effective_position =
        peekaboox_input::resolve_move_position(target.position, args.bounds_policy)
            .map_err(|error| CliError::Failure(error.to_string()))?;
    let target = ResolvedMoveTarget {
        description: target.description,
        position: effective_position,
    };
    let action = peekaboox_input::InputAction::MoveMouse(target.position);

    if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::MoveMouse {
                x: target.position.x,
                y: target.position.y,
                dry_run: args.dry_run,
                duration_ms: args.duration_ms,
                steps: args.steps,
                bounds_policy: args.bounds_policy.name().to_owned(),
                backend: args.backend.name().to_owned(),
                restore: args.restore,
            },
        )?;
        let ApiResult::MoveMouse(metadata) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected move response".to_owned(),
            ));
        };
        print_move_result(&args, &target, metadata, None);
        return Ok(());
    }

    if args.dry_run {
        let backend = peekaboox_input::CommandInputBackend
            .detect_backend_for_with_selection(&action, args.backend)
            .map_err(|error| CliError::Failure(error.to_string()))?;
        print_move_result(
            &args,
            &target,
            ActionResultDto {
                backend_name: backend.name().to_owned(),
                backend_kind: format!("{:?}", backend.backend_kind()).to_ascii_lowercase(),
            },
            None,
        );
        return Ok(());
    }

    let restore_position = if args.restore {
        Some(
            peekaboox_input::current_mouse_position()
                .map_err(|error| CliError::Failure(error.to_string()))?,
        )
    } else {
        None
    };
    let metadata = peekaboox_input::move_mouse_with_options(target.position, options)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    if let Some(position) = restore_position {
        let restore_options = peekaboox_input::MoveMouseOptions {
            duration_ms: args.duration_ms,
            steps: args.steps,
            bounds_policy: args.bounds_policy,
            backend: args.backend,
        };
        peekaboox_input::move_mouse_with_options(position, restore_options)
            .map_err(|error| CliError::Failure(error.to_string()))?;
    }
    print_move_result(
        &args,
        &target,
        input_metadata_dto(metadata),
        restore_position,
    );

    Ok(())
}

pub(super) fn resolve_move_target(args: &MoveArgs) -> Result<ResolvedMoveTarget, CliError> {
    match &args.target {
        MoveTarget::Position(position) => Ok(ResolvedMoveTarget {
            position: *position,
            description: format!("{},{}", position.x, position.y),
        }),
        MoveTarget::Relative(delta) => {
            let current = peekaboox_input::current_mouse_position()
                .map_err(|error| CliError::Failure(error.to_string()))?;
            let x = current.x.checked_add(delta.x).ok_or_else(|| {
                CliError::Failure("relative move x coordinate overflows i32".to_owned())
            })?;
            let y = current.y.checked_add(delta.y).ok_or_else(|| {
                CliError::Failure("relative move y coordinate overflows i32".to_owned())
            })?;
            Ok(ResolvedMoveTarget {
                position: Point::new(x, y),
                description: format!(
                    "relative {},{} from {},{}",
                    delta.x, delta.y, current.x, current.y
                ),
            })
        }
        MoveTarget::ScopeRatio {
            ratio,
            region,
            window_id,
            app,
            window_title,
            title_regex,
        } => {
            let scope = resolve_move_scope(
                *region,
                window_id.as_deref(),
                app.as_deref(),
                window_title.as_deref(),
                title_regex.as_deref(),
            )?;
            let position = point_from_ratio(scope, *ratio)?;
            Ok(ResolvedMoveTarget {
                position,
                description: format!(
                    "ratio {:.3},{:.3} in {}",
                    ratio.0,
                    ratio.1,
                    format_rect(scope)
                ),
            })
        }
        MoveTarget::CurrentPosition => Err(CliError::Failure(
            "--current-position does not resolve to a movement target".to_owned(),
        )),
    }
}

pub(super) fn resolve_move_scope(
    region: Option<Rect>,
    window_id: Option<&str>,
    app: Option<&str>,
    window_title: Option<&str>,
    title_regex: Option<&str>,
) -> Result<Rect, CliError> {
    let window = resolve_move_window(window_id, app, window_title, title_regex)?;
    match (window, region) {
        (Some(window), Some(region)) => offset_move_window_region(window.bounds, region),
        (Some(window), None) => Ok(window.bounds),
        (None, Some(region)) => Ok(region),
        (None, None) => {
            let (width, height) = peekaboox_input::screen_size().ok_or_else(|| {
                CliError::Failure(
                    "move --ratio without --region or window filters requires a detectable screen size"
                        .to_owned(),
                )
            })?;
            Ok(Rect::new(
                0,
                0,
                u32::try_from(width)
                    .map_err(|_| CliError::Failure("screen width overflows u32".to_owned()))?,
                u32::try_from(height)
                    .map_err(|_| CliError::Failure("screen height overflows u32".to_owned()))?,
            ))
        }
    }
}

pub(super) fn resolve_move_window(
    window_id: Option<&str>,
    app: Option<&str>,
    window_title: Option<&str>,
    title_regex: Option<&str>,
) -> Result<Option<WindowInfo>, CliError> {
    if window_id.is_none() && app.is_none() && window_title.is_none() && title_regex.is_none() {
        return Ok(None);
    }

    let query = peekaboox_windows::WindowQuery {
        id: window_id.map(str::to_owned),
        app: app.map(str::to_owned),
        title: window_title.map(str::to_owned),
        title_regex: title_regex.map(str::to_owned),
        focused_only: false,
        limit: Some(1),
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
        .ok_or_else(|| CliError::Failure("no window matched move filters".to_owned()))?;
    if window.bounds.width == 0 || window.bounds.height == 0 {
        return Err(CliError::Failure(format!(
            "window {} has empty bounds",
            window.id
        )));
    }
    Ok(Some(window))
}

pub(super) fn offset_move_window_region(origin: Rect, region: Rect) -> Result<Rect, CliError> {
    if region.x < 0 || region.y < 0 {
        return Err(CliError::Failure(
            "window-relative move region must start inside the window".to_owned(),
        ));
    }
    let right = i64::from(region.x) + i64::from(region.width);
    let bottom = i64::from(region.y) + i64::from(region.height);
    if right > i64::from(origin.width) || bottom > i64::from(origin.height) {
        return Err(CliError::Failure(
            "window-relative move region must fit inside the window".to_owned(),
        ));
    }
    let x = i64::from(origin.x) + i64::from(region.x);
    let y = i64::from(origin.y) + i64::from(region.y);
    Ok(Rect::new(
        i32::try_from(x)
            .map_err(|_| CliError::Failure("window-relative move region x overflow".to_owned()))?,
        i32::try_from(y)
            .map_err(|_| CliError::Failure("window-relative move region y overflow".to_owned()))?,
        region.width,
        region.height,
    ))
}

pub(super) fn point_from_ratio(rect: Rect, ratio: (f32, f32)) -> Result<Point, CliError> {
    let x = f64::from(rect.x) + (f64::from(rect.width.saturating_sub(1)) * f64::from(ratio.0));
    let y = f64::from(rect.y) + (f64::from(rect.height.saturating_sub(1)) * f64::from(ratio.1));
    Ok(Point::new(
        i32::try_from(x.round() as i64)
            .map_err(|_| CliError::Failure("move ratio x coordinate overflows i32".to_owned()))?,
        i32::try_from(y.round() as i64)
            .map_err(|_| CliError::Failure("move ratio y coordinate overflows i32".to_owned()))?,
    ))
}

pub(super) fn move_options_from_args(args: &MoveArgs) -> peekaboox_input::MoveMouseOptions {
    peekaboox_input::MoveMouseOptions {
        duration_ms: args.duration_ms,
        steps: args.steps,
        bounds_policy: args.bounds_policy,
        backend: args.backend,
    }
}

pub(super) fn print_move_result(
    args: &MoveArgs,
    target: &ResolvedMoveTarget,
    metadata: ActionResultDto,
    restored_to: Option<Point>,
) {
    if args.json {
        let _ = print_json_pretty(&serde_json::json!({
            "ok": true,
            "dry_run": args.dry_run,
            "target": {
                "x": target.position.x,
                "y": target.position.y,
                "description": target.description,
            },
            "backend_name": metadata.backend_name,
            "backend_kind": metadata.backend_kind,
            "requested_backend": args.backend.name(),
            "bounds_policy": args.bounds_policy.name(),
            "duration_ms": args.duration_ms,
            "steps": args.steps,
            "restore": args.restore,
            "restored_to": restored_to.map(|point| serde_json::json!({
                "x": point.x,
                "y": point.y,
            })),
        }));
        return;
    }

    if args.dry_run {
        println!(
            "would move mouse to {} via {}",
            target.description, metadata.backend_name
        );
    } else {
        if let Some(restored_to) = restored_to {
            println!(
                "moved mouse to {} via {} and restored to {},{}",
                target.description, metadata.backend_name, restored_to.x, restored_to.y
            );
        } else {
            println!(
                "moved mouse to {} via {}",
                target.description, metadata.backend_name
            );
        }
    }
}

pub(super) fn print_current_position(args: &MoveArgs, position: Point) {
    if args.json {
        let _ = print_json_pretty(&serde_json::json!({
            "ok": true,
            "cursor": {
                "x": position.x,
                "y": position.y,
            }
        }));
    } else {
        println!("cursor at {},{}", position.x, position.y);
    }
}

pub(super) fn parse_move_args(args: Vec<String>) -> Result<MoveCommand, CliError> {
    let mut x = None;
    let mut y = None;
    let mut to = None;
    let mut relative = None;
    let mut dx = None;
    let mut dy = None;
    let mut ratio = None;
    let mut region = None;
    let mut window_id = None;
    let mut app = None;
    let mut window_title = None;
    let mut title_regex = None;
    let mut dry_run = false;
    let mut json = false;
    let mut duration_ms = 0;
    let mut steps = None;
    let mut bounds_policy = peekaboox_input::MoveBoundsPolicy::Allow;
    let mut backend = peekaboox_input::InputToolSelection::Auto;
    let mut restore = false;
    let mut current_position = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--x" => {
                let value = parse_next_string(&args, &mut index, "--x")?;
                x = Some(parse_i32("--x", &value)?);
            }
            "--y" => {
                let value = parse_next_string(&args, &mut index, "--y")?;
                y = Some(parse_i32("--y", &value)?);
            }
            "--to" => {
                let value = parse_next_string(&args, &mut index, "--to")?;
                to = Some(parse_point("--to", &value)?);
            }
            "--relative" => {
                let value = parse_next_string(&args, &mut index, "--relative")?;
                relative = Some(parse_point("--relative", &value)?);
            }
            "--dx" => {
                let value = parse_next_string(&args, &mut index, "--dx")?;
                dx = Some(parse_i32("--dx", &value)?);
            }
            "--dy" => {
                let value = parse_next_string(&args, &mut index, "--dy")?;
                dy = Some(parse_i32("--dy", &value)?);
            }
            "--ratio" => {
                let value = parse_next_string(&args, &mut index, "--ratio")?;
                ratio = Some(parse_ratio_pair("--ratio", &value)?);
            }
            "--region" | "-r" => {
                let value = parse_next_string(&args, &mut index, "--region")?;
                region = Some(parse_rect("--region", &value)?);
            }
            "--window-id" => {
                window_id = Some(parse_next_string(&args, &mut index, "--window-id")?);
            }
            "--app" | "-a" => app = Some(parse_next_string(&args, &mut index, "--app")?),
            "--window-title" => {
                window_title = Some(parse_next_string(&args, &mut index, "--window-title")?)
            }
            "--title-regex" => {
                title_regex = Some(parse_next_string(&args, &mut index, "--title-regex")?)
            }
            "--dry-run" => dry_run = true,
            "--json" => json = true,
            "--duration-ms" => {
                let value = parse_next_string(&args, &mut index, "--duration-ms")?;
                duration_ms = parse_u64("--duration-ms", &value)?;
            }
            "--steps" => {
                let value = parse_next_string(&args, &mut index, "--steps")?;
                steps = Some(parse_positive_u32("--steps", &value)?);
            }
            "--bounds" => {
                let value = parse_next_string(&args, &mut index, "--bounds")?;
                bounds_policy = parse_move_bounds_policy(&value)?;
            }
            "--clamp" => bounds_policy = peekaboox_input::MoveBoundsPolicy::Clamp,
            "--fail-out-of-bounds" => bounds_policy = peekaboox_input::MoveBoundsPolicy::Fail,
            "--backend" => {
                let value = parse_next_string(&args, &mut index, "--backend")?;
                backend = parse_input_backend_selection(&value)?;
            }
            "--restore" => restore = true,
            "--current-position" | "--position" | "--query-position" => current_position = true,
            "--help" | "-h" => return Ok(MoveCommand::Help),
            unknown => {
                return Err(CliError::Failure(format!(
                    "unknown move argument: {unknown}"
                )));
            }
        }

        index += 1;
    }

    let absolute = match (x, y) {
        (Some(x), Some(y)) => Some(Point::new(x, y)),
        (Some(_), None) => return Err(CliError::Failure("missing required --y".to_owned())),
        (None, Some(_)) => return Err(CliError::Failure("missing required --x".to_owned())),
        (None, None) => None,
    };

    let relative_delta = match (relative, dx, dy) {
        (Some(relative), None, None) => Some(relative),
        (None, Some(dx), Some(dy)) => Some(Point::new(dx, dy)),
        (None, Some(_), None) => return Err(CliError::Failure("missing required --dy".to_owned())),
        (None, None, Some(_)) => return Err(CliError::Failure("missing required --dx".to_owned())),
        (Some(_), Some(_), _) | (Some(_), _, Some(_)) => {
            return Err(CliError::Failure(
                "provide either --relative or --dx/--dy, not both".to_owned(),
            ));
        }
        (None, None, None) => None,
    };

    let has_scope = ratio.is_some()
        || region.is_some()
        || window_id.is_some()
        || app.is_some()
        || window_title.is_some()
        || title_regex.is_some();
    let scope_target = if has_scope {
        Some(MoveTarget::ScopeRatio {
            ratio: ratio.unwrap_or((0.5, 0.5)),
            region,
            window_id,
            app,
            window_title,
            title_regex,
        })
    } else {
        None
    };

    let mut targets = Vec::new();
    if let Some(position) = absolute {
        targets.push(MoveTarget::Position(position));
    }
    if let Some(position) = to {
        targets.push(MoveTarget::Position(position));
    }
    if let Some(delta) = relative_delta {
        targets.push(MoveTarget::Relative(delta));
    }
    if let Some(scope_target) = scope_target {
        targets.push(scope_target);
    }
    if current_position {
        targets.push(MoveTarget::CurrentPosition);
    }

    if targets.len() > 1 {
        return Err(CliError::Failure(
            "provide exactly one move target".to_owned(),
        ));
    }

    let target = targets.into_iter().next().ok_or_else(|| {
        CliError::Failure(
            "missing move target; provide --x/--y, --to, --relative/--dx/--dy, --ratio, a region/window scope, or --current-position"
                .to_owned(),
        )
    })?;

    Ok(MoveCommand::Run(MoveArgs {
        target,
        dry_run,
        json,
        duration_ms,
        steps,
        bounds_policy,
        backend,
        restore,
    }))
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct DragArgs {
    pub(super) from: DragEndpoint,
    pub(super) to: DragEndpoint,
    pub(super) button: MouseButton,
    pub(super) duration_ms: u64,
    pub(super) steps: Option<u32>,
    pub(super) bounds_policy: peekaboox_input::MoveBoundsPolicy,
    pub(super) backend: peekaboox_input::InputToolSelection,
    pub(super) restore: bool,
    pub(super) dry_run: bool,
    pub(super) json: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum DragEndpoint {
    Position(Point),
    CurrentPosition,
    ScopeRatio {
        ratio: (f32, f32),
        region: Option<Rect>,
        window_id: Option<String>,
        app: Option<String>,
        window_title: Option<String>,
        title_regex: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DragScopeParts {
    pub(super) region: Option<Rect>,
    pub(super) window_id: Option<String>,
    pub(super) app: Option<String>,
    pub(super) window_title: Option<String>,
    pub(super) title_regex: Option<String>,
}

impl DragScopeParts {
    fn has_scope(&self) -> bool {
        self.region.is_some()
            || self.window_id.is_some()
            || self.app.is_some()
            || self.window_title.is_some()
            || self.title_regex.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedDragTarget {
    pub(super) from: Point,
    pub(super) to: Point,
    pub(super) from_description: String,
    pub(super) to_description: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum DragCommand {
    Run(Box<DragArgs>),
    Help,
}

pub(super) fn drag(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let DragCommand::Run(args) = parse_drag_args(args)? else {
        print_drag_usage();
        return Err(CliError::HelpRequested);
    };

    let target = resolve_drag_target(&args)?;
    let options = drag_options_from_args(&args);
    let from = peekaboox_input::resolve_move_position(target.from, args.bounds_policy)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    let to = peekaboox_input::resolve_move_position(target.to, args.bounds_policy)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    let target = ResolvedDragTarget { from, to, ..target };
    let action = peekaboox_input::InputAction::Drag {
        from: target.from,
        to: target.to,
        button: args.button,
        duration_ms: args.duration_ms,
    };

    if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::Drag {
                from_x: target.from.x,
                from_y: target.from.y,
                to_x: target.to.x,
                to_y: target.to.y,
                button: mouse_button_dto(args.button),
                duration_ms: u32::try_from(args.duration_ms).map_err(|_| {
                    CliError::Failure("--duration-ms must fit into uint32".to_owned())
                })?,
                steps: args.steps,
                bounds_policy: args.bounds_policy.name().to_owned(),
                backend: args.backend.name().to_owned(),
                restore: args.restore,
                dry_run: args.dry_run,
            },
        )?;
        let ApiResult::Drag(metadata) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected drag response".to_owned(),
            ));
        };
        print_drag_result(&args, &target, metadata, None);
        return Ok(());
    }

    if args.dry_run {
        let backend = peekaboox_input::CommandInputBackend
            .detect_backend_for_with_selection(&action, args.backend)
            .map_err(|error| CliError::Failure(error.to_string()))?;
        print_drag_result(
            &args,
            &target,
            ActionResultDto {
                backend_name: backend.name().to_owned(),
                backend_kind: format!("{:?}", backend.backend_kind()).to_ascii_lowercase(),
            },
            None,
        );
        return Ok(());
    }

    let restore_position = if args.restore {
        Some(
            peekaboox_input::current_mouse_position()
                .map_err(|error| CliError::Failure(error.to_string()))?,
        )
    } else {
        None
    };
    let metadata = peekaboox_input::drag_with_options(target.from, target.to, args.button, options)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    if let Some(position) = restore_position {
        let restore_options = peekaboox_input::MoveMouseOptions {
            duration_ms: args.duration_ms,
            steps: args.steps,
            bounds_policy: args.bounds_policy,
            backend: args.backend,
        };
        peekaboox_input::move_mouse_with_options(position, restore_options)
            .map_err(|error| CliError::Failure(error.to_string()))?;
    }
    print_drag_result(
        &args,
        &target,
        input_metadata_dto(metadata),
        restore_position,
    );

    Ok(())
}

pub(super) fn resolve_drag_target(args: &DragArgs) -> Result<ResolvedDragTarget, CliError> {
    let (from, from_description) = resolve_drag_endpoint(&args.from, "from")?;
    let (to, to_description) = resolve_drag_endpoint(&args.to, "to")?;
    Ok(ResolvedDragTarget {
        from,
        to,
        from_description,
        to_description,
    })
}

pub(super) fn resolve_drag_endpoint(
    endpoint: &DragEndpoint,
    name: &str,
) -> Result<(Point, String), CliError> {
    match endpoint {
        DragEndpoint::Position(position) => {
            Ok((*position, format!("{},{}", position.x, position.y)))
        }
        DragEndpoint::CurrentPosition => {
            let position = peekaboox_input::current_mouse_position()
                .map_err(|error| CliError::Failure(error.to_string()))?;
            Ok((
                position,
                format!("current cursor at {},{}", position.x, position.y),
            ))
        }
        DragEndpoint::ScopeRatio {
            ratio,
            region,
            window_id,
            app,
            window_title,
            title_regex,
        } => {
            let scope = resolve_move_scope(
                *region,
                window_id.as_deref(),
                app.as_deref(),
                window_title.as_deref(),
                title_regex.as_deref(),
            )?;
            let position = point_from_ratio(scope, *ratio)?;
            Ok((
                position,
                format!(
                    "{name}-ratio {:.3},{:.3} in {}",
                    ratio.0,
                    ratio.1,
                    format_rect(scope)
                ),
            ))
        }
    }
}

pub(super) fn drag_options_from_args(args: &DragArgs) -> peekaboox_input::DragMouseOptions {
    peekaboox_input::DragMouseOptions {
        duration_ms: args.duration_ms,
        steps: args.steps,
        bounds_policy: args.bounds_policy,
        backend: args.backend,
    }
}

pub(super) fn print_drag_result(
    args: &DragArgs,
    target: &ResolvedDragTarget,
    metadata: ActionResultDto,
    restored_to: Option<Point>,
) {
    if args.json {
        let _ = print_json_pretty(&serde_json::json!({
            "ok": true,
            "dry_run": args.dry_run,
            "from": {
                "x": target.from.x,
                "y": target.from.y,
                "description": target.from_description,
            },
            "to": {
                "x": target.to.x,
                "y": target.to.y,
                "description": target.to_description,
            },
            "button": mouse_button_label(args.button),
            "backend_name": metadata.backend_name,
            "backend_kind": metadata.backend_kind,
            "requested_backend": args.backend.name(),
            "bounds_policy": args.bounds_policy.name(),
            "duration_ms": args.duration_ms,
            "steps": args.steps,
            "restore": args.restore,
            "restored_to": restored_to.map(|point| serde_json::json!({
                "x": point.x,
                "y": point.y,
            })),
        }));
        return;
    }

    if args.dry_run {
        println!(
            "would drag from {},{} to {},{} via {}",
            target.from.x, target.from.y, target.to.x, target.to.y, metadata.backend_name
        );
    } else {
        if let Some(restored_to) = restored_to {
            println!(
                "dragged from {},{} to {},{} with {:?} via {} and restored to {},{}",
                target.from.x,
                target.from.y,
                target.to.x,
                target.to.y,
                args.button,
                metadata.backend_name,
                restored_to.x,
                restored_to.y
            );
        } else {
            println!(
                "dragged from {},{} to {},{} with {:?} via {}",
                target.from.x,
                target.from.y,
                target.to.x,
                target.to.y,
                args.button,
                metadata.backend_name
            );
        }
    }
}

pub(super) fn parse_drag_args(args: Vec<String>) -> Result<DragCommand, CliError> {
    let mut from = None;
    let mut to = None;
    let mut from_x = None;
    let mut from_y = None;
    let mut to_x = None;
    let mut to_y = None;
    let mut from_current = false;
    let mut from_ratio = None;
    let mut to_ratio = None;
    let mut region = None;
    let mut window_id = None;
    let mut app = None;
    let mut window_title = None;
    let mut title_regex = None;
    let mut button = MouseButton::Left;
    let mut duration_ms = 250_u64;
    let mut steps = None;
    let mut bounds_policy = peekaboox_input::MoveBoundsPolicy::Allow;
    let mut backend = peekaboox_input::InputToolSelection::Auto;
    let mut restore = false;
    let mut dry_run = false;
    let mut json = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--from" => {
                let value = parse_next_string(&args, &mut index, "--from")?;
                from = Some(parse_point("--from", &value)?);
            }
            "--to" => {
                let value = parse_next_string(&args, &mut index, "--to")?;
                to = Some(parse_point("--to", &value)?);
            }
            "--from-x" => {
                let value = parse_next_string(&args, &mut index, "--from-x")?;
                from_x = Some(parse_i32("--from-x", &value)?);
            }
            "--from-y" => {
                let value = parse_next_string(&args, &mut index, "--from-y")?;
                from_y = Some(parse_i32("--from-y", &value)?);
            }
            "--to-x" => {
                let value = parse_next_string(&args, &mut index, "--to-x")?;
                to_x = Some(parse_i32("--to-x", &value)?);
            }
            "--to-y" => {
                let value = parse_next_string(&args, &mut index, "--to-y")?;
                to_y = Some(parse_i32("--to-y", &value)?);
            }
            "--from-current" => from_current = true,
            "--from-ratio" => {
                let value = parse_next_string(&args, &mut index, "--from-ratio")?;
                from_ratio = Some(parse_ratio_pair("--from-ratio", &value)?);
            }
            "--to-ratio" => {
                let value = parse_next_string(&args, &mut index, "--to-ratio")?;
                to_ratio = Some(parse_ratio_pair("--to-ratio", &value)?);
            }
            "--region" | "-r" => {
                let value = parse_next_string(&args, &mut index, "--region")?;
                region = Some(parse_rect("--region", &value)?);
            }
            "--window-id" => {
                window_id = Some(parse_next_string(&args, &mut index, "--window-id")?);
            }
            "--app" | "-a" => app = Some(parse_next_string(&args, &mut index, "--app")?),
            "--window-title" => {
                window_title = Some(parse_next_string(&args, &mut index, "--window-title")?)
            }
            "--title-regex" => {
                title_regex = Some(parse_next_string(&args, &mut index, "--title-regex")?)
            }
            "--button" | "-b" => {
                let value = parse_next_string(&args, &mut index, "--button")?;
                button = parse_mouse_button(&value)?;
            }
            "--duration-ms" => {
                let value = parse_next_string(&args, &mut index, "--duration-ms")?;
                duration_ms = parse_u64("--duration-ms", &value)?;
            }
            "--steps" => {
                let value = parse_next_string(&args, &mut index, "--steps")?;
                steps = Some(parse_positive_u32("--steps", &value)?);
            }
            "--bounds" => {
                let value = parse_next_string(&args, &mut index, "--bounds")?;
                bounds_policy = parse_move_bounds_policy(&value)?;
            }
            "--clamp" => bounds_policy = peekaboox_input::MoveBoundsPolicy::Clamp,
            "--fail-out-of-bounds" => bounds_policy = peekaboox_input::MoveBoundsPolicy::Fail,
            "--backend" => {
                let value = parse_next_string(&args, &mut index, "--backend")?;
                backend = parse_drag_backend_selection(&value)?;
            }
            "--restore" => restore = true,
            "--dry-run" => dry_run = true,
            "--json" => json = true,
            "--help" | "-h" => return Ok(DragCommand::Help),
            unknown => {
                return Err(CliError::Failure(format!(
                    "unknown drag argument: {unknown}"
                )));
            }
        }

        index += 1;
    }

    let from_point = merge_optional_drag_point("--from", from, from_x, from_y)?;
    let to_point = merge_optional_drag_point("--to", to, to_x, to_y)?;
    let scope = DragScopeParts {
        region,
        window_id,
        app,
        window_title,
        title_regex,
    };

    if scope.has_scope() && from_ratio.is_none() && to_ratio.is_none() {
        return Err(CliError::Failure(
            "region/window drag scope requires --from-ratio or --to-ratio".to_owned(),
        ));
    }

    let from =
        drag_endpoint_from_parts("from", from_point, from_current, from_ratio, scope.clone())?;
    let to = drag_endpoint_from_parts("to", to_point, false, to_ratio, scope)?;

    Ok(DragCommand::Run(Box::new(DragArgs {
        from,
        to,
        button,
        duration_ms,
        steps,
        bounds_policy,
        backend,
        restore,
        dry_run,
        json,
    })))
}

pub(super) fn drag_endpoint_from_parts(
    name: &str,
    point: Option<Point>,
    current: bool,
    ratio: Option<(f32, f32)>,
    scope: DragScopeParts,
) -> Result<DragEndpoint, CliError> {
    let count = usize::from(point.is_some()) + usize::from(current) + usize::from(ratio.is_some());
    if count != 1 {
        return Err(CliError::Failure(format!(
            "provide exactly one {name} endpoint"
        )));
    }
    if let Some(point) = point {
        return Ok(DragEndpoint::Position(point));
    }
    if current {
        return Ok(DragEndpoint::CurrentPosition);
    }
    Ok(DragEndpoint::ScopeRatio {
        ratio: ratio.expect("count checked ratio endpoint"),
        region: scope.region,
        window_id: scope.window_id,
        app: scope.app,
        window_title: scope.window_title,
        title_regex: scope.title_regex,
    })
}

pub(super) fn merge_optional_drag_point(
    name: &str,
    point: Option<Point>,
    x: Option<i32>,
    y: Option<i32>,
) -> Result<Option<Point>, CliError> {
    match (point, x, y) {
        (Some(point), None, None) => Ok(Some(point)),
        (None, Some(x), Some(y)) => Ok(Some(Point::new(x, y))),
        (Some(_), Some(_), _) | (Some(_), _, Some(_)) => Err(CliError::Failure(format!(
            "provide either {name} or {name}-x/{name}-y, not both"
        ))),
        (None, None, None) => Ok(None),
        (None, Some(_), None) => Err(CliError::Failure(format!("missing required {name}-y"))),
        (None, None, Some(_)) => Err(CliError::Failure(format!("missing required {name}-x"))),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TypeArgs {
    pub(super) source: TypeTextSource,
    pub(super) dry_run: bool,
    pub(super) paste: bool,
    pub(super) preserve_clipboard: bool,
    pub(super) json: bool,
    pub(super) typing_speed_chars_per_second: Option<u32>,
    pub(super) delay_ms: Option<u64>,
    pub(super) key_delay_ms: Option<u64>,
    pub(super) backend: peekaboox_input::InputToolSelection,
    pub(super) clipboard_backend: peekaboox_input::ClipboardBackendSelection,
    pub(super) hotkey_backend: peekaboox_input::PasteHotkeyBackendSelection,
    pub(super) restore_delay_ms: Option<u64>,
    pub(super) restore_policy: peekaboox_input::ClipboardRestorePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PasteArgs {
    pub(super) source: TypeTextSource,
    pub(super) dry_run: bool,
    pub(super) preserve_clipboard: bool,
    pub(super) json: bool,
    pub(super) clipboard_backend: peekaboox_input::ClipboardBackendSelection,
    pub(super) hotkey_backend: peekaboox_input::PasteHotkeyBackendSelection,
    pub(super) delay_ms: Option<u64>,
    pub(super) restore_delay_ms: Option<u64>,
    pub(super) restore_policy: peekaboox_input::ClipboardRestorePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TypeTextSource {
    Arguments(Vec<String>),
    Text(String),
    Stdin,
    File(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TypeCommand {
    Run(TypeArgs),
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PasteCommand {
    Run(PasteArgs),
    Help,
}

pub(super) fn type_text(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let TypeCommand::Run(args) = parse_type_args(args)? else {
        print_type_usage();
        return Err(CliError::HelpRequested);
    };

    run_text_input(args, context)
}

pub(super) fn paste_text(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let PasteCommand::Run(args) = parse_paste_args(args)? else {
        print_paste_usage();
        return Err(CliError::HelpRequested);
    };

    run_paste_input(args, context)
}

pub(super) fn run_text_input(args: TypeArgs, context: &CliContext) -> Result<(), CliError> {
    validate_text_input_args(&args)?;
    let text = read_type_text_source(&args.source)?;
    if text.is_empty() {
        return Err(CliError::Failure("missing text to type".to_owned()));
    }

    let action = if args.paste {
        peekaboox_input::InputAction::PasteText {
            text: text.clone(),
            preserve_clipboard: args.preserve_clipboard,
        }
    } else {
        peekaboox_input::InputAction::TypeText(text.clone())
    };

    if context.use_daemon {
        let result = daemon_request(
            context,
            if args.paste {
                ApiRequest::PasteText {
                    text: text.clone(),
                    preserve_clipboard: args.preserve_clipboard,
                    dry_run: args.dry_run,
                    clipboard_backend: args.clipboard_backend.name().to_owned(),
                    hotkey_backend: args.hotkey_backend.name().to_owned(),
                    delay_ms: args.delay_ms,
                    restore_delay_ms: args.restore_delay_ms,
                    restore_policy: args.restore_policy.name().to_owned(),
                }
            } else {
                ApiRequest::TypeText {
                    text: text.clone(),
                    dry_run: args.dry_run,
                    typing_speed_chars_per_second: args.typing_speed_chars_per_second,
                    delay_ms: args.delay_ms,
                    key_delay_ms: args.key_delay_ms,
                    backend: args.backend.name().to_owned(),
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
        print_type_result(&args, &text, metadata);
        return Ok(());
    }

    if args.dry_run {
        let backend = if args.paste {
            let paste_backend = peekaboox_input::CommandInputBackend
                .detect_paste_backend_for_options(paste_options_from_type_args(&args))
                .map_err(|error| CliError::Failure(error.to_string()))?;
            ActionResultDto {
                backend_name: paste_backend.name(),
                backend_kind: format!("{:?}", paste_backend.backend_kind()).to_ascii_lowercase(),
            }
        } else {
            let input_backend = peekaboox_input::CommandInputBackend
                .detect_backend_for_with_selection(&action, args.backend)
                .map_err(|error| CliError::Failure(error.to_string()))?;
            ActionResultDto {
                backend_name: input_backend.name().to_owned(),
                backend_kind: format!("{:?}", input_backend.backend_kind()).to_ascii_lowercase(),
            }
        };
        print_type_result(&args, &text, backend);
        return Ok(());
    }

    let metadata = if args.paste {
        peekaboox_input::paste_text_with_options(text.clone(), paste_options_from_type_args(&args))
    } else {
        peekaboox_input::type_text_with_options(text.clone(), type_options_from_args(&args))
    }
    .map_err(|error| CliError::Failure(error.to_string()))?;
    print_type_result(
        &args,
        &text,
        ActionResultDto {
            backend_name: metadata.backend_name,
            backend_kind: format!("{:?}", metadata.backend_kind).to_ascii_lowercase(),
        },
    );

    Ok(())
}

pub(super) fn run_paste_input(args: PasteArgs, context: &CliContext) -> Result<(), CliError> {
    let text = read_type_text_source(&args.source)?;
    if text.is_empty() {
        return Err(CliError::Failure("missing text to paste".to_owned()));
    }

    let options = paste_options_from_paste_args(&args);
    if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::PasteText {
                text: text.clone(),
                preserve_clipboard: args.preserve_clipboard,
                dry_run: args.dry_run,
                clipboard_backend: args.clipboard_backend.name().to_owned(),
                hotkey_backend: args.hotkey_backend.name().to_owned(),
                delay_ms: args.delay_ms,
                restore_delay_ms: args.restore_delay_ms,
                restore_policy: args.restore_policy.name().to_owned(),
            },
        )?;
        let ApiResult::PasteText(metadata) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected paste response".to_owned(),
            ));
        };
        print_paste_result(&args, &text, metadata);
        return Ok(());
    }

    if args.dry_run {
        let backend = peekaboox_input::CommandInputBackend
            .detect_paste_backend_for_options(options)
            .map_err(|error| CliError::Failure(error.to_string()))?;
        print_paste_result(
            &args,
            &text,
            ActionResultDto {
                backend_name: backend.name(),
                backend_kind: format!("{:?}", backend.backend_kind()).to_ascii_lowercase(),
            },
        );
        return Ok(());
    }

    let metadata = peekaboox_input::paste_text_with_options(text.clone(), options)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    print_paste_result(
        &args,
        &text,
        ActionResultDto {
            backend_name: metadata.backend_name,
            backend_kind: format!("{:?}", metadata.backend_kind).to_ascii_lowercase(),
        },
    );
    Ok(())
}

pub(super) fn type_options_from_args(args: &TypeArgs) -> peekaboox_input::TypeTextOptions {
    peekaboox_input::TypeTextOptions {
        typing_speed_chars_per_second: args.typing_speed_chars_per_second,
        delay_ms: args.delay_ms,
        key_delay_ms: args.key_delay_ms,
        backend: args.backend,
    }
}

pub(super) fn paste_options_from_type_args(args: &TypeArgs) -> peekaboox_input::PasteTextOptions {
    peekaboox_input::PasteTextOptions {
        preserve_clipboard: args.preserve_clipboard,
        clipboard_backend: args.clipboard_backend,
        hotkey_backend: args.hotkey_backend,
        delay_ms: args.delay_ms.unwrap_or(80),
        restore_delay_ms: args.restore_delay_ms.unwrap_or(120),
        restore_policy: args.restore_policy,
    }
}

pub(super) fn paste_options_from_paste_args(args: &PasteArgs) -> peekaboox_input::PasteTextOptions {
    peekaboox_input::PasteTextOptions {
        preserve_clipboard: args.preserve_clipboard,
        clipboard_backend: args.clipboard_backend,
        hotkey_backend: args.hotkey_backend,
        delay_ms: args.delay_ms.unwrap_or(80),
        restore_delay_ms: args.restore_delay_ms.unwrap_or(120),
        restore_policy: args.restore_policy,
    }
}

pub(super) fn validate_text_input_args(args: &TypeArgs) -> Result<(), CliError> {
    if args.paste {
        if args.typing_speed_chars_per_second.is_some()
            || args.key_delay_ms.is_some()
            || args.backend != peekaboox_input::InputToolSelection::Auto
        {
            return Err(CliError::Failure(
                "paste does not support type speed, key-delay, or type backend options".to_owned(),
            ));
        }
        return Ok(());
    }

    if args.preserve_clipboard {
        return Err(CliError::Failure(
            "--preserve-clipboard requires --paste or the paste command".to_owned(),
        ));
    }
    if args.clipboard_backend != peekaboox_input::ClipboardBackendSelection::Auto
        || args.hotkey_backend != peekaboox_input::PasteHotkeyBackendSelection::Auto
        || args.restore_delay_ms.is_some()
        || args.restore_policy != peekaboox_input::ClipboardRestorePolicy::Strict
    {
        return Err(CliError::Failure(
            "paste backend and clipboard restore options require --paste or the paste command"
                .to_owned(),
        ));
    }
    if args.typing_speed_chars_per_second.is_some() && args.key_delay_ms.is_some() {
        return Err(CliError::Failure(
            "--typing-speed cannot be combined with --key-delay-ms".to_owned(),
        ));
    }
    Ok(())
}

pub(super) fn read_type_text_source(source: &TypeTextSource) -> Result<String, CliError> {
    match source {
        TypeTextSource::Arguments(parts) => Ok(parts.join(" ")),
        TypeTextSource::Text(text) => Ok(text.clone()),
        TypeTextSource::Stdin => {
            let mut text = String::new();
            std::io::stdin()
                .read_to_string(&mut text)
                .map_err(|error| CliError::Failure(format!("failed to read stdin: {error}")))?;
            Ok(text)
        }
        TypeTextSource::File(path) => std::fs::read_to_string(path).map_err(|error| {
            CliError::Failure(format!("failed to read {}: {error}", path.display()))
        }),
    }
}

pub(super) fn print_type_result(args: &TypeArgs, text: &str, metadata: ActionResultDto) {
    if args.json {
        let _ = print_json_pretty(&serde_json::json!({
            "ok": true,
            "dry_run": args.dry_run,
            "action": if args.paste { "paste_text" } else { "type_text" },
            "text_length": text.chars().count(),
            "backend_name": metadata.backend_name,
            "backend_kind": metadata.backend_kind,
            "requested_backend": if args.paste { None } else { Some(args.backend.name()) },
            "requested_clipboard_backend": if args.paste { Some(args.clipboard_backend.name()) } else { None },
            "requested_hotkey_backend": if args.paste { Some(args.hotkey_backend.name()) } else { None },
            "typing_speed_chars_per_second": args.typing_speed_chars_per_second,
            "delay_ms": args.delay_ms,
            "key_delay_ms": args.key_delay_ms,
            "preserve_clipboard": args.preserve_clipboard,
            "restore_delay_ms": if args.paste { args.restore_delay_ms } else { None },
            "restore_policy": if args.paste { Some(args.restore_policy.name()) } else { None },
        }));
        return;
    }

    match (args.dry_run, args.paste) {
        (true, true) => println!("would paste via {}", metadata.backend_name),
        (true, false) => println!("would type via {}", metadata.backend_name),
        (false, true) => println!("pasted text via {}", metadata.backend_name),
        (false, false) => println!("typed text via {}", metadata.backend_name),
    }
}

pub(super) fn print_paste_result(args: &PasteArgs, text: &str, metadata: ActionResultDto) {
    if args.json {
        let _ = print_json_pretty(&serde_json::json!({
            "ok": true,
            "dry_run": args.dry_run,
            "action": "paste_text",
            "text_length": text.chars().count(),
            "backend_name": metadata.backend_name,
            "backend_kind": metadata.backend_kind,
            "requested_clipboard_backend": args.clipboard_backend.name(),
            "requested_hotkey_backend": args.hotkey_backend.name(),
            "delay_ms": args.delay_ms,
            "preserve_clipboard": args.preserve_clipboard,
            "restore_delay_ms": args.restore_delay_ms,
            "restore_policy": args.restore_policy.name(),
        }));
        return;
    }

    if args.dry_run {
        println!("would paste via {}", metadata.backend_name);
    } else {
        println!("pasted text via {}", metadata.backend_name);
    }
}

pub(super) fn parse_type_args(args: Vec<String>) -> Result<TypeCommand, CliError> {
    let mut dry_run = false;
    let mut paste = false;
    let mut preserve_clipboard = false;
    let mut json = false;
    let mut typing_speed_chars_per_second = None;
    let mut delay_ms = None;
    let mut key_delay_ms = None;
    let mut backend = peekaboox_input::InputToolSelection::Auto;
    let mut clipboard_backend = peekaboox_input::ClipboardBackendSelection::Auto;
    let mut hotkey_backend = peekaboox_input::PasteHotkeyBackendSelection::Auto;
    let mut restore_delay_ms = None;
    let mut restore_policy = peekaboox_input::ClipboardRestorePolicy::Strict;
    let mut source = None;
    let mut text_parts = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--dry-run" => dry_run = true,
            "--paste" => paste = true,
            "--preserve-clipboard" => preserve_clipboard = true,
            "--json" => json = true,
            "--text" | "-t" => {
                let value = parse_next_string(&args, &mut index, "--text")?;
                set_type_source(&mut source, TypeTextSource::Text(value), "--text")?;
            }
            "--stdin" => set_type_source(&mut source, TypeTextSource::Stdin, "--stdin")?,
            "--file" => {
                let value = parse_next_string(&args, &mut index, "--file")?;
                set_type_source(
                    &mut source,
                    TypeTextSource::File(PathBuf::from(value)),
                    "--file",
                )?;
            }
            "--typing-speed" | "--typing-speed-cps" => {
                let value = parse_next_string(&args, &mut index, "--typing-speed")?;
                typing_speed_chars_per_second = Some(parse_positive_u32("--typing-speed", &value)?);
            }
            "--delay-ms" => {
                let value = parse_next_string(&args, &mut index, "--delay-ms")?;
                delay_ms = Some(parse_u64("--delay-ms", &value)?);
            }
            "--key-delay-ms" => {
                let value = parse_next_string(&args, &mut index, "--key-delay-ms")?;
                key_delay_ms = Some(parse_u64("--key-delay-ms", &value)?);
            }
            "--backend" => {
                let value = parse_next_string(&args, &mut index, "--backend")?;
                backend = parse_type_backend_selection(&value)?;
            }
            "--clipboard-backend" => {
                let value = parse_next_string(&args, &mut index, "--clipboard-backend")?;
                clipboard_backend = parse_clipboard_backend_selection(&value)?;
            }
            "--hotkey-backend" => {
                let value = parse_next_string(&args, &mut index, "--hotkey-backend")?;
                hotkey_backend = parse_paste_hotkey_backend_selection(&value)?;
            }
            "--restore-delay-ms" => {
                let value = parse_next_string(&args, &mut index, "--restore-delay-ms")?;
                restore_delay_ms = Some(parse_u64("--restore-delay-ms", &value)?);
            }
            "--restore-policy" => {
                let value = parse_next_string(&args, &mut index, "--restore-policy")?;
                restore_policy = parse_clipboard_restore_policy(&value)?;
            }
            "--help" | "-h" => return Ok(TypeCommand::Help),
            "--" => {
                text_parts.extend(args.iter().skip(index + 1).cloned());
                break;
            }
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown type argument: {value}; use -- before text that starts with '-'"
                )));
            }
            value => text_parts.push(value.to_owned()),
        }

        index += 1;
    }

    if !text_parts.is_empty() {
        set_type_source(
            &mut source,
            TypeTextSource::Arguments(text_parts),
            "positional text",
        )?;
    }

    let Some(source) = source else {
        return Err(CliError::Failure("missing text to type".to_owned()));
    };

    Ok(TypeCommand::Run(TypeArgs {
        source,
        dry_run,
        paste,
        preserve_clipboard,
        json,
        typing_speed_chars_per_second,
        delay_ms,
        key_delay_ms,
        backend,
        clipboard_backend,
        hotkey_backend,
        restore_delay_ms,
        restore_policy,
    }))
}

pub(super) fn set_type_source(
    source: &mut Option<TypeTextSource>,
    value: TypeTextSource,
    name: &str,
) -> Result<(), CliError> {
    if source.is_some() {
        return Err(CliError::Failure(format!(
            "{name} cannot be combined with another text source"
        )));
    }
    *source = Some(value);
    Ok(())
}

pub(super) fn parse_paste_args(args: Vec<String>) -> Result<PasteCommand, CliError> {
    let mut dry_run = false;
    let mut preserve_clipboard = false;
    let mut json = false;
    let mut clipboard_backend = peekaboox_input::ClipboardBackendSelection::Auto;
    let mut hotkey_backend = peekaboox_input::PasteHotkeyBackendSelection::Auto;
    let mut delay_ms = None;
    let mut restore_delay_ms = None;
    let mut restore_policy = peekaboox_input::ClipboardRestorePolicy::Strict;
    let mut source = None;
    let mut text_parts = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--dry-run" => dry_run = true,
            "--preserve-clipboard" => preserve_clipboard = true,
            "--json" => json = true,
            "--text" | "-t" => {
                let value = parse_next_string(&args, &mut index, "--text")?;
                set_type_source(&mut source, TypeTextSource::Text(value), "--text")?;
            }
            "--stdin" => set_type_source(&mut source, TypeTextSource::Stdin, "--stdin")?,
            "--file" => {
                let value = parse_next_string(&args, &mut index, "--file")?;
                set_type_source(
                    &mut source,
                    TypeTextSource::File(PathBuf::from(value)),
                    "--file",
                )?;
            }
            "--clipboard-backend" => {
                let value = parse_next_string(&args, &mut index, "--clipboard-backend")?;
                clipboard_backend = parse_clipboard_backend_selection(&value)?;
            }
            "--hotkey-backend" => {
                let value = parse_next_string(&args, &mut index, "--hotkey-backend")?;
                hotkey_backend = parse_paste_hotkey_backend_selection(&value)?;
            }
            "--delay-ms" => {
                let value = parse_next_string(&args, &mut index, "--delay-ms")?;
                delay_ms = Some(parse_u64("--delay-ms", &value)?);
            }
            "--restore-delay-ms" => {
                let value = parse_next_string(&args, &mut index, "--restore-delay-ms")?;
                restore_delay_ms = Some(parse_u64("--restore-delay-ms", &value)?);
            }
            "--restore-policy" => {
                let value = parse_next_string(&args, &mut index, "--restore-policy")?;
                restore_policy = parse_clipboard_restore_policy(&value)?;
            }
            "--help" | "-h" => return Ok(PasteCommand::Help),
            "--" => {
                text_parts.extend(args.iter().skip(index + 1).cloned());
                break;
            }
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown paste argument: {value}; use -- before text that starts with '-'"
                )));
            }
            value => text_parts.push(value.to_owned()),
        }

        index += 1;
    }

    if !text_parts.is_empty() {
        set_type_source(
            &mut source,
            TypeTextSource::Arguments(text_parts),
            "positional text",
        )?;
    }

    let Some(source) = source else {
        return Err(CliError::Failure("missing text to paste".to_owned()));
    };

    Ok(PasteCommand::Run(PasteArgs {
        source,
        dry_run,
        preserve_clipboard,
        json,
        clipboard_backend,
        hotkey_backend,
        delay_ms,
        restore_delay_ms,
        restore_policy,
    }))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct HotkeyArgs {
    pub(super) keys: Vec<String>,
    pub(super) dry_run: bool,
    pub(super) json: bool,
    pub(super) backend: peekaboox_input::InputToolSelection,
    pub(super) delay_ms: Option<u64>,
    pub(super) key_delay_ms: Option<u64>,
    pub(super) repeat: Option<u32>,
    pub(super) interval_ms: Option<u64>,
    pub(super) release_before: bool,
    pub(super) release_after: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum HotkeyCommand {
    Run(HotkeyArgs),
    Help,
}

pub(super) fn hotkey(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let HotkeyCommand::Run(args) = parse_hotkey_args(args)? else {
        print_hotkey_usage();
        return Err(CliError::HelpRequested);
    };

    let options = hotkey_options_from_args(&args);
    let keys = peekaboox_input::normalize_hotkey_keys(&args.keys)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    let action = peekaboox_input::InputAction::Hotkey(keys.clone());

    if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::Hotkey {
                keys: keys.clone(),
                dry_run: args.dry_run,
                backend: args.backend.name().to_owned(),
                delay_ms: args.delay_ms,
                key_delay_ms: args.key_delay_ms,
                repeat: args.repeat,
                interval_ms: args.interval_ms,
                release_before: args.release_before,
                release_after: args.release_after,
            },
        )?;
        let ApiResult::Hotkey(metadata) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected hotkey response".to_owned(),
            ));
        };
        print_hotkey_result(&args, &keys, metadata);
        return Ok(());
    }

    if args.dry_run {
        let backend = peekaboox_input::CommandInputBackend
            .detect_backend_for_with_selection(&action, args.backend)
            .map_err(|error| CliError::Failure(error.to_string()))?;
        print_hotkey_result(
            &args,
            &keys,
            ActionResultDto {
                backend_name: backend.name().to_owned(),
                backend_kind: format!("{:?}", backend.backend_kind()).to_ascii_lowercase(),
            },
        );
        return Ok(());
    }

    let metadata = peekaboox_input::hotkey_with_options(keys.clone(), options)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    print_hotkey_result(&args, &keys, input_metadata_dto(metadata));

    Ok(())
}

pub(super) fn hotkey_options_from_args(args: &HotkeyArgs) -> peekaboox_input::HotkeyOptions {
    peekaboox_input::HotkeyOptions {
        backend: args.backend,
        delay_ms: args.delay_ms,
        key_delay_ms: args.key_delay_ms,
        repeat: args.repeat.unwrap_or(1),
        interval_ms: args.interval_ms.unwrap_or(0),
        release_before: args.release_before,
        release_after: args.release_after,
    }
}

pub(super) fn print_hotkey_result(args: &HotkeyArgs, keys: &[String], metadata: ActionResultDto) {
    if args.json {
        let _ = print_json_pretty(&serde_json::json!({
            "ok": true,
            "dry_run": args.dry_run,
            "action": "hotkey",
            "keys": keys,
            "key_count": keys.len(),
            "backend_name": metadata.backend_name,
            "backend_kind": metadata.backend_kind,
            "requested_backend": args.backend.name(),
            "delay_ms": args.delay_ms,
            "key_delay_ms": args.key_delay_ms,
            "repeat": args.repeat,
            "interval_ms": args.interval_ms,
            "release_before": args.release_before,
            "release_after": args.release_after,
        }));
        return;
    }

    if args.dry_run {
        println!(
            "would press hotkey {} via {}",
            keys.join("+"),
            metadata.backend_name
        );
    } else {
        println!(
            "pressed hotkey {} via {}",
            keys.join("+"),
            metadata.backend_name
        );
    }
}

pub(super) fn parse_hotkey_args(args: Vec<String>) -> Result<HotkeyCommand, CliError> {
    let mut keys = Vec::new();
    let mut dry_run = false;
    let mut json = false;
    let mut backend = peekaboox_input::InputToolSelection::Auto;
    let mut delay_ms = None;
    let mut key_delay_ms = None;
    let mut repeat = None;
    let mut interval_ms = None;
    let mut release_before = false;
    let mut release_after = false;
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
            "--json" => json = true,
            "--backend" => {
                let value = parse_next_string(&args, &mut index, "--backend")?;
                backend = parse_hotkey_backend_selection(&value)?;
            }
            "--delay-ms" => {
                let value = parse_next_string(&args, &mut index, "--delay-ms")?;
                delay_ms = Some(parse_u64("--delay-ms", &value)?);
            }
            "--key-delay-ms" => {
                let value = parse_next_string(&args, &mut index, "--key-delay-ms")?;
                key_delay_ms = Some(parse_u64("--key-delay-ms", &value)?);
            }
            "--repeat" => {
                let value = parse_next_string(&args, &mut index, "--repeat")?;
                repeat = Some(parse_positive_u32("--repeat", &value)?);
            }
            "--interval-ms" => {
                let value = parse_next_string(&args, &mut index, "--interval-ms")?;
                interval_ms = Some(parse_u64("--interval-ms", &value)?);
            }
            "--release-before" => release_before = true,
            "--release-after" => release_after = true,
            "--help" | "-h" => return Ok(HotkeyCommand::Help),
            "--" => {
                keys.extend(args.iter().skip(index + 1).cloned());
                break;
            }
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown hotkey argument: {value}; use -- before key names that start with '-'"
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

    peekaboox_input::normalize_hotkey_keys(&keys)
        .map_err(|error| CliError::Failure(error.to_string()))?;

    Ok(HotkeyCommand::Run(HotkeyArgs {
        keys,
        dry_run,
        json,
        backend,
        delay_ms,
        key_delay_ms,
        repeat,
        interval_ms,
        release_before,
        release_after,
    }))
}
