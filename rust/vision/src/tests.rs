use std::path::Path;

use super::{
    HeuristicVisionBackend, IncrementalCaptureOptions, OcrBackend, OcrOptions, TesseractOcrBackend,
    UiElementDetectionOptions, UiElementSort, UiStateKind, UiStateOptions, VisionBackend,
    VisualAlphaMode, VisualCompareOptions, VisualSizePolicy, compare_frames, compare_image_files,
    detect_ui_elements, detect_ui_elements_from_image_file,
    detect_ui_elements_from_image_file_with_outputs, detect_ui_state,
    detect_ui_state_from_image_files, incremental_capture_delta, load_image_file, rect_union,
    rects_intersect, tesseract_args, tesseract_result_from_tsv, write_visual_diff_image_file,
};
use peekaboox_core::{CaptureFrame, PixelFormat, Rect};

const SAMPLE_TSV: &str = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
5\t1\t1\t1\t1\t1\t10\t20\t40\t12\t96.5\tHello\n\
5\t1\t1\t1\t1\t2\t55\t20\t40\t12\t93.5\tWorld\n\
5\t1\t1\t1\t2\t1\t10\t45\t55\t14\t88.0\tSubmit\n";

#[test]
fn tesseract_args_include_language_psm_and_tsv() {
    let args = tesseract_args(
        Path::new("/tmp/screen.png"),
        &OcrOptions {
            language: Some("eng".to_owned()),
            page_segmentation_mode: Some(11),
            ..OcrOptions::default()
        },
    );

    assert_eq!(
        args,
        vec![
            "/tmp/screen.png",
            "stdout",
            "-l",
            "eng",
            "--psm",
            "11",
            "tsv"
        ]
    );
}

#[test]
fn tesseract_tsv_groups_words_into_lines() {
    let result = tesseract_result_from_tsv(SAMPLE_TSV, None).unwrap();

    assert_eq!(result.text, "Hello World\nSubmit");
    assert_eq!(result.blocks.len(), 2);
    assert_eq!(result.blocks[0].element.bounds, Rect::new(10, 20, 85, 12));
    assert!((result.blocks[0].element.confidence - 0.95).abs() < f32::EPSILON);
}

#[test]
fn tesseract_tsv_filters_by_region() {
    let result = tesseract_result_from_tsv(SAMPLE_TSV, Some(Rect::new(0, 40, 100, 30))).unwrap();

    assert_eq!(result.text, "Submit");
    assert_eq!(result.blocks.len(), 1);
}

#[test]
fn rect_intersection_and_union_handle_bounds() {
    assert!(rects_intersect(
        Rect::new(10, 10, 20, 20),
        Rect::new(25, 25, 10, 10)
    ));
    assert!(!rects_intersect(
        Rect::new(10, 10, 10, 10),
        Rect::new(20, 20, 10, 10)
    ));
    assert_eq!(
        rect_union(Rect::new(10, 10, 20, 20), Rect::new(25, 5, 20, 10)),
        Rect::new(10, 5, 35, 25)
    );
}

#[test]
fn visual_compare_matches_identical_frames() {
    let frame = rgb_frame(2, 2, &[[0, 0, 0], [10, 10, 10], [20, 20, 20], [30, 30, 30]]);

    let diff = compare_frames(&frame, &frame, &VisualCompareOptions::default()).unwrap();

    assert!(diff.matches);
    assert_eq!(diff.compared_pixels, 4);
    assert_eq!(diff.changed_pixels, 0);
    assert_eq!(diff.changed_bounds, None);
    assert_eq!(diff.max_channel_delta, 0);
}

#[test]
fn visual_compare_detects_changed_region() {
    let expected = rgb_frame(
        3,
        2,
        &[
            [255, 255, 255],
            [255, 255, 255],
            [255, 255, 255],
            [255, 255, 255],
            [255, 255, 255],
            [255, 255, 255],
        ],
    );
    let actual = rgb_frame(
        3,
        2,
        &[
            [255, 255, 255],
            [255, 255, 255],
            [255, 255, 255],
            [255, 255, 255],
            [255, 0, 0],
            [255, 255, 255],
        ],
    );

    let diff = compare_frames(&expected, &actual, &VisualCompareOptions::default()).unwrap();

    assert!(!diff.matches);
    assert_eq!(diff.changed_pixels, 1);
    assert_eq!(diff.changed_bounds, Some(Rect::new(1, 1, 1, 1)));
    assert_eq!(diff.max_channel_delta, 255);
}

#[test]
fn visual_compare_threshold_tolerates_small_channel_changes() {
    let expected = rgb_frame(1, 1, &[[100, 100, 100]]);
    let actual = rgb_frame(1, 1, &[[103, 100, 100]]);
    let options = VisualCompareOptions {
        per_channel_threshold: 3,
        ..VisualCompareOptions::default()
    };

    let diff = compare_frames(&expected, &actual, &options).unwrap();

    assert!(diff.matches);
    assert_eq!(diff.changed_pixels, 0);
    assert_eq!(diff.max_channel_delta, 3);
}

#[test]
fn visual_compare_can_limit_region() {
    let expected = rgb_frame(2, 2, &[[0, 0, 0], [0, 0, 0], [0, 0, 0], [0, 0, 0]]);
    let actual = rgb_frame(2, 2, &[[0, 0, 0], [255, 0, 0], [0, 0, 0], [0, 0, 0]]);
    let options = VisualCompareOptions {
        region: Some(Rect::new(0, 0, 1, 2)),
        ..VisualCompareOptions::default()
    };

    let diff = compare_frames(&expected, &actual, &options).unwrap();

    assert!(diff.matches);
    assert_eq!(diff.compared_region, Rect::new(0, 0, 1, 2));
    assert_eq!(diff.compared_pixels, 2);
}

#[test]
fn visual_compare_rejects_mismatched_dimensions() {
    let expected = rgb_frame(1, 1, &[[0, 0, 0]]);
    let actual = rgb_frame(2, 1, &[[0, 0, 0], [0, 0, 0]]);

    let error = compare_frames(&expected, &actual, &VisualCompareOptions::default()).unwrap_err();

    assert!(error.message().contains("matching frame dimensions"));
}

#[test]
fn visual_compare_uses_fixture_images() {
    let baseline = load_fixture_ppm("baseline.ppm");
    let changed = load_fixture_ppm("changed.ppm");

    let diff = compare_frames(&baseline, &changed, &VisualCompareOptions::default()).unwrap();

    assert_eq!(diff.compared_pixels, 12);
    assert_eq!(diff.changed_pixels, 2);
    assert_eq!(diff.changed_bounds, Some(Rect::new(1, 1, 2, 1)));
}

#[test]
fn visual_compare_loads_fixture_files_through_image_decoder() {
    let baseline = fixture_path("baseline.ppm");
    let changed = fixture_path("changed.ppm");

    let decoded = load_image_file(&baseline).unwrap();
    assert_eq!(decoded.format, PixelFormat::Rgba8);
    assert_eq!(decoded.width, 4);
    assert_eq!(decoded.height, 3);

    let diff = compare_image_files(&baseline, &changed, &VisualCompareOptions::default()).unwrap();
    assert_eq!(diff.changed_pixels, 2);
}

#[test]
fn visual_compare_ignores_repeated_regions() {
    let baseline = load_fixture_ppm("baseline.ppm");
    let changed = load_fixture_ppm("changed.ppm");
    let options = VisualCompareOptions {
        ignore_regions: vec![Rect::new(1, 1, 1, 1), Rect::new(2, 1, 1, 1)],
        ..VisualCompareOptions::default()
    };

    let diff = compare_frames(&baseline, &changed, &options).unwrap();

    assert!(diff.matches);
    assert_eq!(diff.compared_pixels, 10);
    assert_eq!(diff.changed_pixels, 0);
    assert_eq!(diff.changed_bounds, None);
}

#[test]
fn visual_compare_applies_absolute_and_metric_gates() {
    let expected = rgb_frame(1, 1, &[[0, 0, 0]]);
    let actual = rgb_frame(1, 1, &[[9, 0, 0]]);
    let options = VisualCompareOptions {
        max_changed_ratio: 1.0,
        max_changed_pixels: Some(0),
        max_mean_absolute_error: Some(1.0),
        max_channel_delta: Some(8),
        ..VisualCompareOptions::default()
    };

    let diff = compare_frames(&expected, &actual, &options).unwrap();

    assert!(!diff.matches);
    assert_eq!(diff.changed_pixels, 1);
    assert_eq!(diff.mean_absolute_error, 3.0);
    assert_eq!(diff.max_channel_delta, 9);
}

#[test]
fn visual_compare_can_compare_or_ignore_alpha() {
    let expected = rgba_frame(1, 1, &[[10, 20, 30, 255]]);
    let actual = rgba_frame(1, 1, &[[10, 20, 30, 0]]);

    let ignored = compare_frames(&expected, &actual, &VisualCompareOptions::default()).unwrap();
    let compared = compare_frames(
        &expected,
        &actual,
        &VisualCompareOptions {
            alpha_mode: VisualAlphaMode::Compare,
            ..VisualCompareOptions::default()
        },
    )
    .unwrap();

    assert!(ignored.matches);
    assert_eq!(ignored.changed_pixels, 0);
    assert!(!compared.matches);
    assert_eq!(compared.changed_pixels, 1);
    assert_eq!(compared.max_channel_delta, 255);
}

#[test]
fn visual_compare_supports_common_region_size_policy() {
    let expected = rgb_frame(2, 1, &[[0, 0, 0], [255, 0, 0]]);
    let actual = rgb_frame(1, 1, &[[0, 0, 0]]);
    let options = VisualCompareOptions {
        size_policy: VisualSizePolicy::CommonRegion,
        ..VisualCompareOptions::default()
    };

    let diff = compare_frames(&expected, &actual, &options).unwrap();

    assert!(diff.matches);
    assert_eq!(diff.compared_region, Rect::new(0, 0, 1, 1));
    assert_eq!(diff.compared_pixels, 1);
}

#[test]
fn visual_compare_supports_resize_actual_size_policy() {
    let expected = solid_rgb_frame(2, 2, [0, 0, 0]);
    let actual = rgb_frame(1, 1, &[[0, 0, 0]]);
    let options = VisualCompareOptions {
        size_policy: VisualSizePolicy::ResizeActual,
        ..VisualCompareOptions::default()
    };

    let diff = compare_frames(&expected, &actual, &options).unwrap();

    assert!(diff.matches);
    assert_eq!(diff.compared_pixels, 4);
}

#[test]
fn visual_compare_writes_diff_image() {
    let baseline = fixture_path("baseline.ppm");
    let changed = fixture_path("changed.ppm");
    let output = std::env::temp_dir().join(format!(
        "peekaboox-vision-diff-{}-{}.png",
        std::process::id(),
        super::monotonic_ms()
    ));

    let diff = write_visual_diff_image_file(
        &baseline,
        &changed,
        &output,
        &VisualCompareOptions::default(),
    )
    .unwrap();

    assert_eq!(diff.changed_pixels, 2);
    assert!(output.is_file());
    let decoded = load_image_file(&output).unwrap();
    assert_eq!(decoded.width, 4);
    assert_eq!(decoded.height, 3);
    let _ = std::fs::remove_file(output);
}

#[test]
fn incremental_capture_delta_emits_initial_full_frame() {
    let frame = rgb_frame(2, 2, &[[1, 2, 3], [4, 5, 6], [7, 8, 9], [10, 11, 12]]);

    let delta =
        incremental_capture_delta(None, &frame, 7, &IncrementalCaptureOptions::default()).unwrap();

    assert_eq!(delta.sequence, 7);
    assert_eq!(delta.frame_width, 2);
    assert_eq!(delta.frame_height, 2);
    assert_eq!(delta.format, PixelFormat::Rgb8);
    assert!(delta.full_frame);
    assert!(delta.is_changed());
    assert_eq!(delta.changed_bounds, Some(Rect::new(0, 0, 2, 2)));
    assert_eq!(delta.changed_pixels, 4);
    assert_eq!(delta.changed_ratio, 1.0);
    assert_eq!(delta.patch_stride, 6);
    assert_eq!(delta.patch_data, frame.data);
    assert_eq!(delta.patch_frame(), Some(frame));
}

#[test]
fn incremental_capture_delta_emits_changed_patch_only() {
    let previous = solid_rgb_frame(4, 3, [0, 0, 0]);
    let mut current = previous.clone();
    fill_rect(&mut current, Rect::new(1, 1, 2, 1), [10, 20, 30]);

    let delta = incremental_capture_delta(
        Some(&previous),
        &current,
        8,
        &IncrementalCaptureOptions::default(),
    )
    .unwrap();

    assert_eq!(delta.sequence, 8);
    assert_eq!(delta.frame_width, 4);
    assert_eq!(delta.frame_height, 3);
    assert!(!delta.full_frame);
    assert!(delta.is_changed());
    assert_eq!(delta.changed_bounds, Some(Rect::new(1, 1, 2, 1)));
    assert_eq!(delta.changed_pixels, 2);
    assert_eq!(delta.changed_ratio, 2.0_f32 / 12.0);
    assert_eq!(delta.patch_stride, 6);
    assert_eq!(delta.patch_data, vec![10, 20, 30, 10, 20, 30]);
    assert_eq!(
        delta.patch_frame(),
        Some(CaptureFrame {
            width: 2,
            height: 1,
            stride: 6,
            format: PixelFormat::Rgb8,
            data: vec![10, 20, 30, 10, 20, 30],
        })
    );
}

#[test]
fn incremental_capture_delta_skips_unchanged_patch() {
    let frame = solid_rgb_frame(2, 2, [12, 34, 56]);

    let delta = incremental_capture_delta(
        Some(&frame),
        &frame,
        9,
        &IncrementalCaptureOptions::default(),
    )
    .unwrap();

    assert_eq!(delta.sequence, 9);
    assert!(!delta.full_frame);
    assert!(!delta.is_changed());
    assert_eq!(delta.changed_bounds, None);
    assert_eq!(delta.changed_pixels, 0);
    assert_eq!(delta.changed_ratio, 0.0);
    assert_eq!(delta.patch_stride, 0);
    assert!(delta.patch_data.is_empty());
    assert_eq!(delta.patch_frame(), None);
}

#[test]
fn incremental_capture_delta_resets_on_pixel_format_change() {
    let previous = solid_rgb_frame(1, 1, [0, 0, 0]);
    let current = CaptureFrame {
        width: 1,
        height: 1,
        stride: 4,
        format: PixelFormat::Rgba8,
        data: vec![0, 0, 0, 255],
    };

    let delta = incremental_capture_delta(
        Some(&previous),
        &current,
        10,
        &IncrementalCaptureOptions::default(),
    )
    .unwrap();

    assert!(delta.full_frame);
    assert_eq!(delta.changed_bounds, Some(Rect::new(0, 0, 1, 1)));
    assert_eq!(delta.patch_data, current.data);
}

#[test]
fn incremental_capture_delta_resets_on_dimension_change() {
    let previous = solid_rgb_frame(1, 1, [0, 0, 0]);
    let current = solid_rgb_frame(2, 1, [0, 0, 0]);
    let options = IncrementalCaptureOptions {
        compare: VisualCompareOptions {
            size_policy: VisualSizePolicy::CommonRegion,
            ..VisualCompareOptions::default()
        },
    };

    let delta = incremental_capture_delta(Some(&previous), &current, 11, &options).unwrap();

    assert!(delta.full_frame);
    assert_eq!(delta.changed_bounds, Some(Rect::new(0, 0, 2, 1)));
    assert_eq!(delta.patch_data, current.data);
}

#[test]
fn incremental_capture_delta_respects_compare_region() {
    let previous = solid_rgb_frame(3, 2, [0, 0, 0]);
    let mut current = previous.clone();
    fill_rect(&mut current, Rect::new(2, 1, 1, 1), [255, 0, 0]);
    let options = IncrementalCaptureOptions {
        compare: VisualCompareOptions {
            region: Some(Rect::new(0, 0, 1, 2)),
            ..VisualCompareOptions::default()
        },
    };

    let delta = incremental_capture_delta(Some(&previous), &current, 10, &options).unwrap();

    assert!(!delta.is_changed());
    assert_eq!(delta.changed_bounds, None);
    assert!(delta.patch_data.is_empty());
}

#[test]
fn ui_state_reports_stable_for_identical_frames() {
    let frame = rgb_frame(
        2,
        2,
        &[[10, 10, 10], [20, 20, 20], [30, 30, 30], [40, 40, 40]],
    );

    let result = detect_ui_state(&[frame.clone(), frame], &UiStateOptions::default()).unwrap();

    assert_eq!(result.state, UiStateKind::Stable);
    assert!(result.is_stable());
    assert!(!result.is_loading());
    assert_eq!(result.compared_transitions, 1);
    assert_eq!(result.stable_transitions, 1);
    assert_eq!(result.loading_transitions, 0);
    assert_eq!(result.trailing_stable_transitions, 1);
    assert_eq!(result.latest_diff.changed_pixels, 0);
    assert_eq!(result.changed_bounds, None);
}

#[test]
fn ui_state_reports_loading_for_large_unsettled_change() {
    let baseline = load_fixture_ppm("baseline.ppm");
    let changed = load_fixture_ppm("changed.ppm");

    let result = detect_ui_state(&[baseline, changed], &UiStateOptions::default()).unwrap();

    assert_eq!(result.state, UiStateKind::Loading);
    assert!(!result.is_stable());
    assert!(result.is_loading());
    assert_eq!(result.loading_transitions, 1);
    assert_eq!(result.trailing_stable_transitions, 0);
    assert_eq!(result.changed_bounds, Some(Rect::new(1, 1, 2, 1)));
}

#[test]
fn ui_state_reports_changing_for_small_non_stable_drift() {
    let before = rgb_frame(10, 10, &[[0, 0, 0]; 100]);
    let mut pixels = [[0, 0, 0]; 100];
    pixels[55] = [8, 0, 0];
    let after = rgb_frame(10, 10, &pixels);
    let options = UiStateOptions {
        stable_max_changed_ratio: 0.0,
        loading_min_changed_ratio: 0.02,
        ..UiStateOptions::default()
    };

    let result = detect_ui_state(&[before, after], &options).unwrap();

    assert_eq!(result.state, UiStateKind::Changing);
    assert_eq!(result.stable_transitions, 0);
    assert_eq!(result.loading_transitions, 0);
    assert_eq!(result.latest_diff.changed_pixels, 1);
    assert!((result.max_changed_ratio - 0.01).abs() < f32::EPSILON);
}

#[test]
fn ui_state_uses_trailing_stability_to_mark_settled_screen() {
    let baseline = load_fixture_ppm("baseline.ppm");
    let changed = load_fixture_ppm("changed.ppm");

    let result = detect_ui_state(
        &[baseline, changed.clone(), changed],
        &UiStateOptions::default(),
    )
    .unwrap();

    assert_eq!(result.state, UiStateKind::Stable);
    assert_eq!(result.compared_transitions, 2);
    assert_eq!(result.stable_transitions, 1);
    assert_eq!(result.loading_transitions, 1);
    assert_eq!(result.trailing_stable_transitions, 1);
    assert_eq!(result.latest_diff.changed_pixels, 0);
}

#[test]
fn ui_state_loads_fixture_files_through_image_decoder() {
    let baseline = fixture_path("baseline.ppm");
    let changed = fixture_path("changed.ppm");
    let result = detect_ui_state_from_image_files(
        &[baseline, changed],
        &UiStateOptions {
            loading_min_changed_ratio: 0.1,
            ..UiStateOptions::default()
        },
    )
    .unwrap();

    assert_eq!(result.state, UiStateKind::Loading);
    assert_eq!(result.loading_transitions, 1);
}

#[test]
fn ui_state_uses_screen_fixture_sequence() {
    let stable = fixture_path("ui_controls.pbm");
    let loading = fixture_path("ui_controls_loading.pbm");
    let result = detect_ui_state_from_image_files(
        &[stable.clone(), loading, stable],
        &UiStateOptions::default(),
    )
    .unwrap();

    assert_eq!(result.state, UiStateKind::Loading);
    assert_eq!(result.loading_transitions, 2);
    assert_eq!(result.latest_diff.changed_pixels, 40);
    assert_eq!(result.changed_bounds, Some(Rect::new(4, 15, 20, 2)));
}

#[test]
fn ui_state_can_ignore_volatile_regions() {
    let stable = load_image_file(fixture_path("ui_controls.pbm")).unwrap();
    let loading = load_image_file(fixture_path("ui_controls_loading.pbm")).unwrap();
    let options = UiStateOptions {
        ignore_regions: vec![Rect::new(4, 15, 20, 2)],
        ..UiStateOptions::default()
    };

    let result = detect_ui_state(&[stable, loading], &options).unwrap();

    assert_eq!(result.state, UiStateKind::Stable);
    assert_eq!(result.stable_transitions, 1);
    assert_eq!(result.latest_diff.changed_pixels, 0);
}

#[test]
fn ui_state_uses_absolute_stable_and_loading_pixel_gates() {
    let baseline = load_fixture_ppm("baseline.ppm");
    let changed = load_fixture_ppm("changed.ppm");
    let options = UiStateOptions {
        stable_max_changed_ratio: 1.0,
        stable_max_changed_pixels: Some(1),
        loading_min_changed_ratio: 1.0,
        loading_min_changed_pixels: Some(2),
        ..UiStateOptions::default()
    };

    let result = detect_ui_state(&[baseline, changed], &options).unwrap();

    assert_eq!(result.state, UiStateKind::Loading);
    assert_eq!(result.stable_transitions, 0);
    assert_eq!(result.loading_transitions, 1);
}

#[test]
fn ui_state_supports_common_region_size_policy() {
    let expected = solid_rgb_frame(2, 2, [0, 0, 0]);
    let actual = rgb_frame(1, 1, &[[0, 0, 0]]);
    let options = UiStateOptions {
        size_policy: VisualSizePolicy::CommonRegion,
        ..UiStateOptions::default()
    };

    let result = detect_ui_state(&[expected, actual], &options).unwrap();

    assert_eq!(result.state, UiStateKind::Stable);
    assert_eq!(result.latest_diff.compared_pixels, 1);
}

#[test]
fn ui_state_rejects_single_frame_and_invalid_options() {
    let frame = rgb_frame(1, 1, &[[0, 0, 0]]);
    let error =
        detect_ui_state(std::slice::from_ref(&frame), &UiStateOptions::default()).unwrap_err();
    assert!(error.message().contains("at least two frames"));

    let error = detect_ui_state(
        &[frame.clone(), frame.clone()],
        &UiStateOptions {
            stable_max_changed_ratio: 0.5,
            loading_min_changed_ratio: 0.1,
            ..UiStateOptions::default()
        },
    )
    .unwrap_err();
    assert!(error.message().contains("less than or equal"));

    let error = detect_ui_state(
        &[frame.clone(), frame],
        &UiStateOptions {
            stable_max_changed_pixels: Some(10),
            loading_min_changed_pixels: Some(5),
            ..UiStateOptions::default()
        },
    )
    .unwrap_err();
    assert!(error.message().contains("stable_max_changed_pixels"));
}

#[test]
fn ui_element_detection_finds_rectangular_visual_regions() {
    let mut frame = solid_rgb_frame(48, 30, [255, 255, 255]);
    fill_rect(&mut frame, Rect::new(4, 5, 14, 8), [210, 210, 210]);
    fill_rect(&mut frame, Rect::new(29, 16, 10, 7), [20, 120, 220]);
    let options = UiElementDetectionOptions {
        min_width: 6,
        min_height: 5,
        min_component_pixels: 20,
        ..UiElementDetectionOptions::default()
    };

    let elements = detect_ui_elements(&frame, &options).unwrap();

    assert_eq!(elements.len(), 2);
    assert_eq!(elements[0].role, "visual-region");
    assert_eq!(elements[0].bounds, Rect::new(4, 5, 14, 8));
    assert_eq!(elements[0].states, vec!["visible".to_owned()]);
    assert!(elements[0].confidence > 0.5);
    assert_eq!(elements[1].bounds, Rect::new(29, 16, 10, 7));
}

#[test]
fn ui_element_detection_loads_screen_fixture_through_decoder() {
    let elements = detect_ui_elements_from_image_file(
        fixture_path("ui_controls.pbm"),
        &UiElementDetectionOptions::default(),
    )
    .unwrap();

    assert_eq!(elements.len(), 2);
    assert_eq!(elements[0].role, "visual-region");
    assert_eq!(elements[0].bounds, Rect::new(4, 4, 12, 8));
    assert_eq!(elements[0].states, vec!["visible".to_owned()]);
    assert!(elements[0].confidence > 0.85);
    assert_eq!(elements[1].bounds, Rect::new(21, 4, 8, 8));
}

#[test]
fn ui_element_detection_respects_region_and_size_filters() {
    let mut frame = solid_rgb_frame(48, 30, [255, 255, 255]);
    fill_rect(&mut frame, Rect::new(4, 5, 14, 8), [210, 210, 210]);
    fill_rect(&mut frame, Rect::new(29, 16, 10, 7), [20, 120, 220]);
    fill_rect(&mut frame, Rect::new(42, 2, 3, 3), [0, 0, 0]);
    let options = UiElementDetectionOptions {
        region: Some(Rect::new(24, 10, 20, 18)),
        min_width: 6,
        min_height: 5,
        min_component_pixels: 20,
        ..UiElementDetectionOptions::default()
    };

    let elements = detect_ui_elements(&frame, &options).unwrap();

    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].bounds, Rect::new(29, 16, 10, 7));
}

#[test]
fn ui_element_detection_respects_ignore_confidence_and_bounds_filters() {
    let mut frame = solid_rgb_frame(48, 30, [255, 255, 255]);
    fill_rect(&mut frame, Rect::new(4, 5, 14, 8), [210, 210, 210]);
    fill_rect(&mut frame, Rect::new(29, 16, 10, 7), [20, 120, 220]);
    let options = UiElementDetectionOptions {
        ignore_regions: vec![Rect::new(4, 5, 14, 8)],
        min_width: 6,
        max_width: Some(12),
        min_height: 5,
        max_height: Some(8),
        min_component_pixels: 20,
        min_confidence: Some(0.85),
        min_area: Some(60),
        max_area: Some(100),
        ..UiElementDetectionOptions::default()
    };

    let elements = detect_ui_elements(&frame, &options).unwrap();

    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].bounds, Rect::new(29, 16, 10, 7));
}

#[test]
fn ui_element_detection_sorts_by_area_and_applies_padding() {
    let mut frame = solid_rgb_frame(40, 24, [255, 255, 255]);
    fill_rect(&mut frame, Rect::new(4, 4, 4, 4), [20, 20, 20]);
    fill_rect(&mut frame, Rect::new(20, 10, 8, 6), [20, 120, 220]);
    let options = UiElementDetectionOptions {
        min_width: 3,
        min_height: 3,
        min_component_pixels: 8,
        max_elements: 1,
        padding: 2,
        sort: UiElementSort::Area,
        ..UiElementDetectionOptions::default()
    };

    let elements = detect_ui_elements(&frame, &options).unwrap();

    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].bounds, Rect::new(18, 8, 12, 10));
}

#[test]
fn ui_element_detection_writes_mask_and_overlay_outputs() {
    let input = fixture_path("ui_controls.pbm");
    let mask = std::env::temp_dir().join(format!(
        "peekaboox-ui-mask-{}-{}.png",
        std::process::id(),
        super::monotonic_ms()
    ));
    let overlay = std::env::temp_dir().join(format!(
        "peekaboox-ui-overlay-{}-{}.png",
        std::process::id(),
        super::monotonic_ms()
    ));

    let elements = detect_ui_elements_from_image_file_with_outputs(
        &input,
        &UiElementDetectionOptions::default(),
        Some(mask.as_path()),
        Some(overlay.as_path()),
    )
    .unwrap();

    assert_eq!(elements.len(), 2);
    assert!(mask.is_file());
    assert!(overlay.is_file());
    let mask_frame = load_image_file(&mask).unwrap();
    let overlay_frame = load_image_file(&overlay).unwrap();
    assert_eq!((mask_frame.width, mask_frame.height), (32, 20));
    assert_eq!((overlay_frame.width, overlay_frame.height), (32, 20));
    let _ = std::fs::remove_file(mask);
    let _ = std::fs::remove_file(overlay);
}

#[test]
fn ui_element_detection_returns_no_elements_for_uniform_frame() {
    let frame = solid_rgb_frame(20, 12, [255, 255, 255]);

    let elements = detect_ui_elements(&frame, &UiElementDetectionOptions::default()).unwrap();

    assert!(elements.is_empty());
}

#[test]
fn heuristic_vision_backend_delegates_ui_detection() {
    let mut frame = solid_rgb_frame(24, 16, [255, 255, 255]);
    fill_rect(&mut frame, Rect::new(5, 4, 12, 8), [80, 80, 80]);
    let backend = HeuristicVisionBackend::new(UiElementDetectionOptions {
        min_width: 6,
        min_height: 5,
        min_component_pixels: 20,
        ..UiElementDetectionOptions::default()
    });

    let elements = backend.detect_ui_elements(&frame).unwrap();

    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].bounds, Rect::new(5, 4, 12, 8));
}

#[test]
fn ui_element_detection_rejects_invalid_options() {
    let frame = solid_rgb_frame(20, 12, [255, 255, 255]);
    let error = detect_ui_elements(
        &frame,
        &UiElementDetectionOptions {
            edge_threshold: 0,
            ..UiElementDetectionOptions::default()
        },
    )
    .unwrap_err();

    assert!(error.message().contains("edge_threshold"));
}

#[test]
fn unavailable_tesseract_backend_returns_typed_error() {
    let backend = TesseractOcrBackend::new(
        "peekaboox-missing-tesseract",
        OcrOptions {
            language: None,
            page_segmentation_mode: None,
            ..OcrOptions::default()
        },
    );
    let error = backend
        .recognize_image(Path::new("/tmp/nonexistent.png"), None)
        .unwrap_err();

    assert!(error.message().contains("not available"));
}

fn rgb_frame(width: u32, height: u32, pixels: &[[u8; 3]]) -> CaptureFrame {
    assert_eq!(pixels.len(), (width * height) as usize);
    CaptureFrame {
        width,
        height,
        stride: width * 3,
        format: PixelFormat::Rgb8,
        data: pixels.iter().flatten().copied().collect(),
    }
}

fn rgba_frame(width: u32, height: u32, pixels: &[[u8; 4]]) -> CaptureFrame {
    assert_eq!(pixels.len(), (width * height) as usize);
    CaptureFrame {
        width,
        height,
        stride: width * 4,
        format: PixelFormat::Rgba8,
        data: pixels.iter().flatten().copied().collect(),
    }
}

fn solid_rgb_frame(width: u32, height: u32, color: [u8; 3]) -> CaptureFrame {
    let mut data = Vec::with_capacity((width * height * 3) as usize);
    for _ in 0..(width * height) {
        data.extend_from_slice(&color);
    }

    CaptureFrame {
        width,
        height,
        stride: width * 3,
        format: PixelFormat::Rgb8,
        data,
    }
}

fn fill_rect(frame: &mut CaptureFrame, rect: Rect, color: [u8; 3]) {
    assert_eq!(frame.format, PixelFormat::Rgb8);
    for y in rect.y..rect.y + rect.height as i32 {
        for x in rect.x..rect.x + rect.width as i32 {
            let offset =
                (u32::try_from(y).unwrap() * frame.stride + u32::try_from(x).unwrap() * 3) as usize;
            frame.data[offset..offset + 3].copy_from_slice(&color);
        }
    }
}

fn load_fixture_ppm(name: &str) -> CaptureFrame {
    let path = fixture_path(name);
    let contents = std::fs::read_to_string(path).unwrap();
    let mut values = contents
        .lines()
        .flat_map(|line| {
            line.split('#')
                .next()
                .unwrap_or_default()
                .split_whitespace()
        })
        .collect::<Vec<_>>()
        .into_iter();
    assert_eq!(values.next(), Some("P3"));
    let width = values.next().unwrap().parse::<u32>().unwrap();
    let height = values.next().unwrap().parse::<u32>().unwrap();
    assert_eq!(values.next(), Some("255"));
    let bytes = values
        .map(|value| value.parse::<u8>().unwrap())
        .collect::<Vec<_>>();

    CaptureFrame {
        width,
        height,
        stride: width * 3,
        format: PixelFormat::Rgb8,
        data: bytes,
    }
}

fn fixture_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/vision")
        .join(name)
}
