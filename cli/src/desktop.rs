use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DesktopProfilesArgs {
    pub(super) json: bool,
    pub(super) app: Option<String>,
    pub(super) target: Option<String>,
    pub(super) command: Option<String>,
    pub(super) desktop_id: Option<String>,
    pub(super) supports: Option<String>,
    pub(super) check: bool,
    pub(super) installed: bool,
    pub(super) available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DesktopFocusArgs {
    pub(super) app: String,
    pub(super) use_gnome_overview: bool,
    pub(super) launch_if_needed: bool,
    pub(super) wait_after_focus_ms: u64,
    pub(super) overview_wait_ms: u64,
    pub(super) window_title: Option<String>,
    pub(super) window_id: Option<String>,
    pub(super) verify: bool,
    pub(super) json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DesktopLocateArgs {
    pub(super) app: String,
    pub(super) target: String,
    pub(super) image: Option<PathBuf>,
    pub(super) prefer_accessibility: bool,
    pub(super) window_title: Option<String>,
    pub(super) window_id: Option<String>,
    pub(super) json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DesktopClickArgs {
    pub(super) app: String,
    pub(super) target: String,
    pub(super) image: Option<PathBuf>,
    pub(super) prefer_accessibility: bool,
    pub(super) window_title: Option<String>,
    pub(super) window_id: Option<String>,
    pub(super) button: MouseButton,
    pub(super) dry_run: bool,
    pub(super) verify: bool,
    pub(super) json: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct DesktopDragArgs {
    pub(super) app: String,
    pub(super) target: String,
    pub(super) image: Option<PathBuf>,
    pub(super) prefer_accessibility: bool,
    pub(super) window_title: Option<String>,
    pub(super) window_id: Option<String>,
    pub(super) button: MouseButton,
    pub(super) from_ratio: (f32, f32),
    pub(super) to_ratio: (f32, f32),
    pub(super) duration_ms: u64,
    pub(super) dry_run: bool,
    pub(super) verify: bool,
    pub(super) json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DesktopTypeIntoArgs {
    pub(super) app: String,
    pub(super) target: String,
    pub(super) text: String,
    pub(super) image: Option<PathBuf>,
    pub(super) prefer_accessibility: bool,
    pub(super) window_title: Option<String>,
    pub(super) window_id: Option<String>,
    pub(super) clear: bool,
    pub(super) dry_run: bool,
    pub(super) verify: bool,
    pub(super) json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DesktopAssertArgs {
    pub(super) app: String,
    pub(super) target: String,
    pub(super) image: Option<PathBuf>,
    pub(super) prefer_accessibility: bool,
    pub(super) window_title: Option<String>,
    pub(super) window_id: Option<String>,
    pub(super) assertion: DesktopAssertion,
    pub(super) json: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum DesktopCommand {
    Profiles(DesktopProfilesArgs),
    Focus(DesktopFocusArgs),
    Locate(DesktopLocateArgs),
    Click(DesktopClickArgs),
    Drag(DesktopDragArgs),
    TypeInto(DesktopTypeIntoArgs),
    Assert(DesktopAssertArgs),
    Help,
}

pub(super) fn desktop(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let command = parse_desktop_args(args)?;
    if context.use_daemon {
        return desktop_daemon(command, context);
    }

    match command {
        DesktopCommand::Profiles(args) => {
            print_desktop_profiles(args)?;
            Ok(())
        }
        DesktopCommand::Focus(args) => {
            let result = peekaboox_desktop::focus_app(
                &args.app,
                &FocusOptions {
                    use_gnome_overview: args.use_gnome_overview,
                    launch_if_needed: args.launch_if_needed,
                    wait_after_focus_ms: args.wait_after_focus_ms,
                    overview_wait_ms: args.overview_wait_ms,
                    window_title: args.window_title,
                    window_id: args.window_id,
                    verify: args.verify,
                },
            )
            .map_err(|error| CliError::Failure(error.to_string()))?;
            if args.json {
                print_desktop_action_result_json(&result)?;
            } else {
                print_desktop_action_result(result);
            }
            Ok(())
        }
        DesktopCommand::Locate(args) => {
            let target = peekaboox_desktop::locate_target(
                &args.app,
                &args.target,
                &LocateOptions {
                    image: args.image,
                    prefer_accessibility: args.prefer_accessibility,
                    window_title: args.window_title,
                    window_id: args.window_id,
                },
            )
            .map_err(|error| CliError::Failure(error.to_string()))?;
            if args.json {
                print_desktop_locate_result_json(&target)?;
            } else {
                print_desktop_locate_result(target);
            }
            Ok(())
        }
        DesktopCommand::Click(args) => {
            let result = peekaboox_desktop::click_target(
                &args.app,
                &args.target,
                &DesktopClickOptions {
                    locate: LocateOptions {
                        image: args.image,
                        prefer_accessibility: args.prefer_accessibility,
                        window_title: args.window_title,
                        window_id: args.window_id,
                    },
                    button: args.button,
                    dry_run: args.dry_run,
                    verify: args.verify,
                },
            )
            .map_err(|error| CliError::Failure(error.to_string()))?;
            if args.json {
                print_desktop_action_result_json(&result)?;
            } else {
                print_desktop_action_result(result);
            }
            Ok(())
        }
        DesktopCommand::Drag(args) => {
            let result = peekaboox_desktop::drag_target(
                &args.app,
                &args.target,
                &DesktopDragOptions {
                    locate: LocateOptions {
                        image: args.image,
                        prefer_accessibility: args.prefer_accessibility,
                        window_title: args.window_title,
                        window_id: args.window_id,
                    },
                    from_ratio: args.from_ratio,
                    to_ratio: args.to_ratio,
                    button: args.button,
                    duration_ms: args.duration_ms,
                    dry_run: args.dry_run,
                    verify: args.verify,
                },
            )
            .map_err(|error| CliError::Failure(error.to_string()))?;
            if args.json {
                print_desktop_action_result_json(&result)?;
            } else {
                print_desktop_action_result(result);
            }
            Ok(())
        }
        DesktopCommand::TypeInto(args) => {
            let result = peekaboox_desktop::type_into_target(
                &args.app,
                &args.target,
                &args.text,
                &TypeIntoOptions {
                    locate: LocateOptions {
                        image: args.image,
                        prefer_accessibility: args.prefer_accessibility,
                        window_title: args.window_title,
                        window_id: args.window_id,
                    },
                    clear: args.clear,
                    dry_run: args.dry_run,
                    verify: args.verify,
                },
            )
            .map_err(|error| CliError::Failure(error.to_string()))?;
            if args.json {
                print_desktop_action_result_json(&result)?;
            } else {
                print_desktop_action_result(result);
            }
            Ok(())
        }
        DesktopCommand::Assert(args) => {
            let result = peekaboox_desktop::assert_target(
                &args.app,
                &args.target,
                &AssertOptions {
                    locate: LocateOptions {
                        image: args.image,
                        prefer_accessibility: args.prefer_accessibility,
                        window_title: args.window_title,
                        window_id: args.window_id,
                    },
                    assertion: args.assertion,
                },
            )
            .map_err(|error| CliError::Failure(error.to_string()))?;
            if args.json {
                print_desktop_action_result_json(&result)?;
            } else {
                print_desktop_action_result(result);
            }
            Ok(())
        }
        DesktopCommand::Help => {
            print_desktop_usage();
            Err(CliError::HelpRequested)
        }
    }
}

pub(super) fn desktop_daemon(
    command: DesktopCommand,
    context: &CliContext,
) -> Result<(), CliError> {
    match command {
        DesktopCommand::Profiles(args) => {
            let result = daemon_request(
                context,
                ApiRequest::DesktopProfiles {
                    app: args.app.clone(),
                    target: args.target.clone(),
                    command: args.command.clone(),
                    desktop_id: args.desktop_id.clone(),
                    supports: args.supports.clone(),
                    check: args.check,
                    installed: args.installed,
                    available: args.available,
                },
            )?;
            let ApiResult::DesktopProfiles(result) = result else {
                return Err(CliError::Failure(
                    "daemon returned unexpected desktop profiles response".to_owned(),
                ));
            };
            print_desktop_profiles_dto(
                result,
                args.check || args.installed || args.available,
                args.json,
            )?;
            Ok(())
        }
        DesktopCommand::Focus(args) => {
            let result = daemon_request(
                context,
                ApiRequest::DesktopFocus {
                    app: args.app,
                    use_gnome_overview: args.use_gnome_overview,
                    launch_if_needed: args.launch_if_needed,
                    wait_after_focus_ms: args.wait_after_focus_ms,
                    overview_wait_ms: args.overview_wait_ms,
                    window_title: args.window_title,
                    window_id: args.window_id,
                    verify: args.verify,
                },
            )?;
            let ApiResult::DesktopAction(result) = result else {
                return Err(CliError::Failure(
                    "daemon returned unexpected desktop focus response".to_owned(),
                ));
            };
            if args.json {
                print_json_pretty(&result)?;
            } else {
                print_desktop_action_dto(result);
            }
            Ok(())
        }
        DesktopCommand::Locate(args) => {
            let result = daemon_request(
                context,
                ApiRequest::DesktopLocate {
                    app: args.app,
                    target: args.target,
                    image_path: args.image.as_ref().map(path_to_daemon_string).transpose()?,
                    prefer_accessibility: args.prefer_accessibility,
                    window_title: args.window_title,
                    window_id: args.window_id,
                },
            )?;
            let ApiResult::DesktopLocate(result) = result else {
                return Err(CliError::Failure(
                    "daemon returned unexpected desktop locate response".to_owned(),
                ));
            };
            if args.json {
                print_json_pretty(&result)?;
            } else {
                print_desktop_locate_dto(result);
            }
            Ok(())
        }
        DesktopCommand::Click(args) => {
            let result = daemon_request(
                context,
                ApiRequest::DesktopClick {
                    app: args.app,
                    target: args.target,
                    image_path: args.image.as_ref().map(path_to_daemon_string).transpose()?,
                    prefer_accessibility: args.prefer_accessibility,
                    window_title: args.window_title,
                    button: mouse_button_dto(args.button),
                    dry_run: args.dry_run,
                    window_id: args.window_id,
                    verify: args.verify,
                },
            )?;
            let ApiResult::DesktopAction(result) = result else {
                return Err(CliError::Failure(
                    "daemon returned unexpected desktop click response".to_owned(),
                ));
            };
            if args.json {
                print_json_pretty(&result)?;
            } else {
                print_desktop_action_dto(result);
            }
            Ok(())
        }
        DesktopCommand::Drag(args) => {
            let result = daemon_request(
                context,
                ApiRequest::DesktopDrag {
                    app: args.app,
                    target: args.target,
                    image_path: args.image.as_ref().map(path_to_daemon_string).transpose()?,
                    prefer_accessibility: args.prefer_accessibility,
                    window_title: args.window_title,
                    button: mouse_button_dto(args.button),
                    from_ratio_x: args.from_ratio.0,
                    from_ratio_y: args.from_ratio.1,
                    to_ratio_x: args.to_ratio.0,
                    to_ratio_y: args.to_ratio.1,
                    duration_ms: args.duration_ms,
                    dry_run: args.dry_run,
                    window_id: args.window_id,
                    verify: args.verify,
                },
            )?;
            let ApiResult::DesktopAction(result) = result else {
                return Err(CliError::Failure(
                    "daemon returned unexpected desktop drag response".to_owned(),
                ));
            };
            if args.json {
                print_json_pretty(&result)?;
            } else {
                print_desktop_action_dto(result);
            }
            Ok(())
        }
        DesktopCommand::TypeInto(args) => {
            let result = daemon_request(
                context,
                ApiRequest::DesktopTypeInto {
                    app: args.app,
                    target: args.target,
                    text: args.text,
                    image_path: args.image.as_ref().map(path_to_daemon_string).transpose()?,
                    prefer_accessibility: args.prefer_accessibility,
                    window_title: args.window_title,
                    clear: args.clear,
                    dry_run: args.dry_run,
                    window_id: args.window_id,
                    verify: args.verify,
                },
            )?;
            let ApiResult::DesktopAction(result) = result else {
                return Err(CliError::Failure(
                    "daemon returned unexpected desktop type-into response".to_owned(),
                ));
            };
            if args.json {
                print_json_pretty(&result)?;
            } else {
                print_desktop_action_dto(result);
            }
            Ok(())
        }
        DesktopCommand::Assert(args) => {
            let (assertion, expected_text) = desktop_assertion_dto(&args.assertion);
            let result = daemon_request(
                context,
                ApiRequest::DesktopAssert {
                    app: args.app,
                    target: args.target,
                    image_path: args.image.as_ref().map(path_to_daemon_string).transpose()?,
                    prefer_accessibility: args.prefer_accessibility,
                    window_title: args.window_title,
                    assertion,
                    expected_text,
                    window_id: args.window_id,
                },
            )?;
            let ApiResult::DesktopAction(result) = result else {
                return Err(CliError::Failure(
                    "daemon returned unexpected desktop assert response".to_owned(),
                ));
            };
            if args.json {
                print_json_pretty(&result)?;
            } else {
                print_desktop_action_dto(result);
            }
            Ok(())
        }
        DesktopCommand::Help => {
            print_desktop_usage();
            Err(CliError::HelpRequested)
        }
    }
}

pub(super) fn print_desktop_action_result(result: peekaboox_desktop::DesktopActionResult) {
    println!(
        "{} {}: {} via {}",
        result.app, result.action, result.detail, result.backend_name
    );
    if let Some(detail) = result.verification_detail {
        eprintln!(
            "verification: {}{}",
            if result.verified { "passed: " } else { "" },
            detail
        );
    }
    print_focus_diagnostics(&result.focus_diagnostics);
}

pub(super) fn print_desktop_action_dto(result: DesktopActionResultDto) {
    println!(
        "{} {}: {} via {}",
        result.app, result.action, result.detail, result.backend_name
    );
    if let Some(detail) = result.verification_detail {
        eprintln!(
            "verification: {}{}",
            if result.verified { "passed: " } else { "" },
            detail
        );
    }
    print_focus_diagnostics(&result.focus_diagnostics);
}

pub(super) fn print_focus_diagnostics(diagnostics: &[String]) {
    if diagnostics.is_empty() {
        return;
    }
    eprintln!("focus diagnostics:");
    for diagnostic in diagnostics {
        eprintln!("- {diagnostic}");
    }
}

pub(super) fn print_desktop_locate_result(target: peekaboox_desktop::ResolvedDesktopTarget) {
    println!(
        "{} {} {},{} rect={} via {}",
        target.app,
        target.target,
        target.point.x,
        target.point.y,
        target
            .rect
            .map(format_rect)
            .unwrap_or_else(|| "-".to_owned()),
        target.source.label()
    );
}

pub(super) fn print_desktop_locate_dto(target: DesktopLocateResultDto) {
    println!(
        "{} {} {},{} rect={} via {}",
        target.app,
        target.target,
        target.point.x,
        target.point.y,
        target
            .rect
            .map(Rect::from)
            .map(format_rect)
            .unwrap_or_else(|| "-".to_owned()),
        target.source
    );
}

pub(super) fn print_desktop_profiles(args: DesktopProfilesArgs) -> Result<(), CliError> {
    let result = peekaboox_desktop::desktop_profiles_with_query(&desktop_profile_query(&args))
        .map_err(|error| CliError::Failure(error.to_string()))?;
    if args.json {
        print_json_pretty(&desktop_profile_list_json(&result))
    } else {
        for profile in &result.profiles {
            print_desktop_profile_line(profile, args.check || args.installed || args.available);
        }
        Ok(())
    }
}

pub(super) fn desktop_profile_query(args: &DesktopProfilesArgs) -> DesktopProfileQuery {
    DesktopProfileQuery {
        app: args.app.clone(),
        target: args.target.clone(),
        command: args.command.clone(),
        desktop_id: args.desktop_id.clone(),
        supports: args.supports.clone(),
        check_availability: args.check,
        installed_only: args.installed,
        available_only: args.available,
    }
}

pub(super) fn print_desktop_profile_line(
    profile: &peekaboox_desktop::DesktopProfileInfo,
    show_availability: bool,
) {
    let targets = profile
        .targets
        .iter()
        .map(|target| {
            if target.supports.is_empty() {
                target.name.clone()
            } else {
                format!("{}[{}]", target.name, target.supports.join("+"))
            }
        })
        .collect::<Vec<_>>();
    let commands = profile
        .commands
        .iter()
        .map(|command| match command.available {
            Some(true) => format!("{}:available", command.display),
            Some(false) => format!("{}:missing", command.display),
            None => command.display.clone(),
        })
        .collect::<Vec<_>>();
    let mut fields = vec![
        format!("targets={}", targets.join(",")),
        format!("aliases={}", profile.aliases.join(",")),
        format!("desktop_ids={}", profile.desktop_ids.join(",")),
        format!("commands={}", commands.join(",")),
    ];
    if show_availability {
        fields.push(format!(
            "installed={}",
            profile
                .availability
                .installed
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_owned())
        ));
    }
    println!("{} {}", profile.id, fields.join(" "));
}

pub(super) fn desktop_profile_list_json(
    result: &peekaboox_desktop::DesktopProfileList,
) -> serde_json::Value {
    serde_json::json!({
        "schema_version": &result.schema_version,
        "count": result.count,
        "profiles": result.profiles.iter().map(desktop_profile_json).collect::<Vec<_>>(),
    })
}

pub(super) fn desktop_profile_json(
    profile: &peekaboox_desktop::DesktopProfileInfo,
) -> serde_json::Value {
    serde_json::json!({
        "id": &profile.id,
        "aliases": &profile.aliases,
        "search_name": &profile.search_name,
        "desktop_ids": &profile.desktop_ids,
        "commands": profile.commands.iter().map(|command| serde_json::json!({
            "program": &command.program,
            "args": &command.args,
            "display": &command.display,
            "available": command.available,
        })).collect::<Vec<_>>(),
        "targets": profile.targets.iter().map(|target| serde_json::json!({
            "name": &target.name,
            "supports": &target.supports,
            "sources": &target.sources,
            "can_locate": target.can_locate,
            "can_click": target.can_click,
            "can_drag": target.can_drag,
            "can_type": target.can_type,
            "can_assert_present": target.can_assert_present,
            "can_assert_active": target.can_assert_active,
            "can_assert_contains": target.can_assert_contains,
            "accessibility_selector": &target.accessibility_selector,
            "visual_layout": target.visual_layout,
            "visual_rect": target.visual_rect,
        })).collect::<Vec<_>>(),
        "availability": {
            "checked": profile.availability.checked,
            "installed": profile.availability.installed,
            "command_available": profile.availability.command_available,
            "desktop_entry_available": profile.availability.desktop_entry_available,
            "available_commands": &profile.availability.available_commands,
            "available_desktop_ids": &profile.availability.available_desktop_ids,
        },
    })
}

pub(super) fn print_desktop_profiles_dto(
    result: DesktopProfilesResultDto,
    show_availability: bool,
    json: bool,
) -> Result<(), CliError> {
    if json {
        return print_json_pretty(&result);
    }
    for profile in &result.profiles {
        print_desktop_profile_dto_line(profile, show_availability);
    }
    Ok(())
}

pub(super) fn print_desktop_profile_dto_line(profile: &DesktopProfileDto, show_availability: bool) {
    let targets = profile
        .targets
        .iter()
        .map(|target| {
            if target.supports.is_empty() {
                target.name.clone()
            } else {
                format!("{}[{}]", target.name, target.supports.join("+"))
            }
        })
        .collect::<Vec<_>>();
    let commands = profile
        .commands
        .iter()
        .map(|command| match command.available {
            Some(true) => format!("{}:available", command.display),
            Some(false) => format!("{}:missing", command.display),
            None => command.display.clone(),
        })
        .collect::<Vec<_>>();
    let mut fields = vec![
        format!("targets={}", targets.join(",")),
        format!("aliases={}", profile.aliases.join(",")),
        format!("desktop_ids={}", profile.desktop_ids.join(",")),
        format!("commands={}", commands.join(",")),
    ];
    if show_availability {
        fields.push(format!(
            "installed={}",
            profile
                .availability
                .installed
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_owned())
        ));
    }
    println!("{} {}", profile.id, fields.join(" "));
}

pub(super) fn print_desktop_action_result_json(
    result: &peekaboox_desktop::DesktopActionResult,
) -> Result<(), CliError> {
    print_json_pretty(&serde_json::json!({
        "app": &result.app,
        "action": &result.action,
        "detail": &result.detail,
        "backend_name": &result.backend_name,
        "verified": result.verified,
        "verification_detail": &result.verification_detail,
        "focus_diagnostics": &result.focus_diagnostics,
    }))
}

pub(super) fn print_desktop_locate_result_json(
    target: &peekaboox_desktop::ResolvedDesktopTarget,
) -> Result<(), CliError> {
    print_json_pretty(&serde_json::json!({
        "app": &target.app,
        "target": &target.target,
        "point": {
            "x": target.point.x,
            "y": target.point.y,
        },
        "rect": target.rect.map(|rect| serde_json::json!({
            "x": rect.x,
            "y": rect.y,
            "width": rect.width,
            "height": rect.height,
        })),
        "source": target.source.label(),
    }))
}

pub(super) fn parse_desktop_args(args: Vec<String>) -> Result<DesktopCommand, CliError> {
    let Some((command, rest)) = args.split_first() else {
        return Ok(DesktopCommand::Help);
    };

    match command.as_str() {
        "profiles" | "apps" => parse_desktop_profiles_args(rest.to_vec()),
        "focus" => parse_desktop_focus_args(rest.to_vec()),
        "locate" => parse_desktop_locate_args(rest.to_vec()),
        "click" => parse_desktop_click_args(rest.to_vec()),
        "drag" => parse_desktop_drag_args(rest.to_vec()),
        "type-into" | "type" => parse_desktop_type_into_args(rest.to_vec()),
        "assert" => parse_desktop_assert_args(rest.to_vec(), false),
        "assert-not" => parse_desktop_assert_args(rest.to_vec(), true),
        "--help" | "-h" | "help" => Ok(DesktopCommand::Help),
        unknown => Err(CliError::Failure(format!(
            "unknown desktop command: {unknown}"
        ))),
    }
}

pub(super) fn parse_desktop_profiles_args(args: Vec<String>) -> Result<DesktopCommand, CliError> {
    let mut json = false;
    let mut app = None;
    let mut target = None;
    let mut command = None;
    let mut desktop_id = None;
    let mut supports = None;
    let mut check = false;
    let mut installed = false;
    let mut available = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--json" => json = true,
            "--app" | "-a" => app = Some(parse_next_string(&args, &mut index, "--app")?),
            "--target" | "-t" => target = Some(parse_next_string(&args, &mut index, "--target")?),
            "--command" => command = Some(parse_next_string(&args, &mut index, "--command")?),
            "--desktop-id" => {
                desktop_id = Some(parse_next_string(&args, &mut index, "--desktop-id")?)
            }
            "--supports" => supports = Some(parse_next_string(&args, &mut index, "--supports")?),
            "--check" | "--availability" => check = true,
            "--installed" => {
                check = true;
                installed = true;
            }
            "--available" => {
                check = true;
                available = true;
            }
            "--help" | "-h" => return Ok(DesktopCommand::Help),
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown desktop profiles argument: {value}"
                )));
            }
            value if app.is_none() => app = Some(value.to_owned()),
            value => {
                return Err(CliError::Failure(format!(
                    "unexpected desktop profiles argument: {value}"
                )));
            }
        }
        index += 1;
    }

    Ok(DesktopCommand::Profiles(DesktopProfilesArgs {
        json,
        app,
        target,
        command,
        desktop_id,
        supports,
        check,
        installed,
        available,
    }))
}

pub(super) fn parse_desktop_focus_args(args: Vec<String>) -> Result<DesktopCommand, CliError> {
    let mut app = None;
    let mut use_gnome_overview = true;
    let mut launch_if_needed = true;
    let mut wait_after_focus_ms = 1_000_u64;
    let mut overview_wait_ms = 800_u64;
    let mut window_title = None;
    let mut window_id = None;
    let mut verify = false;
    let mut json = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--app" | "-a" => app = Some(parse_next_string(&args, &mut index, "--app")?),
            "--window-title" | "--title" => {
                window_title = Some(parse_next_string(&args, &mut index, "--window-title")?)
            }
            "--window-id" => window_id = Some(parse_next_string(&args, &mut index, "--window-id")?),
            "--no-overview" => use_gnome_overview = false,
            "--no-launch" => launch_if_needed = false,
            "--verify" => verify = true,
            "--json" => json = true,
            "--wait-ms" => {
                wait_after_focus_ms = parse_u64(
                    "--wait-ms",
                    &parse_next_string(&args, &mut index, "--wait-ms")?,
                )?;
            }
            "--overview-wait-ms" => {
                overview_wait_ms = parse_u64(
                    "--overview-wait-ms",
                    &parse_next_string(&args, &mut index, "--overview-wait-ms")?,
                )?;
            }
            "--help" | "-h" => return Ok(DesktopCommand::Help),
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown desktop focus argument: {value}"
                )));
            }
            value if app.is_none() => app = Some(value.to_owned()),
            value => {
                return Err(CliError::Failure(format!(
                    "unexpected desktop focus argument: {value}"
                )));
            }
        }
        index += 1;
    }

    Ok(DesktopCommand::Focus(DesktopFocusArgs {
        app: app.ok_or_else(|| CliError::Failure("missing --app".to_owned()))?,
        use_gnome_overview,
        launch_if_needed,
        wait_after_focus_ms,
        overview_wait_ms,
        window_title,
        window_id,
        verify,
        json,
    }))
}

pub(super) fn parse_desktop_locate_args(args: Vec<String>) -> Result<DesktopCommand, CliError> {
    let mut app = None;
    let mut target = None;
    let mut image = None;
    let mut prefer_accessibility = true;
    let mut window_title = None;
    let mut window_id = None;
    let mut json = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--app" | "-a" => app = Some(parse_next_string(&args, &mut index, "--app")?),
            "--target" | "-t" => target = Some(parse_next_string(&args, &mut index, "--target")?),
            "--window-title" | "--title" => {
                window_title = Some(parse_next_string(&args, &mut index, "--window-title")?)
            }
            "--window-id" => window_id = Some(parse_next_string(&args, &mut index, "--window-id")?),
            "--image" | "-i" => {
                image = Some(PathBuf::from(parse_next_string(
                    &args, &mut index, "--image",
                )?))
            }
            "--no-accessibility" => prefer_accessibility = false,
            "--json" => json = true,
            "--help" | "-h" => return Ok(DesktopCommand::Help),
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown desktop locate argument: {value}"
                )));
            }
            value if app.is_none() => app = Some(value.to_owned()),
            value if target.is_none() => target = Some(value.to_owned()),
            value => {
                return Err(CliError::Failure(format!(
                    "unexpected desktop locate argument: {value}"
                )));
            }
        }
        index += 1;
    }

    Ok(DesktopCommand::Locate(DesktopLocateArgs {
        app: app.ok_or_else(|| CliError::Failure("missing --app".to_owned()))?,
        target: target.ok_or_else(|| CliError::Failure("missing --target".to_owned()))?,
        image,
        prefer_accessibility,
        window_title,
        window_id,
        json,
    }))
}

pub(super) fn parse_desktop_click_args(args: Vec<String>) -> Result<DesktopCommand, CliError> {
    let mut app = None;
    let mut target = None;
    let mut image = None;
    let mut prefer_accessibility = true;
    let mut window_title = None;
    let mut window_id = None;
    let mut button = MouseButton::Left;
    let mut dry_run = false;
    let mut verify = false;
    let mut json = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--app" | "-a" => app = Some(parse_next_string(&args, &mut index, "--app")?),
            "--target" | "-t" => target = Some(parse_next_string(&args, &mut index, "--target")?),
            "--window-title" | "--title" => {
                window_title = Some(parse_next_string(&args, &mut index, "--window-title")?)
            }
            "--window-id" => window_id = Some(parse_next_string(&args, &mut index, "--window-id")?),
            "--image" | "-i" => {
                image = Some(PathBuf::from(parse_next_string(
                    &args, &mut index, "--image",
                )?))
            }
            "--button" | "-b" => {
                button = parse_mouse_button(&parse_next_string(&args, &mut index, "--button")?)?
            }
            "--dry-run" => dry_run = true,
            "--verify" => verify = true,
            "--json" => json = true,
            "--no-accessibility" => prefer_accessibility = false,
            "--help" | "-h" => return Ok(DesktopCommand::Help),
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown desktop click argument: {value}"
                )));
            }
            value if app.is_none() => app = Some(value.to_owned()),
            value if target.is_none() => target = Some(value.to_owned()),
            value => {
                return Err(CliError::Failure(format!(
                    "unexpected desktop click argument: {value}"
                )));
            }
        }
        index += 1;
    }

    Ok(DesktopCommand::Click(DesktopClickArgs {
        app: app.ok_or_else(|| CliError::Failure("missing --app".to_owned()))?,
        target: target.ok_or_else(|| CliError::Failure("missing --target".to_owned()))?,
        image,
        prefer_accessibility,
        window_title,
        window_id,
        button,
        dry_run,
        verify,
        json,
    }))
}

pub(super) fn parse_desktop_drag_args(args: Vec<String>) -> Result<DesktopCommand, CliError> {
    let mut app = None;
    let mut target = None;
    let mut image = None;
    let mut prefer_accessibility = true;
    let mut window_title = None;
    let mut window_id = None;
    let mut button = MouseButton::Left;
    let mut from_ratio = None;
    let mut to_ratio = None;
    let mut duration_ms = 250_u64;
    let mut dry_run = false;
    let mut verify = false;
    let mut json = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--app" | "-a" => app = Some(parse_next_string(&args, &mut index, "--app")?),
            "--target" | "-t" => target = Some(parse_next_string(&args, &mut index, "--target")?),
            "--window-title" | "--title" => {
                window_title = Some(parse_next_string(&args, &mut index, "--window-title")?)
            }
            "--window-id" => window_id = Some(parse_next_string(&args, &mut index, "--window-id")?),
            "--image" | "-i" => {
                image = Some(PathBuf::from(parse_next_string(
                    &args, &mut index, "--image",
                )?))
            }
            "--button" | "-b" => {
                button = parse_mouse_button(&parse_next_string(&args, &mut index, "--button")?)?
            }
            "--from-ratio" | "--from" => {
                from_ratio = Some(parse_ratio_pair(
                    "--from-ratio",
                    &parse_next_string(&args, &mut index, "--from-ratio")?,
                )?)
            }
            "--to-ratio" | "--to" => {
                to_ratio = Some(parse_ratio_pair(
                    "--to-ratio",
                    &parse_next_string(&args, &mut index, "--to-ratio")?,
                )?)
            }
            "--duration-ms" => {
                duration_ms = parse_u64(
                    "--duration-ms",
                    &parse_next_string(&args, &mut index, "--duration-ms")?,
                )?;
            }
            "--dry-run" => dry_run = true,
            "--verify" => verify = true,
            "--json" => json = true,
            "--no-accessibility" => prefer_accessibility = false,
            "--help" | "-h" => return Ok(DesktopCommand::Help),
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown desktop drag argument: {value}"
                )));
            }
            value if app.is_none() => app = Some(value.to_owned()),
            value if target.is_none() => target = Some(value.to_owned()),
            value => {
                return Err(CliError::Failure(format!(
                    "unexpected desktop drag argument: {value}"
                )));
            }
        }
        index += 1;
    }

    Ok(DesktopCommand::Drag(DesktopDragArgs {
        app: app.ok_or_else(|| CliError::Failure("missing --app".to_owned()))?,
        target: target.ok_or_else(|| CliError::Failure("missing --target".to_owned()))?,
        image,
        prefer_accessibility,
        window_title,
        window_id,
        button,
        from_ratio: from_ratio
            .ok_or_else(|| CliError::Failure("missing --from-ratio".to_owned()))?,
        to_ratio: to_ratio.ok_or_else(|| CliError::Failure("missing --to-ratio".to_owned()))?,
        duration_ms,
        dry_run,
        verify,
        json,
    }))
}

pub(super) fn parse_desktop_type_into_args(args: Vec<String>) -> Result<DesktopCommand, CliError> {
    let mut app = None;
    let mut target = None;
    let mut image = None;
    let mut prefer_accessibility = true;
    let mut window_title = None;
    let mut window_id = None;
    let mut clear = false;
    let mut dry_run = false;
    let mut verify = false;
    let mut json = false;
    let mut text_parts = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--app" | "-a" => app = Some(parse_next_string(&args, &mut index, "--app")?),
            "--target" | "-t" => target = Some(parse_next_string(&args, &mut index, "--target")?),
            "--window-title" | "--title" => {
                window_title = Some(parse_next_string(&args, &mut index, "--window-title")?)
            }
            "--window-id" => window_id = Some(parse_next_string(&args, &mut index, "--window-id")?),
            "--image" | "-i" => {
                image = Some(PathBuf::from(parse_next_string(
                    &args, &mut index, "--image",
                )?))
            }
            "--clear" => clear = true,
            "--dry-run" => dry_run = true,
            "--verify" => verify = true,
            "--json" => json = true,
            "--no-accessibility" => prefer_accessibility = false,
            "--help" | "-h" => return Ok(DesktopCommand::Help),
            value if value.starts_with('-') && text_parts.is_empty() => {
                return Err(CliError::Failure(format!(
                    "unknown desktop type-into argument: {value}"
                )));
            }
            value if app.is_none() => app = Some(value.to_owned()),
            value if target.is_none() => target = Some(value.to_owned()),
            value => text_parts.push(value.to_owned()),
        }
        index += 1;
    }

    let text = text_parts.join(" ");
    if text.is_empty() {
        return Err(CliError::Failure("missing text to type".to_owned()));
    }

    Ok(DesktopCommand::TypeInto(DesktopTypeIntoArgs {
        app: app.ok_or_else(|| CliError::Failure("missing --app".to_owned()))?,
        target: target.ok_or_else(|| CliError::Failure("missing --target".to_owned()))?,
        text,
        image,
        prefer_accessibility,
        window_title,
        window_id,
        clear,
        dry_run,
        verify,
        json,
    }))
}

pub(super) fn parse_desktop_assert_args(
    args: Vec<String>,
    negated: bool,
) -> Result<DesktopCommand, CliError> {
    let mut app = None;
    let mut target = None;
    let mut image = None;
    let mut prefer_accessibility = true;
    let mut window_title = None;
    let mut window_id = None;
    let mut assertion = None;
    let mut json = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--app" | "-a" => app = Some(parse_next_string(&args, &mut index, "--app")?),
            "--target" | "-t" => target = Some(parse_next_string(&args, &mut index, "--target")?),
            "--window-title" | "--title" => {
                window_title = Some(parse_next_string(&args, &mut index, "--window-title")?)
            }
            "--window-id" => window_id = Some(parse_next_string(&args, &mut index, "--window-id")?),
            "--image" | "-i" => {
                image = Some(PathBuf::from(parse_next_string(
                    &args, &mut index, "--image",
                )?))
            }
            "--present" => {
                assertion = Some(if negated {
                    DesktopAssertion::NotPresent
                } else {
                    DesktopAssertion::Present
                })
            }
            "--active" => {
                assertion = Some(if negated {
                    DesktopAssertion::NotActive
                } else {
                    DesktopAssertion::Active
                })
            }
            "--not-active" => {
                assertion = Some(if negated {
                    DesktopAssertion::Active
                } else {
                    DesktopAssertion::NotActive
                })
            }
            "--contains" => {
                let expected = parse_next_string(&args, &mut index, "--contains")?;
                assertion = Some(if negated {
                    DesktopAssertion::NotContains(expected)
                } else {
                    DesktopAssertion::Contains(expected)
                });
            }
            "--no-accessibility" => prefer_accessibility = false,
            "--json" => json = true,
            "--help" | "-h" => return Ok(DesktopCommand::Help),
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown desktop assert argument: {value}"
                )));
            }
            value if app.is_none() => app = Some(value.to_owned()),
            value if target.is_none() => target = Some(value.to_owned()),
            value => {
                return Err(CliError::Failure(format!(
                    "unexpected desktop assert argument: {value}"
                )));
            }
        }
        index += 1;
    }

    Ok(DesktopCommand::Assert(DesktopAssertArgs {
        app: app.ok_or_else(|| CliError::Failure("missing --app".to_owned()))?,
        target: target.ok_or_else(|| CliError::Failure("missing --target".to_owned()))?,
        image,
        prefer_accessibility,
        window_title,
        window_id,
        assertion: assertion.unwrap_or({
            if negated {
                DesktopAssertion::NotPresent
            } else {
                DesktopAssertion::Present
            }
        }),
        json,
    }))
}
