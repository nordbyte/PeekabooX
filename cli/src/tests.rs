use std::path::PathBuf;

use peekaboox_input::{
    ClipboardBackendSelection, ClipboardRestorePolicy, InputToolSelection, MouseButton,
    MoveBoundsPolicy, PasteHotkeyBackendSelection,
};
use peekaboox_ipc::CaptureBackendProbeDto;

use super::{
    CaptureArgs, CaptureBackendsArgs, CaptureBackendsCommand, CaptureCommand, CaptureDeltaArgs,
    CaptureDeltaCommand, CaptureDmaBufArgs, CaptureDmaBufCommand, CaptureDmaBufImportTarget,
    CaptureOutputFormat, CliContext, CliError, ClickArgs, ClickCommand, ClickTarget, CompareArgs,
    CompareCommand, DesktopAssertArgs, DesktopClickArgs, DesktopCommand, DesktopDragArgs,
    DesktopFocusArgs, DesktopLocateArgs, DesktopProfilesArgs, DesktopTypeIntoArgs, DragArgs,
    DragCommand, DragEndpoint, ElementsArgs, ElementsCommand, GlobalArgs, HotkeyArgs,
    HotkeyCommand, MoveArgs, MoveCommand, MoveTarget, OcrArgs, OcrCommand, PasteArgs, PasteCommand,
    PluginsArgs, PluginsCommand, TypeArgs, TypeCommand, TypeTextSource, UiStateArgs,
    UiStateCommand, VisionElementsArgs, VisionElementsCommand, WindowsArgs, WindowsCommand,
    parse_capture_args, parse_capture_backends_args, parse_capture_delta_args,
    parse_capture_dmabuf_args, parse_click_args, parse_compare_args, parse_desktop_args,
    parse_drag_args, parse_elements_args, parse_global_args, parse_hotkey_args, parse_move_args,
    parse_ocr_args, parse_paste_args, parse_plugins_args, parse_see_args, parse_type_args,
    parse_ui_state_args, parse_vision_elements_args, parse_windows_args,
};
use peekaboox_core::{Point, Rect};
use peekaboox_desktop::DesktopAssertion;
use peekaboox_vision::{
    OcrConfig, OcrPreprocessingOptions, UiElementSort, VisualAlphaMode, VisualSizePolicy,
};

#[test]
fn capture_defaults_to_screenshot_png() {
    let args = parse_capture_args(vec![]).unwrap();

    assert_eq!(
        args,
        CaptureCommand::Run(CaptureArgs {
            output: PathBuf::from("screenshot.png"),
            region: None,
            window_id: None,
            app: None,
            window_title: None,
            title_regex: None,
            format: CaptureOutputFormat::Png,
            jpeg_quality: 90,
            json: false,
            stdout: false,
            no_overwrite: false,
            include_semantic_tree: false,
        })
    );
}

#[test]
fn see_defaults_to_internal_json_capture_with_elements() {
    let args = parse_see_args(vec![]).unwrap();

    assert!(args.capture.json);
    assert!(args.capture.include_semantic_tree);
    assert!(args.include_elements);
    assert!(!args.json);
}

#[test]
fn see_no_elements_does_not_request_semantic_tree() {
    let args = parse_see_args(vec!["--no-elements".to_owned()]).unwrap();

    assert!(args.capture.json);
    assert!(!args.capture.include_semantic_tree);
    assert!(!args.include_elements);
}

#[test]
fn config_set_refuses_invalid_existing_config() {
    let root = std::env::temp_dir().join(format!(
        "peekaboox-config-test-{}-{}",
        std::process::id(),
        super::unix_time_ms_u64()
    ));
    let config_path = root.join("config.json");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(&config_path, "{invalid-json").unwrap();

    let error = super::read_config_json_or_default_if_missing(&config_path).unwrap_err();

    assert!(matches!(
        error,
        CliError::Failure(message) if message.contains("invalid config JSON")
    ));
    assert_eq!(
        std::fs::read_to_string(&config_path).unwrap(),
        "{invalid-json"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn parses_global_daemon_flags() {
    let args = parse_global_args(vec![
        "--daemon".to_owned(),
        "--socket".to_owned(),
        "/tmp/peekaboox.sock".to_owned(),
        "windows".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        args,
        GlobalArgs {
            context: CliContext {
                use_daemon: true,
                socket: PathBuf::from("/tmp/peekaboox.sock")
            },
            args: vec!["windows".to_owned()]
        }
    );
}

#[test]
fn capture_accepts_output_argument() {
    let args =
        parse_capture_args(vec!["--output".to_owned(), "tmp/screenshot.png".to_owned()]).unwrap();

    assert_eq!(
        args,
        CaptureCommand::Run(CaptureArgs {
            output: PathBuf::from("tmp/screenshot.png"),
            region: None,
            window_id: None,
            app: None,
            window_title: None,
            title_regex: None,
            format: CaptureOutputFormat::Png,
            jpeg_quality: 90,
            json: false,
            stdout: false,
            no_overwrite: false,
            include_semantic_tree: false,
        })
    );
}

#[test]
fn capture_accepts_region_and_window_id_targets() {
    let region = parse_capture_args(vec![
        "--output".to_owned(),
        "tmp/region.png".to_owned(),
        "--region".to_owned(),
        "10,20,100,40".to_owned(),
    ])
    .unwrap();
    let window = parse_capture_args(vec!["--window-id".to_owned(), "window-1".to_owned()]).unwrap();

    assert_eq!(
        region,
        CaptureCommand::Run(CaptureArgs {
            output: PathBuf::from("tmp/region.png"),
            region: Some(Rect::new(10, 20, 100, 40)),
            window_id: None,
            app: None,
            window_title: None,
            title_regex: None,
            format: CaptureOutputFormat::Png,
            jpeg_quality: 90,
            json: false,
            stdout: false,
            no_overwrite: false,
            include_semantic_tree: false,
        })
    );
    assert_eq!(
        window,
        CaptureCommand::Run(CaptureArgs {
            output: PathBuf::from("screenshot.png"),
            region: None,
            window_id: Some("window-1".to_owned()),
            app: None,
            window_title: None,
            title_regex: None,
            format: CaptureOutputFormat::Png,
            jpeg_quality: 90,
            json: false,
            stdout: false,
            no_overwrite: false,
            include_semantic_tree: false,
        })
    );
}

#[test]
fn capture_accepts_window_relative_region_and_filters() {
    let command = parse_capture_args(vec![
        "--region".to_owned(),
        "10,20,100,40".to_owned(),
        "--window-id".to_owned(),
        "window-1".to_owned(),
        "--app".to_owned(),
        "calculator".to_owned(),
        "--window-title".to_owned(),
        "Calculator".to_owned(),
        "--title-regex".to_owned(),
        "Calc.*".to_owned(),
        "--json".to_owned(),
        "--include-semantic-tree".to_owned(),
        "--no-overwrite".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        CaptureCommand::Run(CaptureArgs {
            output: PathBuf::from("screenshot.png"),
            region: Some(Rect::new(10, 20, 100, 40)),
            window_id: Some("window-1".to_owned()),
            app: Some("calculator".to_owned()),
            window_title: Some("Calculator".to_owned()),
            title_regex: Some("Calc.*".to_owned()),
            format: CaptureOutputFormat::Png,
            jpeg_quality: 90,
            json: true,
            stdout: false,
            no_overwrite: true,
            include_semantic_tree: true,
        })
    );
}

#[test]
fn capture_accepts_stdout_and_xwd_format() {
    let stdout = parse_capture_args(vec!["--stdout".to_owned()]).unwrap();
    let xwd = parse_capture_args(vec!["--format".to_owned(), "xwd".to_owned()]).unwrap();

    assert_eq!(
        stdout,
        CaptureCommand::Run(CaptureArgs {
            output: PathBuf::from("screenshot.png"),
            region: None,
            window_id: None,
            app: None,
            window_title: None,
            title_regex: None,
            format: CaptureOutputFormat::Png,
            jpeg_quality: 90,
            json: false,
            stdout: true,
            no_overwrite: false,
            include_semantic_tree: false,
        })
    );
    assert_eq!(
        xwd,
        CaptureCommand::Run(CaptureArgs {
            output: PathBuf::from("screenshot.xwd"),
            region: None,
            window_id: None,
            app: None,
            window_title: None,
            title_regex: None,
            format: CaptureOutputFormat::Xwd,
            jpeg_quality: 90,
            json: false,
            stdout: false,
            no_overwrite: false,
            include_semantic_tree: false,
        })
    );
}

#[test]
fn capture_accepts_jpeg_format_and_quality() {
    let command = parse_capture_args(vec![
        "--format".to_owned(),
        "jpeg".to_owned(),
        "--quality".to_owned(),
        "80".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        CaptureCommand::Run(CaptureArgs {
            output: PathBuf::from("screenshot.jpg"),
            region: None,
            window_id: None,
            app: None,
            window_title: None,
            title_regex: None,
            format: CaptureOutputFormat::Jpeg,
            jpeg_quality: 80,
            json: false,
            stdout: false,
            no_overwrite: false,
            include_semantic_tree: false,
        })
    );
}

#[test]
fn capture_rejects_missing_output_value() {
    let error = parse_capture_args(vec!["--output".to_owned()]).unwrap_err();

    assert_eq!(
        error,
        CliError::Failure("missing value for --output".to_owned())
    );
}

#[test]
fn capture_help_is_not_a_failure() {
    let command = parse_capture_args(vec!["--help".to_owned()]).unwrap();

    assert_eq!(command, CaptureCommand::Help);
}

#[test]
fn capture_delta_accepts_stream_reset_region_and_threshold() {
    let args = parse_capture_delta_args(vec![
        "--stream".to_owned(),
        "agent-loop".to_owned(),
        "--reset".to_owned(),
        "--region".to_owned(),
        "10,20,100,40".to_owned(),
        "--threshold".to_owned(),
        "3".to_owned(),
        "--full-frame".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        args,
        CaptureDeltaCommand::Run(CaptureDeltaArgs {
            stream_id: Some("agent-loop".to_owned()),
            reset: true,
            region: Some(Rect::new(10, 20, 100, 40)),
            window_id: None,
            per_channel_threshold: 3,
            low_bandwidth: false,
            json: false,
        })
    );
}

#[test]
fn capture_delta_accepts_json_output() {
    let args = parse_capture_delta_args(vec!["--json".to_owned()]).unwrap();

    assert_eq!(
        args,
        CaptureDeltaCommand::Run(CaptureDeltaArgs {
            stream_id: None,
            reset: false,
            region: None,
            window_id: None,
            per_channel_threshold: 0,
            low_bandwidth: true,
            json: true,
        })
    );
}

#[test]
fn capture_delta_accepts_window_id_target() {
    let args = parse_capture_delta_args(vec![
        "--stream".to_owned(),
        "agent-loop".to_owned(),
        "--window-id".to_owned(),
        "window-1".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        args,
        CaptureDeltaCommand::Run(CaptureDeltaArgs {
            stream_id: Some("agent-loop".to_owned()),
            reset: false,
            region: None,
            window_id: Some("window-1".to_owned()),
            per_channel_threshold: 0,
            low_bandwidth: true,
            json: false,
        })
    );
}

#[test]
fn capture_delta_help_is_not_a_failure() {
    let command = parse_capture_delta_args(vec!["--help".to_owned()]).unwrap();

    assert_eq!(command, CaptureDeltaCommand::Help);
}

#[test]
fn capture_backends_accepts_no_arguments() {
    let command = parse_capture_backends_args(vec![]).unwrap();

    assert_eq!(
        command,
        CaptureBackendsCommand::Run(CaptureBackendsArgs {
            output: PathBuf::from("screenshot.png"),
            region: None,
            diagnose: false,
            json: false,
            probe: CaptureBackendProbeDto::None,
        })
    );
}

#[test]
fn capture_backends_accepts_diagnostics_json_output_format_region_and_probe() {
    let command = parse_capture_backends_args(vec![
        "--format".to_owned(),
        "xwd".to_owned(),
        "--region".to_owned(),
        "0,0,320,180".to_owned(),
        "--probe".to_owned(),
        "all".to_owned(),
        "--diagnose".to_owned(),
        "--json".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        CaptureBackendsCommand::Run(CaptureBackendsArgs {
            output: PathBuf::from("screenshot.xwd"),
            region: Some(Rect::new(0, 0, 320, 180)),
            diagnose: true,
            json: true,
            probe: CaptureBackendProbeDto::All,
        })
    );
}

#[test]
fn capture_backends_help_is_not_a_failure() {
    let command = parse_capture_backends_args(vec!["--help".to_owned()]).unwrap();

    assert_eq!(command, CaptureBackendsCommand::Help);
}

#[test]
fn capture_backends_rejects_positional_arguments() {
    let error = parse_capture_backends_args(vec!["extra".to_owned()]).unwrap_err();

    assert_eq!(
        error,
        CliError::Failure("unknown capture-backends argument: extra".to_owned())
    );
}

#[test]
fn capture_dmabuf_accepts_no_arguments() {
    let command = parse_capture_dmabuf_args(vec![]).unwrap();

    assert_eq!(
        command,
        CaptureDmaBufCommand::Run(CaptureDmaBufArgs {
            import_target: CaptureDmaBufImportTarget::Compute
        })
    );
}

#[test]
fn capture_dmabuf_accepts_egl_import_target() {
    let command = parse_capture_dmabuf_args(vec!["--import".to_owned(), "egl".to_owned()]).unwrap();

    assert_eq!(
        command,
        CaptureDmaBufCommand::Run(CaptureDmaBufArgs {
            import_target: CaptureDmaBufImportTarget::Egl
        })
    );
}

#[test]
fn capture_dmabuf_accepts_egl_texture_import_target() {
    let command =
        parse_capture_dmabuf_args(vec!["--import".to_owned(), "egl-texture".to_owned()]).unwrap();

    assert_eq!(
        command,
        CaptureDmaBufCommand::Run(CaptureDmaBufArgs {
            import_target: CaptureDmaBufImportTarget::EglTexture
        })
    );
}

#[test]
fn capture_dmabuf_help_is_not_a_failure() {
    let command = parse_capture_dmabuf_args(vec!["--help".to_owned()]).unwrap();

    assert_eq!(command, CaptureDmaBufCommand::Help);
}

#[test]
fn capture_dmabuf_rejects_positional_arguments() {
    let error = parse_capture_dmabuf_args(vec!["extra".to_owned()]).unwrap_err();

    assert_eq!(
        error,
        CliError::Failure("unknown capture-dmabuf argument: extra".to_owned())
    );
}

#[test]
fn capture_dmabuf_rejects_missing_import_target() {
    let error = parse_capture_dmabuf_args(vec!["--import".to_owned()]).unwrap_err();

    assert_eq!(
        error,
        CliError::Failure("missing value for --import".to_owned())
    );
}

#[test]
fn capture_dmabuf_rejects_unknown_import_target() {
    let error =
        parse_capture_dmabuf_args(vec!["--import".to_owned(), "vulkan".to_owned()]).unwrap_err();

    assert_eq!(
        error,
        CliError::Failure("unsupported capture-dmabuf import target: vulkan".to_owned())
    );
}

#[test]
fn plugins_accepts_paths_and_json_flag() {
    let command = parse_plugins_args(vec![
        "--path".to_owned(),
        "examples/plugins".to_owned(),
        "--json".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        PluginsCommand::Run(PluginsArgs {
            paths: vec![PathBuf::from("examples/plugins")],
            json: true,
        })
    );
}

#[test]
fn plugins_help_is_not_a_failure() {
    let command = parse_plugins_args(vec!["--help".to_owned()]).unwrap();

    assert_eq!(command, PluginsCommand::Help);
}

#[test]
fn plugins_rejects_missing_path() {
    let error = parse_plugins_args(vec!["--path".to_owned()]).unwrap_err();

    assert_eq!(
        error,
        CliError::Failure("missing value for --path".to_owned())
    );
}

#[test]
fn windows_accepts_no_arguments() {
    let command = parse_windows_args(vec![]).unwrap();

    assert_eq!(
        command,
        WindowsCommand::Run(WindowsArgs {
            json: false,
            id: None,
            app: None,
            title: None,
            title_regex: None,
            focused: false,
            limit: None,
            sort: peekaboox_windows::WindowSort::Backend,
            backend: peekaboox_windows::WindowBackendSelection::Auto,
            diagnose: false,
        })
    );
}

#[test]
fn windows_accepts_json() {
    let command = parse_windows_args(vec!["--json".to_owned()]).unwrap();

    assert_eq!(
        command,
        WindowsCommand::Run(WindowsArgs {
            json: true,
            id: None,
            app: None,
            title: None,
            title_regex: None,
            focused: false,
            limit: None,
            sort: peekaboox_windows::WindowSort::Backend,
            backend: peekaboox_windows::WindowBackendSelection::Auto,
            diagnose: false,
        })
    );
}

#[test]
fn windows_help_is_not_a_failure() {
    let command = parse_windows_args(vec!["--help".to_owned()]).unwrap();

    assert_eq!(command, WindowsCommand::Help);
}

#[test]
fn windows_accepts_filters_sort_backend_and_diagnose() {
    let command = parse_windows_args(vec![
        "--focused".to_owned(),
        "--app".to_owned(),
        "Calculator".to_owned(),
        "--title-regex".to_owned(),
        "Calc.*".to_owned(),
        "--id".to_owned(),
        "42".to_owned(),
        "--limit".to_owned(),
        "1".to_owned(),
        "--sort".to_owned(),
        "focused".to_owned(),
        "--backend".to_owned(),
        "xdotool".to_owned(),
        "--diagnose".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        WindowsCommand::Run(WindowsArgs {
            json: false,
            id: Some("42".to_owned()),
            app: Some("Calculator".to_owned()),
            title: None,
            title_regex: Some("Calc.*".to_owned()),
            focused: true,
            limit: Some(1),
            sort: peekaboox_windows::WindowSort::Focused,
            backend: peekaboox_windows::WindowBackendSelection::Xdotool,
            diagnose: true,
        })
    );
}

#[test]
fn elements_defaults_to_all_with_limit() {
    let command = parse_elements_args(vec![]).unwrap();

    assert_eq!(
        command,
        ElementsCommand::Run(ElementsArgs {
            selector: String::new(),
            limit: 50,
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
            json: false
        })
    );
}

#[test]
fn elements_accepts_structured_selector_parts() {
    let command = parse_elements_args(vec![
        "--role".to_owned(),
        "push button".to_owned(),
        "--text".to_owned(),
        "Submit".to_owned(),
        "--state".to_owned(),
        "enabled".to_owned(),
        "--contains".to_owned(),
        "25,30".to_owned(),
        "--min-confidence".to_owned(),
        "0.9".to_owned(),
        "--limit".to_owned(),
        "5".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        ElementsCommand::Run(ElementsArgs {
            selector: "role=push button,label=Submit,state=enabled,contains=25,30,confidence>=0.9"
                .to_owned(),
            limit: 5,
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
            json: false
        })
    );
}

#[test]
fn elements_accepts_vision_fallback_flag() {
    let command = parse_elements_args(vec![
        "--role".to_owned(),
        "visual-region".to_owned(),
        "--vision-fallback".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        ElementsCommand::Run(ElementsArgs {
            selector: "role=visual-region".to_owned(),
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
            json: false
        })
    );
}

#[test]
fn elements_accepts_scope_and_vision_options() {
    let command = parse_elements_args(vec![
        "--selector".to_owned(),
        "label-regex=^Save,not-state=disabled,min-width=40".to_owned(),
        "--app".to_owned(),
        "text-editor".to_owned(),
        "--window-title".to_owned(),
        "Draft".to_owned(),
        "--window-id".to_owned(),
        "window-1".to_owned(),
        "--vision-region".to_owned(),
        "10,20,300,200".to_owned(),
        "--vision-threshold".to_owned(),
        "24".to_owned(),
        "--vision-min-width".to_owned(),
        "8".to_owned(),
        "--vision-max-elements".to_owned(),
        "25".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        ElementsCommand::Run(ElementsArgs {
            selector: "label-regex=^Save,not-state=disabled,min-width=40".to_owned(),
            limit: 50,
            vision_fallback: false,
            app: Some("text-editor".to_owned()),
            window_title: Some("Draft".to_owned()),
            window_id: Some("window-1".to_owned()),
            vision_region: Some(Rect::new(10, 20, 300, 200)),
            vision_edge_threshold: Some(24),
            vision_min_width: Some(8),
            vision_min_height: None,
            vision_min_component_pixels: None,
            vision_max_elements: Some(25),
            vision_merge_distance: None,
            json: false
        })
    );
}

#[test]
fn elements_rejects_invalid_selector_values() {
    let error = parse_elements_args(vec!["--bounds".to_owned(), "bad".to_owned()]).unwrap_err();

    assert!(matches!(error, CliError::Failure(message) if message.contains("bounds")));
}

#[test]
fn ocr_accepts_region_and_language() {
    let command = parse_ocr_args(vec![
        "--image".to_owned(),
        "tests/fixtures/ocr/sample.png".to_owned(),
        "--region".to_owned(),
        "10,20,300,80".to_owned(),
        "--language".to_owned(),
        "eng".to_owned(),
        "--psm".to_owned(),
        "6".to_owned(),
        "--oem".to_owned(),
        "1".to_owned(),
        "--dpi".to_owned(),
        "300".to_owned(),
        "--min-confidence".to_owned(),
        "0.5".to_owned(),
        "--whitelist".to_owned(),
        "ABC123".to_owned(),
        "--config".to_owned(),
        "preserve_interword_spaces=1".to_owned(),
        "--scale".to_owned(),
        "2".to_owned(),
        "--grayscale".to_owned(),
        "--threshold".to_owned(),
        "180".to_owned(),
        "--invert".to_owned(),
        "--contrast".to_owned(),
        "10".to_owned(),
        "--deskew".to_owned(),
        "--words".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        OcrCommand::Run(Box::new(OcrArgs {
            image: Some(PathBuf::from("tests/fixtures/ocr/sample.png")),
            region: Some(Rect::new(10, 20, 300, 80)),
            app: None,
            window_title: None,
            window_id: None,
            language: Some("eng".to_owned()),
            page_segmentation_mode: Some(6),
            engine_mode: Some(1),
            dpi: Some(300),
            min_confidence: Some(0.5),
            whitelist: Some("ABC123".to_owned()),
            config: vec![OcrConfig {
                key: "preserve_interword_spaces".to_owned(),
                value: "1".to_owned()
            }],
            preprocessing: OcrPreprocessingOptions {
                scale: Some(2.0),
                grayscale: true,
                threshold: Some(180),
                invert: true,
                contrast: Some(10.0),
                deskew: true
            },
            json: false,
            words: true
        }))
    );
}

#[test]
fn ocr_rejects_bad_region() {
    let error = parse_ocr_args(vec!["--region".to_owned(), "10,20,0,80".to_owned()]).unwrap_err();

    assert_eq!(
        error,
        CliError::Failure("--region width and height must be greater than zero".to_owned())
    );
}

#[test]
fn compare_accepts_positional_paths_and_tolerance() {
    let command = parse_compare_args(vec![
        "before.png".to_owned(),
        "after.png".to_owned(),
        "--threshold".to_owned(),
        "4".to_owned(),
        "--max-changed-ratio".to_owned(),
        "0.01".to_owned(),
        "--region".to_owned(),
        "10,20,300,80".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        CompareCommand::Run(CompareArgs {
            expected: PathBuf::from("before.png"),
            actual: PathBuf::from("after.png"),
            region: Some(Rect::new(10, 20, 300, 80)),
            ignore_regions: Vec::new(),
            per_channel_threshold: 4,
            max_changed_ratio: 0.01,
            max_changed_pixels: None,
            max_mean_absolute_error: None,
            max_channel_delta: None,
            size_policy: VisualSizePolicy::Error,
            alpha_mode: VisualAlphaMode::Ignore,
            diff_output: None,
            report: None,
            no_fail: false,
            json: false
        })
    );
}

#[test]
fn compare_accepts_visual_regression_options() {
    let command = parse_compare_args(vec![
        "--expected".to_owned(),
        "before.png".to_owned(),
        "--actual".to_owned(),
        "after.png".to_owned(),
        "--ignore-region".to_owned(),
        "1,2,3,4".to_owned(),
        "--ignore-region".to_owned(),
        "5,6,7,8".to_owned(),
        "--max-changed-pixels".to_owned(),
        "12".to_owned(),
        "--max-mae".to_owned(),
        "3.5".to_owned(),
        "--max-channel-delta".to_owned(),
        "9".to_owned(),
        "--size-policy".to_owned(),
        "common-region".to_owned(),
        "--alpha".to_owned(),
        "compare".to_owned(),
        "--diff-output".to_owned(),
        "diff.png".to_owned(),
        "--report".to_owned(),
        "report.json".to_owned(),
        "--no-fail".to_owned(),
        "--json".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        CompareCommand::Run(CompareArgs {
            expected: PathBuf::from("before.png"),
            actual: PathBuf::from("after.png"),
            region: None,
            ignore_regions: vec![Rect::new(1, 2, 3, 4), Rect::new(5, 6, 7, 8)],
            per_channel_threshold: 0,
            max_changed_ratio: 0.0,
            max_changed_pixels: Some(12),
            max_mean_absolute_error: Some(3.5),
            max_channel_delta: Some(9),
            size_policy: VisualSizePolicy::CommonRegion,
            alpha_mode: VisualAlphaMode::Compare,
            diff_output: Some(PathBuf::from("diff.png")),
            report: Some(PathBuf::from("report.json")),
            no_fail: true,
            json: true
        })
    );
}

#[test]
fn compare_rejects_missing_actual_path() {
    let error = parse_compare_args(vec!["before.png".to_owned()]).unwrap_err();

    assert_eq!(
        error,
        CliError::Failure("missing --actual image path".to_owned())
    );
}

#[test]
fn compare_rejects_bad_ratio() {
    let error = parse_compare_args(vec![
        "before.png".to_owned(),
        "after.png".to_owned(),
        "--max-changed-ratio".to_owned(),
        "1.1".to_owned(),
    ])
    .unwrap_err();

    assert_eq!(
        error,
        CliError::Failure(
            "--max-changed-ratio must be between 0.0 and 1.0, got \"1.1\"".to_owned()
        )
    );
}

#[test]
fn ui_state_accepts_paths_and_thresholds() {
    let command = parse_ui_state_args(vec![
        "first.png".to_owned(),
        "--image".to_owned(),
        "second.png".to_owned(),
        "third.png".to_owned(),
        "--threshold".to_owned(),
        "4".to_owned(),
        "--stable-max-changed-ratio".to_owned(),
        "0.002".to_owned(),
        "--loading-min-changed-ratio".to_owned(),
        "0.03".to_owned(),
        "--required-stable-transitions".to_owned(),
        "2".to_owned(),
        "--region".to_owned(),
        "10,20,300,80".to_owned(),
        "--ignore-region".to_owned(),
        "11,22,33,44".to_owned(),
        "--stable-max-changed-pixels".to_owned(),
        "9".to_owned(),
        "--stable-max-mae".to_owned(),
        "1.5".to_owned(),
        "--stable-max-channel-delta".to_owned(),
        "12".to_owned(),
        "--loading-min-changed-pixels".to_owned(),
        "10".to_owned(),
        "--size-policy".to_owned(),
        "resize-actual".to_owned(),
        "--alpha".to_owned(),
        "compare".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        UiStateCommand::Run(UiStateArgs {
            image_paths: vec![
                PathBuf::from("first.png"),
                PathBuf::from("second.png"),
                PathBuf::from("third.png")
            ],
            region: Some(Rect::new(10, 20, 300, 80)),
            ignore_regions: vec![Rect::new(11, 22, 33, 44)],
            per_channel_threshold: 4,
            stable_max_changed_ratio: 0.002,
            stable_max_changed_pixels: Some(9),
            stable_max_mean_absolute_error: Some(1.5),
            stable_max_channel_delta: Some(12),
            loading_min_changed_ratio: 0.03,
            loading_min_changed_pixels: Some(10),
            required_stable_transitions: 2,
            size_policy: VisualSizePolicy::ResizeActual,
            alpha_mode: VisualAlphaMode::Compare,
            json: false
        })
    );
}

#[test]
fn ui_state_rejects_single_path() {
    let error = parse_ui_state_args(vec!["first.png".to_owned()]).unwrap_err();

    assert_eq!(
        error,
        CliError::Failure("state requires at least two image paths".to_owned())
    );
}

#[test]
fn ui_state_rejects_inverted_thresholds() {
    let error = parse_ui_state_args(vec![
        "first.png".to_owned(),
        "second.png".to_owned(),
        "--stable-max-changed-ratio".to_owned(),
        "0.1".to_owned(),
        "--loading-min-changed-ratio".to_owned(),
        "0.01".to_owned(),
    ])
    .unwrap_err();

    assert_eq!(
        error,
        CliError::Failure(
            "--stable-max-changed-ratio must be less than or equal to --loading-min-changed-ratio"
                .to_owned()
        )
    );
}

#[test]
fn ui_state_rejects_inverted_absolute_thresholds() {
    let error = parse_ui_state_args(vec![
        "first.png".to_owned(),
        "second.png".to_owned(),
        "--stable-max-changed-pixels".to_owned(),
        "10".to_owned(),
        "--loading-min-changed-pixels".to_owned(),
        "2".to_owned(),
    ])
    .unwrap_err();

    assert_eq!(
            error,
            CliError::Failure(
                "--stable-max-changed-pixels must be less than or equal to --loading-min-changed-pixels"
                    .to_owned()
            )
        );
}

#[test]
fn vision_elements_accepts_image_and_detection_options() {
    let command = parse_vision_elements_args(vec![
        "screen.png".to_owned(),
        "--threshold".to_owned(),
        "32".to_owned(),
        "--min-width".to_owned(),
        "9".to_owned(),
        "--min-height".to_owned(),
        "7".to_owned(),
        "--min-component-pixels".to_owned(),
        "20".to_owned(),
        "--max-elements".to_owned(),
        "12".to_owned(),
        "--merge-distance".to_owned(),
        "3".to_owned(),
        "--region".to_owned(),
        "10,20,300,80".to_owned(),
        "--ignore-region".to_owned(),
        "10,20,30,40".to_owned(),
        "--min-confidence".to_owned(),
        "0.72".to_owned(),
        "--max-width".to_owned(),
        "200".to_owned(),
        "--max-height".to_owned(),
        "100".to_owned(),
        "--min-area".to_owned(),
        "63".to_owned(),
        "--max-area".to_owned(),
        "2000".to_owned(),
        "--padding".to_owned(),
        "4".to_owned(),
        "--sort".to_owned(),
        "confidence".to_owned(),
        "--mask-output".to_owned(),
        "mask.png".to_owned(),
        "--overlay-output".to_owned(),
        "overlay.png".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        VisionElementsCommand::Run(VisionElementsArgs {
            image: PathBuf::from("screen.png"),
            region: Some(Rect::new(10, 20, 300, 80)),
            ignore_regions: vec![Rect::new(10, 20, 30, 40)],
            edge_threshold: 32,
            min_width: 9,
            min_height: 7,
            min_component_pixels: 20,
            min_confidence: Some(0.72),
            max_width: Some(200),
            max_height: Some(100),
            min_area: Some(63),
            max_area: Some(2000),
            max_elements: 12,
            merge_distance: 3,
            padding: 4,
            sort: UiElementSort::Confidence,
            mask_output: Some(PathBuf::from("mask.png")),
            overlay_output: Some(PathBuf::from("overlay.png")),
            json: false
        })
    );
}

#[test]
fn vision_elements_rejects_missing_image() {
    let error =
        parse_vision_elements_args(vec!["--threshold".to_owned(), "24".to_owned()]).unwrap_err();

    assert_eq!(error, CliError::Failure("missing --image path".to_owned()));
}

#[test]
fn vision_elements_rejects_zero_threshold() {
    let error = parse_vision_elements_args(vec![
        "screen.png".to_owned(),
        "--threshold".to_owned(),
        "0".to_owned(),
    ])
    .unwrap_err();

    assert_eq!(
        error,
        CliError::Failure("--threshold must be greater than zero".to_owned())
    );
}

#[test]
fn desktop_profiles_accepts_no_arguments() {
    let command = parse_desktop_args(vec!["profiles".to_owned()]).unwrap();

    assert_eq!(
        command,
        DesktopCommand::Profiles(DesktopProfilesArgs {
            json: false,
            app: None,
            target: None,
            command: None,
            desktop_id: None,
            supports: None,
            check: false,
            installed: false,
            available: false
        })
    );
}

#[test]
fn desktop_profiles_accepts_json_and_filters() {
    let command = parse_desktop_args(vec![
        "profiles".to_owned(),
        "--json".to_owned(),
        "--app".to_owned(),
        "telegram".to_owned(),
        "--target".to_owned(),
        "message-input".to_owned(),
        "--command".to_owned(),
        "flatpak".to_owned(),
        "--desktop-id".to_owned(),
        "org.telegram.desktop".to_owned(),
        "--supports".to_owned(),
        "type-into".to_owned(),
        "--availability".to_owned(),
        "--installed".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        DesktopCommand::Profiles(DesktopProfilesArgs {
            json: true,
            app: Some("telegram".to_owned()),
            target: Some("message-input".to_owned()),
            command: Some("flatpak".to_owned()),
            desktop_id: Some("org.telegram.desktop".to_owned()),
            supports: Some("type-into".to_owned()),
            check: true,
            installed: true,
            available: false
        })
    );
}

#[test]
fn desktop_focus_accepts_app_and_wait_options() {
    let command = parse_desktop_args(vec![
        "focus".to_owned(),
        "--app".to_owned(),
        "telegram".to_owned(),
        "--no-overview".to_owned(),
        "--wait-ms".to_owned(),
        "250".to_owned(),
        "--overview-wait-ms".to_owned(),
        "125".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        DesktopCommand::Focus(DesktopFocusArgs {
            app: "telegram".to_owned(),
            use_gnome_overview: false,
            launch_if_needed: true,
            wait_after_focus_ms: 250,
            overview_wait_ms: 125,
            window_title: None,
            window_id: None,
            verify: false,
            json: false
        })
    );
}

#[test]
fn desktop_focus_accepts_window_title_filter() {
    let command = parse_desktop_args(vec![
        "focus".to_owned(),
        "--app".to_owned(),
        "text-editor".to_owned(),
        "--window-title".to_owned(),
        "peekaboox-draft.txt".to_owned(),
        "--no-launch".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        DesktopCommand::Focus(DesktopFocusArgs {
            app: "text-editor".to_owned(),
            use_gnome_overview: true,
            launch_if_needed: false,
            wait_after_focus_ms: 1_000,
            overview_wait_ms: 800,
            window_title: Some("peekaboox-draft.txt".to_owned()),
            window_id: None,
            verify: false,
            json: false
        })
    );
}

#[test]
fn desktop_focus_accepts_window_id_verify_and_json() {
    let command = parse_desktop_args(vec![
        "focus".to_owned(),
        "--app".to_owned(),
        "telegram".to_owned(),
        "--window-id".to_owned(),
        "123".to_owned(),
        "--verify".to_owned(),
        "--json".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        DesktopCommand::Focus(DesktopFocusArgs {
            app: "telegram".to_owned(),
            use_gnome_overview: true,
            launch_if_needed: true,
            wait_after_focus_ms: 1_000,
            overview_wait_ms: 800,
            window_title: None,
            window_id: Some("123".to_owned()),
            verify: true,
            json: true
        })
    );
}

#[test]
fn desktop_locate_accepts_positional_app_and_target() {
    let command = parse_desktop_args(vec![
        "locate".to_owned(),
        "telegram".to_owned(),
        "send-button".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        DesktopCommand::Locate(DesktopLocateArgs {
            app: "telegram".to_owned(),
            target: "send-button".to_owned(),
            image: None,
            prefer_accessibility: true,
            window_title: None,
            window_id: None,
            json: false
        })
    );
}

#[test]
fn desktop_click_accepts_button_dry_run_and_image() {
    let command = parse_desktop_args(vec![
        "click".to_owned(),
        "--app".to_owned(),
        "telegram".to_owned(),
        "--target".to_owned(),
        "search-input".to_owned(),
        "--button".to_owned(),
        "right".to_owned(),
        "--image".to_owned(),
        "screen.png".to_owned(),
        "--dry-run".to_owned(),
        "--no-accessibility".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        DesktopCommand::Click(DesktopClickArgs {
            app: "telegram".to_owned(),
            target: "search-input".to_owned(),
            image: Some(PathBuf::from("screen.png")),
            prefer_accessibility: false,
            window_title: None,
            window_id: None,
            button: MouseButton::Right,
            dry_run: true,
            verify: false,
            json: false
        })
    );
}

#[test]
fn desktop_drag_accepts_ratio_endpoints() {
    let command = parse_desktop_args(vec![
        "drag".to_owned(),
        "--app".to_owned(),
        "drawing".to_owned(),
        "--target".to_owned(),
        "canvas".to_owned(),
        "--from-ratio".to_owned(),
        "0.2,0.3".to_owned(),
        "--to-ratio".to_owned(),
        "0.8,0.7".to_owned(),
        "--duration-ms".to_owned(),
        "400".to_owned(),
        "--dry-run".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        DesktopCommand::Drag(DesktopDragArgs {
            app: "drawing".to_owned(),
            target: "canvas".to_owned(),
            image: None,
            prefer_accessibility: true,
            window_title: None,
            window_id: None,
            button: MouseButton::Left,
            from_ratio: (0.2, 0.3),
            to_ratio: (0.8, 0.7),
            duration_ms: 400,
            dry_run: true,
            verify: false,
            json: false
        })
    );
}

#[test]
fn desktop_type_into_joins_text() {
    let command = parse_desktop_args(vec![
        "type-into".to_owned(),
        "telegram".to_owned(),
        "message-input".to_owned(),
        "--clear".to_owned(),
        "PeekabooX".to_owned(),
        "Example".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        DesktopCommand::TypeInto(DesktopTypeIntoArgs {
            app: "telegram".to_owned(),
            target: "message-input".to_owned(),
            text: "PeekabooX Example".to_owned(),
            image: None,
            prefer_accessibility: true,
            window_title: None,
            window_id: None,
            clear: true,
            dry_run: false,
            verify: false,
            json: false
        })
    );
}

#[test]
fn desktop_assert_not_active_maps_to_not_active_guard() {
    let command = parse_desktop_args(vec![
        "assert".to_owned(),
        "--app".to_owned(),
        "telegram".to_owned(),
        "--target".to_owned(),
        "send-button".to_owned(),
        "--not-active".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        DesktopCommand::Assert(DesktopAssertArgs {
            app: "telegram".to_owned(),
            target: "send-button".to_owned(),
            image: None,
            prefer_accessibility: true,
            window_title: None,
            window_id: None,
            assertion: DesktopAssertion::NotActive,
            json: false
        })
    );
}

#[test]
fn desktop_assert_not_negates_contains() {
    let command = parse_desktop_args(vec![
        "assert-not".to_owned(),
        "telegram".to_owned(),
        "header".to_owned(),
        "--contains".to_owned(),
        "Alerts".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        DesktopCommand::Assert(DesktopAssertArgs {
            app: "telegram".to_owned(),
            target: "header".to_owned(),
            image: None,
            prefer_accessibility: true,
            window_title: None,
            window_id: None,
            assertion: DesktopAssertion::NotContains("Alerts".to_owned()),
            json: false
        })
    );
}

#[test]
fn click_requires_coordinates() {
    let error = parse_click_args(vec!["--x".to_owned(), "10".to_owned()]).unwrap_err();

    assert_eq!(error, CliError::Failure("missing required --y".to_owned()));
}

#[test]
fn click_accepts_coordinates_button_and_dry_run() {
    let command = parse_click_args(vec![
        "--x".to_owned(),
        "10".to_owned(),
        "--y".to_owned(),
        "20".to_owned(),
        "--button".to_owned(),
        "right".to_owned(),
        "--dry-run".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        ClickCommand::Run(ClickArgs {
            target: ClickTarget::Coordinates(Point::new(10, 20)),
            button: MouseButton::Right,
            dry_run: true,
            json: false,
            vision_fallback: false,
            bounds_policy: MoveBoundsPolicy::Allow,
            backend: InputToolSelection::Auto,
            restore: false
        })
    );
}

#[test]
fn click_accepts_text_selector() {
    let command = parse_click_args(vec![
        "--text".to_owned(),
        "Submit".to_owned(),
        "--dry-run".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        ClickCommand::Run(ClickArgs {
            target: ClickTarget::SemanticSelector("Submit".to_owned()),
            button: MouseButton::Left,
            dry_run: true,
            json: false,
            vision_fallback: false,
            bounds_policy: MoveBoundsPolicy::Allow,
            backend: InputToolSelection::Auto,
            restore: false
        })
    );
}

#[test]
fn click_accepts_vision_fallback_flag() {
    let command = parse_click_args(vec![
        "--selector".to_owned(),
        "role=visual-region,contains=10,20".to_owned(),
        "--vision-fallback".to_owned(),
        "--dry-run".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        ClickCommand::Run(ClickArgs {
            target: ClickTarget::SemanticSelector("role=visual-region,contains=10,20".to_owned()),
            button: MouseButton::Left,
            dry_run: true,
            json: false,
            vision_fallback: true,
            bounds_policy: MoveBoundsPolicy::Allow,
            backend: InputToolSelection::Auto,
            restore: false
        })
    );
}

#[test]
fn click_accepts_scoped_ratio_options_and_json() {
    let command = parse_click_args(vec![
        "--ratio".to_owned(),
        "0.25,0.75".to_owned(),
        "--region".to_owned(),
        "10,20,300,200".to_owned(),
        "--button".to_owned(),
        "middle".to_owned(),
        "--bounds".to_owned(),
        "clamp".to_owned(),
        "--backend".to_owned(),
        "xdotool".to_owned(),
        "--restore".to_owned(),
        "--dry-run".to_owned(),
        "--json".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        ClickCommand::Run(ClickArgs {
            target: ClickTarget::ScopeRatio {
                ratio: (0.25, 0.75),
                region: Some(Rect::new(10, 20, 300, 200)),
                window_id: None,
                app: None,
                window_title: None,
                title_regex: None
            },
            button: MouseButton::Middle,
            dry_run: true,
            json: true,
            vision_fallback: false,
            bounds_policy: MoveBoundsPolicy::Clamp,
            backend: InputToolSelection::Xdotool,
            restore: true
        })
    );
}

#[test]
fn click_rejects_coordinates_and_selector_together() {
    let error = parse_click_args(vec![
        "--x".to_owned(),
        "10".to_owned(),
        "--y".to_owned(),
        "20".to_owned(),
        "--selector".to_owned(),
        "role=button".to_owned(),
    ])
    .unwrap_err();

    assert_eq!(
        error,
        CliError::Failure("provide exactly one click target".to_owned())
    );
}

#[test]
fn move_accepts_coordinates_and_dry_run() {
    let command = parse_move_args(vec![
        "--x".to_owned(),
        "10".to_owned(),
        "--y".to_owned(),
        "20".to_owned(),
        "--dry-run".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        MoveCommand::Run(MoveArgs {
            target: MoveTarget::Position(Point::new(10, 20)),
            dry_run: true,
            json: false,
            duration_ms: 0,
            steps: None,
            bounds_policy: MoveBoundsPolicy::Allow,
            backend: InputToolSelection::Auto,
            restore: false,
        })
    );
}

#[test]
fn move_requires_y_coordinate() {
    let error = parse_move_args(vec!["--x".to_owned(), "10".to_owned()]).unwrap_err();

    assert_eq!(error, CliError::Failure("missing required --y".to_owned()));
}

#[test]
fn move_accepts_compact_relative_scope_and_options() {
    let compact = parse_move_args(vec![
        "--to".to_owned(),
        "10,20".to_owned(),
        "--duration-ms".to_owned(),
        "120".to_owned(),
        "--steps".to_owned(),
        "6".to_owned(),
        "--backend".to_owned(),
        "xdotool".to_owned(),
        "--clamp".to_owned(),
        "--restore".to_owned(),
        "--json".to_owned(),
    ])
    .unwrap();
    let relative = parse_move_args(vec!["--relative".to_owned(), "-5,6".to_owned()]).unwrap();
    let scoped = parse_move_args(vec![
        "--window-title".to_owned(),
        "Calculator".to_owned(),
        "--region".to_owned(),
        "10,20,300,200".to_owned(),
        "--ratio".to_owned(),
        "0.25,0.75".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        compact,
        MoveCommand::Run(MoveArgs {
            target: MoveTarget::Position(Point::new(10, 20)),
            dry_run: false,
            json: true,
            duration_ms: 120,
            steps: Some(6),
            bounds_policy: MoveBoundsPolicy::Clamp,
            backend: InputToolSelection::Xdotool,
            restore: true,
        })
    );
    assert_eq!(
        relative,
        MoveCommand::Run(MoveArgs {
            target: MoveTarget::Relative(Point::new(-5, 6)),
            dry_run: false,
            json: false,
            duration_ms: 0,
            steps: None,
            bounds_policy: MoveBoundsPolicy::Allow,
            backend: InputToolSelection::Auto,
            restore: false,
        })
    );
    assert_eq!(
        scoped,
        MoveCommand::Run(MoveArgs {
            target: MoveTarget::ScopeRatio {
                ratio: (0.25, 0.75),
                region: Some(Rect::new(10, 20, 300, 200)),
                window_id: None,
                app: None,
                window_title: Some("Calculator".to_owned()),
                title_regex: None,
            },
            dry_run: false,
            json: false,
            duration_ms: 0,
            steps: None,
            bounds_policy: MoveBoundsPolicy::Allow,
            backend: InputToolSelection::Auto,
            restore: false,
        })
    );
}

#[test]
fn drag_accepts_compact_points() {
    let command = parse_drag_args(vec![
        "--from".to_owned(),
        "10,20".to_owned(),
        "--to".to_owned(),
        "40,80".to_owned(),
        "--button".to_owned(),
        "middle".to_owned(),
        "--duration-ms".to_owned(),
        "500".to_owned(),
        "--dry-run".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        DragCommand::Run(Box::new(DragArgs {
            from: DragEndpoint::Position(Point::new(10, 20)),
            to: DragEndpoint::Position(Point::new(40, 80)),
            button: MouseButton::Middle,
            duration_ms: 500,
            steps: None,
            bounds_policy: MoveBoundsPolicy::Allow,
            backend: InputToolSelection::Auto,
            restore: false,
            dry_run: true,
            json: false
        }))
    );
}

#[test]
fn drag_accepts_split_points() {
    let command = parse_drag_args(vec![
        "--from-x".to_owned(),
        "10".to_owned(),
        "--from-y".to_owned(),
        "20".to_owned(),
        "--to-x".to_owned(),
        "40".to_owned(),
        "--to-y".to_owned(),
        "80".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        DragCommand::Run(Box::new(DragArgs {
            from: DragEndpoint::Position(Point::new(10, 20)),
            to: DragEndpoint::Position(Point::new(40, 80)),
            button: MouseButton::Left,
            duration_ms: 250,
            steps: None,
            bounds_policy: MoveBoundsPolicy::Allow,
            backend: InputToolSelection::Auto,
            restore: false,
            dry_run: false,
            json: false
        }))
    );
}

#[test]
fn drag_accepts_current_ratio_scope_and_options() {
    let command = parse_drag_args(vec![
        "--from-current".to_owned(),
        "--to-ratio".to_owned(),
        "0.8,0.25".to_owned(),
        "--region".to_owned(),
        "10,20,300,200".to_owned(),
        "--steps".to_owned(),
        "8".to_owned(),
        "--backend".to_owned(),
        "xdotool".to_owned(),
        "--clamp".to_owned(),
        "--restore".to_owned(),
        "--json".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        DragCommand::Run(Box::new(DragArgs {
            from: DragEndpoint::CurrentPosition,
            to: DragEndpoint::ScopeRatio {
                ratio: (0.8, 0.25),
                region: Some(Rect::new(10, 20, 300, 200)),
                window_id: None,
                app: None,
                window_title: None,
                title_regex: None,
            },
            button: MouseButton::Left,
            duration_ms: 250,
            steps: Some(8),
            bounds_policy: MoveBoundsPolicy::Clamp,
            backend: InputToolSelection::Xdotool,
            restore: true,
            dry_run: false,
            json: true
        }))
    );
}

#[test]
fn drag_rejects_mixed_point_styles() {
    let error = parse_drag_args(vec![
        "--from".to_owned(),
        "10,20".to_owned(),
        "--from-x".to_owned(),
        "10".to_owned(),
        "--to".to_owned(),
        "40,80".to_owned(),
    ])
    .unwrap_err();

    assert_eq!(
        error,
        CliError::Failure("provide either --from or --from-x/--from-y, not both".to_owned())
    );
}

#[test]
fn type_joins_remaining_text_arguments() {
    let command = parse_type_args(vec!["hello".to_owned(), "world".to_owned()]).unwrap();

    assert_eq!(
        command,
        TypeCommand::Run(TypeArgs {
            source: TypeTextSource::Arguments(vec!["hello".to_owned(), "world".to_owned()]),
            dry_run: false,
            paste: false,
            preserve_clipboard: false,
            json: false,
            typing_speed_chars_per_second: None,
            delay_ms: None,
            key_delay_ms: None,
            backend: InputToolSelection::Auto,
            clipboard_backend: ClipboardBackendSelection::Auto,
            hotkey_backend: PasteHotkeyBackendSelection::Auto,
            restore_delay_ms: None,
            restore_policy: ClipboardRestorePolicy::Strict,
        })
    );
}

#[test]
fn type_accepts_dry_run() {
    let command = parse_type_args(vec!["--dry-run".to_owned(), "hello".to_owned()]).unwrap();

    assert_eq!(
        command,
        TypeCommand::Run(TypeArgs {
            source: TypeTextSource::Arguments(vec!["hello".to_owned()]),
            dry_run: true,
            paste: false,
            preserve_clipboard: false,
            json: false,
            typing_speed_chars_per_second: None,
            delay_ms: None,
            key_delay_ms: None,
            backend: InputToolSelection::Auto,
            clipboard_backend: ClipboardBackendSelection::Auto,
            hotkey_backend: PasteHotkeyBackendSelection::Auto,
            restore_delay_ms: None,
            restore_policy: ClipboardRestorePolicy::Strict,
        })
    );
}

#[test]
fn type_accepts_paste_flag() {
    let command = parse_type_args(vec!["--paste".to_owned(), "hello".to_owned()]).unwrap();

    assert_eq!(
        command,
        TypeCommand::Run(TypeArgs {
            source: TypeTextSource::Arguments(vec!["hello".to_owned()]),
            dry_run: false,
            paste: true,
            preserve_clipboard: false,
            json: false,
            typing_speed_chars_per_second: None,
            delay_ms: None,
            key_delay_ms: None,
            backend: InputToolSelection::Auto,
            clipboard_backend: ClipboardBackendSelection::Auto,
            hotkey_backend: PasteHotkeyBackendSelection::Auto,
            restore_delay_ms: None,
            restore_policy: ClipboardRestorePolicy::Strict,
        })
    );
}

#[test]
fn type_accepts_preserve_clipboard_for_paste() {
    let command = parse_type_args(vec![
        "--paste".to_owned(),
        "--preserve-clipboard".to_owned(),
        "hello".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        TypeCommand::Run(TypeArgs {
            source: TypeTextSource::Arguments(vec!["hello".to_owned()]),
            dry_run: false,
            paste: true,
            preserve_clipboard: true,
            json: false,
            typing_speed_chars_per_second: None,
            delay_ms: None,
            key_delay_ms: None,
            backend: InputToolSelection::Auto,
            clipboard_backend: ClipboardBackendSelection::Auto,
            hotkey_backend: PasteHotkeyBackendSelection::Auto,
            restore_delay_ms: None,
            restore_policy: ClipboardRestorePolicy::Strict,
        })
    );
}

#[test]
fn type_accepts_timing_backend_json_and_explicit_text() {
    let command = parse_type_args(vec![
        "--json".to_owned(),
        "--backend".to_owned(),
        "wtype".to_owned(),
        "--typing-speed".to_owned(),
        "20".to_owned(),
        "--delay-ms".to_owned(),
        "10".to_owned(),
        "--text".to_owned(),
        "hello".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        TypeCommand::Run(TypeArgs {
            source: TypeTextSource::Text("hello".to_owned()),
            dry_run: false,
            paste: false,
            preserve_clipboard: false,
            json: true,
            typing_speed_chars_per_second: Some(20),
            delay_ms: Some(10),
            key_delay_ms: None,
            backend: InputToolSelection::Wtype,
            clipboard_backend: ClipboardBackendSelection::Auto,
            hotkey_backend: PasteHotkeyBackendSelection::Auto,
            restore_delay_ms: None,
            restore_policy: ClipboardRestorePolicy::Strict,
        })
    );
}

#[test]
fn type_accepts_file_stdin_and_dash_separator_sources() {
    let file_command =
        parse_type_args(vec!["--file".to_owned(), "/tmp/example.txt".to_owned()]).unwrap();
    let stdin_command = parse_type_args(vec!["--stdin".to_owned()]).unwrap();
    let dash_command = parse_type_args(vec![
        "--".to_owned(),
        "--literal".to_owned(),
        "text".to_owned(),
    ])
    .unwrap();

    assert!(matches!(
        file_command,
        TypeCommand::Run(TypeArgs {
            source: TypeTextSource::File(_),
            ..
        })
    ));
    assert!(matches!(
        stdin_command,
        TypeCommand::Run(TypeArgs {
            source: TypeTextSource::Stdin,
            ..
        })
    ));
    assert!(matches!(
        dash_command,
        TypeCommand::Run(TypeArgs {
            source: TypeTextSource::Arguments(_),
            ..
        })
    ));
}

#[test]
fn paste_accepts_backend_timing_restore_and_sources() {
    let command = parse_paste_args(vec![
        "--dry-run".to_owned(),
        "--json".to_owned(),
        "--preserve-clipboard".to_owned(),
        "--clipboard-backend".to_owned(),
        "xclip".to_owned(),
        "--hotkey-backend".to_owned(),
        "xdotool".to_owned(),
        "--delay-ms".to_owned(),
        "30".to_owned(),
        "--restore-delay-ms".to_owned(),
        "70".to_owned(),
        "--restore-policy".to_owned(),
        "best-effort".to_owned(),
        "--text".to_owned(),
        "hello".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        PasteCommand::Run(PasteArgs {
            source: TypeTextSource::Text("hello".to_owned()),
            dry_run: true,
            preserve_clipboard: true,
            json: true,
            clipboard_backend: ClipboardBackendSelection::Xclip,
            hotkey_backend: PasteHotkeyBackendSelection::Xdotool,
            delay_ms: Some(30),
            restore_delay_ms: Some(70),
            restore_policy: ClipboardRestorePolicy::BestEffort,
        })
    );

    let stdin_command = parse_paste_args(vec!["--stdin".to_owned()]).unwrap();
    let dash_command = parse_paste_args(vec![
        "--".to_owned(),
        "--literal".to_owned(),
        "text".to_owned(),
    ])
    .unwrap();

    assert!(matches!(
        stdin_command,
        PasteCommand::Run(PasteArgs {
            source: TypeTextSource::Stdin,
            ..
        })
    ));
    assert!(matches!(
        dash_command,
        PasteCommand::Run(PasteArgs {
            source: TypeTextSource::Arguments(_),
            ..
        })
    ));
}

#[test]
fn hotkey_accepts_positional_keys_and_dry_run() {
    let command = parse_hotkey_args(vec![
        "--dry-run".to_owned(),
        "--json".to_owned(),
        "--backend".to_owned(),
        "ydotool".to_owned(),
        "--delay-ms".to_owned(),
        "25".to_owned(),
        "--key-delay-ms".to_owned(),
        "30".to_owned(),
        "--repeat".to_owned(),
        "2".to_owned(),
        "--interval-ms".to_owned(),
        "40".to_owned(),
        "--release-before".to_owned(),
        "--release-after".to_owned(),
        "ctrl".to_owned(),
        "s".to_owned(),
    ])
    .unwrap();

    assert_eq!(
        command,
        HotkeyCommand::Run(HotkeyArgs {
            keys: vec!["ctrl".to_owned(), "s".to_owned()],
            dry_run: true,
            json: true,
            backend: InputToolSelection::Ydotool,
            delay_ms: Some(25),
            key_delay_ms: Some(30),
            repeat: Some(2),
            interval_ms: Some(40),
            release_before: true,
            release_after: true,
        })
    );
}

#[test]
fn hotkey_accepts_dash_separator_and_rejects_empty_chords() {
    let command = parse_hotkey_args(vec![
        "--key".to_owned(),
        "control+escape".to_owned(),
        "--".to_owned(),
        "s".to_owned(),
    ])
    .unwrap();

    assert!(matches!(
        command,
        HotkeyCommand::Run(HotkeyArgs {
            keys,
            ..
        }) if keys == vec!["control+escape".to_owned(), "s".to_owned()]
    ));

    let error = parse_hotkey_args(vec!["ctrl++".to_owned()]).unwrap_err();
    assert!(format!("{error:?}").contains("empty"));
}

#[test]
fn hotkey_requires_keys() {
    let error = parse_hotkey_args(vec![]).unwrap_err();

    assert_eq!(
        error,
        CliError::Failure("missing hotkey; provide one or more keys".to_owned())
    );
}
