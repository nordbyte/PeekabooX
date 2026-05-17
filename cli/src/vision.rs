use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WindowsArgs {
    pub(super) json: bool,
    pub(super) id: Option<String>,
    pub(super) app: Option<String>,
    pub(super) title: Option<String>,
    pub(super) title_regex: Option<String>,
    pub(super) focused: bool,
    pub(super) limit: Option<usize>,
    pub(super) sort: peekaboox_windows::WindowSort,
    pub(super) backend: peekaboox_windows::WindowBackendSelection,
    pub(super) diagnose: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum WindowsCommand {
    Run(WindowsArgs),
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ElementsArgs {
    pub(super) selector: String,
    pub(super) limit: usize,
    pub(super) vision_fallback: bool,
    pub(super) app: Option<String>,
    pub(super) window_title: Option<String>,
    pub(super) window_id: Option<String>,
    pub(super) vision_region: Option<Rect>,
    pub(super) vision_edge_threshold: Option<u8>,
    pub(super) vision_min_width: Option<u32>,
    pub(super) vision_min_height: Option<u32>,
    pub(super) vision_min_component_pixels: Option<u32>,
    pub(super) vision_max_elements: Option<u32>,
    pub(super) vision_merge_distance: Option<u32>,
    pub(super) json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ElementsCommand {
    Run(ElementsArgs),
    Help,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct OcrArgs {
    pub(super) image: Option<PathBuf>,
    pub(super) region: Option<Rect>,
    pub(super) app: Option<String>,
    pub(super) window_title: Option<String>,
    pub(super) window_id: Option<String>,
    pub(super) language: Option<String>,
    pub(super) page_segmentation_mode: Option<u8>,
    pub(super) engine_mode: Option<u8>,
    pub(super) dpi: Option<u32>,
    pub(super) min_confidence: Option<f32>,
    pub(super) whitelist: Option<String>,
    pub(super) config: Vec<OcrConfig>,
    pub(super) preprocessing: OcrPreprocessingOptions,
    pub(super) json: bool,
    pub(super) words: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum OcrCommand {
    Run(Box<OcrArgs>),
    Help,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct CompareArgs {
    pub(super) expected: PathBuf,
    pub(super) actual: PathBuf,
    pub(super) region: Option<Rect>,
    pub(super) ignore_regions: Vec<Rect>,
    pub(super) per_channel_threshold: u8,
    pub(super) max_changed_ratio: f32,
    pub(super) max_changed_pixels: Option<u64>,
    pub(super) max_mean_absolute_error: Option<f32>,
    pub(super) max_channel_delta: Option<u8>,
    pub(super) size_policy: VisualSizePolicy,
    pub(super) alpha_mode: VisualAlphaMode,
    pub(super) diff_output: Option<PathBuf>,
    pub(super) report: Option<PathBuf>,
    pub(super) no_fail: bool,
    pub(super) json: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum CompareCommand {
    Run(CompareArgs),
    Help,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct UiStateArgs {
    pub(super) image_paths: Vec<PathBuf>,
    pub(super) region: Option<Rect>,
    pub(super) ignore_regions: Vec<Rect>,
    pub(super) per_channel_threshold: u8,
    pub(super) stable_max_changed_ratio: f32,
    pub(super) stable_max_changed_pixels: Option<u64>,
    pub(super) stable_max_mean_absolute_error: Option<f32>,
    pub(super) stable_max_channel_delta: Option<u8>,
    pub(super) loading_min_changed_ratio: f32,
    pub(super) loading_min_changed_pixels: Option<u64>,
    pub(super) required_stable_transitions: u32,
    pub(super) size_policy: VisualSizePolicy,
    pub(super) alpha_mode: VisualAlphaMode,
    pub(super) json: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum UiStateCommand {
    Run(UiStateArgs),
    Help,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct VisionElementsArgs {
    pub(super) image: PathBuf,
    pub(super) region: Option<Rect>,
    pub(super) ignore_regions: Vec<Rect>,
    pub(super) edge_threshold: u8,
    pub(super) min_width: u32,
    pub(super) min_height: u32,
    pub(super) min_component_pixels: u32,
    pub(super) min_confidence: Option<f32>,
    pub(super) max_width: Option<u32>,
    pub(super) max_height: Option<u32>,
    pub(super) min_area: Option<u64>,
    pub(super) max_area: Option<u64>,
    pub(super) max_elements: u32,
    pub(super) merge_distance: u32,
    pub(super) padding: u32,
    pub(super) sort: UiElementSort,
    pub(super) mask_output: Option<PathBuf>,
    pub(super) overlay_output: Option<PathBuf>,
    pub(super) json: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum VisionElementsCommand {
    Run(VisionElementsArgs),
    Help,
}

pub(super) fn windows(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let WindowsCommand::Run(args) = parse_windows_args(args)? else {
        print_windows_usage();
        return Err(CliError::HelpRequested);
    };
    let query = window_query_from_args(&args);

    if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::ListWindows {
                id: args.id.clone(),
                app: args.app.clone(),
                title: args.title.clone(),
                title_regex: args.title_regex.clone(),
                focused: args.focused,
                limit: args.limit,
                sort: Some(args.sort.name().to_owned()),
                backend: Some(args.backend.name().to_owned()),
                diagnose: args.diagnose,
            },
        )?;
        let ApiResult::ListWindows(metadata) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected windows response".to_owned(),
            ));
        };
        if args.json {
            print_json_pretty(&metadata)?;
        } else {
            print_window_dto_table(metadata, args.diagnose);
        }
        return Ok(());
    }

    let metadata = peekaboox_windows::list_windows_with_query(query)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    let metadata = window_list_dto(metadata);

    if args.json {
        print_json_pretty(&metadata)?;
        return Ok(());
    }

    for warning in &metadata.warnings {
        eprintln!("warning: {warning}");
    }

    if metadata.windows.is_empty() {
        println!("no windows found via {}", metadata.backend_name);
        if args.diagnose {
            print_window_backend_reports(&metadata.backend_reports);
        }
        return Ok(());
    }

    println!(
        "{:<14} {:<7} {:<10} {:<11} {:<11} {:<18} TITLE",
        "ID", "FOCUS", "STATE", "POSITION", "SIZE", "APP"
    );

    for window in metadata.windows {
        print_window_dto(window);
    }

    if args.diagnose {
        print_window_backend_reports(&metadata.backend_reports);
    }

    Ok(())
}

pub(super) fn window_query_from_args(args: &WindowsArgs) -> peekaboox_windows::WindowQuery {
    peekaboox_windows::WindowQuery {
        id: args.id.clone(),
        app: args.app.clone(),
        title: args.title.clone(),
        title_regex: args.title_regex.clone(),
        focused_only: args.focused,
        limit: args.limit,
        sort: args.sort,
        backend: args.backend,
        diagnose: args.diagnose,
    }
}

pub(super) fn print_window_dto_table(metadata: WindowListResultDto, diagnose: bool) {
    for warning in &metadata.warnings {
        eprintln!("warning: {warning}");
    }

    if metadata.windows.is_empty() {
        println!("no windows found via {}", metadata.backend_name);
        if diagnose {
            print_window_backend_reports(&metadata.backend_reports);
        }
        return;
    }

    println!(
        "{:<14} {:<7} {:<10} {:<11} {:<11} {:<18} TITLE",
        "ID", "FOCUS", "STATE", "POSITION", "SIZE", "APP"
    );

    for window in metadata.windows {
        print_window_dto(window);
    }

    if diagnose {
        print_window_backend_reports(&metadata.backend_reports);
    }
}

pub(super) fn print_window_dto(window: WindowDto) {
    println!(
        "{:<14} {:<7} {:<10} {:<11} {:<11} {:<18} {}",
        window.id,
        if window.focused { "yes" } else { "no" },
        window.state,
        format!("{},{}", window.bounds.x, window.bounds.y),
        format!("{}x{}", window.bounds.width, window.bounds.height),
        window.app_id.unwrap_or_else(|| "-".to_owned()),
        window.title
    );
}

pub(super) fn print_window_backend_reports(reports: &[WindowBackendReportDto]) {
    if reports.is_empty() {
        return;
    }

    println!();
    println!(
        "{:<24} {:<10} {:<5} {:<7} {:<8} ERROR",
        "BACKEND", "KIND", "RAW", "MATCH", "SELECTED"
    );

    for report in reports {
        println!(
            "{:<24} {:<10} {:<5} {:<7} {:<8} {}",
            report.backend_name,
            report.backend_kind,
            report.raw_window_count,
            report.matched_window_count,
            if report.selected { "yes" } else { "no" },
            report.error.as_deref().unwrap_or("-")
        );
    }
}

pub(super) fn window_list_dto(
    metadata: peekaboox_windows::WindowListMetadata,
) -> WindowListResultDto {
    WindowListResultDto {
        backend_name: metadata.backend_name,
        backend_kind: backend_kind_label(metadata.backend_kind),
        warnings: metadata.warnings,
        backend_reports: metadata
            .backend_reports
            .into_iter()
            .map(|report| WindowBackendReportDto {
                backend_name: report.backend_name,
                backend_kind: backend_kind_label(report.backend_kind),
                raw_window_count: report.raw_window_count,
                matched_window_count: report.matched_window_count,
                selected: report.selected,
                error: report.error,
            })
            .collect(),
        windows: metadata
            .windows
            .into_iter()
            .map(|window| WindowDto {
                id: window.id,
                title: window.title,
                app_id: window.app_id,
                bounds: window.bounds.into(),
                focused: window.focused,
                state: format!("{:?}", window.state).to_ascii_lowercase(),
            })
            .collect(),
    }
}

pub(super) fn parse_windows_args(args: Vec<String>) -> Result<WindowsCommand, CliError> {
    let mut json = false;
    let mut id = None;
    let mut app = None;
    let mut title = None;
    let mut title_regex = None;
    let mut focused = false;
    let mut limit = None;
    let mut sort = peekaboox_windows::WindowSort::Backend;
    let mut backend = peekaboox_windows::WindowBackendSelection::Auto;
    let mut diagnose = false;
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--id" => id = Some(require_next_arg(&mut iter, "--id")?),
            "--app" => app = Some(require_next_arg(&mut iter, "--app")?),
            "--title" => title = Some(require_next_arg(&mut iter, "--title")?),
            "--title-regex" => title_regex = Some(require_next_arg(&mut iter, "--title-regex")?),
            "--focused" => focused = true,
            "--limit" => {
                let value = require_next_arg(&mut iter, "--limit")?;
                limit = Some(parse_positive_usize("--limit", &value)?);
            }
            "--sort" => {
                let value = require_next_arg(&mut iter, "--sort")?;
                sort = peekaboox_windows::WindowSort::from_name(&value).ok_or_else(|| {
                    CliError::Failure(format!(
                        "invalid windows sort: {value}; expected backend, focused, title, app, area, id, or state"
                    ))
                })?;
            }
            "--backend" => {
                let value = require_next_arg(&mut iter, "--backend")?;
                backend = peekaboox_windows::WindowBackendSelection::from_name(&value).ok_or_else(
                    || {
                        CliError::Failure(format!(
                            "invalid windows backend: {value}; expected auto, gnome, at-spi, or xdotool"
                        ))
                    },
                )?;
            }
            "--diagnose" => diagnose = true,
            "--help" | "-h" => return Ok(WindowsCommand::Help),
            unknown => {
                return Err(CliError::Failure(format!(
                    "unknown windows argument: {unknown}"
                )));
            }
        }
    }
    Ok(WindowsCommand::Run(WindowsArgs {
        json,
        id,
        app,
        title,
        title_regex,
        focused,
        limit,
        sort,
        backend,
        diagnose,
    }))
}

pub(super) fn elements(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let ElementsCommand::Run(args) = parse_elements_args(args)? else {
        print_elements_usage();
        return Err(CliError::HelpRequested);
    };

    if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::FindElements {
                selector: args.selector.clone(),
                vision_fallback: args.vision_fallback,
                app: args.app.clone(),
                window_title: args.window_title.clone(),
                window_id: args.window_id.clone(),
                vision_region: args.vision_region.map(RectDto::from),
                vision_edge_threshold: args.vision_edge_threshold,
                vision_min_width: args.vision_min_width,
                vision_min_height: args.vision_min_height,
                vision_min_component_pixels: args.vision_min_component_pixels,
                vision_max_elements: args.vision_max_elements,
                vision_merge_distance: args.vision_merge_distance,
            },
        )?;
        let ApiResult::FindElements(metadata) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected elements response".to_owned(),
            ));
        };
        if args.json {
            print_json_pretty(&limited_element_list_dto(metadata, args.limit))?;
        } else {
            print_element_dto_table(metadata, args.limit);
        }
        return Ok(());
    }

    let metadata = find_elements_metadata(&args)?;
    if args.json {
        print_json_pretty(&limited_element_list_dto(
            element_list_dto(metadata),
            args.limit,
        ))?;
    } else {
        print_element_table(metadata, args.limit);
    }

    Ok(())
}

pub(super) fn find_elements_metadata(
    args: &ElementsArgs,
) -> Result<AccessibilityTreeMetadata, CliError> {
    let query = ElementQuery::parse(&args.selector)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    match peekaboox_accessibility::semantic_tree() {
        Ok(mut metadata) => {
            metadata.elements.retain(|element| {
                query.matches(element) && element_matches_cli_scope(element, args)
            });
            if !metadata.elements.is_empty() || !args.vision_fallback {
                return Ok(metadata);
            }

            let mut fallback = vision_fallback_metadata(&query, args)?;
            fallback
                .warnings
                .push("no accessibility elements matched; used vision fallback".to_owned());
            Ok(fallback)
        }
        Err(error) if args.vision_fallback => {
            let mut fallback = vision_fallback_metadata(&query, args)?;
            fallback.warnings.push(format!(
                "accessibility lookup failed: {error}; used vision fallback"
            ));
            Ok(fallback)
        }
        Err(error) => Err(CliError::Failure(error.to_string())),
    }
}

pub(super) fn vision_fallback_metadata(
    query: &ElementQuery,
    args: &ElementsArgs,
) -> Result<AccessibilityTreeMetadata, CliError> {
    let screenshot = vision_fallback_temp_path();
    let capture_region = vision_capture_region_from_elements_args(args)?;
    capture_cli_to_file(&screenshot, capture_region)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    let options = element_vision_options_from_elements_args(args)?;
    let result = peekaboox_vision::detect_ui_elements_from_image_file(&screenshot, &options)
        .map_err(|error| CliError::Failure(error.to_string()));
    remove_temp_file(&screenshot, "vision fallback screenshot");

    let mut elements = result?;
    apply_elements_scope_metadata(&mut elements, args, capture_region);
    elements.retain(|element| query.matches(element));
    Ok(AccessibilityTreeMetadata {
        backend_name: "heuristic_vision".to_owned(),
        backend_kind: BackendKind::Vision,
        warnings: Vec::new(),
        elements,
    })
}

pub(super) fn element_matches_cli_scope(element: &UiElement, args: &ElementsArgs) -> bool {
    args.window_id
        .as_deref()
        .is_none_or(|window_id| element.window_id.as_deref() == Some(window_id))
        && args.window_title.as_deref().is_none_or(|window_title| {
            element
                .window_title
                .as_deref()
                .is_some_and(|value| contains_case_insensitive(value, window_title))
        })
        && args.app.as_deref().is_none_or(|app| {
            element
                .app_id
                .as_deref()
                .is_some_and(|value| contains_case_insensitive(value, app))
        })
}

pub(super) fn vision_capture_region_from_elements_args(
    args: &ElementsArgs,
) -> Result<Option<Rect>, CliError> {
    if args.vision_region.is_some() {
        return Ok(args.vision_region);
    }
    if args.window_id.is_none() && args.window_title.is_none() && args.app.is_none() {
        return Ok(None);
    }
    let window = resolve_window_for_ocr(
        args.window_id.as_deref(),
        args.window_title.as_deref(),
        args.app.as_deref(),
    )?;
    Ok(Some(window.bounds))
}

pub(super) fn element_vision_options_from_elements_args(
    args: &ElementsArgs,
) -> Result<UiElementDetectionOptions, CliError> {
    let defaults = UiElementDetectionOptions::default();
    Ok(UiElementDetectionOptions {
        region: None,
        edge_threshold: args
            .vision_edge_threshold
            .unwrap_or(defaults.edge_threshold),
        min_width: args.vision_min_width.unwrap_or(defaults.min_width),
        min_height: args.vision_min_height.unwrap_or(defaults.min_height),
        min_component_pixels: args
            .vision_min_component_pixels
            .unwrap_or(defaults.min_component_pixels),
        max_elements: args
            .vision_max_elements
            .map(usize::try_from)
            .transpose()
            .map_err(|_| CliError::Failure("--vision-max-elements is too large".to_owned()))?
            .unwrap_or(defaults.max_elements),
        merge_distance: args
            .vision_merge_distance
            .unwrap_or(defaults.merge_distance),
        ..defaults
    })
}

pub(super) fn apply_elements_scope_metadata(
    elements: &mut [UiElement],
    args: &ElementsArgs,
    capture_region: Option<Rect>,
) {
    for element in elements {
        if let Some(region) = capture_region {
            element.bounds.x += region.x;
            element.bounds.y += region.y;
            element.center = element.bounds.center();
        }
        element.window_id.clone_from(&args.window_id);
        element.window_title.clone_from(&args.window_title);
        element.app_id.clone_from(&args.app);
    }
}

pub(super) fn vision_fallback_temp_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "peekaboox-vision-fallback-{}-{}.png",
        std::process::id(),
        monotonic_ms()
    ))
}

pub(super) fn remove_temp_file(path: &PathBuf, description: &str) {
    if let Err(error) = std::fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        eprintln!("failed to remove {description} {}: {error}", path.display());
    }
}

pub(super) fn monotonic_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

pub(super) fn print_element_table(mut metadata: AccessibilityTreeMetadata, limit: usize) {
    for warning in metadata.warnings {
        eprintln!("warning: {warning}");
    }

    if metadata.elements.is_empty() {
        println!("no elements found via {}", metadata.backend_name);
        return;
    }

    let total = metadata.elements.len();
    if limit > 0 && metadata.elements.len() > limit {
        metadata.elements.truncate(limit);
    }

    println!(
        "{:<20} {:<5} {:<11} {:<11} {:<24} LABEL",
        "ROLE", "CONF", "POSITION", "SIZE", "STATES"
    );

    for element in metadata.elements {
        print_element(element);
    }

    if limit > 0 && total > limit {
        eprintln!("showing {limit} of {total} elements");
    }
}

pub(super) fn print_element(element: UiElement) {
    println!(
        "{:<20} {:<5} {:<11} {:<11} {:<24} {}",
        element.role,
        format!("{:.2}", element.confidence),
        format!("{},{}", element.bounds.x, element.bounds.y),
        format!("{}x{}", element.bounds.width, element.bounds.height),
        format_states(&element.states),
        element.label.unwrap_or_else(|| "-".to_owned())
    );
}

pub(super) fn print_element_dto_table(mut metadata: ElementListResultDto, limit: usize) {
    for warning in metadata.warnings {
        eprintln!("warning: {warning}");
    }

    if metadata.elements.is_empty() {
        println!("no elements found via {}", metadata.backend_name);
        return;
    }

    let total = metadata.elements.len();
    if limit > 0 && metadata.elements.len() > limit {
        metadata.elements.truncate(limit);
    }

    println!(
        "{:<20} {:<5} {:<11} {:<11} {:<24} LABEL",
        "ROLE", "CONF", "POSITION", "SIZE", "STATES"
    );

    for element in metadata.elements {
        print_element_dto(element);
    }

    if limit > 0 && total > limit {
        eprintln!("showing {limit} of {total} elements");
    }
}

pub(super) fn print_element_dto(element: ElementDto) {
    println!(
        "{:<20} {:<5} {:<11} {:<11} {:<24} {}",
        element.role,
        format!("{:.2}", element.confidence),
        format!("{},{}", element.bounds.x, element.bounds.y),
        format!("{}x{}", element.bounds.width, element.bounds.height),
        format_states(&element.states),
        element.label.unwrap_or_else(|| "-".to_owned())
    );
}

pub(super) fn element_list_dto(metadata: AccessibilityTreeMetadata) -> ElementListResultDto {
    ElementListResultDto {
        backend_name: metadata.backend_name,
        backend_kind: backend_kind_label(metadata.backend_kind),
        warnings: metadata.warnings,
        cache_hit: false,
        cache_age_ms: 0,
        vision_fallback_used: metadata.backend_kind == BackendKind::Vision,
        elements: metadata.elements.into_iter().map(element_dto).collect(),
    }
}

pub(super) fn limited_element_list_dto(
    mut metadata: ElementListResultDto,
    limit: usize,
) -> ElementListResultDto {
    if limit > 0 && metadata.elements.len() > limit {
        metadata.elements.truncate(limit);
    }
    metadata
}

pub(super) fn element_dto(element: UiElement) -> ElementDto {
    ElementDto {
        id: element.id,
        role: element.role,
        label: element.label,
        bounds: element.bounds.into(),
        center: element
            .center
            .or_else(|| element.bounds.center())
            .map(Into::into),
        confidence: element.confidence,
        states: element.states,
        window_id: element.window_id,
        window_title: element.window_title,
        app_id: element.app_id,
        parent_id: element.parent_id,
        child_ids: element.child_ids,
    }
}

pub(super) fn format_states(states: &[String]) -> String {
    if states.is_empty() {
        "-".to_owned()
    } else {
        states.join("|")
    }
}

pub(super) fn parse_elements_args(args: Vec<String>) -> Result<ElementsCommand, CliError> {
    let mut selector = None;
    let mut selector_parts = Vec::new();
    let mut limit = 50;
    let mut vision_fallback = false;
    let mut app = None;
    let mut window_title = None;
    let mut window_id = None;
    let mut vision_region = None;
    let mut vision_edge_threshold = None;
    let mut vision_min_width = None;
    let mut vision_min_height = None;
    let mut vision_min_component_pixels = None;
    let mut vision_max_elements = None;
    let mut vision_merge_distance = None;
    let mut json = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--selector" | "-s" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --selector".to_owned()));
                };
                selector = Some(value.to_owned());
            }
            "--id" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --id".to_owned()));
                };
                selector_parts.push(format!("id={value}"));
            }
            "--role" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --role".to_owned()));
                };
                selector_parts.push(format!("role={value}"));
            }
            "--role-exact" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --role-exact".to_owned(),
                    ));
                };
                selector_parts.push(format!("role-exact={value}"));
            }
            "--role-regex" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --role-regex".to_owned(),
                    ));
                };
                selector_parts.push(format!("role-regex={value}"));
            }
            "--label" | "--text" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(format!(
                        "missing value for {}",
                        args[index - 1]
                    )));
                };
                selector_parts.push(format!("label={value}"));
            }
            "--label-exact" | "--text-exact" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(format!(
                        "missing value for {}",
                        args[index - 1]
                    )));
                };
                selector_parts.push(format!("label-exact={value}"));
            }
            "--label-regex" | "--text-regex" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(format!(
                        "missing value for {}",
                        args[index - 1]
                    )));
                };
                selector_parts.push(format!("label-regex={value}"));
            }
            "--state" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --state".to_owned()));
                };
                selector_parts.push(format!("state={value}"));
            }
            "--not-state" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --not-state".to_owned(),
                    ));
                };
                selector_parts.push(format!("not-state={value}"));
            }
            "--bounds" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --bounds".to_owned()));
                };
                selector_parts.push(format!("bounds={value}"));
            }
            "--contains" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --contains".to_owned()));
                };
                selector_parts.push(format!("contains={value}"));
            }
            "--within" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --within".to_owned()));
                };
                selector_parts.push(format!("within={value}"));
            }
            "--intersects" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --intersects".to_owned(),
                    ));
                };
                selector_parts.push(format!("intersects={value}"));
            }
            "--min-width" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --min-width".to_owned(),
                    ));
                };
                parse_positive_u32("--min-width", value)?;
                selector_parts.push(format!("min-width={value}"));
            }
            "--min-height" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --min-height".to_owned(),
                    ));
                };
                parse_positive_u32("--min-height", value)?;
                selector_parts.push(format!("min-height={value}"));
            }
            "--min-confidence" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --min-confidence".to_owned(),
                    ));
                };
                value.parse::<f32>().map_err(|_| {
                    CliError::Failure(format!("--min-confidence must be a float, got {value:?}"))
                })?;
                selector_parts.push(format!("confidence>={value}"));
            }
            "--limit" | "-n" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --limit".to_owned()));
                };
                limit = parse_usize("--limit", value)?;
            }
            "--vision-fallback" => vision_fallback = true,
            "--app" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --app".to_owned()));
                };
                app = non_empty_cli_string(value);
            }
            "--window-title" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --window-title".to_owned(),
                    ));
                };
                window_title = non_empty_cli_string(value);
            }
            "--window-id" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --window-id".to_owned(),
                    ));
                };
                window_id = non_empty_cli_string(value);
            }
            "--vision-region" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --vision-region".to_owned(),
                    ));
                };
                vision_region = Some(parse_rect("--vision-region", value)?);
            }
            "--vision-threshold" | "--vision-edge-threshold" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --vision-threshold".to_owned(),
                    ));
                };
                let threshold = parse_u8("--vision-threshold", value)?;
                if threshold == 0 {
                    return Err(CliError::Failure(
                        "--vision-threshold must be greater than zero".to_owned(),
                    ));
                }
                vision_edge_threshold = Some(threshold);
            }
            "--vision-min-width" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --vision-min-width".to_owned(),
                    ));
                };
                vision_min_width = Some(parse_positive_u32("--vision-min-width", value)?);
            }
            "--vision-min-height" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --vision-min-height".to_owned(),
                    ));
                };
                vision_min_height = Some(parse_positive_u32("--vision-min-height", value)?);
            }
            "--vision-min-component-pixels" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --vision-min-component-pixels".to_owned(),
                    ));
                };
                vision_min_component_pixels =
                    Some(parse_positive_u32("--vision-min-component-pixels", value)?);
            }
            "--vision-max-elements" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --vision-max-elements".to_owned(),
                    ));
                };
                vision_max_elements = Some(parse_positive_u32("--vision-max-elements", value)?);
            }
            "--vision-merge-distance" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --vision-merge-distance".to_owned(),
                    ));
                };
                vision_merge_distance = Some(value.parse::<u32>().map_err(|_| {
                    CliError::Failure(format!(
                        "--vision-merge-distance must be an integer, got {value:?}"
                    ))
                })?);
            }
            "--json" => json = true,
            "--help" | "-h" => return Ok(ElementsCommand::Help),
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown elements argument: {value}"
                )));
            }
            value => {
                if selector.is_some() {
                    return Err(CliError::Failure(
                        "provide only one positional selector".to_owned(),
                    ));
                }
                selector = Some(value.to_owned());
            }
        }

        index += 1;
    }

    let selector = match selector {
        Some(selector) if selector_parts.is_empty() => selector,
        Some(selector) => {
            selector_parts.insert(0, selector);
            selector_parts.join(",")
        }
        None => selector_parts.join(","),
    };
    ElementQuery::parse(&selector).map_err(|error| CliError::Failure(error.to_string()))?;

    Ok(ElementsCommand::Run(ElementsArgs {
        selector,
        limit,
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
        json,
    }))
}

pub(super) fn ocr(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let OcrCommand::Run(args) = parse_ocr_args(args)? else {
        print_ocr_usage();
        return Err(CliError::HelpRequested);
    };
    let args = *args;

    if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::Ocr {
                image_path: args.image.as_ref().map(path_to_daemon_string).transpose()?,
                region: args.region.map(RectDto::from),
                app: args.app.clone(),
                window_title: args.window_title.clone(),
                window_id: args.window_id.clone(),
                language: args.language.clone(),
                page_segmentation_mode: args.page_segmentation_mode,
                engine_mode: args.engine_mode,
                dpi: args.dpi,
                min_confidence: args.min_confidence,
                whitelist: args.whitelist.clone(),
                config: args
                    .config
                    .iter()
                    .map(|config| format!("{}={}", config.key, config.value))
                    .collect(),
                scale: args.preprocessing.scale,
                grayscale: args.preprocessing.grayscale,
                threshold: args.preprocessing.threshold,
                invert: args.preprocessing.invert,
                contrast: args.preprocessing.contrast,
                deskew: args.preprocessing.deskew,
            },
        )?;
        let ApiResult::Ocr(result) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected OCR response".to_owned(),
            ));
        };
        if args.json {
            print_json_pretty(&result)?;
        } else if args.words {
            print_ocr_dto_words(result);
        } else {
            print_ocr_dto_result(result);
        }
        return Ok(());
    }

    let backend = TesseractOcrBackend::new("tesseract", ocr_options(&args));
    if !backend.is_available() {
        return Err(CliError::Failure(
            "OCR backend tesseract is not available; install tesseract-ocr".to_owned(),
        ));
    }

    let result = if let Some(image) = args.image.as_ref() {
        peekaboox_vision::ocr_image_file_with_backend(&backend, image, args.region)
    } else {
        let region = ocr_capture_region_from_args(&args)?;
        match region {
            Some(region) => peekaboox_vision::ocr_region_with_backend(&backend, region),
            None => peekaboox_vision::ocr_screen_with_backend(&backend),
        }
    }
    .map_err(|error| CliError::Failure(error.to_string()))?;
    if args.json {
        print_json_pretty(&ocr_result_dto(result))?;
    } else if args.words {
        print_ocr_words(result);
    } else {
        print_ocr_result(result);
    }

    Ok(())
}

pub(super) fn parse_ocr_args(args: Vec<String>) -> Result<OcrCommand, CliError> {
    let mut image = None;
    let mut region = None;
    let mut app = None;
    let mut window_title = None;
    let mut window_id = None;
    let mut language = None;
    let mut page_segmentation_mode = None;
    let mut engine_mode = None;
    let mut dpi = None;
    let mut min_confidence = None;
    let mut whitelist = None;
    let mut config = Vec::new();
    let mut preprocessing = OcrPreprocessingOptions::default();
    let mut json = false;
    let mut words = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--image" | "-i" => {
                image = Some(PathBuf::from(parse_next_string(
                    &args, &mut index, "--image",
                )?));
            }
            "--region" | "-r" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --region".to_owned()));
                };
                region = Some(parse_rect("--region", value)?);
            }
            "--language" | "-l" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --language".to_owned()));
                };
                language = non_empty_cli_string(value);
            }
            "--app" | "-a" => app = Some(parse_next_string(&args, &mut index, "--app")?),
            "--window-title" | "--title" => {
                window_title = Some(parse_next_string(&args, &mut index, "--window-title")?)
            }
            "--window-id" | "--window" => {
                window_id = Some(parse_next_string(&args, &mut index, "--window-id")?)
            }
            "--psm" | "--page-segmentation-mode" => {
                page_segmentation_mode = Some(parse_ocr_psm(&parse_next_string(
                    &args, &mut index, "--psm",
                )?)?);
            }
            "--oem" | "--engine-mode" => {
                engine_mode = Some(parse_ocr_oem(&parse_next_string(
                    &args, &mut index, "--oem",
                )?)?);
            }
            "--dpi" => {
                dpi = Some(parse_positive_u32(
                    "--dpi",
                    &parse_next_string(&args, &mut index, "--dpi")?,
                )?)
            }
            "--min-confidence" => {
                min_confidence = Some(parse_ocr_confidence(&parse_next_string(
                    &args,
                    &mut index,
                    "--min-confidence",
                )?)?);
            }
            "--whitelist" => {
                whitelist =
                    non_empty_cli_string(&parse_next_string(&args, &mut index, "--whitelist")?);
            }
            "--config" | "-c" => {
                config.push(parse_ocr_config(&parse_next_string(
                    &args, &mut index, "--config",
                )?)?);
            }
            "--scale" => {
                preprocessing.scale = Some(parse_ocr_scale(&parse_next_string(
                    &args, &mut index, "--scale",
                )?)?);
            }
            "--grayscale" | "--greyscale" => preprocessing.grayscale = true,
            "--threshold" => {
                preprocessing.threshold = Some(parse_u8(
                    "--threshold",
                    &parse_next_string(&args, &mut index, "--threshold")?,
                )?);
            }
            "--invert" => preprocessing.invert = true,
            "--contrast" => {
                preprocessing.contrast = Some(parse_ocr_contrast(&parse_next_string(
                    &args,
                    &mut index,
                    "--contrast",
                )?)?);
            }
            "--deskew" => preprocessing.deskew = true,
            "--words" => words = true,
            "--json" => json = true,
            "--help" | "-h" => return Ok(OcrCommand::Help),
            unknown => {
                return Err(CliError::Failure(format!(
                    "unknown ocr argument: {unknown}"
                )));
            }
        }

        index += 1;
    }

    Ok(OcrCommand::Run(Box::new(OcrArgs {
        image,
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
        preprocessing,
        json,
        words,
    })))
}

pub(super) fn print_ocr_result(result: OcrResult) {
    for warning in result.warnings {
        eprintln!("warning: {warning}");
    }

    if result.text.trim().is_empty() {
        println!("no OCR text found via {}", result.backend_name);
    } else {
        println!("{}", result.text);
    }
}

pub(super) fn print_ocr_words(result: OcrResult) {
    for warning in result.warnings {
        eprintln!("warning: {warning}");
    }

    if result.words.is_empty() {
        println!("no OCR words found via {}", result.backend_name);
        return;
    }

    println!("{:<5} {:<11} {:<11} TEXT", "CONF", "POSITION", "SIZE");
    for word in result.words {
        println!(
            "{:<5} {:<11} {:<11} {}",
            format!("{:.2}", word.element.confidence),
            format!("{},{}", word.element.bounds.x, word.element.bounds.y),
            format!(
                "{}x{}",
                word.element.bounds.width, word.element.bounds.height
            ),
            word.text
        );
    }
}

pub(super) fn print_ocr_dto_result(result: OcrResultDto) {
    for warning in result.warnings {
        eprintln!("warning: {warning}");
    }

    if result.text.trim().is_empty() {
        println!("no OCR text found via {}", result.backend_name);
    } else {
        println!("{}", result.text);
    }
}

pub(super) fn print_ocr_dto_words(result: OcrResultDto) {
    for warning in result.warnings {
        eprintln!("warning: {warning}");
    }

    if result.words.is_empty() {
        println!("no OCR words found via {}", result.backend_name);
        return;
    }

    println!("{:<5} {:<11} {:<11} TEXT", "CONF", "POSITION", "SIZE");
    for word in result.words {
        println!(
            "{:<5} {:<11} {:<11} {}",
            format!("{:.2}", word.element.confidence),
            format!("{},{}", word.element.bounds.x, word.element.bounds.y),
            format!(
                "{}x{}",
                word.element.bounds.width, word.element.bounds.height
            ),
            word.text
        );
    }
}

pub(super) fn ocr_result_dto(result: OcrResult) -> OcrResultDto {
    OcrResultDto {
        backend_name: result.backend_name,
        text: result.text,
        blocks: result
            .blocks
            .into_iter()
            .map(|block| OcrBlockDto {
                text: block.text,
                element: element_dto(block.element),
            })
            .collect(),
        words: result
            .words
            .into_iter()
            .map(|word| OcrBlockDto {
                text: word.text,
                element: element_dto(word.element),
            })
            .collect(),
        warnings: result.warnings,
    }
}

pub(super) fn ocr_options(args: &OcrArgs) -> OcrOptions {
    let mut options = OcrOptions::default();
    if let Some(language) = args.language.clone() {
        options.language = Some(language);
    }
    if let Some(psm) = args.page_segmentation_mode {
        options.page_segmentation_mode = Some(psm);
    }
    if let Some(oem) = args.engine_mode {
        options.engine_mode = Some(oem);
    }
    if let Some(dpi) = args.dpi {
        options.dpi = Some(dpi);
    }
    if let Some(min_confidence) = args.min_confidence {
        options.min_confidence = min_confidence;
    }
    if let Some(whitelist) = args.whitelist.clone() {
        options.whitelist = Some(whitelist);
    }
    options.config = args.config.clone();
    options.preprocessing = args.preprocessing.clone();
    options
}

pub(super) fn ocr_capture_region_from_args(args: &OcrArgs) -> Result<Option<Rect>, CliError> {
    if args.window_id.is_none() && args.window_title.is_none() && args.app.is_none() {
        return Ok(args.region);
    }
    let window = resolve_window_for_ocr(
        args.window_id.as_deref(),
        args.window_title.as_deref(),
        args.app.as_deref(),
    )?;
    if window.bounds.width == 0 || window.bounds.height == 0 {
        return Err(CliError::Failure(format!(
            "window {} has empty bounds",
            window.id
        )));
    }
    let region = match args.region {
        Some(region) => offset_region(window.bounds, region)?,
        None => window.bounds,
    };
    Ok(Some(region))
}

pub(super) fn resolve_window_for_ocr(
    window_id: Option<&str>,
    window_title: Option<&str>,
    app: Option<&str>,
) -> Result<peekaboox_core::WindowInfo, CliError> {
    let metadata =
        peekaboox_windows::list_windows().map_err(|error| CliError::Failure(error.to_string()))?;
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
        return Err(CliError::Failure(
            "no window matched OCR window filters".to_owned(),
        ));
    }
    matches.sort_by_key(|window| !window.focused);
    Ok(matches.remove(0))
}

pub(super) fn offset_region(origin: Rect, region: Rect) -> Result<Rect, CliError> {
    let x = i64::from(origin.x) + i64::from(region.x);
    let y = i64::from(origin.y) + i64::from(region.y);
    let x = i32::try_from(x)
        .map_err(|_| CliError::Failure("OCR region x coordinate overflows i32".to_owned()))?;
    let y = i32::try_from(y)
        .map_err(|_| CliError::Failure("OCR region y coordinate overflows i32".to_owned()))?;
    Ok(Rect::new(x, y, region.width, region.height))
}

pub(super) fn compare(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let CompareCommand::Run(args) = parse_compare_args(args)? else {
        print_compare_usage();
        return Err(CliError::HelpRequested);
    };

    if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::CompareImages {
                expected_path: path_to_daemon_string(&args.expected)?,
                actual_path: path_to_daemon_string(&args.actual)?,
                region: args.region.map(RectDto::from),
                ignore_regions: args
                    .ignore_regions
                    .iter()
                    .copied()
                    .map(RectDto::from)
                    .collect(),
                per_channel_threshold: args.per_channel_threshold,
                max_changed_ratio: args.max_changed_ratio,
                max_changed_pixels: args.max_changed_pixels,
                max_mean_absolute_error: args.max_mean_absolute_error,
                max_channel_delta: args.max_channel_delta,
                size_policy: visual_size_policy_name(args.size_policy).to_owned(),
                alpha_mode: visual_alpha_mode_name(args.alpha_mode).to_owned(),
                diff_output: args
                    .diff_output
                    .as_ref()
                    .map(path_to_daemon_string)
                    .transpose()?,
            },
        )?;
        let ApiResult::VisualDiff(result) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected visual diff response".to_owned(),
            ));
        };
        if args.json {
            print_json_pretty(&result)?;
        } else {
            print_visual_diff_dto(&result);
        }
        if let Some(report) = &args.report {
            write_json_pretty_file(report, &result)?;
        }
        return if args.no_fail {
            Ok(())
        } else {
            visual_diff_exit_status(result.matches)
        };
    }

    let options = visual_compare_options(&args);
    let result = if let Some(diff_output) = &args.diff_output {
        peekaboox_vision::write_visual_diff_image_file(
            &args.expected,
            &args.actual,
            diff_output,
            &options,
        )
    } else {
        peekaboox_vision::compare_image_files(&args.expected, &args.actual, &options)
    }
    .map_err(|error| CliError::Failure(error.to_string()))?;
    let result_dto = visual_diff_dto(&result);
    if args.json {
        print_json_pretty(&result_dto)?;
    } else {
        print_visual_diff(&result);
    }
    if let Some(report) = &args.report {
        write_json_pretty_file(report, &result_dto)?;
    }
    if args.no_fail {
        Ok(())
    } else {
        visual_diff_exit_status(result.matches)
    }
}

pub(super) fn parse_compare_args(args: Vec<String>) -> Result<CompareCommand, CliError> {
    let mut expected = None;
    let mut actual = None;
    let mut region = None;
    let mut ignore_regions = Vec::new();
    let mut per_channel_threshold = 0_u8;
    let mut max_changed_ratio = 0.0_f32;
    let mut max_changed_pixels = None;
    let mut max_mean_absolute_error = None;
    let mut max_channel_delta = None;
    let mut size_policy = VisualSizePolicy::Error;
    let mut alpha_mode = VisualAlphaMode::Ignore;
    let mut diff_output = None;
    let mut report = None;
    let mut no_fail = false;
    let mut json = false;
    let mut positional = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--expected" | "-e" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --expected".to_owned()));
                };
                expected = Some(PathBuf::from(value));
            }
            "--actual" | "-a" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --actual".to_owned()));
                };
                actual = Some(PathBuf::from(value));
            }
            "--region" | "-r" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --region".to_owned()));
                };
                region = Some(parse_rect("--region", value)?);
            }
            "--ignore-region" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --ignore-region".to_owned(),
                    ));
                };
                ignore_regions.push(parse_rect("--ignore-region", value)?);
            }
            "--threshold" | "-t" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --threshold".to_owned(),
                    ));
                };
                per_channel_threshold = parse_u8("--threshold", value)?;
            }
            "--max-changed-ratio" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --max-changed-ratio".to_owned(),
                    ));
                };
                max_changed_ratio = parse_unit_f32("--max-changed-ratio", value)?;
            }
            "--max-changed-pixels" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --max-changed-pixels".to_owned(),
                    ));
                };
                max_changed_pixels = Some(parse_u64("--max-changed-pixels", value)?);
            }
            "--max-mae" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --max-mae".to_owned()));
                };
                max_mean_absolute_error = Some(parse_visual_mae("--max-mae", value)?);
            }
            "--max-channel-delta" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --max-channel-delta".to_owned(),
                    ));
                };
                max_channel_delta = Some(parse_u8("--max-channel-delta", value)?);
            }
            "--size-policy" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --size-policy".to_owned(),
                    ));
                };
                size_policy = parse_visual_size_policy(value)?;
            }
            "--alpha" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --alpha".to_owned()));
                };
                alpha_mode = parse_visual_alpha_mode(value)?;
            }
            "--diff-output" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --diff-output".to_owned(),
                    ));
                };
                diff_output = Some(PathBuf::from(value));
            }
            "--report" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --report".to_owned()));
                };
                report = Some(PathBuf::from(value));
            }
            "--no-fail" => no_fail = true,
            "--json" => json = true,
            "--help" | "-h" => return Ok(CompareCommand::Help),
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown compare argument: {value}"
                )));
            }
            value => positional.push(PathBuf::from(value)),
        }

        index += 1;
    }

    match positional.as_slice() {
        [] => {}
        [expected_path] if expected.is_none() => expected = Some(expected_path.clone()),
        [actual_path] if actual.is_none() => actual = Some(actual_path.clone()),
        [expected_path, actual_path] => {
            if expected.is_none() {
                expected = Some(expected_path.clone());
            }
            if actual.is_none() {
                actual = Some(actual_path.clone());
            }
        }
        _ => {
            return Err(CliError::Failure(
                "compare accepts at most two positional paths".to_owned(),
            ));
        }
    }

    let expected =
        expected.ok_or_else(|| CliError::Failure("missing --expected image path".to_owned()))?;
    let actual =
        actual.ok_or_else(|| CliError::Failure("missing --actual image path".to_owned()))?;

    Ok(CompareCommand::Run(CompareArgs {
        expected,
        actual,
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
        report,
        no_fail,
        json,
    }))
}

pub(super) fn visual_compare_options(args: &CompareArgs) -> VisualCompareOptions {
    VisualCompareOptions {
        region: args.region,
        ignore_regions: args.ignore_regions.clone(),
        per_channel_threshold: args.per_channel_threshold,
        max_changed_ratio: args.max_changed_ratio,
        max_changed_pixels: args.max_changed_pixels,
        max_mean_absolute_error: args.max_mean_absolute_error,
        max_channel_delta: args.max_channel_delta,
        size_policy: args.size_policy,
        alpha_mode: args.alpha_mode,
    }
}

pub(super) fn print_visual_diff(result: &VisualDiffResult) {
    println!(
        "matches={} changed={}/{} ratio={:.6} mae={:.3} max_delta={} region={} changed_bounds={}",
        result.matches,
        result.changed_pixels,
        result.compared_pixels,
        result.changed_ratio,
        result.mean_absolute_error,
        result.max_channel_delta,
        format_rect(result.compared_region),
        result
            .changed_bounds
            .map(format_rect)
            .unwrap_or_else(|| "-".to_owned())
    );
}

pub(super) fn print_visual_diff_dto(result: &VisualDiffDto) {
    println!(
        "matches={} changed={}/{} ratio={:.6} mae={:.3} max_delta={} region={} changed_bounds={}",
        result.matches,
        result.changed_pixels,
        result.compared_pixels,
        result.changed_ratio,
        result.mean_absolute_error,
        result.max_channel_delta,
        format_rect(Rect::from(result.compared_region)),
        result
            .changed_bounds
            .map(Rect::from)
            .map(format_rect)
            .unwrap_or_else(|| "-".to_owned())
    );
}

pub(super) fn visual_diff_dto(result: &VisualDiffResult) -> VisualDiffDto {
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

pub(super) fn print_capture_delta_dto(result: &CaptureDeltaResultDto) {
    println!(
        "stream={} sequence={} low_bandwidth={} full_frame={} frame={}x{} format={} capture_region={} changed_pixels={} ratio={:.6} changed_bounds={} patch_stride={} patch_base64_bytes={} backend={}",
        result.stream_id,
        result.sequence,
        result.low_bandwidth,
        result.full_frame,
        result.frame_width,
        result.frame_height,
        result.pixel_format,
        result
            .capture_region
            .map(Rect::from)
            .map(format_rect)
            .unwrap_or_else(|| "-".to_owned()),
        result.changed_pixels,
        result.changed_ratio,
        result
            .changed_bounds
            .map(Rect::from)
            .map(format_rect)
            .unwrap_or_else(|| "-".to_owned()),
        result.patch_stride,
        result.patch_base64.len(),
        result.backend_name
    );
}

pub(super) fn visual_diff_exit_status(matches: bool) -> Result<(), CliError> {
    if matches {
        Ok(())
    } else {
        Err(CliError::Failure(
            "visual comparison did not match tolerance".to_owned(),
        ))
    }
}

pub(super) fn ui_state(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let UiStateCommand::Run(args) = parse_ui_state_args(args)? else {
        print_ui_state_usage();
        return Err(CliError::HelpRequested);
    };

    if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::DetectUiState {
                image_paths: args
                    .image_paths
                    .iter()
                    .map(path_to_daemon_string)
                    .collect::<Result<Vec<_>, _>>()?,
                region: args.region.map(RectDto::from),
                ignore_regions: args
                    .ignore_regions
                    .iter()
                    .copied()
                    .map(RectDto::from)
                    .collect(),
                per_channel_threshold: args.per_channel_threshold,
                stable_max_changed_ratio: args.stable_max_changed_ratio,
                stable_max_changed_pixels: args.stable_max_changed_pixels,
                stable_max_mean_absolute_error: args.stable_max_mean_absolute_error,
                stable_max_channel_delta: args.stable_max_channel_delta,
                loading_min_changed_ratio: args.loading_min_changed_ratio,
                loading_min_changed_pixels: args.loading_min_changed_pixels,
                required_stable_transitions: args.required_stable_transitions,
                size_policy: visual_size_policy_name(args.size_policy).to_owned(),
                alpha_mode: visual_alpha_mode_name(args.alpha_mode).to_owned(),
            },
        )?;
        let ApiResult::UiState(result) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected UI state response".to_owned(),
            ));
        };
        if args.json {
            print_json_pretty(&result)?;
        } else {
            print_ui_state_dto(&result);
        }
        return Ok(());
    }

    let options = ui_state_options(&args);
    let result = peekaboox_vision::detect_ui_state_from_image_files(&args.image_paths, &options)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    if args.json {
        print_json_pretty(&ui_state_dto(&result))?;
    } else {
        print_ui_state(&result);
    }
    Ok(())
}

pub(super) fn parse_ui_state_args(args: Vec<String>) -> Result<UiStateCommand, CliError> {
    let mut image_paths = Vec::new();
    let mut region = None;
    let mut ignore_regions = Vec::new();
    let mut per_channel_threshold = UiStateOptions::default().per_channel_threshold;
    let mut stable_max_changed_ratio = UiStateOptions::default().stable_max_changed_ratio;
    let mut stable_max_changed_pixels = None;
    let mut stable_max_mean_absolute_error = None;
    let mut stable_max_channel_delta = None;
    let mut loading_min_changed_ratio = UiStateOptions::default().loading_min_changed_ratio;
    let mut loading_min_changed_pixels = None;
    let mut required_stable_transitions =
        u32::try_from(UiStateOptions::default().required_stable_transitions).unwrap_or(1);
    let mut size_policy = VisualSizePolicy::Error;
    let mut alpha_mode = VisualAlphaMode::Ignore;
    let mut json = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--image" | "-i" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --image".to_owned()));
                };
                image_paths.push(PathBuf::from(value));
            }
            "--region" | "-r" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --region".to_owned()));
                };
                region = Some(parse_rect("--region", value)?);
            }
            "--ignore-region" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --ignore-region".to_owned(),
                    ));
                };
                ignore_regions.push(parse_rect("--ignore-region", value)?);
            }
            "--threshold" | "-t" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --threshold".to_owned(),
                    ));
                };
                per_channel_threshold = parse_u8("--threshold", value)?;
            }
            "--stable-max-changed-ratio" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --stable-max-changed-ratio".to_owned(),
                    ));
                };
                stable_max_changed_ratio = parse_unit_f32("--stable-max-changed-ratio", value)?;
            }
            "--stable-max-changed-pixels" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --stable-max-changed-pixels".to_owned(),
                    ));
                };
                stable_max_changed_pixels = Some(parse_u64("--stable-max-changed-pixels", value)?);
            }
            "--stable-max-mae" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --stable-max-mae".to_owned(),
                    ));
                };
                stable_max_mean_absolute_error = Some(parse_visual_mae("--stable-max-mae", value)?);
            }
            "--stable-max-channel-delta" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --stable-max-channel-delta".to_owned(),
                    ));
                };
                stable_max_channel_delta = Some(parse_u8("--stable-max-channel-delta", value)?);
            }
            "--loading-min-changed-ratio" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --loading-min-changed-ratio".to_owned(),
                    ));
                };
                loading_min_changed_ratio = parse_unit_f32("--loading-min-changed-ratio", value)?;
            }
            "--loading-min-changed-pixels" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --loading-min-changed-pixels".to_owned(),
                    ));
                };
                loading_min_changed_pixels =
                    Some(parse_u64("--loading-min-changed-pixels", value)?);
            }
            "--required-stable-transitions" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --required-stable-transitions".to_owned(),
                    ));
                };
                required_stable_transitions =
                    parse_positive_u32("--required-stable-transitions", value)?;
            }
            "--size-policy" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --size-policy".to_owned(),
                    ));
                };
                size_policy = parse_visual_size_policy(value)?;
            }
            "--alpha" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --alpha".to_owned()));
                };
                alpha_mode = parse_visual_alpha_mode(value)?;
            }
            "--json" => json = true,
            "--help" | "-h" => return Ok(UiStateCommand::Help),
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown state argument: {value}"
                )));
            }
            value => image_paths.push(PathBuf::from(value)),
        }

        index += 1;
    }

    if image_paths.len() < 2 {
        return Err(CliError::Failure(
            "state requires at least two image paths".to_owned(),
        ));
    }
    if stable_max_changed_ratio > loading_min_changed_ratio {
        return Err(CliError::Failure(
            "--stable-max-changed-ratio must be less than or equal to --loading-min-changed-ratio"
                .to_owned(),
        ));
    }
    if let (Some(stable_max), Some(loading_min)) =
        (stable_max_changed_pixels, loading_min_changed_pixels)
        && stable_max > loading_min
    {
        return Err(CliError::Failure(
            "--stable-max-changed-pixels must be less than or equal to --loading-min-changed-pixels"
                .to_owned(),
        ));
    }

    Ok(UiStateCommand::Run(UiStateArgs {
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
        json,
    }))
}

pub(super) fn ui_state_options(args: &UiStateArgs) -> UiStateOptions {
    UiStateOptions {
        region: args.region,
        ignore_regions: args.ignore_regions.clone(),
        per_channel_threshold: args.per_channel_threshold,
        stable_max_changed_ratio: args.stable_max_changed_ratio,
        stable_max_changed_pixels: args.stable_max_changed_pixels,
        stable_max_mean_absolute_error: args.stable_max_mean_absolute_error,
        stable_max_channel_delta: args.stable_max_channel_delta,
        loading_min_changed_ratio: args.loading_min_changed_ratio,
        loading_min_changed_pixels: args.loading_min_changed_pixels,
        required_stable_transitions: usize::try_from(args.required_stable_transitions).unwrap_or(1),
        size_policy: args.size_policy,
        alpha_mode: args.alpha_mode,
    }
}

pub(super) fn print_ui_state(result: &UiStateResult) {
    println!(
        "state={} transitions={} stable={} loading={} trailing_stable={} latest_ratio={:.6} max_ratio={:.6} mean_ratio={:.6} changed_bounds={}",
        format!("{:?}", result.state).to_ascii_lowercase(),
        result.compared_transitions,
        result.stable_transitions,
        result.loading_transitions,
        result.trailing_stable_transitions,
        result.latest_diff.changed_ratio,
        result.max_changed_ratio,
        result.mean_changed_ratio,
        result
            .changed_bounds
            .map(format_rect)
            .unwrap_or_else(|| "-".to_owned())
    );
}

pub(super) fn print_ui_state_dto(result: &UiStateDto) {
    println!(
        "state={} transitions={} stable={} loading={} trailing_stable={} latest_ratio={:.6} max_ratio={:.6} mean_ratio={:.6} changed_bounds={}",
        result.state,
        result.compared_transitions,
        result.stable_transitions,
        result.loading_transitions,
        result.trailing_stable_transitions,
        result.latest_diff.changed_ratio,
        result.max_changed_ratio,
        result.mean_changed_ratio,
        result
            .changed_bounds
            .map(Rect::from)
            .map(format_rect)
            .unwrap_or_else(|| "-".to_owned())
    );
}

pub(super) fn ui_state_dto(result: &UiStateResult) -> UiStateDto {
    UiStateDto {
        state: format!("{:?}", result.state).to_ascii_lowercase(),
        compared_transitions: u64::try_from(result.compared_transitions).unwrap_or(u64::MAX),
        stable_transitions: u64::try_from(result.stable_transitions).unwrap_or(u64::MAX),
        loading_transitions: u64::try_from(result.loading_transitions).unwrap_or(u64::MAX),
        trailing_stable_transitions: u64::try_from(result.trailing_stable_transitions)
            .unwrap_or(u64::MAX),
        latest_diff: visual_diff_dto(&result.latest_diff),
        max_changed_ratio: result.max_changed_ratio,
        mean_changed_ratio: result.mean_changed_ratio,
        changed_bounds: result.changed_bounds.map(Into::into),
    }
}

pub(super) fn vision_elements(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let VisionElementsCommand::Run(args) = parse_vision_elements_args(args)? else {
        print_vision_elements_usage();
        return Err(CliError::HelpRequested);
    };

    if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::DetectUiElements {
                image_path: path_to_daemon_string(&args.image)?,
                region: args.region.map(RectDto::from),
                ignore_regions: args
                    .ignore_regions
                    .iter()
                    .copied()
                    .map(RectDto::from)
                    .collect(),
                edge_threshold: args.edge_threshold,
                min_width: args.min_width,
                min_height: args.min_height,
                min_component_pixels: args.min_component_pixels,
                min_confidence: args.min_confidence,
                max_width: args.max_width,
                max_height: args.max_height,
                min_area: args.min_area,
                max_area: args.max_area,
                max_elements: args.max_elements,
                merge_distance: args.merge_distance,
                padding: args.padding,
                sort: ui_element_sort_name(args.sort).to_owned(),
                mask_output_path: args
                    .mask_output
                    .as_ref()
                    .map(path_to_daemon_string)
                    .transpose()?,
                overlay_output_path: args
                    .overlay_output
                    .as_ref()
                    .map(path_to_daemon_string)
                    .transpose()?,
            },
        )?;
        let ApiResult::DetectUiElements(metadata) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected vision elements response".to_owned(),
            ));
        };
        if args.json {
            print_json_pretty(&metadata)?;
        } else {
            print_element_dto_table(metadata, 0);
        }
        return Ok(());
    }

    let options = vision_element_options(&args)?;
    let elements = peekaboox_vision::detect_ui_elements_from_image_file_with_outputs(
        &args.image,
        &options,
        args.mask_output.as_deref(),
        args.overlay_output.as_deref(),
    )
    .map_err(|error| CliError::Failure(error.to_string()))?;
    let metadata = AccessibilityTreeMetadata {
        backend_name: "heuristic_vision".to_owned(),
        backend_kind: BackendKind::Vision,
        warnings: Vec::new(),
        elements,
    };
    if args.json {
        print_json_pretty(&element_list_dto(metadata))?;
    } else {
        print_element_table(metadata, 0);
    }
    Ok(())
}

pub(super) fn parse_vision_elements_args(
    args: Vec<String>,
) -> Result<VisionElementsCommand, CliError> {
    let defaults = UiElementDetectionOptions::default();
    let mut image = None;
    let mut region = None;
    let mut ignore_regions = Vec::new();
    let mut edge_threshold = defaults.edge_threshold;
    let mut min_width = defaults.min_width;
    let mut min_height = defaults.min_height;
    let mut min_component_pixels = defaults.min_component_pixels;
    let mut min_confidence = defaults.min_confidence;
    let mut max_width = defaults.max_width;
    let mut max_height = defaults.max_height;
    let mut min_area = defaults.min_area;
    let mut max_area = defaults.max_area;
    let mut max_elements = u32::try_from(defaults.max_elements).unwrap_or(100);
    let mut merge_distance = defaults.merge_distance;
    let mut padding = defaults.padding;
    let mut sort = defaults.sort;
    let mut mask_output = None;
    let mut overlay_output = None;
    let mut json = false;
    let mut positional = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--image" | "-i" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --image".to_owned()));
                };
                image = Some(PathBuf::from(value));
            }
            "--region" | "-r" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --region".to_owned()));
                };
                region = Some(parse_rect("--region", value)?);
            }
            "--ignore-region" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --ignore-region".to_owned(),
                    ));
                };
                ignore_regions.push(parse_rect("--ignore-region", value)?);
            }
            "--threshold" | "--edge-threshold" | "-t" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --threshold".to_owned(),
                    ));
                };
                edge_threshold = parse_u8("--threshold", value)?;
                if edge_threshold == 0 {
                    return Err(CliError::Failure(
                        "--threshold must be greater than zero".to_owned(),
                    ));
                }
            }
            "--min-width" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --min-width".to_owned(),
                    ));
                };
                min_width = parse_positive_u32("--min-width", value)?;
            }
            "--min-height" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --min-height".to_owned(),
                    ));
                };
                min_height = parse_positive_u32("--min-height", value)?;
            }
            "--min-component-pixels" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --min-component-pixels".to_owned(),
                    ));
                };
                min_component_pixels = parse_positive_u32("--min-component-pixels", value)?;
            }
            "--min-confidence" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --min-confidence".to_owned(),
                    ));
                };
                min_confidence = Some(parse_unit_f32("--min-confidence", value)?);
            }
            "--max-width" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --max-width".to_owned(),
                    ));
                };
                max_width = Some(parse_positive_u32("--max-width", value)?);
            }
            "--max-height" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --max-height".to_owned(),
                    ));
                };
                max_height = Some(parse_positive_u32("--max-height", value)?);
            }
            "--min-area" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --min-area".to_owned()));
                };
                min_area = Some(parse_u64("--min-area", value)?);
                if min_area == Some(0) {
                    return Err(CliError::Failure(
                        "--min-area must be greater than zero".to_owned(),
                    ));
                }
            }
            "--max-area" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --max-area".to_owned()));
                };
                max_area = Some(parse_u64("--max-area", value)?);
                if max_area == Some(0) {
                    return Err(CliError::Failure(
                        "--max-area must be greater than zero".to_owned(),
                    ));
                }
            }
            "--max-elements" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --max-elements".to_owned(),
                    ));
                };
                max_elements = parse_positive_u32("--max-elements", value)?;
            }
            "--merge-distance" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --merge-distance".to_owned(),
                    ));
                };
                merge_distance = value.parse::<u32>().map_err(|_| {
                    CliError::Failure(format!(
                        "--merge-distance must be an integer, got {value:?}"
                    ))
                })?;
            }
            "--padding" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --padding".to_owned()));
                };
                padding = parse_u32("--padding", value)?;
            }
            "--sort" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --sort".to_owned()));
                };
                sort = parse_ui_element_sort(value)?;
            }
            "--mask-output" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --mask-output".to_owned(),
                    ));
                };
                mask_output = Some(PathBuf::from(value));
            }
            "--overlay-output" | "--debug-output" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --overlay-output".to_owned(),
                    ));
                };
                overlay_output = Some(PathBuf::from(value));
            }
            "--json" => json = true,
            "--help" | "-h" => return Ok(VisionElementsCommand::Help),
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown vision-elements argument: {value}"
                )));
            }
            value => positional.push(PathBuf::from(value)),
        }

        index += 1;
    }

    match positional.as_slice() {
        [] => {}
        [image_path] if image.is_none() => image = Some(image_path.clone()),
        _ => {
            return Err(CliError::Failure(
                "vision-elements accepts exactly one image path".to_owned(),
            ));
        }
    }
    let image = image.ok_or_else(|| CliError::Failure("missing --image path".to_owned()))?;
    if let Some(max_width) = max_width
        && min_width > max_width
    {
        return Err(CliError::Failure(
            "--min-width must be less than or equal to --max-width".to_owned(),
        ));
    }
    if let Some(max_height) = max_height
        && min_height > max_height
    {
        return Err(CliError::Failure(
            "--min-height must be less than or equal to --max-height".to_owned(),
        ));
    }
    if let (Some(min_area), Some(max_area)) = (min_area, max_area)
        && min_area > max_area
    {
        return Err(CliError::Failure(
            "--min-area must be less than or equal to --max-area".to_owned(),
        ));
    }

    Ok(VisionElementsCommand::Run(VisionElementsArgs {
        image,
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
        mask_output,
        overlay_output,
        json,
    }))
}

pub(super) fn vision_element_options(
    args: &VisionElementsArgs,
) -> Result<UiElementDetectionOptions, CliError> {
    Ok(UiElementDetectionOptions {
        region: args.region,
        ignore_regions: args.ignore_regions.clone(),
        edge_threshold: args.edge_threshold,
        min_width: args.min_width,
        min_height: args.min_height,
        min_component_pixels: args.min_component_pixels,
        min_confidence: args.min_confidence,
        max_width: args.max_width,
        max_height: args.max_height,
        min_area: args.min_area,
        max_area: args.max_area,
        max_elements: usize::try_from(args.max_elements)
            .map_err(|_| CliError::Failure("--max-elements is too large".to_owned()))?,
        merge_distance: args.merge_distance,
        padding: args.padding,
        sort: args.sort,
    })
}
