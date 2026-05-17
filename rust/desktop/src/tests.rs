use super::*;

#[test]
fn supported_apps_contains_desktop_profiles() {
    assert_eq!(
        supported_apps(),
        &[
            "telegram",
            "paint",
            "drawing",
            "pinta",
            "kolourpaint",
            "text-editor",
            "calendar"
        ]
    );
    assert!(resolve_profile("telegram-desktop").is_ok());
    assert_eq!(resolve_profile("pinta").unwrap().id, "pinta");
    assert_eq!(
        resolve_profile("gnome-text-editor").unwrap().id,
        "text-editor"
    );
    assert_eq!(resolve_profile("gnome-calendar").unwrap().id, "calendar");
    assert_eq!(
        resolve_profile("org.gnome.Calendar").unwrap().id,
        "calendar"
    );
}

#[test]
fn desktop_profile_query_prefers_specific_profiles_over_paint_aggregate() {
    let drawing = desktop_profiles_with_query(&DesktopProfileQuery {
        app: Some("drawing".to_owned()),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(drawing.count, 1);
    assert_eq!(drawing.profiles[0].id, "drawing");

    let drawing_desktop_id = desktop_profiles_with_query(&DesktopProfileQuery {
        app: Some("com.github.maoschanz.drawing".to_owned()),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(drawing_desktop_id.count, 1);
    assert_eq!(drawing_desktop_id.profiles[0].id, "drawing");

    let paint = desktop_profiles_with_query(&DesktopProfileQuery {
        app: Some("paint".to_owned()),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(paint.count, 1);
    assert_eq!(paint.profiles[0].id, "paint");
}

#[test]
fn calendar_profile_exposes_fast_event_targets() {
    let profile = resolve_profile("calendar").unwrap();
    let targets = profile.supported_targets();

    assert!(targets.contains(&"window".to_owned()));
    assert!(targets.contains(&"new-event-button".to_owned()));
    assert!(targets.contains(&"edit-details-button".to_owned()));
    assert!(targets.contains(&"save-event-button".to_owned()));
    assert!(target_info(&profile, "title-field").can_type);
    assert_eq!(
        profile.accessibility_selector("save-button"),
        Some("role=push button,label-regex=Save Event|Save event|Save")
    );
}

#[test]
fn desktop_profile_query_loads_external_profile_files() {
    let dir = temp_profile_dir("external-profile");
    let path = dir.join("calculator.json");
    fs::write(
        &path,
        r#"{
  "schema_version": "desktop-profile.v1",
  "id": "calculator",
  "kind": "generic",
  "aliases": ["calc", "org.gnome.Calculator"],
  "search_name": "Calculator",
  "desktop_ids": ["org.gnome.Calculator"],
  "commands": [{"program": "gnome-calculator"}],
  "targets": [
    {
      "name": "display",
      "supports": ["type-into", "drag", "assert-contains"],
      "visual": {
        "type": "relative-rect",
        "x": 0.1,
        "y": 0.1,
        "width": 0.8,
        "height": 0.2,
        "point_x": 0.75,
        "point_y": 0.5
      }
    },
    {
      "name": "title",
      "text_anchor": "Total",
      "visual": {
        "type": "ocr-text",
        "region": {
          "x": 0.1,
          "y": 0.0,
          "width": 0.8,
          "height": 0.2
        }
      }
    },
    {
      "name": "dark-pixel",
      "supports": ["click"],
      "color_anchor": {
        "red": 20,
        "green": 20,
        "blue": 20,
        "tolerance": 0
      },
      "wait": {
        "before_ms": 0
      },
      "visual": {
        "type": "relative-rect",
        "x": 0.2,
        "y": 0.2,
        "width": 0.1,
        "height": 0.1
      }
    }
  ]
}"#,
    )
    .unwrap();

    let result = desktop_profiles_with_query_and_paths(
        &DesktopProfileQuery {
            app: Some("calc".to_owned()),
            target: Some("display".to_owned()),
            supports: Some("assert_contains".to_owned()),
            ..Default::default()
        },
        std::slice::from_ref(&dir),
    )
    .unwrap();

    assert_eq!(result.count, 1);
    let profile = &result.profiles[0];
    assert_eq!(profile.id, "calculator");
    assert_eq!(profile.commands[0].display, "gnome-calculator");
    let display = profile
        .targets
        .iter()
        .find(|target| target.name == "display")
        .unwrap();
    assert!(display.can_type);
    assert!(display.can_drag);
    assert!(display.can_assert_contains);

    let catalog = profile_catalog_from_paths(std::slice::from_ref(&dir)).unwrap();
    let profile = catalog
        .iter()
        .find(|profile| profile.id == "calculator")
        .unwrap();
    let frame = blank_frame(1_000, 800, (20, 20, 20));
    let resolved = profile
        .resolve_visual_target("display", &frame, WindowScope::default())
        .unwrap();

    assert_eq!(resolved.point, Point::new(699, 160));
    assert_eq!(resolved.rect, Some(Rect::new(100, 80, 800, 160)));
    let title = profile.custom_target("title").unwrap();
    let title_info = custom_target_info(title);
    assert!(title_info.sources.contains(&"ocr".to_owned()));
    assert!(title_info.supports.contains(&"text-anchor".to_owned()));

    let color_anchor = profile
        .resolve_visual_target("dark-pixel", &frame, WindowScope::default())
        .unwrap();
    assert_eq!(color_anchor.point, Point::new(200, 160));
    assert_eq!(color_anchor.rect, Some(Rect::new(200, 160, 100, 80)));
    let color_info = custom_target_info(profile.custom_target("dark-pixel").unwrap());
    assert!(color_info.sources.contains(&"color-anchor".to_owned()));
    assert!(color_info.supports.contains(&"wait".to_owned()));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn desktop_profile_loader_rejects_invalid_schema() {
    let dir = temp_profile_dir("invalid-schema");
    let path = dir.join("bad.json");
    fs::write(
        &path,
        r#"{
  "schema_version": "desktop-profile.v0",
  "id": "bad",
  "search_name": "Bad"
}"#,
    )
    .unwrap();

    let error = profile_catalog_from_paths(std::slice::from_ref(&dir))
        .unwrap_err()
        .to_string();
    assert!(error.contains("unsupported desktop profile schema_version"));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn locates_telegram_targets_from_visual_layout() {
    let frame = synthetic_telegram_frame(false);

    let search = locate_search_input(&frame).unwrap();
    let result = locate_search_result(&frame).unwrap();
    let input = locate_message_input(&frame).unwrap();
    let send = locate_send_button(&frame).unwrap();

    assert_eq!(search.point, Point::new(897, 158));
    assert_eq!(result.point, Point::new(825, 216));
    assert_eq!(input.point, Point::new(1118, 845));
    assert_eq!(send.point, Point::new(1645, 845));
}

#[test]
fn detects_active_telegram_draft_send_button() {
    assert!(!draft_send_button_active(&synthetic_telegram_frame(false)).unwrap());
    assert!(draft_send_button_active(&synthetic_telegram_frame(true)).unwrap());
}

#[test]
fn locates_overview_icon_by_telegram_blue_component() {
    let mut frame = blank_frame(1_920, 1_200, (10, 10, 10));
    fill_rect(&mut frame, 970, 220, 48, 48, (50, 170, 230));

    let target = locate_overview_icon(&frame).unwrap();

    assert_eq!(target.point, Point::new(994, 244));
}

#[test]
fn locates_paint_canvas_from_visual_layout() {
    let mut frame = blank_frame(1_280, 900, (34, 36, 40));
    fill_rect(&mut frame, 220, 150, 820, 620, (248, 248, 247));
    fill_rect(&mut frame, 20, 20, 200, 70, (245, 245, 245));

    let target = locate_paint_canvas(&frame).unwrap();

    assert_eq!(target.rect, Some(Rect::new(220, 152, 820, 620)));
    assert_eq!(target.point, Point::new(507, 369));
}

#[test]
fn locates_paint_canvas_outline_inside_white_workspace() {
    let mut frame = blank_frame(1_920, 1_200, (34, 36, 40));
    fill_rect(&mut frame, 68, 140, 1_852, 1_060, (248, 248, 248));
    fill_rect(&mut frame, 938, 200, 4, 608, (44, 130, 230));
    fill_rect(&mut frame, 134, 804, 808, 4, (44, 130, 230));

    let target = locate_paint_canvas(&frame).unwrap();

    assert_eq!(target.rect, Some(Rect::new(134, 200, 808, 608)));
    assert_eq!(target.point, Point::new(416, 412));
}

#[test]
fn point_in_rect_ratio_maps_inside_rectangle() {
    let point = point_in_rect_ratio(Rect::new(100, 200, 401, 201), (0.25, 0.5)).unwrap();

    assert_eq!(point, Point::new(200, 300));
}

#[test]
fn text_editor_document_rect_stays_inside_window_chrome() {
    let rect = text_editor_document_rect(Rect::new(10, 20, 1_000, 700));

    assert_eq!(rect, Rect::new(35, 90, 950, 592));
}

#[test]
fn preferred_profile_window_respects_title_hint() {
    let profile = text_editor_profile();
    let windows = vec![
        peekaboox_core::WindowInfo {
            id: "focused-user-doc".to_owned(),
            title: "notes.txt - Text Editor".to_owned(),
            app_id: Some("gnome-text-editor".to_owned()),
            bounds: Rect::new(0, 0, 900, 700),
            focused: true,
            state: peekaboox_core::WindowState::Normal,
        },
        peekaboox_core::WindowInfo {
            id: "draft".to_owned(),
            title: "peekaboox-draft.txt - Text Editor".to_owned(),
            app_id: Some("gnome-text-editor".to_owned()),
            bounds: Rect::new(200, 120, 700, 520),
            focused: false,
            state: peekaboox_core::WindowState::Normal,
        },
    ];

    let selected = preferred_profile_window(
        &profile,
        &windows,
        WindowScope {
            title_hint: Some("peekaboox-draft"),
            window_id: None,
        },
    )
    .unwrap();

    assert_eq!(selected.id, "draft");
}

#[test]
fn preferred_profile_window_respects_window_id() {
    let profile = text_editor_profile();
    let windows = vec![
        peekaboox_core::WindowInfo {
            id: "first".to_owned(),
            title: "Text Editor".to_owned(),
            app_id: Some("gnome-text-editor".to_owned()),
            bounds: Rect::new(0, 0, 900, 700),
            focused: true,
            state: peekaboox_core::WindowState::Normal,
        },
        peekaboox_core::WindowInfo {
            id: "second".to_owned(),
            title: "Text Editor".to_owned(),
            app_id: Some("gnome-text-editor".to_owned()),
            bounds: Rect::new(200, 120, 700, 520),
            focused: false,
            state: peekaboox_core::WindowState::Normal,
        },
    ];

    let selected = preferred_profile_window(
        &profile,
        &windows,
        WindowScope {
            title_hint: None,
            window_id: Some("second"),
        },
    )
    .unwrap();

    assert_eq!(selected.id, "second");
}

#[test]
fn live_actions_focus_before_locating_only_for_real_screen_actions() {
    let locate = LocateOptions::default();
    assert!(should_focus_before_live_action(&locate, false));
    assert!(!should_focus_before_live_action(&locate, true));

    let image_locate = LocateOptions {
        image: Some(PathBuf::from("screen.png")),
        ..Default::default()
    };
    assert!(!should_focus_before_live_action(&image_locate, false));
}

#[test]
fn live_action_focus_options_preserve_window_scope_and_verify() {
    let locate = LocateOptions {
        window_title: Some("draft.txt".to_owned()),
        window_id: Some("window-42".to_owned()),
        ..Default::default()
    };

    let options = focus_options_for_live_action(&locate);

    assert!(options.use_gnome_overview);
    assert!(options.launch_if_needed);
    assert!(options.verify);
    assert_eq!(options.overview_wait_ms, ACTION_FOCUS_OVERVIEW_WAIT_MS);
    assert_eq!(options.window_title.as_deref(), Some("draft.txt"));
    assert_eq!(options.window_id.as_deref(), Some("window-42"));
}

#[test]
fn action_detail_includes_focus_context_when_available() {
    let detail = action_detail_with_focus(
        Some(&DesktopActionResult {
            app: "text-editor".to_owned(),
            action: "focus".to_owned(),
            detail: "already focused".to_owned(),
            backend_name: "at-spi".to_owned(),
            verified: true,
            verification_detail: Some("window focused".to_owned()),
            focus_diagnostics: vec!["at-spi: grabbed focus".to_owned()],
        }),
        "typed into document".to_owned(),
    );

    assert_eq!(
        detail,
        "typed into document; focus already focused via at-spi"
    );
    assert_eq!(
        action_detail_with_focus(None, "clicked target".to_owned()),
        "clicked target"
    );
}

#[test]
fn focus_diagnostics_are_carried_from_pre_action_focus() {
    let focus = DesktopActionResult {
        app: "text-editor".to_owned(),
        action: "focus".to_owned(),
        detail: "already focused".to_owned(),
        backend_name: "window-manager".to_owned(),
        verified: true,
        verification_detail: Some("window focused".to_owned()),
        focus_diagnostics: vec![
            "windows: selected window-1".to_owned(),
            "verify: window window-1 is focused".to_owned(),
        ],
    };

    assert_eq!(
        focus_diagnostics_from(Some(&focus)),
        vec![
            "windows: selected window-1".to_owned(),
            "verify: window window-1 is focused".to_owned(),
        ]
    );
    assert!(focus_diagnostics_from(None).is_empty());
}

#[test]
fn gnome_dock_focus_candidate_maps_left_dock_label_to_icon_center() {
    let profile = text_editor_profile();
    let elements = vec![
        UiElement {
            id: "chrome-label".to_owned(),
            role: "label".to_owned(),
            label: Some("Google Chrome".to_owned()),
            bounds: Rect::new(69, 250, 130, 32),
            center: Some(Point::new(134, 266)),
            confidence: 1.0,
            states: Vec::new(),
            window_id: Some("gnome-shell".to_owned()),
            window_title: None,
            app_id: Some("gnome-shell".to_owned()),
            parent_id: None,
            child_ids: Vec::new(),
        },
        UiElement {
            id: "text-editor-label".to_owned(),
            role: "label".to_owned(),
            label: Some("Text Editor".to_owned()),
            bounds: Rect::new(69, 573, 99, 32),
            center: Some(Point::new(118, 589)),
            confidence: 1.0,
            states: Vec::new(),
            window_id: Some("gnome-shell".to_owned()),
            window_title: None,
            app_id: Some("gnome-shell".to_owned()),
            parent_id: None,
            child_ids: Vec::new(),
        },
        UiElement {
            id: "menu-label".to_owned(),
            role: "label".to_owned(),
            label: Some("Open Text Editor".to_owned()),
            bounds: Rect::new(1_633, 54, 120, 18),
            center: Some(Point::new(1_693, 63)),
            confidence: 1.0,
            states: vec!["visible".to_owned()],
            window_id: Some("gnome-shell".to_owned()),
            window_title: None,
            app_id: Some("gnome-shell".to_owned()),
            parent_id: None,
            child_ids: Vec::new(),
        },
    ];

    assert_eq!(
        gnome_dock_focus_candidate(&profile, &elements),
        Some(("Text Editor".to_owned(), Point::new(34, 589)))
    );
}

#[test]
fn accessibility_focus_candidates_prefer_window_then_focusable_children() {
    let window = peekaboox_core::WindowInfo {
        id: "window-1".to_owned(),
        title: "Text Editor".to_owned(),
        app_id: Some("gnome-text-editor".to_owned()),
        bounds: Rect::new(0, 0, 900, 700),
        focused: false,
        state: peekaboox_core::WindowState::Normal,
    };
    let elements = vec![
        UiElement {
            id: "window-1".to_owned(),
            role: "application".to_owned(),
            label: Some("Text Editor".to_owned()),
            bounds: Rect::new(0, 0, 900, 700),
            center: Some(Point::new(450, 350)),
            confidence: 1.0,
            states: vec!["focusable".to_owned()],
            window_id: Some("window-1".to_owned()),
            window_title: Some("Text Editor".to_owned()),
            app_id: Some("gnome-text-editor".to_owned()),
            parent_id: None,
            child_ids: Vec::new(),
        },
        UiElement {
            id: "document".to_owned(),
            role: "text box".to_owned(),
            label: Some("Document".to_owned()),
            bounds: Rect::new(20, 80, 860, 590),
            center: Some(Point::new(450, 375)),
            confidence: 1.0,
            states: vec!["focusable".to_owned(), "editable".to_owned()],
            window_id: Some("window-1".to_owned()),
            window_title: Some("Text Editor".to_owned()),
            app_id: Some("gnome-text-editor".to_owned()),
            parent_id: Some("window-1".to_owned()),
            child_ids: Vec::new(),
        },
        UiElement {
            id: "other-window-document".to_owned(),
            role: "text box".to_owned(),
            label: Some("Document".to_owned()),
            bounds: Rect::new(20, 80, 860, 590),
            center: Some(Point::new(450, 375)),
            confidence: 1.0,
            states: vec!["focusable".to_owned()],
            window_id: Some("window-2".to_owned()),
            window_title: Some("Other".to_owned()),
            app_id: Some("gnome-text-editor".to_owned()),
            parent_id: Some("window-2".to_owned()),
            child_ids: Vec::new(),
        },
    ];

    assert_eq!(
        accessibility_focus_candidate_ids(&window, &elements),
        vec!["window-1".to_owned(), "document".to_owned()]
    );
}

fn temp_profile_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "peekaboox-desktop-{name}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn text_editor_profile() -> AppProfile {
    builtin_profiles()
        .into_iter()
        .find(|profile| profile.id == TEXT_EDITOR_PROFILE_ID)
        .unwrap()
}

fn synthetic_telegram_frame(active_send: bool) -> CaptureFrame {
    let mut frame = blank_frame(1_920, 1_200, (10, 10, 10));
    let panel = (44, 52, 60);
    fill_rect(&mut frame, 647, 108, 1_023, 77, panel);
    fill_rect(&mut frame, 719, 862, 951, 1, panel);
    fill_rect(&mut frame, 719, 823, 951, 40, panel);
    fill_rect(&mut frame, 647, 185, 1_023, 638, (20, 24, 28));
    if active_send {
        fill_rect(&mut frame, 1610, 817, 48, 38, (40, 190, 190));
    }
    frame
}

fn blank_frame(width: u32, height: u32, color: (u8, u8, u8)) -> CaptureFrame {
    let mut data = vec![0; usize::try_from(width * height * 3).unwrap()];
    for y in 0..height {
        for x in 0..width {
            let index = usize::try_from((y * width + x) * 3).unwrap();
            data[index] = color.0;
            data[index + 1] = color.1;
            data[index + 2] = color.2;
        }
    }
    CaptureFrame {
        width,
        height,
        stride: width * 3,
        format: PixelFormat::Rgb8,
        data,
    }
}

fn fill_rect(
    frame: &mut CaptureFrame,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    color: (u8, u8, u8),
) {
    for py in y..(y + height) {
        for px in x..(x + width) {
            let index = usize::try_from((py * frame.width + px) * 3).unwrap();
            frame.data[index] = color.0;
            frame.data[index + 1] = color.1;
            frame.data[index + 2] = color.2;
        }
    }
}
