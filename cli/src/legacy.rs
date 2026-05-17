use super::*;

#[derive(Debug, Clone)]
pub(super) struct SeeArgs {
    pub(super) capture: CaptureArgs,
    pub(super) id: Option<String>,
    pub(super) output_dir: Option<PathBuf>,
    pub(super) annotate: bool,
    pub(super) include_elements: bool,
    pub(super) json: bool,
}

pub(super) fn see(args: Vec<String>, _context: &CliContext) -> Result<(), CliError> {
    let args = parse_see_args(args)?;
    let snapshot = create_snapshot(&args)?;
    if args.json {
        print_json_pretty(&snapshot)?;
    } else {
        println!(
            "snapshot {} image={} metadata={}",
            snapshot["id"].as_str().unwrap_or("-"),
            snapshot["image_path"].as_str().unwrap_or("-"),
            snapshot["metadata_path"].as_str().unwrap_or("-")
        );
        if let Some(path) = snapshot["annotated_path"].as_str() {
            println!("annotated={path}");
        }
    }
    Ok(())
}

pub(super) fn parse_see_args(args: Vec<String>) -> Result<SeeArgs, CliError> {
    let mut capture_args = Vec::new();
    let mut id = None;
    let mut output_dir = None;
    let mut annotate = false;
    let mut include_elements = true;
    let mut json = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--id" | "--snapshot-id" => id = Some(parse_next_string(&args, &mut index, "--id")?),
            "--output-dir" | "--dir" => {
                output_dir = Some(PathBuf::from(parse_next_string(
                    &args,
                    &mut index,
                    "--output-dir",
                )?));
            }
            "--annotate" | "--overlay" => annotate = true,
            "--no-elements" | "--no-semantic-tree" => include_elements = false,
            "--json" => {
                json = true;
                capture_args.push("--json".to_owned());
            }
            "--help" | "-h" => {
                print_see_usage();
                return Err(CliError::HelpRequested);
            }
            other => capture_args.push(other.to_owned()),
        }
        index += 1;
    }

    capture_args.push("--json".to_owned());
    if include_elements {
        capture_args.push("--include-semantic-tree".to_owned());
    }
    let mut capture = match parse_capture_args(capture_args)? {
        CaptureCommand::Run(args) => args,
        CaptureCommand::Help => {
            print_see_usage();
            return Err(CliError::HelpRequested);
        }
    };
    capture.include_semantic_tree = include_elements;
    capture.stdout = false;
    capture.json = true;
    Ok(SeeArgs {
        capture,
        id,
        output_dir,
        annotate,
        include_elements,
        json,
    })
}

pub(super) fn create_snapshot(args: &SeeArgs) -> Result<serde_json::Value, CliError> {
    let snapshot_id = args
        .id
        .clone()
        .unwrap_or_else(|| format!("snapshot-{}-{}", std::process::id(), unix_time_ms_u64()));
    let dir = args
        .output_dir
        .clone()
        .unwrap_or_else(|| state_dir().join("snapshots").join(&snapshot_id));
    std::fs::create_dir_all(&dir).map_err(|error| {
        CliError::Failure(format!("failed to create {}: {error}", dir.display()))
    })?;
    let image_path = dir.join(match args.capture.format {
        CaptureOutputFormat::Png => "screen.png",
        CaptureOutputFormat::Jpeg => "screen.jpg",
        CaptureOutputFormat::Xwd => "screen.xwd",
    });
    let mut capture = args.capture.clone();
    capture.output = image_path.clone();
    if args.capture.no_overwrite {
        ensure_path_absent(&metadata_path_for_dir(&dir), "snapshot metadata")?;
        if args.annotate && capture.format != CaptureOutputFormat::Xwd {
            ensure_path_absent(&dir.join("annotated.png"), "annotated snapshot")?;
        }
    }
    let target = capture_target_from_args(&capture)?;
    let result = capture_cli_execute(&capture, target)?;
    let annotated_path = if args.annotate && capture.format != CaptureOutputFormat::Xwd {
        let overlay = dir.join("annotated.png");
        let options = UiElementDetectionOptions::default();
        let _ = peekaboox_vision::detect_ui_elements_from_image_file_with_outputs(
            &image_path,
            &options,
            None,
            Some(&overlay),
        )
        .map_err(|error| CliError::Failure(error.to_string()))?;
        Some(overlay)
    } else {
        None
    };
    let metadata_path = metadata_path_for_dir(&dir);
    let snapshot = serde_json::json!({
        "id": snapshot_id,
        "schema_version": 1,
        "created_at_unix_ms": unix_time_ms_u64(),
        "image_path": image_path.display().to_string(),
        "annotated_path": annotated_path.as_ref().map(|path| path.display().to_string()),
        "metadata_path": metadata_path.display().to_string(),
        "include_elements": args.include_elements,
        "capture": result.metadata,
    });
    if args.capture.no_overwrite {
        write_json_pretty_file_no_overwrite(&metadata_path, &snapshot)?;
    } else {
        write_json_pretty_file(&metadata_path, &snapshot)?;
    }
    Ok(snapshot)
}

pub(super) fn metadata_path_for_dir(dir: &Path) -> PathBuf {
    dir.join("snapshot.json")
}

pub(super) fn agent_command(args: Vec<String>, _context: &CliContext) -> Result<(), CliError> {
    let mut goal = None;
    let mut positional = Vec::new();
    let mut dry_run = false;
    let mut list_sessions = false;
    let mut resume = None;
    let mut model = None;
    let mut max_steps = 8_u32;
    let mut json = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "run" => {}
            "sessions" | "list-sessions" => list_sessions = true,
            "--goal" | "-g" => goal = Some(parse_next_string(&args, &mut index, "--goal")?),
            "--dry-run" => dry_run = true,
            "--resume" | "--session" => {
                resume = Some(parse_next_string(&args, &mut index, "--resume")?)
            }
            "--list-sessions" => list_sessions = true,
            "--model" => model = Some(parse_next_string(&args, &mut index, "--model")?),
            "--max-steps" => {
                max_steps = parse_positive_u32(
                    "--max-steps",
                    &parse_next_string(&args, &mut index, "--max-steps")?,
                )?
            }
            "--json" => json = true,
            "--help" | "-h" => {
                print_agent_usage();
                return Err(CliError::HelpRequested);
            }
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown agent argument: {value}"
                )));
            }
            value => positional.push(value.to_owned()),
        }
        index += 1;
    }

    if list_sessions {
        return print_agent_sessions(json);
    }
    let goal = goal
        .or_else(|| (!positional.is_empty()).then(|| positional.join(" ")))
        .ok_or_else(|| {
            CliError::Failure("agent requires --goal or positional goal text".to_owned())
        })?;
    let session_id =
        resume.unwrap_or_else(|| format!("agent-{}-{}", std::process::id(), unix_time_ms_u64()));
    let steps = plan_agent_steps(&goal, max_steps);
    let session_path = state_dir()
        .join("agent")
        .join("sessions")
        .join(format!("{session_id}.json"));
    let status = if dry_run { "dry-run" } else { "completed" };
    let mut snapshot = None;
    if !dry_run {
        let snapshot_args = SeeArgs {
            capture: CaptureArgs {
                output: PathBuf::from("screen.png"),
                region: None,
                window_id: None,
                app: None,
                window_title: None,
                title_regex: None,
                format: CaptureOutputFormat::Png,
                jpeg_quality: 90,
                json: true,
                stdout: false,
                no_overwrite: false,
                include_semantic_tree: true,
            },
            id: Some(format!("{session_id}-observe")),
            output_dir: None,
            annotate: false,
            include_elements: true,
            json: true,
        };
        snapshot = Some(create_snapshot(&snapshot_args)?);
    }
    let session = serde_json::json!({
        "id": session_id,
        "goal": goal,
        "status": status,
        "model": model.unwrap_or_else(|| "local-planner".to_owned()),
        "max_steps": max_steps,
        "steps": steps,
        "snapshot": snapshot,
        "updated_at_unix_ms": unix_time_ms_u64(),
    });
    write_json_pretty_file(&session_path, &session)?;
    if json {
        print_json_pretty(&session)?;
    } else {
        println!(
            "agent session {} status={} path={}",
            session["id"].as_str().unwrap_or("-"),
            status,
            session_path.display()
        );
    }
    Ok(())
}

pub(super) fn plan_agent_steps(goal: &str, max_steps: u32) -> Vec<serde_json::Value> {
    let templates = [
        ("observe", "capture a fresh snapshot and semantic tree"),
        ("inspect", "identify relevant windows and UI elements"),
        ("plan", "select the safest available action path"),
        ("act", "execute one bounded desktop action when permitted"),
        ("verify", "capture/inspect the result before continuing"),
    ];
    templates
        .iter()
        .take(max_steps as usize)
        .enumerate()
        .map(|(index, (name, detail))| {
            serde_json::json!({
                "index": index + 1,
                "name": name,
                "detail": detail,
                "goal": goal,
            })
        })
        .collect()
}

pub(super) fn print_agent_sessions(json: bool) -> Result<(), CliError> {
    let dir = state_dir().join("agent").join("sessions");
    let sessions = read_json_files_in_dir(&dir)?;
    if json {
        return print_json_pretty(&serde_json::json!({ "sessions": sessions }));
    }
    if sessions.is_empty() {
        println!("no agent sessions");
        return Ok(());
    }
    for session in sessions {
        println!(
            "{} status={} goal={}",
            session["id"].as_str().unwrap_or("-"),
            session["status"].as_str().unwrap_or("-"),
            session["goal"].as_str().unwrap_or("-")
        );
    }
    Ok(())
}

pub(super) fn set_value_command(args: Vec<String>) -> Result<(), CliError> {
    let mut selector = None;
    let mut value = None;
    let mut dry_run = false;
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--selector" | "-s" => {
                selector = Some(parse_next_string(&args, &mut index, "--selector")?)
            }
            "--id" => {
                selector = Some(format!(
                    "id-exact={}",
                    parse_next_string(&args, &mut index, "--id")?
                ))
            }
            "--value" | "-v" => value = Some(parse_next_string(&args, &mut index, "--value")?),
            "--dry-run" => dry_run = true,
            "--json" => json = true,
            "--help" | "-h" => {
                print_set_value_usage();
                return Err(CliError::HelpRequested);
            }
            value_arg if selector.is_none() => selector = Some(value_arg.to_owned()),
            value_arg if value.is_none() => value = Some(value_arg.to_owned()),
            unknown => {
                return Err(CliError::Failure(format!(
                    "unexpected set-value argument: {unknown}"
                )));
            }
        }
        index += 1;
    }
    let selector = selector.ok_or_else(|| CliError::Failure("missing --selector".to_owned()))?;
    let value = value.ok_or_else(|| CliError::Failure("missing --value".to_owned()))?;
    if dry_run {
        let element = peekaboox_accessibility::find_elements_by_selector(&selector)
            .map_err(|error| CliError::Failure(error.to_string()))?
            .elements
            .into_iter()
            .next()
            .ok_or_else(|| CliError::Failure(format!("no element matched {selector:?}")))?;
        let payload = serde_json::json!({
            "ok": true,
            "dry_run": true,
            "selector": selector,
            "value": value,
            "element": element_dto(element),
        });
        if json {
            print_json_pretty(&payload)?;
        } else {
            println!(
                "would set value on {}",
                payload["element"]["id"].as_str().unwrap_or("-")
            );
        }
        return Ok(());
    }
    let result = peekaboox_accessibility::set_value(&selector, &value)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    let payload = serde_json::json!({
        "ok": result.ok,
        "backend_name": result.backend_name,
        "backend_kind": backend_kind_label(result.backend_kind),
        "strategy": result.strategy,
        "value": result.value,
        "element": element_dto(result.element),
    });
    if json {
        print_json_pretty(&payload)?;
    } else {
        println!(
            "set value via {} using {}",
            payload["backend_name"].as_str().unwrap_or("-"),
            payload["strategy"].as_str().unwrap_or("-")
        );
    }
    Ok(())
}

pub(super) fn perform_action_command(args: Vec<String>) -> Result<(), CliError> {
    let mut selector = None;
    let mut action = None;
    let mut index_value = None;
    let mut dry_run = false;
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--selector" | "-s" => {
                selector = Some(parse_next_string(&args, &mut index, "--selector")?)
            }
            "--id" => {
                selector = Some(format!(
                    "id-exact={}",
                    parse_next_string(&args, &mut index, "--id")?
                ))
            }
            "--action" | "-a" => action = Some(parse_next_string(&args, &mut index, "--action")?),
            "--index" => {
                index_value = Some(parse_i32(
                    "--index",
                    &parse_next_string(&args, &mut index, "--index")?,
                )?)
            }
            "--dry-run" => dry_run = true,
            "--json" => json = true,
            "--help" | "-h" => {
                print_perform_action_usage();
                return Err(CliError::HelpRequested);
            }
            value if selector.is_none() => selector = Some(value.to_owned()),
            value if action.is_none() => action = Some(value.to_owned()),
            unknown => {
                return Err(CliError::Failure(format!(
                    "unexpected perform-action argument: {unknown}"
                )));
            }
        }
        index += 1;
    }
    let selector = selector.ok_or_else(|| CliError::Failure("missing --selector".to_owned()))?;
    if dry_run {
        let element = peekaboox_accessibility::find_elements_by_selector(&selector)
            .map_err(|error| CliError::Failure(error.to_string()))?
            .elements
            .into_iter()
            .next()
            .ok_or_else(|| CliError::Failure(format!("no element matched {selector:?}")))?;
        let payload = serde_json::json!({
            "ok": true,
            "dry_run": true,
            "selector": selector,
            "action": action,
            "index": index_value,
            "element": element_dto(element),
        });
        if json {
            print_json_pretty(&payload)?;
        } else {
            println!(
                "would perform action on {}",
                payload["element"]["id"].as_str().unwrap_or("-")
            );
        }
        return Ok(());
    }
    let result = peekaboox_accessibility::perform_action(&selector, action.as_deref(), index_value)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    let payload = serde_json::json!({
        "ok": result.ok,
        "backend_name": result.backend_name,
        "backend_kind": backend_kind_label(result.backend_kind),
        "action": result.action,
        "action_index": result.action_index,
        "element": element_dto(result.element),
    });
    if json {
        print_json_pretty(&payload)?;
    } else {
        println!(
            "performed action {} via {}",
            payload["action"].as_str().unwrap_or("-"),
            payload["backend_name"].as_str().unwrap_or("-")
        );
    }
    Ok(())
}

pub(super) fn press(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let mut rewritten = vec!["--release-before".to_owned(), "--release-after".to_owned()];
    rewritten.extend(args);
    hotkey(rewritten, context)
}

pub(super) fn swipe(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let mut rewritten = Vec::new();
    let mut has_duration = false;
    for arg in &args {
        if arg == "--duration-ms" {
            has_duration = true;
        }
    }
    rewritten.extend(args);
    if !has_duration {
        rewritten.push("--duration-ms".to_owned());
        rewritten.push("350".to_owned());
    }
    drag(rewritten, context)
}

pub(super) fn scroll(args: Vec<String>) -> Result<(), CliError> {
    let mut direction = peekaboox_input::ScrollDirection::Down;
    let mut amount = 3_u32;
    let mut position = None;
    let mut dry_run = false;
    let mut json = false;
    let mut delay_ms = 20_u64;
    let mut bounds_policy = peekaboox_input::MoveBoundsPolicy::Allow;
    let mut backend = peekaboox_input::InputToolSelection::Auto;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--direction" | "-d" => {
                direction =
                    parse_scroll_direction(&parse_next_string(&args, &mut index, "--direction")?)?
            }
            "--amount" | "-n" => {
                amount = parse_positive_u32(
                    "--amount",
                    &parse_next_string(&args, &mut index, "--amount")?,
                )?
            }
            "--at" | "--position" | "--to" => {
                position = Some(parse_point(
                    "--at",
                    &parse_next_string(&args, &mut index, "--at")?,
                )?)
            }
            "--delay-ms" => {
                delay_ms = parse_u64(
                    "--delay-ms",
                    &parse_next_string(&args, &mut index, "--delay-ms")?,
                )?
            }
            "--bounds" => {
                bounds_policy =
                    parse_move_bounds_policy(&parse_next_string(&args, &mut index, "--bounds")?)?
            }
            "--backend" => {
                backend = parse_hotkey_backend_selection(&parse_next_string(
                    &args,
                    &mut index,
                    "--backend",
                )?)?
            }
            "--dry-run" => dry_run = true,
            "--json" => json = true,
            "--help" | "-h" => {
                print_scroll_usage();
                return Err(CliError::HelpRequested);
            }
            "up" => direction = peekaboox_input::ScrollDirection::Up,
            "down" => direction = peekaboox_input::ScrollDirection::Down,
            "left" => direction = peekaboox_input::ScrollDirection::Left,
            "right" => direction = peekaboox_input::ScrollDirection::Right,
            value => {
                return Err(CliError::Failure(format!(
                    "unknown scroll argument: {value}"
                )));
            }
        }
        index += 1;
    }
    let action = peekaboox_input::InputAction::Scroll {
        direction,
        amount,
        position,
    };
    let metadata = if dry_run {
        let backend = peekaboox_input::CommandInputBackend
            .detect_backend_for_with_selection(&action, backend)
            .map_err(|error| CliError::Failure(error.to_string()))?;
        ActionResultDto {
            backend_name: backend.name().to_owned(),
            backend_kind: format!("{:?}", backend.backend_kind()).to_ascii_lowercase(),
        }
    } else {
        input_metadata_dto(
            peekaboox_input::scroll_with_options(
                direction,
                amount,
                position,
                peekaboox_input::ScrollOptions {
                    backend,
                    bounds_policy,
                    delay_ms,
                },
            )
            .map_err(|error| CliError::Failure(error.to_string()))?,
        )
    };
    if json {
        print_json_pretty(&serde_json::json!({
            "ok": true,
            "dry_run": dry_run,
            "direction": direction.name(),
            "amount": amount,
            "position": position.map(|point| serde_json::json!({"x": point.x, "y": point.y})),
            "backend_name": metadata.backend_name,
            "backend_kind": metadata.backend_kind,
        }))?;
    } else if dry_run {
        println!(
            "would scroll {} {} via {}",
            amount,
            direction.name(),
            metadata.backend_name
        );
    } else {
        println!(
            "scrolled {} {} via {}",
            amount,
            direction.name(),
            metadata.backend_name
        );
    }
    Ok(())
}

pub(super) fn parse_scroll_direction(
    value: &str,
) -> Result<peekaboox_input::ScrollDirection, CliError> {
    match value {
        "up" => Ok(peekaboox_input::ScrollDirection::Up),
        "down" => Ok(peekaboox_input::ScrollDirection::Down),
        "left" => Ok(peekaboox_input::ScrollDirection::Left),
        "right" => Ok(peekaboox_input::ScrollDirection::Right),
        _ => Err(CliError::Failure(format!(
            "--direction must be up, down, left, or right, got {value:?}"
        ))),
    }
}

pub(super) fn app_command(args: Vec<String>) -> Result<(), CliError> {
    let Some((command, rest)) = args.split_first() else {
        print_app_usage();
        return Err(CliError::HelpRequested);
    };
    match command.as_str() {
        "list" => list_apps(rest),
        "launch" | "open" => launch_app(rest),
        "focus" => focus_app_command(rest),
        "quit" | "kill" => quit_app(rest),
        "--help" | "-h" | "help" => {
            print_app_usage();
            Err(CliError::HelpRequested)
        }
        other => Err(CliError::Failure(format!("unknown app command: {other}"))),
    }
}

pub(super) fn list_apps(args: &[String]) -> Result<(), CliError> {
    let json = args.iter().any(|arg| arg == "--json");
    let entries = desktop_entries()?;
    if json {
        return print_json_pretty(&serde_json::json!({ "apps": entries }));
    }
    for entry in entries {
        println!(
            "{} name={} path={}",
            entry["id"].as_str().unwrap_or("-"),
            entry["name"].as_str().unwrap_or("-"),
            entry["path"].as_str().unwrap_or("-")
        );
    }
    Ok(())
}

pub(super) fn launch_app(args: &[String]) -> Result<(), CliError> {
    let mut target = None;
    let mut dry_run = false;
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--id" | "--app" | "--desktop-id" => {
                target = Some(require_slice_value(args, &mut index, "--id")?)
            }
            "--dry-run" => dry_run = true,
            "--json" => json = true,
            value if target.is_none() => target = Some(value.to_owned()),
            value => {
                return Err(CliError::Failure(format!(
                    "unexpected app launch argument: {value}"
                )));
            }
        }
        index += 1;
    }
    let target = target.ok_or_else(|| CliError::Failure("missing app id or command".to_owned()))?;
    let command = launch_app_command(&target);
    if dry_run {
        let payload =
            serde_json::json!({"ok": true, "dry_run": true, "target": target, "command": command});
        if json {
            print_json_pretty(&payload)?;
        } else {
            println!("would launch {}", payload["target"].as_str().unwrap_or("-"));
        }
        return Ok(());
    }
    run_command_vec_cli(&command.0, &command.1)?;
    if json {
        print_json_pretty(
            &serde_json::json!({"ok": true, "target": target, "program": command.0, "args": command.1}),
        )?;
    } else {
        println!("launched {target}");
    }
    Ok(())
}

pub(super) fn focus_app_command(args: &[String]) -> Result<(), CliError> {
    let mut app = None;
    let mut json = false;
    for arg in args {
        if arg == "--json" {
            json = true;
        } else if app.is_none() {
            app = Some(arg.clone());
        } else {
            return Err(CliError::Failure(format!(
                "unexpected app focus argument: {arg}"
            )));
        }
    }
    let app = app.ok_or_else(|| CliError::Failure("missing app name".to_owned()))?;
    let result = peekaboox_desktop::focus_app(&app, &FocusOptions::default())
        .map_err(|error| CliError::Failure(error.to_string()))?;
    if json {
        print_desktop_action_result_json(&result)?;
    } else {
        print_desktop_action_result(result);
    }
    Ok(())
}

pub(super) fn quit_app(args: &[String]) -> Result<(), CliError> {
    let app = args
        .iter()
        .find(|arg| !arg.starts_with('-'))
        .ok_or_else(|| CliError::Failure("missing app process name".to_owned()))?;
    run_command_vec_cli("pkill", &["-f".to_owned(), app.clone()])?;
    println!("sent quit signal to {app}");
    Ok(())
}

pub(super) fn launcher_command(args: Vec<String>) -> Result<(), CliError> {
    let Some((command, rest)) = args.split_first() else {
        return list_apps(&[]);
    };
    match command.as_str() {
        "list" => list_apps(rest),
        "open" | "launch" => launch_app(rest),
        "--help" | "-h" | "help" => {
            print_launcher_usage();
            Err(CliError::HelpRequested)
        }
        value => launch_app(args.as_slice())
            .map_err(|_| CliError::Failure(format!("unknown launcher command: {value}"))),
    }
}

pub(super) fn window_command(args: Vec<String>) -> Result<(), CliError> {
    let Some((command, rest)) = args.split_first() else {
        return windows(
            Vec::new(),
            &CliContext {
                use_daemon: false,
                socket: default_socket_path(),
            },
        );
    };
    match command.as_str() {
        "list" => windows(
            rest.to_vec(),
            &CliContext {
                use_daemon: false,
                socket: default_socket_path(),
            },
        ),
        "focus" | "activate" => window_xdotool_action(rest, "windowactivate"),
        "close" => window_xdotool_action(rest, "windowclose"),
        "minimize" => window_xdotool_action(rest, "windowminimize"),
        "move" => window_move_or_resize(rest, true),
        "resize" => window_move_or_resize(rest, false),
        "maximize" => window_wmctrl_state(rest, "add,maximized_vert,maximized_horz"),
        "unmaximize" | "restore" => {
            window_wmctrl_state(rest, "remove,maximized_vert,maximized_horz")
        }
        "fullscreen" => window_wmctrl_state(rest, "toggle,fullscreen"),
        "--help" | "-h" | "help" => {
            print_window_usage();
            Err(CliError::HelpRequested)
        }
        other => Err(CliError::Failure(format!(
            "unknown window command: {other}"
        ))),
    }
}

pub(super) fn window_xdotool_action(args: &[String], action: &str) -> Result<(), CliError> {
    let (id, dry_run, json) = resolve_window_action_args(args)?;
    if dry_run {
        print_window_action_json_or_text(json, true, action, &id)?;
        return Ok(());
    }
    run_command_vec_cli("xdotool", &[action.to_owned(), id.clone()])?;
    print_window_action_json_or_text(json, false, action, &id)
}

pub(super) fn window_move_or_resize(args: &[String], move_window: bool) -> Result<(), CliError> {
    let mut id_args = Vec::new();
    let mut x = None;
    let mut y = None;
    let mut width = None;
    let mut height = None;
    let mut dry_run = false;
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--x" => {
                x = Some(parse_i32(
                    "--x",
                    &require_slice_value(args, &mut index, "--x")?,
                )?)
            }
            "--y" => {
                y = Some(parse_i32(
                    "--y",
                    &require_slice_value(args, &mut index, "--y")?,
                )?)
            }
            "--width" | "-w" => {
                width = Some(parse_positive_u32(
                    "--width",
                    &require_slice_value(args, &mut index, "--width")?,
                )?)
            }
            "--height" | "-h" => {
                height = Some(parse_positive_u32(
                    "--height",
                    &require_slice_value(args, &mut index, "--height")?,
                )?)
            }
            "--dry-run" => dry_run = true,
            "--json" => json = true,
            value => id_args.push(value.to_owned()),
        }
        index += 1;
    }
    let (id, _, _) = resolve_window_action_args(&id_args)?;
    let (program, command_args, label) = if move_window {
        let x = x.ok_or_else(|| CliError::Failure("window move requires --x".to_owned()))?;
        let y = y.ok_or_else(|| CliError::Failure("window move requires --y".to_owned()))?;
        (
            "xdotool",
            vec![
                "windowmove".to_owned(),
                id.clone(),
                x.to_string(),
                y.to_string(),
            ],
            "windowmove",
        )
    } else {
        let width =
            width.ok_or_else(|| CliError::Failure("window resize requires --width".to_owned()))?;
        let height = height
            .ok_or_else(|| CliError::Failure("window resize requires --height".to_owned()))?;
        (
            "xdotool",
            vec![
                "windowsize".to_owned(),
                id.clone(),
                width.to_string(),
                height.to_string(),
            ],
            "windowsize",
        )
    };
    if !dry_run {
        run_command_vec_cli(program, &command_args)?;
    }
    print_window_action_json_or_text(json, dry_run, label, &id)
}

pub(super) fn window_wmctrl_state(args: &[String], state: &str) -> Result<(), CliError> {
    let (id, dry_run, json) = resolve_window_action_args(args)?;
    if !dry_run {
        run_command_vec_cli(
            "wmctrl",
            &[
                "-ir".to_owned(),
                id.clone(),
                "-b".to_owned(),
                state.to_owned(),
            ],
        )?;
    }
    print_window_action_json_or_text(json, dry_run, state, &id)
}

pub(super) fn resolve_window_action_args(
    args: &[String],
) -> Result<(String, bool, bool), CliError> {
    let mut id = None;
    let mut app = None;
    let mut title = None;
    let mut dry_run = false;
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--id" | "--window-id" => id = Some(require_slice_value(args, &mut index, "--id")?),
            "--app" => app = Some(require_slice_value(args, &mut index, "--app")?),
            "--title" | "--window-title" => {
                title = Some(require_slice_value(args, &mut index, "--title")?)
            }
            "--dry-run" => dry_run = true,
            "--json" => json = true,
            value if id.is_none() => id = Some(value.to_owned()),
            value => {
                return Err(CliError::Failure(format!(
                    "unexpected window argument: {value}"
                )));
            }
        }
        index += 1;
    }
    let id = match id {
        Some(id) => id,
        None => resolve_window_id(app, title)?,
    };
    Ok((id, dry_run, json))
}

pub(super) fn resolve_window_id(
    app: Option<String>,
    title: Option<String>,
) -> Result<String, CliError> {
    let metadata = peekaboox_windows::list_windows_with_query(peekaboox_windows::WindowQuery {
        app,
        title,
        limit: Some(1),
        sort: peekaboox_windows::WindowSort::Focused,
        ..peekaboox_windows::WindowQuery::default()
    })
    .map_err(|error| CliError::Failure(error.to_string()))?;
    metadata
        .windows
        .into_iter()
        .next()
        .map(|window| window.id)
        .ok_or_else(|| CliError::Failure("no window matched filters".to_owned()))
}

pub(super) fn print_window_action_json_or_text(
    json: bool,
    dry_run: bool,
    action: &str,
    id: &str,
) -> Result<(), CliError> {
    if json {
        print_json_pretty(&serde_json::json!({
            "ok": true,
            "dry_run": dry_run,
            "action": action,
            "window_id": id,
        }))
    } else {
        println!(
            "{} window {} action={}",
            if dry_run { "would update" } else { "updated" },
            id,
            action
        );
        Ok(())
    }
}

pub(super) fn dialog_command(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let Some((command, rest)) = args.split_first() else {
        return elements(
            vec![
                "--role-regex".to_owned(),
                "dialog|file chooser|alert".to_owned(),
            ],
            context,
        );
    };
    match command.as_str() {
        "list" => elements(
            [
                "--role-regex".to_owned(),
                "dialog|file chooser|alert".to_owned(),
            ]
            .into_iter()
            .chain(rest.to_vec())
            .collect(),
            context,
        ),
        "click" => {
            let label = rest
                .first()
                .ok_or_else(|| CliError::Failure("dialog click requires label".to_owned()))?;
            click(
                vec![
                    "--selector".to_owned(),
                    format!("role=push button,label={label}"),
                ],
                context,
            )
        }
        "accept" | "ok" => click(
            vec![
                "--selector".to_owned(),
                "role=push button,label=OK".to_owned(),
                "--vision-fallback".to_owned(),
            ],
            context,
        ),
        "cancel" => click(
            vec![
                "--selector".to_owned(),
                "role=push button,label=Cancel".to_owned(),
                "--vision-fallback".to_owned(),
            ],
            context,
        ),
        "type" | "set-value" => {
            let selector = rest
                .first()
                .ok_or_else(|| CliError::Failure("dialog type requires selector".to_owned()))?;
            let text = rest
                .get(1)
                .ok_or_else(|| CliError::Failure("dialog type requires text".to_owned()))?;
            set_value_command(vec![selector.clone(), text.clone()])
        }
        "--help" | "-h" | "help" => {
            print_dialog_usage();
            Err(CliError::HelpRequested)
        }
        other => Err(CliError::Failure(format!(
            "unknown dialog command: {other}"
        ))),
    }
}

pub(super) fn menu_command(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let Some((command, rest)) = args.split_first() else {
        return elements(
            vec![
                "--role-regex".to_owned(),
                "menu|menu item|menu bar|status".to_owned(),
            ],
            context,
        );
    };
    match command.as_str() {
        "list" => elements(
            [
                "--role-regex".to_owned(),
                "menu|menu item|menu bar|status".to_owned(),
            ]
            .into_iter()
            .chain(rest.to_vec())
            .collect(),
            context,
        ),
        "click" | "open" => {
            let label = rest
                .first()
                .ok_or_else(|| CliError::Failure("menu click requires label".to_owned()))?;
            click(
                vec![
                    "--selector".to_owned(),
                    format!("role-regex=menu|menu item|status,label={label}"),
                    "--vision-fallback".to_owned(),
                ],
                context,
            )
        }
        "--help" | "-h" | "help" => {
            print_menu_usage();
            Err(CliError::HelpRequested)
        }
        other => Err(CliError::Failure(format!("unknown menu command: {other}"))),
    }
}

pub(super) fn workspace_command(args: Vec<String>) -> Result<(), CliError> {
    let Some((command, rest)) = args.split_first() else {
        return workspace_list(false);
    };
    match command.as_str() {
        "list" => workspace_list(rest.iter().any(|arg| arg == "--json")),
        "switch" => {
            let index = rest
                .first()
                .ok_or_else(|| CliError::Failure("workspace switch requires index".to_owned()))?;
            run_command_vec_cli("wmctrl", &["-s".to_owned(), index.clone()])?;
            println!("switched workspace {index}");
            Ok(())
        }
        "move-window" => {
            let id = rest.first().ok_or_else(|| {
                CliError::Failure("workspace move-window requires window id".to_owned())
            })?;
            let workspace = rest.get(1).ok_or_else(|| {
                CliError::Failure("workspace move-window requires workspace index".to_owned())
            })?;
            run_command_vec_cli(
                "wmctrl",
                &[
                    "-ir".to_owned(),
                    id.clone(),
                    "-t".to_owned(),
                    workspace.clone(),
                ],
            )?;
            println!("moved window {id} to workspace {workspace}");
            Ok(())
        }
        "--help" | "-h" | "help" => {
            print_workspace_usage();
            Err(CliError::HelpRequested)
        }
        other => Err(CliError::Failure(format!(
            "unknown workspace command: {other}"
        ))),
    }
}

pub(super) fn workspace_list(json: bool) -> Result<(), CliError> {
    let output = run_command_capture_cli("wmctrl", &["-d".to_owned()])?;
    let workspaces = output
        .lines()
        .map(|line| serde_json::json!({ "raw": line }))
        .collect::<Vec<_>>();
    if json {
        print_json_pretty(&serde_json::json!({ "workspaces": workspaces }))
    } else {
        print!("{output}");
        Ok(())
    }
}

pub(super) fn config_command(args: Vec<String>) -> Result<(), CliError> {
    let path = config_path();
    let Some((command, rest)) = args.split_first() else {
        return config_show(&path);
    };
    match command.as_str() {
        "path" => {
            println!("{}", path.display());
            Ok(())
        }
        "show" => config_show(&path),
        "init" => {
            let config = default_config();
            write_json_pretty_file(&path, &config)?;
            println!("created {}", path.display());
            Ok(())
        }
        "get" => {
            let key = rest
                .first()
                .ok_or_else(|| CliError::Failure("config get requires key".to_owned()))?;
            let config = read_config_json(&path)?;
            println!(
                "{}",
                config
                    .pointer(&json_pointer_for_key(key))
                    .unwrap_or(&serde_json::Value::Null)
            );
            Ok(())
        }
        "set" => {
            let key = rest
                .first()
                .ok_or_else(|| CliError::Failure("config set requires key".to_owned()))?;
            let value = rest
                .get(1)
                .ok_or_else(|| CliError::Failure("config set requires value".to_owned()))?;
            let mut config = read_config_json_or_default_if_missing(&path)?;
            set_json_key(&mut config, key, parse_jsonish_value(value));
            write_json_pretty_file(&path, &config)?;
            println!("updated {}", path.display());
            Ok(())
        }
        "--help" | "-h" | "help" => {
            print_config_usage();
            Err(CliError::HelpRequested)
        }
        other => Err(CliError::Failure(format!(
            "unknown config command: {other}"
        ))),
    }
}

pub(super) fn config_show(path: &Path) -> Result<(), CliError> {
    let config = read_config_json(path).unwrap_or_else(|_| default_config());
    print_json_pretty(&config)
}

pub(super) fn permissions_command(args: Vec<String>) -> Result<(), CliError> {
    let json = args.iter().any(|arg| arg == "--json");
    let checks = serde_json::json!({
        "uinput_writable": std::fs::OpenOptions::new().write(true).open("/dev/uinput").is_ok(),
        "commands": {
            "xdotool": command_exists_cli("xdotool"),
            "ydotool": command_exists_cli("ydotool"),
            "wtype": command_exists_cli("wtype"),
            "wl-copy": command_exists_cli("wl-copy"),
            "wmctrl": command_exists_cli("wmctrl"),
            "tesseract": command_exists_cli("tesseract"),
        },
        "hints": [
            "Install ydotool and grant /dev/uinput access for global Wayland input.",
            "Install wmctrl/xdotool for X11 window and workspace management.",
            "Run peekaboox doctor --json for full diagnostics."
        ],
    });
    if json {
        print_json_pretty(&checks)
    } else {
        println!("uinput_writable={}", checks["uinput_writable"]);
        println!(
            "xdotool={} ydotool={} wtype={} wmctrl={}",
            checks["commands"]["xdotool"],
            checks["commands"]["ydotool"],
            checks["commands"]["wtype"],
            checks["commands"]["wmctrl"]
        );
        Ok(())
    }
}

pub(super) fn tools_command(args: Vec<String>) -> Result<(), CliError> {
    let json = args.iter().any(|arg| arg == "--json");
    let commands = cli_command_registry();
    if json {
        print_json_pretty(&serde_json::json!({ "commands": commands }))
    } else {
        println!("{}", commands.join("\n"));
        Ok(())
    }
}

pub(super) fn completions_command(args: Vec<String>) -> Result<(), CliError> {
    let shell = args.first().map(String::as_str).unwrap_or("bash");
    let commands = cli_command_registry().join(" ");
    match shell {
        "bash" => println!("complete -W \"{commands}\" peekaboox"),
        "zsh" => println!("#compdef peekaboox\n_arguments '1:command:({commands})'"),
        "fish" => {
            for command in cli_command_registry() {
                println!("complete -c peekaboox -f -a {command}");
            }
        }
        "--help" | "-h" | "help" => {
            print_completions_usage();
            return Err(CliError::HelpRequested);
        }
        other => {
            return Err(CliError::Failure(format!(
                "unsupported completion shell: {other}; expected bash, zsh, or fish"
            )));
        }
    }
    Ok(())
}

pub(super) fn clean_command(args: Vec<String>) -> Result<(), CliError> {
    let mut all = false;
    let mut dry_run = false;
    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--all" => all = true,
            "--dry-run" => dry_run = true,
            "--json" => json = true,
            "--help" | "-h" => {
                print_clean_usage();
                return Err(CliError::HelpRequested);
            }
            other => {
                return Err(CliError::Failure(format!(
                    "unknown clean argument: {other}"
                )));
            }
        }
    }
    let targets = if all {
        vec![
            state_dir().join("snapshots"),
            state_dir().join("agent").join("sessions"),
        ]
    } else {
        vec![state_dir().join("snapshots")]
    };
    let mut removed = Vec::new();
    for target in targets {
        if !target.exists() {
            continue;
        }
        removed.push(target.display().to_string());
        if !dry_run {
            std::fs::remove_dir_all(&target).map_err(|error| {
                CliError::Failure(format!("failed to remove {}: {error}", target.display()))
            })?;
        }
    }
    if json {
        print_json_pretty(&serde_json::json!({"dry_run": dry_run, "removed": removed}))
    } else {
        println!(
            "{} {} paths",
            if dry_run { "would remove" } else { "removed" },
            removed.len()
        );
        Ok(())
    }
}

pub(super) fn desktop_entries() -> Result<Vec<serde_json::Value>, CliError> {
    let mut dirs = vec![PathBuf::from("/usr/share/applications")];
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/applications"));
    }
    let mut entries = Vec::new();
    for dir in dirs {
        let Ok(read_dir) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("desktop") {
                continue;
            }
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            if content.lines().any(|line| line.trim() == "NoDisplay=true") {
                continue;
            }
            let name = content
                .lines()
                .find_map(|line| line.strip_prefix("Name="))
                .unwrap_or_else(|| {
                    path.file_stem()
                        .and_then(|value| value.to_str())
                        .unwrap_or("-")
                });
            let id = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("-")
                .to_owned();
            entries.push(serde_json::json!({
                "id": id,
                "name": name,
                "path": path.display().to_string(),
            }));
        }
    }
    entries.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
    Ok(entries)
}

pub(super) fn launch_app_command(target: &str) -> (String, Vec<String>) {
    if target.contains("://") || Path::new(target).exists() {
        return ("xdg-open".to_owned(), vec![target.to_owned()]);
    }
    if command_exists_cli("gtk-launch") {
        return ("gtk-launch".to_owned(), vec![target.to_owned()]);
    }
    let mut parts = target.split_whitespace();
    let program = parts.next().unwrap_or(target).to_owned();
    let args = parts.map(str::to_owned).collect::<Vec<_>>();
    (program, args)
}

pub(super) fn read_json_files_in_dir(dir: &Path) -> Result<Vec<serde_json::Value>, CliError> {
    let mut values = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(values);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
            values.push(value);
        }
    }
    values.sort_by(|left, right| {
        left["updated_at_unix_ms"]
            .as_u64()
            .cmp(&right["updated_at_unix_ms"].as_u64())
            .reverse()
    });
    Ok(values)
}

pub(super) fn state_dir() -> PathBuf {
    if let Some(value) = std::env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(value).join("peekaboox");
    }
    home_dir().join(".local/state/peekaboox")
}

pub(super) fn config_path() -> PathBuf {
    if let Some(value) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(value).join("peekaboox/config.json");
    }
    home_dir().join(".config/peekaboox/config.json")
}

pub(super) fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub(super) fn default_config() -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "capture": {
            "format": "png",
            "jpeg_quality": 90
        },
        "input": {
            "backend": "auto",
            "bounds_policy": "allow"
        },
        "mcp": {
            "transport": "stdio",
            "host": "127.0.0.1",
            "port": 47778
        }
    })
}

pub(super) fn read_config_json(path: &Path) -> Result<serde_json::Value, CliError> {
    let content = std::fs::read_to_string(path).map_err(|error| {
        CliError::Failure(format!("failed to read {}: {error}", path.display()))
    })?;
    serde_json::from_str(&content).map_err(|error| {
        CliError::Failure(format!(
            "invalid config JSON in {}: {error}",
            path.display()
        ))
    })
}

pub(super) fn read_config_json_or_default_if_missing(
    path: &Path,
) -> Result<serde_json::Value, CliError> {
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).map_err(|error| {
            CliError::Failure(format!(
                "invalid config JSON in {}; fix or move the existing file before running config set: {error}",
                path.display()
            ))
        }),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(default_config()),
        Err(error) => Err(CliError::Failure(format!(
            "failed to read {}: {error}",
            path.display()
        ))),
    }
}

pub(super) fn json_pointer_for_key(key: &str) -> String {
    format!("/{}", key.replace('.', "/"))
}

pub(super) fn set_json_key(root: &mut serde_json::Value, key: &str, value: serde_json::Value) {
    let parts = key
        .split('.')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return;
    }
    let mut current = root;
    for part in &parts[..parts.len() - 1] {
        if !current.is_object() {
            *current = serde_json::json!({});
        }
        current = current
            .as_object_mut()
            .expect("object ensured")
            .entry((*part).to_owned())
            .or_insert_with(|| serde_json::json!({}));
    }
    if !current.is_object() {
        *current = serde_json::json!({});
    }
    current
        .as_object_mut()
        .expect("object ensured")
        .insert(parts[parts.len() - 1].to_owned(), value);
}

pub(super) fn parse_jsonish_value(value: &str) -> serde_json::Value {
    serde_json::from_str(value).unwrap_or_else(|_| serde_json::Value::String(value.to_owned()))
}

pub(super) fn cli_command_registry() -> Vec<&'static str> {
    vec![
        "agent",
        "app",
        "capture",
        "capture-backends",
        "capture-delta",
        "capture-dmabuf",
        "clean",
        "click",
        "compare",
        "completions",
        "config",
        "desktop",
        "diagnose",
        "dialog",
        "dock",
        "doctor",
        "drag",
        "elements",
        "hotkey",
        "image",
        "launcher",
        "menu",
        "menubar",
        "move",
        "ocr",
        "paste",
        "perform-action",
        "permissions",
        "plugins",
        "plugin-call",
        "press",
        "scroll",
        "see",
        "set-value",
        "state",
        "swipe",
        "tools",
        "type",
        "vision-elements",
        "window",
        "windows",
        "workspace",
    ]
}

pub(super) fn run_command_vec_cli(program: &str, args: &[String]) -> Result<(), CliError> {
    let output = Command::new(program)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| CliError::Failure(format!("failed to run {program}: {error}")))?;
    if !output.status.success() {
        return Err(CliError::Failure(format!(
            "{program} failed with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

pub(super) fn run_command_capture_cli(program: &str, args: &[String]) -> Result<String, CliError> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|error| CliError::Failure(format!("failed to run {program}: {error}")))?;
    if !output.status.success() {
        return Err(CliError::Failure(format!(
            "{program} failed with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub(super) fn command_exists_cli(command: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|path| path.join(command).is_file()))
}
