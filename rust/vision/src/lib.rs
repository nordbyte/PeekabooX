use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;

use image::{DynamicImage, ImageReader, Rgba, RgbaImage, imageops};
use peekaboox_core::{CaptureFrame, PeekabooXError, PixelFormat, Rect, Result, UiElement};

#[derive(Debug, Clone, PartialEq)]
pub struct OcrText {
    pub text: String,
    pub element: UiElement,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OcrResult {
    pub backend_name: String,
    pub text: String,
    pub blocks: Vec<OcrText>,
    pub words: Vec<OcrText>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisualCompareOptions {
    pub region: Option<Rect>,
    pub ignore_regions: Vec<Rect>,
    pub per_channel_threshold: u8,
    pub max_changed_ratio: f32,
    pub max_changed_pixels: Option<u64>,
    pub max_mean_absolute_error: Option<f32>,
    pub max_channel_delta: Option<u8>,
    pub size_policy: VisualSizePolicy,
    pub alpha_mode: VisualAlphaMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualSizePolicy {
    Error,
    CommonRegion,
    ResizeActual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualAlphaMode {
    Ignore,
    Compare,
}

impl Default for VisualCompareOptions {
    fn default() -> Self {
        Self {
            region: None,
            ignore_regions: Vec::new(),
            per_channel_threshold: 0,
            max_changed_ratio: 0.0,
            max_changed_pixels: None,
            max_mean_absolute_error: None,
            max_channel_delta: None,
            size_policy: VisualSizePolicy::Error,
            alpha_mode: VisualAlphaMode::Ignore,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisualDiffResult {
    pub compared_region: Rect,
    pub compared_pixels: u64,
    pub changed_pixels: u64,
    pub changed_ratio: f32,
    pub mean_absolute_error: f32,
    pub max_channel_delta: u8,
    pub changed_bounds: Option<Rect>,
    pub matches: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct IncrementalCaptureOptions {
    pub compare: VisualCompareOptions,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IncrementalCaptureDelta {
    pub sequence: u64,
    pub frame_width: u32,
    pub frame_height: u32,
    pub format: PixelFormat,
    pub full_frame: bool,
    pub changed_bounds: Option<Rect>,
    pub changed_pixels: u64,
    pub changed_ratio: f32,
    pub patch_stride: u32,
    pub patch_data: Vec<u8>,
}

impl IncrementalCaptureDelta {
    pub const fn is_changed(&self) -> bool {
        self.changed_bounds.is_some()
    }

    pub fn patch_frame(&self) -> Option<CaptureFrame> {
        let bounds = self.changed_bounds?;
        Some(CaptureFrame {
            width: bounds.width,
            height: bounds.height,
            stride: self.patch_stride,
            format: self.format,
            data: self.patch_data.clone(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiStateKind {
    Stable,
    Loading,
    Changing,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiStateOptions {
    pub region: Option<Rect>,
    pub ignore_regions: Vec<Rect>,
    pub per_channel_threshold: u8,
    pub stable_max_changed_ratio: f32,
    pub stable_max_changed_pixels: Option<u64>,
    pub stable_max_mean_absolute_error: Option<f32>,
    pub stable_max_channel_delta: Option<u8>,
    pub loading_min_changed_ratio: f32,
    pub loading_min_changed_pixels: Option<u64>,
    pub required_stable_transitions: usize,
    pub size_policy: VisualSizePolicy,
    pub alpha_mode: VisualAlphaMode,
}

impl Default for UiStateOptions {
    fn default() -> Self {
        Self {
            region: None,
            ignore_regions: Vec::new(),
            per_channel_threshold: 2,
            stable_max_changed_ratio: 0.001,
            stable_max_changed_pixels: None,
            stable_max_mean_absolute_error: None,
            stable_max_channel_delta: None,
            loading_min_changed_ratio: 0.02,
            loading_min_changed_pixels: None,
            required_stable_transitions: 1,
            size_policy: VisualSizePolicy::Error,
            alpha_mode: VisualAlphaMode::Ignore,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiStateResult {
    pub state: UiStateKind,
    pub compared_transitions: usize,
    pub stable_transitions: usize,
    pub loading_transitions: usize,
    pub trailing_stable_transitions: usize,
    pub latest_diff: VisualDiffResult,
    pub max_changed_ratio: f32,
    pub mean_changed_ratio: f32,
    pub changed_bounds: Option<Rect>,
}

impl UiStateResult {
    pub const fn is_stable(&self) -> bool {
        matches!(self.state, UiStateKind::Stable)
    }

    pub const fn is_loading(&self) -> bool {
        matches!(self.state, UiStateKind::Loading)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiElementDetectionOptions {
    pub region: Option<Rect>,
    pub ignore_regions: Vec<Rect>,
    pub edge_threshold: u8,
    pub min_width: u32,
    pub min_height: u32,
    pub min_component_pixels: u32,
    pub min_confidence: Option<f32>,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub min_area: Option<u64>,
    pub max_area: Option<u64>,
    pub max_elements: usize,
    pub merge_distance: u32,
    pub padding: u32,
    pub sort: UiElementSort,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiElementSort {
    Position,
    Area,
    Confidence,
}

impl Default for UiElementDetectionOptions {
    fn default() -> Self {
        Self {
            region: None,
            ignore_regions: Vec::new(),
            edge_threshold: 24,
            min_width: 8,
            min_height: 8,
            min_component_pixels: 12,
            min_confidence: None,
            max_width: None,
            max_height: None,
            min_area: None,
            max_area: None,
            max_elements: 100,
            merge_distance: 2,
            padding: 0,
            sort: UiElementSort::Position,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcrConfig {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct OcrPreprocessingOptions {
    pub scale: Option<f32>,
    pub grayscale: bool,
    pub threshold: Option<u8>,
    pub invert: bool,
    pub contrast: Option<f32>,
    pub deskew: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OcrOptions {
    pub language: Option<String>,
    pub page_segmentation_mode: Option<u8>,
    pub engine_mode: Option<u8>,
    pub dpi: Option<u32>,
    pub min_confidence: f32,
    pub whitelist: Option<String>,
    pub config: Vec<OcrConfig>,
    pub preprocessing: OcrPreprocessingOptions,
}

impl Default for OcrOptions {
    fn default() -> Self {
        Self {
            language: std::env::var("PEEKABOOX_OCR_LANGUAGE").ok(),
            page_segmentation_mode: Some(6),
            engine_mode: None,
            dpi: None,
            min_confidence: 0.0,
            whitelist: None,
            config: Vec::new(),
            preprocessing: OcrPreprocessingOptions::default(),
        }
    }
}

pub trait OcrBackend {
    fn recognize_image(&self, image_path: &Path, region: Option<Rect>) -> Result<OcrResult>;
}

pub trait VisionBackend {
    fn extract_text(&self, frame: &CaptureFrame) -> Result<Vec<OcrText>>;
    fn detect_ui_elements(&self, frame: &CaptureFrame) -> Result<Vec<UiElement>>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct TesseractOcrBackend {
    command: String,
    options: OcrOptions,
}

impl Default for TesseractOcrBackend {
    fn default() -> Self {
        Self {
            command: "tesseract".to_owned(),
            options: OcrOptions::default(),
        }
    }
}

impl TesseractOcrBackend {
    pub fn new(command: impl Into<String>, options: OcrOptions) -> Self {
        Self {
            command: command.into(),
            options,
        }
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn options(&self) -> &OcrOptions {
        &self.options
    }

    pub fn is_available(&self) -> bool {
        command_exists(&self.command)
    }
}

impl OcrBackend for TesseractOcrBackend {
    fn recognize_image(&self, image_path: &Path, region: Option<Rect>) -> Result<OcrResult> {
        validate_ocr_options(&self.options)?;
        if !self.is_available() {
            return Err(PeekabooXError::new(format!(
                "OCR backend {} is not available; install tesseract-ocr",
                self.command
            )));
        }

        let prepared = prepare_ocr_image(image_path, region, &self.options)?;
        let output = match Command::new(&self.command)
            .args(tesseract_args(&prepared.path, &self.options))
            .output()
        {
            Ok(output) => output,
            Err(error) => {
                if prepared.temporary {
                    remove_temp_file(&prepared.path);
                }
                return Err(PeekabooXError::new(format!(
                    "failed to execute OCR backend {}: {error}",
                    self.command
                )));
            }
        };

        if !output.status.success() {
            if prepared.temporary {
                remove_temp_file(&prepared.path);
            }
            return Err(PeekabooXError::new(format!(
                "OCR backend {} failed with status {}; stderr: {}",
                self.command,
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }

        let tsv = match String::from_utf8(output.stdout) {
            Ok(tsv) => tsv,
            Err(error) => {
                if prepared.temporary {
                    remove_temp_file(&prepared.path);
                }
                return Err(PeekabooXError::new(format!(
                    "OCR backend returned non-UTF-8 TSV output: {error}"
                )));
            }
        };
        let result = tesseract_result_from_tsv_with_transform(
            &tsv,
            &prepared.transform,
            &self.options,
            prepared.warnings,
        );
        if prepared.temporary {
            remove_temp_file(&prepared.path);
        }
        result
    }
}

#[derive(Debug, Default)]
pub struct UnimplementedVisionBackend;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct HeuristicVisionBackend {
    options: UiElementDetectionOptions,
}

impl HeuristicVisionBackend {
    pub fn new(options: UiElementDetectionOptions) -> Self {
        Self { options }
    }

    pub fn options(&self) -> &UiElementDetectionOptions {
        &self.options
    }
}

impl VisionBackend for HeuristicVisionBackend {
    fn extract_text(&self, _frame: &CaptureFrame) -> Result<Vec<OcrText>> {
        Err(PeekabooXError::new(
            "frame-based OCR backend is unavailable in this environment",
        ))
    }

    fn detect_ui_elements(&self, frame: &CaptureFrame) -> Result<Vec<UiElement>> {
        detect_ui_elements(frame, &self.options)
    }
}

impl VisionBackend for UnimplementedVisionBackend {
    fn extract_text(&self, _frame: &CaptureFrame) -> Result<Vec<OcrText>> {
        Err(PeekabooXError::new(
            "frame-based OCR backend is unavailable in this environment",
        ))
    }

    fn detect_ui_elements(&self, _frame: &CaptureFrame) -> Result<Vec<UiElement>> {
        Err(PeekabooXError::new(
            "vision UI detection backend is unavailable in this environment",
        ))
    }
}

pub fn ocr_screen() -> Result<OcrResult> {
    ocr_screen_with_backend(&TesseractOcrBackend::default())
}

pub fn ocr_region(region: Rect) -> Result<OcrResult> {
    ocr_region_with_backend(&TesseractOcrBackend::default(), region)
}

pub fn ocr_image_file(path: impl AsRef<Path>, region: Option<Rect>) -> Result<OcrResult> {
    TesseractOcrBackend::default().recognize_image(path.as_ref(), region)
}

pub fn ocr_image_file_with_backend(
    backend: &impl OcrBackend,
    path: impl AsRef<Path>,
    region: Option<Rect>,
) -> Result<OcrResult> {
    backend.recognize_image(path.as_ref(), region)
}

pub fn load_image_file(path: impl AsRef<Path>) -> Result<CaptureFrame> {
    let path = path.as_ref();
    let image = ImageReader::open(path)
        .map_err(|error| {
            PeekabooXError::new(format!("failed to open image {}: {error}", path.display()))
        })?
        .decode()
        .map_err(|error| {
            PeekabooXError::new(format!(
                "failed to decode image {}: {error}",
                path.display()
            ))
        })?;

    Ok(capture_frame_from_image(image))
}

pub fn decode_image_bytes(bytes: &[u8]) -> Result<CaptureFrame> {
    let image = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| PeekabooXError::new(format!("failed to detect image format: {error}")))?
        .decode()
        .map_err(|error| PeekabooXError::new(format!("failed to decode image bytes: {error}")))?;

    Ok(capture_frame_from_image(image))
}

pub fn compare_image_files(
    expected_path: impl AsRef<Path>,
    actual_path: impl AsRef<Path>,
    options: &VisualCompareOptions,
) -> Result<VisualDiffResult> {
    let expected = load_image_file(expected_path)?;
    let actual = load_image_file(actual_path)?;

    compare_frames(&expected, &actual, options)
}

pub fn write_visual_diff_image_file(
    expected_path: impl AsRef<Path>,
    actual_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
    options: &VisualCompareOptions,
) -> Result<VisualDiffResult> {
    let expected = load_image_file(expected_path)?;
    let actual = load_image_file(actual_path)?;
    let (result, image) = compare_frames_internal(&expected, &actual, options, true)?;
    let image = image.expect("diff image was requested");
    let output_path = output_path.as_ref();
    if let Some(parent) = output_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            PeekabooXError::new(format!(
                "failed to create visual diff output directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    image.save(output_path).map_err(|error| {
        PeekabooXError::new(format!(
            "failed to write visual diff image {}: {error}",
            output_path.display()
        ))
    })?;

    Ok(result)
}

pub fn compare_image_bytes(
    expected_image: &[u8],
    actual_image: &[u8],
    options: &VisualCompareOptions,
) -> Result<VisualDiffResult> {
    let expected = decode_image_bytes(expected_image)?;
    let actual = decode_image_bytes(actual_image)?;

    compare_frames(&expected, &actual, options)
}

pub fn detect_ui_state_from_image_files<P: AsRef<Path>>(
    paths: &[P],
    options: &UiStateOptions,
) -> Result<UiStateResult> {
    let frames = paths
        .iter()
        .map(|path| load_image_file(path.as_ref()))
        .collect::<Result<Vec<_>>>()?;

    detect_ui_state(&frames, options)
}

pub fn detect_ui_state_from_image_bytes<B: AsRef<[u8]>>(
    images: &[B],
    options: &UiStateOptions,
) -> Result<UiStateResult> {
    let frames = images
        .iter()
        .map(|image| decode_image_bytes(image.as_ref()))
        .collect::<Result<Vec<_>>>()?;

    detect_ui_state(&frames, options)
}

pub fn detect_ui_state(frames: &[CaptureFrame], options: &UiStateOptions) -> Result<UiStateResult> {
    validate_ui_state_options(options)?;
    if frames.len() < 2 {
        return Err(PeekabooXError::new(
            "UI state detection requires at least two frames",
        ));
    }

    let compare_options = VisualCompareOptions {
        region: options.region,
        ignore_regions: options.ignore_regions.clone(),
        per_channel_threshold: options.per_channel_threshold,
        max_changed_ratio: options.stable_max_changed_ratio,
        max_changed_pixels: options.stable_max_changed_pixels,
        max_mean_absolute_error: options.stable_max_mean_absolute_error,
        max_channel_delta: options.stable_max_channel_delta,
        size_policy: options.size_policy,
        alpha_mode: options.alpha_mode,
    };
    let mut latest_diff = None;
    let mut stable_transitions = 0_usize;
    let mut loading_transitions = 0_usize;
    let mut trailing_stable_transitions = 0_usize;
    let mut max_changed_ratio = 0.0_f32;
    let mut changed_ratio_sum = 0.0_f32;
    let mut changed_bounds = None;

    for pair in frames.windows(2) {
        let diff = compare_frames(&pair[0], &pair[1], &compare_options)?;
        let transition_is_stable = diff.matches;
        if transition_is_stable {
            stable_transitions += 1;
            trailing_stable_transitions += 1;
        } else {
            trailing_stable_transitions = 0;
        }

        let transition_is_loading = diff.changed_ratio >= options.loading_min_changed_ratio
            || options
                .loading_min_changed_pixels
                .is_some_and(|minimum| diff.changed_pixels >= minimum);
        if transition_is_loading {
            loading_transitions += 1;
        }

        max_changed_ratio = max_changed_ratio.max(diff.changed_ratio);
        changed_ratio_sum += diff.changed_ratio;
        if let Some(bounds) = diff.changed_bounds {
            changed_bounds =
                Some(changed_bounds.map_or(bounds, |current| rect_union(current, bounds)));
        }
        latest_diff = Some(diff);
    }

    let compared_transitions = frames.len() - 1;
    let state = if trailing_stable_transitions >= options.required_stable_transitions {
        UiStateKind::Stable
    } else if loading_transitions > 0 {
        UiStateKind::Loading
    } else {
        UiStateKind::Changing
    };

    Ok(UiStateResult {
        state,
        compared_transitions,
        stable_transitions,
        loading_transitions,
        trailing_stable_transitions,
        latest_diff: latest_diff.expect("frames length was checked before comparing"),
        max_changed_ratio,
        mean_changed_ratio: changed_ratio_sum / compared_transitions as f32,
        changed_bounds,
    })
}

pub fn detect_ui_elements_from_image_file(
    path: impl AsRef<Path>,
    options: &UiElementDetectionOptions,
) -> Result<Vec<UiElement>> {
    let frame = load_image_file(path)?;

    detect_ui_elements(&frame, options)
}

pub fn detect_ui_elements_from_image_file_with_outputs(
    path: impl AsRef<Path>,
    options: &UiElementDetectionOptions,
    mask_output: Option<&Path>,
    overlay_output: Option<&Path>,
) -> Result<Vec<UiElement>> {
    let frame = load_image_file(path)?;

    detect_ui_elements_with_outputs(&frame, options, mask_output, overlay_output)
}

pub fn detect_ui_elements_from_image_bytes(
    image: &[u8],
    options: &UiElementDetectionOptions,
) -> Result<Vec<UiElement>> {
    let frame = decode_image_bytes(image)?;

    detect_ui_elements(&frame, options)
}

pub fn detect_ui_elements_from_image_bytes_with_outputs(
    image: &[u8],
    options: &UiElementDetectionOptions,
    mask_output: Option<&Path>,
    overlay_output: Option<&Path>,
) -> Result<Vec<UiElement>> {
    let frame = decode_image_bytes(image)?;

    detect_ui_elements_with_outputs(&frame, options, mask_output, overlay_output)
}

pub fn detect_ui_elements(
    frame: &CaptureFrame,
    options: &UiElementDetectionOptions,
) -> Result<Vec<UiElement>> {
    detect_ui_elements_internal(frame, options).map(|result| result.elements)
}

pub fn detect_ui_elements_with_outputs(
    frame: &CaptureFrame,
    options: &UiElementDetectionOptions,
    mask_output: Option<&Path>,
    overlay_output: Option<&Path>,
) -> Result<Vec<UiElement>> {
    let result = detect_ui_elements_internal(frame, options)?;
    if let Some(mask_output) = mask_output {
        write_ui_mask_image(
            &result.mask,
            result.region,
            frame.width,
            frame.height,
            mask_output,
        )?;
    }
    if let Some(overlay_output) = overlay_output {
        write_ui_overlay_image(frame, &result.elements, overlay_output)?;
    }

    Ok(result.elements)
}

struct UiElementDetectionResult {
    elements: Vec<UiElement>,
    region: Rect,
    mask: Vec<bool>,
}

fn detect_ui_elements_internal(
    frame: &CaptureFrame,
    options: &UiElementDetectionOptions,
) -> Result<UiElementDetectionResult> {
    validate_frame(frame, "UI element detection")?;
    validate_ui_element_detection_options(options)?;

    let region = ui_detection_region(frame, options.region)?;
    let mask = ui_saliency_mask(frame, region, options)?;
    let mut components = connected_components(&mask, region)?;
    components.retain(|component| component_matches_options(component, options));
    merge_close_components(&mut components, options.merge_distance);
    components.retain(|component| component_matches_options(component, options));
    sort_components(&mut components, options.sort);
    components.truncate(options.max_elements);
    apply_component_padding(&mut components, options.padding, frame.width, frame.height)?;

    let elements = components
        .iter()
        .enumerate()
        .map(|(index, component)| ui_element_from_component(index, component))
        .collect();

    Ok(UiElementDetectionResult {
        elements,
        region,
        mask,
    })
}

pub fn compare_frames(
    expected: &CaptureFrame,
    actual: &CaptureFrame,
    options: &VisualCompareOptions,
) -> Result<VisualDiffResult> {
    compare_frames_internal(expected, actual, options, false).map(|(result, _)| result)
}

fn compare_frames_internal(
    expected: &CaptureFrame,
    actual: &CaptureFrame,
    options: &VisualCompareOptions,
    build_diff_image: bool,
) -> Result<(VisualDiffResult, Option<RgbaImage>)> {
    validate_visual_compare_options(options)?;
    validate_frame(expected, "expected")?;
    validate_frame(actual, "actual")?;

    let prepared = prepare_visual_frames(expected, actual, options)?;
    validate_ignore_regions(&options.ignore_regions, prepared.region)?;

    let mut changed_pixels = 0_u64;
    let mut absolute_error_sum = 0_u64;
    let mut max_channel_delta = 0_u8;
    let mut changed_bounds = ChangedBounds::default();
    let mut compared_pixels = 0_u64;
    let channel_count = match options.alpha_mode {
        VisualAlphaMode::Ignore => 3_u64,
        VisualAlphaMode::Compare => 4_u64,
    };
    let mut diff_image = build_diff_image.then(|| {
        RgbaImage::from_pixel(
            prepared.expected.width,
            prepared.expected.height,
            Rgba([0, 0, 0, 0]),
        )
    });

    for y in prepared.region.y..region_end_i32(prepared.region.y, prepared.region.height)? {
        for x in prepared.region.x..region_end_i32(prepared.region.x, prepared.region.width)? {
            if point_is_ignored(&options.ignore_regions, x, y)? {
                continue;
            }

            compared_pixels += 1;
            let expected_pixel =
                pixel_compare_channels(&prepared.expected, x as u32, y as u32, options.alpha_mode)?;
            let actual_pixel =
                pixel_compare_channels(&prepared.actual, x as u32, y as u32, options.alpha_mode)?;
            let deltas = (0..usize::try_from(channel_count).unwrap())
                .map(|index| expected_pixel[index].abs_diff(actual_pixel[index]))
                .collect::<Vec<_>>();
            let pixel_max_delta = deltas.iter().copied().max().unwrap_or_default();

            absolute_error_sum += deltas.iter().copied().map(u64::from).sum::<u64>();
            max_channel_delta = max_channel_delta.max(pixel_max_delta);
            if pixel_max_delta > options.per_channel_threshold {
                changed_pixels += 1;
                changed_bounds.include(x, y);
                if let Some(image) = diff_image.as_mut() {
                    image.put_pixel(x as u32, y as u32, Rgba([255, 0, 0, 255]));
                }
            }
        }
    }

    if compared_pixels == 0 {
        return Err(PeekabooXError::new(
            "visual comparison compares zero pixels after applying ignore regions",
        ));
    }

    let changed_ratio = changed_pixels as f32 / compared_pixels as f32;
    let mean_absolute_error = absolute_error_sum as f32 / (compared_pixels * channel_count) as f32;
    let matches = changed_ratio <= options.max_changed_ratio
        && options
            .max_changed_pixels
            .is_none_or(|maximum| changed_pixels <= maximum)
        && options
            .max_mean_absolute_error
            .is_none_or(|maximum| mean_absolute_error <= maximum)
        && options
            .max_channel_delta
            .is_none_or(|maximum| max_channel_delta <= maximum);

    Ok((
        VisualDiffResult {
            compared_region: prepared.region,
            compared_pixels,
            changed_pixels,
            changed_ratio,
            mean_absolute_error,
            max_channel_delta,
            changed_bounds: changed_bounds.into_rect(),
            matches,
        },
        diff_image,
    ))
}

pub fn incremental_capture_delta(
    previous: Option<&CaptureFrame>,
    current: &CaptureFrame,
    sequence: u64,
    options: &IncrementalCaptureOptions,
) -> Result<IncrementalCaptureDelta> {
    validate_frame(current, "current")?;

    if let Some(previous) = previous {
        let diff = compare_frames(previous, current, &options.compare)?;
        let Some(changed_bounds) = diff.changed_bounds else {
            return Ok(IncrementalCaptureDelta {
                sequence,
                frame_width: current.width,
                frame_height: current.height,
                format: current.format,
                full_frame: false,
                changed_bounds: None,
                changed_pixels: 0,
                changed_ratio: 0.0,
                patch_stride: 0,
                patch_data: Vec::new(),
            });
        };

        let (patch_stride, patch_data) = frame_region_patch(current, changed_bounds)?;
        return Ok(IncrementalCaptureDelta {
            sequence,
            frame_width: current.width,
            frame_height: current.height,
            format: current.format,
            full_frame: false,
            changed_bounds: Some(changed_bounds),
            changed_pixels: diff.changed_pixels,
            changed_ratio: diff.changed_ratio,
            patch_stride,
            patch_data,
        });
    }

    let full_bounds = Rect::new(0, 0, current.width, current.height);
    let (patch_stride, patch_data) = frame_region_patch(current, full_bounds)?;
    Ok(IncrementalCaptureDelta {
        sequence,
        frame_width: current.width,
        frame_height: current.height,
        format: current.format,
        full_frame: true,
        changed_bounds: Some(full_bounds),
        changed_pixels: u64::from(current.width) * u64::from(current.height),
        changed_ratio: 1.0,
        patch_stride,
        patch_data,
    })
}

fn capture_frame_from_image(image: DynamicImage) -> CaptureFrame {
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();

    CaptureFrame {
        width,
        height,
        stride: width * 4,
        format: PixelFormat::Rgba8,
        data: rgba.into_raw(),
    }
}

pub fn ocr_screen_with_backend(backend: &impl OcrBackend) -> Result<OcrResult> {
    let screenshot = capture_temp_path();
    capture_to_temp_file(&screenshot)?;
    let result = backend.recognize_image(&screenshot, None);
    remove_temp_file(&screenshot);
    result
}

pub fn ocr_region_with_backend(backend: &impl OcrBackend, region: Rect) -> Result<OcrResult> {
    let screenshot = capture_temp_path();
    capture_to_temp_file(&screenshot)?;
    let result = backend.recognize_image(&screenshot, Some(region));
    remove_temp_file(&screenshot);
    result
}

pub fn tesseract_args(image_path: &Path, options: &OcrOptions) -> Vec<String> {
    let mut args = vec![image_path.display().to_string(), "stdout".to_owned()];

    if let Some(language) = options.language.as_deref()
        && !language.trim().is_empty()
    {
        args.push("-l".to_owned());
        args.push(language.to_owned());
    }

    if let Some(page_segmentation_mode) = options.page_segmentation_mode {
        args.push("--psm".to_owned());
        args.push(page_segmentation_mode.to_string());
    }

    if let Some(engine_mode) = options.engine_mode {
        args.push("--oem".to_owned());
        args.push(engine_mode.to_string());
    }

    if let Some(dpi) = options.dpi {
        args.push("--dpi".to_owned());
        args.push(dpi.to_string());
    }

    if let Some(whitelist) = options.whitelist.as_deref()
        && !whitelist.trim().is_empty()
    {
        args.push("-c".to_owned());
        args.push(format!("tessedit_char_whitelist={whitelist}"));
    }

    for config in &options.config {
        let key = config.key.trim();
        if !key.is_empty() {
            args.push("-c".to_owned());
            args.push(format!("{key}={}", config.value));
        }
    }

    args.push("tsv".to_owned());
    args
}

#[cfg(test)]
fn tesseract_result_from_tsv(tsv: &str, region: Option<Rect>) -> Result<OcrResult> {
    let words = parse_tesseract_words(tsv, region, 0.0)?;
    ocr_result_from_words(words, Vec::new())
}

fn tesseract_result_from_tsv_with_transform(
    tsv: &str,
    transform: &OcrCoordinateTransform,
    options: &OcrOptions,
    warnings: Vec<String>,
) -> Result<OcrResult> {
    let mut words = parse_tesseract_words(tsv, None, options.min_confidence)?;
    for word in &mut words {
        word.bounds = transform.map_rect(word.bounds);
    }
    ocr_result_from_words(words, warnings)
}

fn ocr_result_from_words(words: Vec<TesseractWord>, warnings: Vec<String>) -> Result<OcrResult> {
    let word_blocks = words.iter().map(ocr_text_from_word).collect::<Vec<_>>();
    let blocks = group_words_into_lines(words);
    let text = blocks
        .iter()
        .map(|block| block.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    Ok(OcrResult {
        backend_name: "tesseract".to_owned(),
        text,
        blocks,
        words: word_blocks,
        warnings,
    })
}

#[derive(Debug, Clone, PartialEq)]
struct TesseractWord {
    key: LineKey,
    text: String,
    bounds: Rect,
    confidence: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct LineKey {
    page_num: i32,
    block_num: i32,
    par_num: i32,
    line_num: i32,
}

fn parse_tesseract_words(
    tsv: &str,
    region: Option<Rect>,
    min_confidence: f32,
) -> Result<Vec<TesseractWord>> {
    let mut lines = tsv.lines();
    let Some(header) = lines.next() else {
        return Ok(Vec::new());
    };

    let columns = header.split('\t').collect::<Vec<_>>();
    let column_index = |name: &str| {
        columns
            .iter()
            .position(|column| *column == name)
            .ok_or_else(|| {
                PeekabooXError::new(format!("OCR TSV output is missing {name:?} column"))
            })
    };
    let level_index = column_index("level")?;
    let page_index = column_index("page_num")?;
    let block_index = column_index("block_num")?;
    let par_index = column_index("par_num")?;
    let line_index = column_index("line_num")?;
    let left_index = column_index("left")?;
    let top_index = column_index("top")?;
    let width_index = column_index("width")?;
    let height_index = column_index("height")?;
    let confidence_index = column_index("conf")?;
    let text_index = column_index("text")?;

    let mut words = Vec::new();
    for line in lines {
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() <= text_index {
            continue;
        }
        if parse_i32_field(&fields, level_index).unwrap_or_default() != 5 {
            continue;
        }

        let text = fields[text_index].trim();
        if text.is_empty() {
            continue;
        }

        let confidence = parse_f32_field(&fields, confidence_index).unwrap_or(-1.0);
        if confidence < 0.0 {
            continue;
        }
        let confidence = confidence / 100.0;
        if confidence < min_confidence {
            continue;
        }

        let bounds = Rect::new(
            parse_i32_field(&fields, left_index)?,
            parse_i32_field(&fields, top_index)?,
            parse_u32_field(&fields, width_index)?,
            parse_u32_field(&fields, height_index)?,
        );
        if region.is_some_and(|region| !rects_intersect(bounds, region)) {
            continue;
        }

        words.push(TesseractWord {
            key: LineKey {
                page_num: parse_i32_field(&fields, page_index)?,
                block_num: parse_i32_field(&fields, block_index)?,
                par_num: parse_i32_field(&fields, par_index)?,
                line_num: parse_i32_field(&fields, line_index)?,
            },
            text: text.to_owned(),
            bounds,
            confidence,
        });
    }

    Ok(words)
}

fn ocr_text_from_word(word: &TesseractWord) -> OcrText {
    OcrText {
        text: word.text.clone(),
        element: UiElement {
            id: format!(
                "ocr-word:{}:{}:{}:{}",
                word.bounds.x, word.bounds.y, word.bounds.width, word.bounds.height
            ),
            role: "word".to_owned(),
            label: Some(word.text.clone()),
            bounds: word.bounds,
            center: word.bounds.center(),
            confidence: word.confidence,
            states: Vec::new(),
            window_id: None,
            window_title: None,
            app_id: None,
            parent_id: None,
            child_ids: Vec::new(),
        },
    }
}

fn group_words_into_lines(words: Vec<TesseractWord>) -> Vec<OcrText> {
    let mut lines = BTreeMap::<LineKey, Vec<TesseractWord>>::new();
    for word in words {
        lines.entry(word.key).or_default().push(word);
    }

    lines.into_values().filter_map(line_from_words).collect()
}

fn line_from_words(words: Vec<TesseractWord>) -> Option<OcrText> {
    let mut words = words;
    words.sort_by(|left, right| {
        left.bounds
            .y
            .cmp(&right.bounds.y)
            .then_with(|| left.bounds.x.cmp(&right.bounds.x))
    });
    let text = words
        .iter()
        .map(|word| word.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let bounds = words.iter().map(|word| word.bounds).reduce(rect_union)?;
    let confidence = words.iter().map(|word| word.confidence).sum::<f32>() / words.len() as f32;

    Some(OcrText {
        text: text.clone(),
        element: UiElement {
            id: format!(
                "ocr:{}:{}:{}:{}",
                bounds.x, bounds.y, bounds.width, bounds.height
            ),
            role: "text".to_owned(),
            label: Some(text),
            bounds,
            center: bounds.center(),
            confidence,
            states: Vec::new(),
            window_id: None,
            window_title: None,
            app_id: None,
            parent_id: None,
            child_ids: Vec::new(),
        },
    })
}

fn parse_i32_field(fields: &[&str], index: usize) -> Result<i32> {
    fields
        .get(index)
        .ok_or_else(|| PeekabooXError::new("OCR TSV row has too few columns"))?
        .parse::<i32>()
        .map_err(|error| PeekabooXError::new(format!("OCR TSV integer parse failed: {error}")))
}

fn parse_u32_field(fields: &[&str], index: usize) -> Result<u32> {
    fields
        .get(index)
        .ok_or_else(|| PeekabooXError::new("OCR TSV row has too few columns"))?
        .parse::<u32>()
        .map_err(|error| PeekabooXError::new(format!("OCR TSV unsigned parse failed: {error}")))
}

fn parse_f32_field(fields: &[&str], index: usize) -> Result<f32> {
    fields
        .get(index)
        .ok_or_else(|| PeekabooXError::new("OCR TSV row has too few columns"))?
        .parse::<f32>()
        .map_err(|error| PeekabooXError::new(format!("OCR TSV confidence parse failed: {error}")))
}

fn rects_intersect(left: Rect, right: Rect) -> bool {
    let left_right = i64::from(left.x) + i64::from(left.width);
    let left_bottom = i64::from(left.y) + i64::from(left.height);
    let right_right = i64::from(right.x) + i64::from(right.width);
    let right_bottom = i64::from(right.y) + i64::from(right.height);

    i64::from(left.x) < right_right
        && left_right > i64::from(right.x)
        && i64::from(left.y) < right_bottom
        && left_bottom > i64::from(right.y)
}

fn rect_union(left: Rect, right: Rect) -> Rect {
    let x1 = left.x.min(right.x);
    let y1 = left.y.min(right.y);
    let x2 = (i64::from(left.x) + i64::from(left.width))
        .max(i64::from(right.x) + i64::from(right.width));
    let y2 = (i64::from(left.y) + i64::from(left.height))
        .max(i64::from(right.y) + i64::from(right.height));

    Rect::new(
        x1,
        y1,
        u32::try_from(x2 - i64::from(x1)).unwrap_or(u32::MAX),
        u32::try_from(y2 - i64::from(y1)).unwrap_or(u32::MAX),
    )
}

#[derive(Debug, Clone, PartialEq)]
struct PreparedOcrImage {
    path: PathBuf,
    temporary: bool,
    transform: OcrCoordinateTransform,
    warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct OcrCoordinateTransform {
    origin_x: f32,
    origin_y: f32,
    scale: f32,
    rotation_degrees: f32,
    processed_width: u32,
    processed_height: u32,
}

impl OcrCoordinateTransform {
    fn identity() -> Self {
        Self {
            origin_x: 0.0,
            origin_y: 0.0,
            scale: 1.0,
            rotation_degrees: 0.0,
            processed_width: 0,
            processed_height: 0,
        }
    }

    fn map_rect(self, rect: Rect) -> Rect {
        let scale = if self.scale > 0.0 { self.scale } else { 1.0 };
        let mut corners = [
            (rect.x as f32, rect.y as f32),
            (rect.x as f32 + rect.width as f32, rect.y as f32),
            (rect.x as f32, rect.y as f32 + rect.height as f32),
            (
                rect.x as f32 + rect.width as f32,
                rect.y as f32 + rect.height as f32,
            ),
        ];

        if self.rotation_degrees.abs() > f32::EPSILON
            && self.processed_width > 0
            && self.processed_height > 0
        {
            let radians = (-self.rotation_degrees).to_radians();
            let cos = radians.cos();
            let sin = radians.sin();
            let center_x = (self.processed_width.saturating_sub(1)) as f32 / 2.0;
            let center_y = (self.processed_height.saturating_sub(1)) as f32 / 2.0;
            for corner in &mut corners {
                let dx = corner.0 - center_x;
                let dy = corner.1 - center_y;
                *corner = (
                    center_x + dx * cos - dy * sin,
                    center_y + dx * sin + dy * cos,
                );
            }
        }

        let min_x = corners
            .iter()
            .map(|corner| corner.0)
            .fold(f32::INFINITY, f32::min);
        let min_y = corners
            .iter()
            .map(|corner| corner.1)
            .fold(f32::INFINITY, f32::min);
        let max_x = corners
            .iter()
            .map(|corner| corner.0)
            .fold(f32::NEG_INFINITY, f32::max);
        let max_y = corners
            .iter()
            .map(|corner| corner.1)
            .fold(f32::NEG_INFINITY, f32::max);

        let x = self.origin_x + min_x / scale;
        let y = self.origin_y + min_y / scale;
        let width = ((max_x - min_x) / scale).max(1.0).ceil() as u32;
        let height = ((max_y - min_y) / scale).max(1.0).ceil() as u32;

        Rect::new(x.round() as i32, y.round() as i32, width, height)
    }
}

fn validate_ocr_options(options: &OcrOptions) -> Result<()> {
    if let Some(psm) = options.page_segmentation_mode
        && psm > 13
    {
        return Err(PeekabooXError::new(
            "OCR page segmentation mode must be between 0 and 13",
        ));
    }
    if let Some(oem) = options.engine_mode
        && oem > 3
    {
        return Err(PeekabooXError::new(
            "OCR engine mode must be between 0 and 3",
        ));
    }
    if matches!(options.dpi, Some(0)) {
        return Err(PeekabooXError::new("OCR DPI must be greater than zero"));
    }
    if !options.min_confidence.is_finite() || !(0.0..=1.0).contains(&options.min_confidence) {
        return Err(PeekabooXError::new(
            "OCR minimum confidence must be between 0.0 and 1.0",
        ));
    }
    if let Some(scale) = options.preprocessing.scale
        && (!scale.is_finite() || !(0.1..=8.0).contains(&scale))
    {
        return Err(PeekabooXError::new(
            "OCR preprocessing scale must be between 0.1 and 8.0",
        ));
    }
    if let Some(contrast) = options.preprocessing.contrast
        && (!contrast.is_finite() || !(-255.0..=255.0).contains(&contrast))
    {
        return Err(PeekabooXError::new(
            "OCR preprocessing contrast must be between -255.0 and 255.0",
        ));
    }
    for config in &options.config {
        let key = config.key.trim();
        if key.is_empty() || key.contains('=') || key.split_whitespace().count() > 1 {
            return Err(PeekabooXError::new(
                "OCR config keys must be non-empty names without whitespace or '='",
            ));
        }
    }

    Ok(())
}

fn prepare_ocr_image(
    image_path: &Path,
    region: Option<Rect>,
    options: &OcrOptions,
) -> Result<PreparedOcrImage> {
    if region.is_none() && !requires_ocr_image_preparation(options) {
        return Ok(PreparedOcrImage {
            path: image_path.to_path_buf(),
            temporary: false,
            transform: OcrCoordinateTransform::identity(),
            warnings: Vec::new(),
        });
    }

    let mut image = ImageReader::open(image_path)
        .map_err(|error| {
            PeekabooXError::new(format!(
                "failed to open OCR image {}: {error}",
                image_path.display()
            ))
        })?
        .decode()
        .map_err(|error| {
            PeekabooXError::new(format!(
                "failed to decode OCR image {}: {error}",
                image_path.display()
            ))
        })?;
    let mut transform = OcrCoordinateTransform::identity();
    let mut warnings = Vec::new();

    if let Some(region) = region {
        let crop = image_region(&image, region, "OCR region")?;
        image = image.crop_imm(crop.x as u32, crop.y as u32, crop.width, crop.height);
        transform.origin_x = crop.x as f32;
        transform.origin_y = crop.y as f32;
    }

    apply_ocr_preprocessing(&mut image, &mut transform, &mut warnings, options)?;

    let path = ocr_preprocessed_temp_path();
    image.save(&path).map_err(|error| {
        PeekabooXError::new(format!(
            "failed to save OCR temporary image {}: {error}",
            path.display()
        ))
    })?;

    Ok(PreparedOcrImage {
        path,
        temporary: true,
        transform,
        warnings,
    })
}

fn requires_ocr_image_preparation(options: &OcrOptions) -> bool {
    let preprocessing = &options.preprocessing;
    preprocessing.scale.is_some()
        || preprocessing.grayscale
        || preprocessing.threshold.is_some()
        || preprocessing.invert
        || preprocessing.contrast.is_some()
        || preprocessing.deskew
}

fn image_region(image: &DynamicImage, region: Rect, name: &str) -> Result<Rect> {
    if region.width == 0 || region.height == 0 {
        return Err(PeekabooXError::new(format!("{name} must be non-empty")));
    }
    if region.x < 0 || region.y < 0 {
        return Err(PeekabooXError::new(format!(
            "{name} must be inside image bounds"
        )));
    }
    let right = i64::from(region.x) + i64::from(region.width);
    let bottom = i64::from(region.y) + i64::from(region.height);
    if right > i64::from(image.width()) || bottom > i64::from(image.height()) {
        return Err(PeekabooXError::new(format!("{name} exceeds image bounds")));
    }

    Ok(region)
}

fn apply_ocr_preprocessing(
    image: &mut DynamicImage,
    transform: &mut OcrCoordinateTransform,
    warnings: &mut Vec<String>,
    options: &OcrOptions,
) -> Result<()> {
    let preprocessing = &options.preprocessing;

    if let Some(scale) = preprocessing.scale {
        let width = ((image.width() as f32) * scale).round().max(1.0) as u32;
        let height = ((image.height() as f32) * scale).round().max(1.0) as u32;
        let resized = imageops::resize(
            &image.to_rgba8(),
            width,
            height,
            imageops::FilterType::CatmullRom,
        );
        *image = DynamicImage::ImageRgba8(resized);
        transform.scale *= scale;
    }

    if preprocessing.grayscale || preprocessing.threshold.is_some() || preprocessing.deskew {
        *image = image.grayscale();
    }

    if let Some(contrast) = preprocessing.contrast {
        *image = image.adjust_contrast(contrast);
    }

    if preprocessing.deskew {
        let rgba = image.to_rgba8();
        match estimate_deskew_correction_degrees(&rgba) {
            Some(correction) if correction.abs() >= 0.25 => {
                let rotated = rotate_rgba_nearest(&rgba, correction);
                *image = DynamicImage::ImageRgba8(rotated);
                transform.rotation_degrees += correction;
                warnings.push(format!(
                    "OCR deskew correction applied: {correction:.1} degrees"
                ));
            }
            Some(_) => warnings.push("OCR deskew skipped: image already appears level".to_owned()),
            None => warnings.push("OCR deskew skipped: no dark text pixels found".to_owned()),
        }
    }

    if let Some(threshold) = preprocessing.threshold {
        *image = DynamicImage::ImageRgba8(threshold_rgba(&image.to_rgba8(), threshold));
    }

    if preprocessing.invert {
        image.invert();
    }

    transform.processed_width = image.width();
    transform.processed_height = image.height();
    Ok(())
}

fn threshold_rgba(image: &RgbaImage, threshold: u8) -> RgbaImage {
    let mut output = image.clone();
    for pixel in output.pixels_mut() {
        let [r, g, b, a] = pixel.0;
        let luma =
            (0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b)).round() as u8;
        let value = if luma >= threshold { 255 } else { 0 };
        *pixel = Rgba([value, value, value, a]);
    }
    output
}

fn estimate_deskew_correction_degrees(image: &RgbaImage) -> Option<f32> {
    let dark_pixels = dark_pixel_coordinates(image);
    if dark_pixels.len() < 8 {
        return None;
    }

    let mut best_degrees = 0.0;
    let mut best_score = f64::NEG_INFINITY;
    for step in -14..=14 {
        let degrees = step as f32 * 0.5;
        let score = horizontal_projection_score(&dark_pixels, image.height(), degrees);
        if score > best_score {
            best_score = score;
            best_degrees = degrees;
        }
    }

    Some(best_degrees)
}

fn dark_pixel_coordinates(image: &RgbaImage) -> Vec<(f32, f32)> {
    let mut pixels = Vec::new();
    for (x, y, pixel) in image.enumerate_pixels() {
        let [r, g, b, a] = pixel.0;
        if a < 16 {
            continue;
        }
        let luma = 0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b);
        if luma < 200.0 {
            pixels.push((x as f32, y as f32));
        }
    }
    pixels
}

fn horizontal_projection_score(pixels: &[(f32, f32)], height: u32, degrees: f32) -> f64 {
    let radians = degrees.to_radians();
    let cos = radians.cos();
    let sin = radians.sin();
    let center_y = (height.saturating_sub(1)) as f32 / 2.0;
    let mut buckets = vec![0_u32; height.max(1) as usize];
    for (x, y) in pixels {
        let projected_y = center_y + (*x * sin + (*y - center_y) * cos);
        let bucket = projected_y.round() as i32;
        if (0..height as i32).contains(&bucket) {
            buckets[bucket as usize] += 1;
        }
    }

    buckets
        .into_iter()
        .map(|count| {
            let count = f64::from(count);
            count * count
        })
        .sum()
}

fn rotate_rgba_nearest(source: &RgbaImage, degrees: f32) -> RgbaImage {
    let radians = degrees.to_radians();
    let cos = radians.cos();
    let sin = radians.sin();
    let width = source.width();
    let height = source.height();
    let center_x = (width.saturating_sub(1)) as f32 / 2.0;
    let center_y = (height.saturating_sub(1)) as f32 / 2.0;
    let mut output = RgbaImage::from_pixel(width, height, Rgba([255, 255, 255, 255]));

    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - center_x;
            let dy = y as f32 - center_y;
            let source_x = center_x + dx * cos + dy * sin;
            let source_y = center_y - dx * sin + dy * cos;
            if source_x >= 0.0
                && source_y >= 0.0
                && source_x < width as f32
                && source_y < height as f32
            {
                let source_x = (source_x.round() as u32).min(width.saturating_sub(1));
                let source_y = (source_y.round() as u32).min(height.saturating_sub(1));
                let pixel = source.get_pixel(source_x, source_y);
                output.put_pixel(x, y, *pixel);
            }
        }
    }

    output
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VisualComponent {
    bounds: Rect,
    pixels: u32,
    score_area: u64,
}

fn validate_ui_element_detection_options(options: &UiElementDetectionOptions) -> Result<()> {
    if options.edge_threshold == 0 {
        return Err(PeekabooXError::new(
            "UI element detection edge_threshold must be greater than zero",
        ));
    }
    if options.min_width == 0 || options.min_height == 0 {
        return Err(PeekabooXError::new(
            "UI element detection min_width and min_height must be greater than zero",
        ));
    }
    if options.min_component_pixels == 0 {
        return Err(PeekabooXError::new(
            "UI element detection min_component_pixels must be greater than zero",
        ));
    }
    if options.max_elements == 0 {
        return Err(PeekabooXError::new(
            "UI element detection max_elements must be greater than zero",
        ));
    }
    if options
        .min_confidence
        .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
    {
        return Err(PeekabooXError::new(
            "UI element detection min_confidence must be a finite value between 0.0 and 1.0",
        ));
    }
    if options.max_width.is_some_and(|value| value == 0)
        || options.max_height.is_some_and(|value| value == 0)
    {
        return Err(PeekabooXError::new(
            "UI element detection max_width and max_height must be greater than zero",
        ));
    }
    if let Some(max_width) = options.max_width
        && options.min_width > max_width
    {
        return Err(PeekabooXError::new(
            "UI element detection min_width must be less than or equal to max_width",
        ));
    }
    if let Some(max_height) = options.max_height
        && options.min_height > max_height
    {
        return Err(PeekabooXError::new(
            "UI element detection min_height must be less than or equal to max_height",
        ));
    }
    if options.min_area.is_some_and(|value| value == 0)
        || options.max_area.is_some_and(|value| value == 0)
    {
        return Err(PeekabooXError::new(
            "UI element detection min_area and max_area must be greater than zero",
        ));
    }
    if let (Some(min_area), Some(max_area)) = (options.min_area, options.max_area)
        && min_area > max_area
    {
        return Err(PeekabooXError::new(
            "UI element detection min_area must be less than or equal to max_area",
        ));
    }
    for region in &options.ignore_regions {
        if region.width == 0 || region.height == 0 {
            return Err(PeekabooXError::new(
                "UI element detection ignore regions must be non-empty",
            ));
        }
        if region.x < 0 || region.y < 0 {
            return Err(PeekabooXError::new(
                "UI element detection ignore regions must not use negative coordinates",
            ));
        }
        region_end_i32(region.x, region.width)?;
        region_end_i32(region.y, region.height)?;
    }

    Ok(())
}

fn ui_detection_region(frame: &CaptureFrame, region: Option<Rect>) -> Result<Rect> {
    let region = region.unwrap_or_else(|| Rect::new(0, 0, frame.width, frame.height));
    if region.width == 0 || region.height == 0 {
        return Err(PeekabooXError::new(
            "UI element detection region must be non-empty",
        ));
    }
    if region.x < 0 || region.y < 0 {
        return Err(PeekabooXError::new(
            "UI element detection region must be inside frame bounds",
        ));
    }

    let right = i64::from(region.x) + i64::from(region.width);
    let bottom = i64::from(region.y) + i64::from(region.height);
    if right > i64::from(frame.width) || bottom > i64::from(frame.height) {
        return Err(PeekabooXError::new(
            "UI element detection region exceeds frame bounds",
        ));
    }

    Ok(region)
}

fn ui_saliency_mask(
    frame: &CaptureFrame,
    region: Rect,
    options: &UiElementDetectionOptions,
) -> Result<Vec<bool>> {
    let width = usize::try_from(region.width)
        .map_err(|_| PeekabooXError::new("UI detection region width overflows usize"))?;
    let height = usize::try_from(region.height)
        .map_err(|_| PeekabooXError::new("UI detection region height overflows usize"))?;
    let background = region_background_rgb(frame, region)?;
    let mut mask = vec![false; width * height];

    for relative_y in 0..height {
        for relative_x in 0..width {
            let absolute_x = u32::try_from(region.x)
                .ok()
                .and_then(|x| x.checked_add(u32::try_from(relative_x).ok()?))
                .ok_or_else(|| PeekabooXError::new("UI detection x coordinate overflows u32"))?;
            let absolute_y = u32::try_from(region.y)
                .ok()
                .and_then(|y| y.checked_add(u32::try_from(relative_y).ok()?))
                .ok_or_else(|| PeekabooXError::new("UI detection y coordinate overflows u32"))?;
            if point_is_ignored(
                &options.ignore_regions,
                i32::try_from(absolute_x)
                    .map_err(|_| PeekabooXError::new("UI detection x coordinate overflows i32"))?,
                i32::try_from(absolute_y)
                    .map_err(|_| PeekabooXError::new("UI detection y coordinate overflows i32"))?,
            )? {
                continue;
            }
            let pixel = pixel_rgb(frame, absolute_x, absolute_y)?;
            let contrast = max_rgb_delta(pixel, background);
            if contrast >= options.edge_threshold
                || has_local_edge(
                    frame,
                    region,
                    relative_x,
                    relative_y,
                    pixel,
                    background,
                    options.edge_threshold,
                )?
            {
                mask[relative_y * width + relative_x] = true;
            }
        }
    }

    Ok(mask)
}

fn region_background_rgb(frame: &CaptureFrame, region: Rect) -> Result<[u8; 3]> {
    let left = u32::try_from(region.x)
        .map_err(|_| PeekabooXError::new("UI detection region x overflows u32"))?;
    let top = u32::try_from(region.y)
        .map_err(|_| PeekabooXError::new("UI detection region y overflows u32"))?;
    let right = left
        .checked_add(region.width)
        .and_then(|value| value.checked_sub(1))
        .ok_or_else(|| PeekabooXError::new("UI detection region right overflows u32"))?;
    let bottom = top
        .checked_add(region.height)
        .and_then(|value| value.checked_sub(1))
        .ok_or_else(|| PeekabooXError::new("UI detection region bottom overflows u32"))?;
    let samples = [
        pixel_rgb(frame, left, top)?,
        pixel_rgb(frame, right, top)?,
        pixel_rgb(frame, left, bottom)?,
        pixel_rgb(frame, right, bottom)?,
    ];
    let channel_average = |channel: usize| {
        u8::try_from(
            samples
                .iter()
                .map(|sample| u32::from(sample[channel]))
                .sum::<u32>()
                / samples.len() as u32,
        )
        .unwrap_or(u8::MAX)
    };

    Ok([channel_average(0), channel_average(1), channel_average(2)])
}

fn has_local_edge(
    frame: &CaptureFrame,
    region: Rect,
    relative_x: usize,
    relative_y: usize,
    pixel: [u8; 3],
    background: [u8; 3],
    threshold: u8,
) -> Result<bool> {
    let width = usize::try_from(region.width)
        .map_err(|_| PeekabooXError::new("UI detection region width overflows usize"))?;
    let height = usize::try_from(region.height)
        .map_err(|_| PeekabooXError::new("UI detection region height overflows usize"))?;
    let neighbors = [
        relative_x.checked_sub(1).map(|x| (x, relative_y)),
        (relative_x + 1 < width).then_some((relative_x + 1, relative_y)),
        relative_y.checked_sub(1).map(|y| (relative_x, y)),
        (relative_y + 1 < height).then_some((relative_x, relative_y + 1)),
    ];

    for neighbor in neighbors.into_iter().flatten() {
        let absolute_x = u32::try_from(region.x)
            .ok()
            .and_then(|x| x.checked_add(u32::try_from(neighbor.0).ok()?))
            .ok_or_else(|| PeekabooXError::new("UI detection neighbor x overflows u32"))?;
        let absolute_y = u32::try_from(region.y)
            .ok()
            .and_then(|y| y.checked_add(u32::try_from(neighbor.1).ok()?))
            .ok_or_else(|| PeekabooXError::new("UI detection neighbor y overflows u32"))?;
        let neighbor_pixel = pixel_rgb(frame, absolute_x, absolute_y)?;
        if max_rgb_delta(pixel, neighbor_pixel) >= threshold
            && max_rgb_delta(pixel, background) >= max_rgb_delta(neighbor_pixel, background)
        {
            return Ok(true);
        }
    }

    Ok(false)
}

fn connected_components(mask: &[bool], region: Rect) -> Result<Vec<VisualComponent>> {
    let width = usize::try_from(region.width)
        .map_err(|_| PeekabooXError::new("UI detection region width overflows usize"))?;
    let height = usize::try_from(region.height)
        .map_err(|_| PeekabooXError::new("UI detection region height overflows usize"))?;
    let mut visited = vec![false; mask.len()];
    let mut components = Vec::new();

    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            if !mask[index] || visited[index] {
                continue;
            }

            let component = flood_fill_component(mask, width, height, region, x, y, &mut visited)?;
            components.push(component);
        }
    }

    Ok(components)
}

fn flood_fill_component(
    mask: &[bool],
    width: usize,
    height: usize,
    region: Rect,
    start_x: usize,
    start_y: usize,
    visited: &mut [bool],
) -> Result<VisualComponent> {
    let mut stack = vec![(start_x, start_y)];
    let mut bounds = ChangedBounds::default();
    let mut pixels = 0_u32;

    while let Some((x, y)) = stack.pop() {
        let index = y * width + x;
        if visited[index] || !mask[index] {
            continue;
        }

        visited[index] = true;
        pixels = pixels.saturating_add(1);
        let absolute_x = region
            .x
            .checked_add(
                i32::try_from(x)
                    .map_err(|_| PeekabooXError::new("UI detection x overflows i32"))?,
            )
            .ok_or_else(|| PeekabooXError::new("UI detection x coordinate overflows i32"))?;
        let absolute_y = region
            .y
            .checked_add(
                i32::try_from(y)
                    .map_err(|_| PeekabooXError::new("UI detection y overflows i32"))?,
            )
            .ok_or_else(|| PeekabooXError::new("UI detection y coordinate overflows i32"))?;
        bounds.include(absolute_x, absolute_y);

        for neighbor_y in y.saturating_sub(1)..=(y + 1).min(height - 1) {
            for neighbor_x in x.saturating_sub(1)..=(x + 1).min(width - 1) {
                if neighbor_x == x && neighbor_y == y {
                    continue;
                }
                let neighbor_index = neighbor_y * width + neighbor_x;
                if mask[neighbor_index] && !visited[neighbor_index] {
                    stack.push((neighbor_x, neighbor_y));
                }
            }
        }
    }

    let bounds = bounds
        .into_rect()
        .ok_or_else(|| PeekabooXError::new("UI detection component has no bounds"))?;
    Ok(VisualComponent {
        bounds,
        pixels,
        score_area: u64::from(bounds.width) * u64::from(bounds.height),
    })
}

fn component_matches_options(
    component: &VisualComponent,
    options: &UiElementDetectionOptions,
) -> bool {
    let area = component_area(component);
    component.bounds.width >= options.min_width
        && options
            .max_width
            .is_none_or(|maximum| component.bounds.width <= maximum)
        && component.bounds.height >= options.min_height
        && options
            .max_height
            .is_none_or(|maximum| component.bounds.height <= maximum)
        && component.pixels >= options.min_component_pixels
        && options.min_area.is_none_or(|minimum| area >= minimum)
        && options.max_area.is_none_or(|maximum| area <= maximum)
        && options
            .min_confidence
            .is_none_or(|minimum| component_confidence(component) >= minimum)
}

fn merge_close_components(components: &mut Vec<VisualComponent>, merge_distance: u32) {
    let mut index = 0;
    while index < components.len() {
        let mut other_index = index + 1;
        while other_index < components.len() {
            if rects_within_distance(
                components[index].bounds,
                components[other_index].bounds,
                merge_distance,
            ) {
                let other = components.remove(other_index);
                components[index].bounds = rect_union(components[index].bounds, other.bounds);
                components[index].pixels = components[index].pixels.saturating_add(other.pixels);
                components[index].score_area = component_area(&components[index]);
            } else {
                other_index += 1;
            }
        }
        index += 1;
    }
}

fn rects_within_distance(left: Rect, right: Rect, distance: u32) -> bool {
    let distance = i64::from(distance);
    let left_right = i64::from(left.x) + i64::from(left.width);
    let left_bottom = i64::from(left.y) + i64::from(left.height);
    let right_right = i64::from(right.x) + i64::from(right.width);
    let right_bottom = i64::from(right.y) + i64::from(right.height);

    i64::from(left.x) - distance < right_right
        && left_right + distance > i64::from(right.x)
        && i64::from(left.y) - distance < right_bottom
        && left_bottom + distance > i64::from(right.y)
}

fn component_area(component: &VisualComponent) -> u64 {
    u64::from(component.bounds.width) * u64::from(component.bounds.height)
}

fn component_confidence(component: &VisualComponent) -> f32 {
    let area = component.score_area.max(1);
    let density = component.pixels as f32 / area as f32;

    (0.45 + density.min(1.0) * 0.45).min(0.95)
}

fn apply_component_padding(
    components: &mut [VisualComponent],
    padding: u32,
    frame_width: u32,
    frame_height: u32,
) -> Result<()> {
    if padding == 0 {
        return Ok(());
    }

    let frame_right = i64::from(frame_width);
    let frame_bottom = i64::from(frame_height);
    let padding = i64::from(padding);
    for component in components {
        let right = i64::from(component.bounds.x) + i64::from(component.bounds.width);
        let bottom = i64::from(component.bounds.y) + i64::from(component.bounds.height);
        let left = (i64::from(component.bounds.x) - padding).max(0);
        let top = (i64::from(component.bounds.y) - padding).max(0);
        let right = (right + padding).min(frame_right);
        let bottom = (bottom + padding).min(frame_bottom);
        component.bounds = Rect::new(
            i32::try_from(left)
                .map_err(|_| PeekabooXError::new("UI detection padded x overflows i32"))?,
            i32::try_from(top)
                .map_err(|_| PeekabooXError::new("UI detection padded y overflows i32"))?,
            u32::try_from(right.saturating_sub(left))
                .map_err(|_| PeekabooXError::new("UI detection padded width overflows u32"))?,
            u32::try_from(bottom.saturating_sub(top))
                .map_err(|_| PeekabooXError::new("UI detection padded height overflows u32"))?,
        );
    }

    Ok(())
}

fn sort_components(components: &mut [VisualComponent], sort: UiElementSort) {
    match sort {
        UiElementSort::Position => components.sort_by(component_position_cmp),
        UiElementSort::Area => components.sort_by(|left, right| {
            component_area(right)
                .cmp(&component_area(left))
                .then_with(|| component_position_cmp(left, right))
        }),
        UiElementSort::Confidence => components.sort_by(|left, right| {
            component_confidence(right)
                .partial_cmp(&component_confidence(left))
                .unwrap_or(Ordering::Equal)
                .then_with(|| component_position_cmp(left, right))
        }),
    }
}

fn component_position_cmp(left: &VisualComponent, right: &VisualComponent) -> Ordering {
    left.bounds
        .y
        .cmp(&right.bounds.y)
        .then_with(|| left.bounds.x.cmp(&right.bounds.x))
        .then_with(|| left.bounds.width.cmp(&right.bounds.width))
        .then_with(|| left.bounds.height.cmp(&right.bounds.height))
}

fn ui_element_from_component(index: usize, component: &VisualComponent) -> UiElement {
    UiElement {
        id: format!(
            "vision:{}:{}:{}:{}:{}",
            index,
            component.bounds.x,
            component.bounds.y,
            component.bounds.width,
            component.bounds.height
        ),
        role: "visual-region".to_owned(),
        label: None,
        bounds: component.bounds,
        center: component.bounds.center(),
        confidence: component_confidence(component),
        states: vec!["visible".to_owned()],
        window_id: None,
        window_title: None,
        app_id: None,
        parent_id: None,
        child_ids: Vec::new(),
    }
}

fn write_ui_mask_image(
    mask: &[bool],
    region: Rect,
    frame_width: u32,
    frame_height: u32,
    output_path: &Path,
) -> Result<()> {
    let region_width = usize::try_from(region.width)
        .map_err(|_| PeekabooXError::new("UI detection mask width overflows usize"))?;
    let region_height = usize::try_from(region.height)
        .map_err(|_| PeekabooXError::new("UI detection mask height overflows usize"))?;
    if mask.len() != region_width.saturating_mul(region_height) {
        return Err(PeekabooXError::new(
            "UI detection mask dimensions do not match detection region",
        ));
    }

    let mut image = RgbaImage::from_pixel(frame_width, frame_height, Rgba([0, 0, 0, 255]));
    for y in 0..region_height {
        for x in 0..region_width {
            if !mask[y * region_width + x] {
                continue;
            }
            let absolute_x = region
                .x
                .checked_add(
                    i32::try_from(x)
                        .map_err(|_| PeekabooXError::new("UI detection mask x overflows i32"))?,
                )
                .ok_or_else(|| PeekabooXError::new("UI detection mask x coordinate overflows"))?;
            let absolute_y = region
                .y
                .checked_add(
                    i32::try_from(y)
                        .map_err(|_| PeekabooXError::new("UI detection mask y overflows i32"))?,
                )
                .ok_or_else(|| PeekabooXError::new("UI detection mask y coordinate overflows"))?;
            image.put_pixel(
                u32::try_from(absolute_x)
                    .map_err(|_| PeekabooXError::new("UI detection mask x is negative"))?,
                u32::try_from(absolute_y)
                    .map_err(|_| PeekabooXError::new("UI detection mask y is negative"))?,
                Rgba([255, 255, 255, 255]),
            );
        }
    }

    write_rgba_image_file(output_path, &image, "UI detection mask")
}

fn write_ui_overlay_image(
    frame: &CaptureFrame,
    elements: &[UiElement],
    output_path: &Path,
) -> Result<()> {
    let mut image = rgba_image_from_frame(frame)?;
    let colors = [
        Rgba([255, 0, 0, 255]),
        Rgba([0, 160, 255, 255]),
        Rgba([0, 190, 90, 255]),
        Rgba([255, 180, 0, 255]),
        Rgba([190, 70, 255, 255]),
    ];
    for (index, element) in elements.iter().enumerate() {
        draw_rect_outline(&mut image, element.bounds, colors[index % colors.len()])?;
    }

    write_rgba_image_file(output_path, &image, "UI detection overlay")
}

fn draw_rect_outline(image: &mut RgbaImage, bounds: Rect, color: Rgba<u8>) -> Result<()> {
    if bounds.width == 0 || bounds.height == 0 {
        return Ok(());
    }

    let left = i64::from(bounds.x).clamp(0, i64::from(image.width()));
    let top = i64::from(bounds.y).clamp(0, i64::from(image.height()));
    let right = (i64::from(bounds.x) + i64::from(bounds.width)).clamp(0, i64::from(image.width()));
    let bottom =
        (i64::from(bounds.y) + i64::from(bounds.height)).clamp(0, i64::from(image.height()));
    if left >= right || top >= bottom {
        return Ok(());
    }

    let left = u32::try_from(left)
        .map_err(|_| PeekabooXError::new("UI detection overlay left overflows u32"))?;
    let top = u32::try_from(top)
        .map_err(|_| PeekabooXError::new("UI detection overlay top overflows u32"))?;
    let right = u32::try_from(right.saturating_sub(1))
        .map_err(|_| PeekabooXError::new("UI detection overlay right overflows u32"))?;
    let bottom = u32::try_from(bottom.saturating_sub(1))
        .map_err(|_| PeekabooXError::new("UI detection overlay bottom overflows u32"))?;

    for x in left..=right {
        image.put_pixel(x, top, color);
        image.put_pixel(x, bottom, color);
    }
    for y in top..=bottom {
        image.put_pixel(left, y, color);
        image.put_pixel(right, y, color);
    }

    Ok(())
}

fn write_rgba_image_file(output_path: &Path, image: &RgbaImage, description: &str) -> Result<()> {
    if let Some(parent) = output_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            PeekabooXError::new(format!(
                "failed to create {description} output directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    image.save(output_path).map_err(|error| {
        PeekabooXError::new(format!(
            "failed to write {description} image {}: {error}",
            output_path.display()
        ))
    })
}

fn max_rgb_delta(left: [u8; 3], right: [u8; 3]) -> u8 {
    [
        left[0].abs_diff(right[0]),
        left[1].abs_diff(right[1]),
        left[2].abs_diff(right[2]),
    ]
    .into_iter()
    .max()
    .unwrap_or_default()
}

fn validate_ui_state_options(options: &UiStateOptions) -> Result<()> {
    if !(0.0..=1.0).contains(&options.stable_max_changed_ratio)
        || !options.stable_max_changed_ratio.is_finite()
    {
        return Err(PeekabooXError::new(
            "stable_max_changed_ratio must be a finite value between 0.0 and 1.0",
        ));
    }
    if !(0.0..=1.0).contains(&options.loading_min_changed_ratio)
        || !options.loading_min_changed_ratio.is_finite()
    {
        return Err(PeekabooXError::new(
            "loading_min_changed_ratio must be a finite value between 0.0 and 1.0",
        ));
    }
    if options
        .stable_max_mean_absolute_error
        .is_some_and(|value| !value.is_finite() || !(0.0..=255.0).contains(&value))
    {
        return Err(PeekabooXError::new(
            "stable_max_mean_absolute_error must be a finite value between 0.0 and 255.0",
        ));
    }
    if options.stable_max_changed_ratio > options.loading_min_changed_ratio {
        return Err(PeekabooXError::new(
            "stable_max_changed_ratio must be less than or equal to loading_min_changed_ratio",
        ));
    }
    if let (Some(stable_max), Some(loading_min)) = (
        options.stable_max_changed_pixels,
        options.loading_min_changed_pixels,
    ) && stable_max > loading_min
    {
        return Err(PeekabooXError::new(
            "stable_max_changed_pixels must be less than or equal to loading_min_changed_pixels",
        ));
    }
    if options.required_stable_transitions == 0 {
        return Err(PeekabooXError::new(
            "required_stable_transitions must be greater than zero",
        ));
    }

    Ok(())
}

#[derive(Debug, Default)]
struct ChangedBounds {
    min_x: Option<i32>,
    min_y: Option<i32>,
    max_x: i32,
    max_y: i32,
}

impl ChangedBounds {
    fn include(&mut self, x: i32, y: i32) {
        self.min_x = Some(self.min_x.map_or(x, |min_x| min_x.min(x)));
        self.min_y = Some(self.min_y.map_or(y, |min_y| min_y.min(y)));
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
    }

    fn into_rect(self) -> Option<Rect> {
        let min_x = self.min_x?;
        let min_y = self.min_y?;
        Some(Rect::new(
            min_x,
            min_y,
            u32::try_from(i64::from(self.max_x) - i64::from(min_x) + 1).ok()?,
            u32::try_from(i64::from(self.max_y) - i64::from(min_y) + 1).ok()?,
        ))
    }
}

fn validate_frame(frame: &CaptureFrame, name: &str) -> Result<()> {
    let bytes_per_pixel = bytes_per_pixel(frame.format);
    let width_bytes = usize::try_from(frame.width)
        .ok()
        .and_then(|width| width.checked_mul(bytes_per_pixel))
        .ok_or_else(|| PeekabooXError::new(format!("{name} frame width overflows usize")))?;
    let stride = usize::try_from(frame.stride)
        .map_err(|_| PeekabooXError::new(format!("{name} frame stride overflows usize")))?;
    if stride < width_bytes {
        return Err(PeekabooXError::new(format!(
            "{name} frame stride {} is smaller than row width {}",
            frame.stride, width_bytes
        )));
    }
    if frame.height == 0 || frame.width == 0 {
        return Err(PeekabooXError::new(format!(
            "{name} frame dimensions must be greater than zero"
        )));
    }

    let required_len = required_frame_len(frame, bytes_per_pixel)?;
    if frame.data.len() < required_len {
        return Err(PeekabooXError::new(format!(
            "{name} frame data is too short: expected at least {required_len} bytes, got {}",
            frame.data.len()
        )));
    }

    Ok(())
}

fn required_frame_len(frame: &CaptureFrame, bytes_per_pixel: usize) -> Result<usize> {
    let height = usize::try_from(frame.height)
        .map_err(|_| PeekabooXError::new("frame height overflows usize"))?;
    let stride = usize::try_from(frame.stride)
        .map_err(|_| PeekabooXError::new("frame stride overflows usize"))?;
    let width = usize::try_from(frame.width)
        .map_err(|_| PeekabooXError::new("frame width overflows usize"))?;
    let row_bytes = width
        .checked_mul(bytes_per_pixel)
        .ok_or_else(|| PeekabooXError::new("frame row size overflows usize"))?;

    height
        .checked_sub(1)
        .and_then(|rows_before_last| rows_before_last.checked_mul(stride))
        .and_then(|prefix| prefix.checked_add(row_bytes))
        .ok_or_else(|| PeekabooXError::new("frame data length overflows usize"))
}

fn comparison_region(frame: &CaptureFrame, region: Option<Rect>) -> Result<Rect> {
    comparison_region_for_dimensions(frame.width, frame.height, region)
}

fn comparison_region_for_dimensions(width: u32, height: u32, region: Option<Rect>) -> Result<Rect> {
    let region = region.unwrap_or_else(|| Rect::new(0, 0, width, height));
    if region.width == 0 || region.height == 0 {
        return Err(PeekabooXError::new(
            "visual comparison region must be non-empty",
        ));
    }
    if region.x < 0 || region.y < 0 {
        return Err(PeekabooXError::new(
            "visual comparison region must be inside frame bounds",
        ));
    }

    let right = i64::from(region.x) + i64::from(region.width);
    let bottom = i64::from(region.y) + i64::from(region.height);
    if right > i64::from(width) || bottom > i64::from(height) {
        return Err(PeekabooXError::new(
            "visual comparison region exceeds frame bounds",
        ));
    }

    Ok(region)
}

struct PreparedVisualFrames {
    expected: CaptureFrame,
    actual: CaptureFrame,
    region: Rect,
}

fn validate_visual_compare_options(options: &VisualCompareOptions) -> Result<()> {
    if !options.max_changed_ratio.is_finite() || !(0.0..=1.0).contains(&options.max_changed_ratio) {
        return Err(PeekabooXError::new(
            "max_changed_ratio must be a finite value between 0.0 and 1.0",
        ));
    }
    if options
        .max_mean_absolute_error
        .is_some_and(|value| !value.is_finite() || !(0.0..=255.0).contains(&value))
    {
        return Err(PeekabooXError::new(
            "max_mean_absolute_error must be a finite value between 0.0 and 255.0",
        ));
    }

    Ok(())
}

fn prepare_visual_frames(
    expected: &CaptureFrame,
    actual: &CaptureFrame,
    options: &VisualCompareOptions,
) -> Result<PreparedVisualFrames> {
    match options.size_policy {
        VisualSizePolicy::Error => {
            if expected.width != actual.width || expected.height != actual.height {
                return Err(PeekabooXError::new(format!(
                    "visual comparison requires matching frame dimensions, got expected {}x{} and actual {}x{}",
                    expected.width, expected.height, actual.width, actual.height
                )));
            }
            Ok(PreparedVisualFrames {
                expected: expected.clone(),
                actual: actual.clone(),
                region: comparison_region(expected, options.region)?,
            })
        }
        VisualSizePolicy::CommonRegion => {
            let common_width = expected.width.min(actual.width);
            let common_height = expected.height.min(actual.height);
            let region = if let Some(region) = options.region {
                comparison_region_for_dimensions(expected.width, expected.height, Some(region))?;
                comparison_region_for_dimensions(actual.width, actual.height, Some(region))?
            } else {
                comparison_region_for_dimensions(common_width, common_height, None)?
            };
            Ok(PreparedVisualFrames {
                expected: expected.clone(),
                actual: actual.clone(),
                region,
            })
        }
        VisualSizePolicy::ResizeActual => {
            let actual = resize_frame(actual, expected.width, expected.height)?;
            Ok(PreparedVisualFrames {
                expected: expected.clone(),
                actual,
                region: comparison_region(expected, options.region)?,
            })
        }
    }
}

fn resize_frame(frame: &CaptureFrame, width: u32, height: u32) -> Result<CaptureFrame> {
    if frame.width == width && frame.height == height {
        return Ok(frame.clone());
    }
    let source = rgba_image_from_frame(frame)?;
    let resized = imageops::resize(&source, width, height, imageops::FilterType::Nearest);

    Ok(CaptureFrame {
        width,
        height,
        stride: width * 4,
        format: PixelFormat::Rgba8,
        data: resized.into_raw(),
    })
}

fn rgba_image_from_frame(frame: &CaptureFrame) -> Result<RgbaImage> {
    let mut image = RgbaImage::new(frame.width, frame.height);
    for y in 0..frame.height {
        for x in 0..frame.width {
            image.put_pixel(x, y, Rgba(pixel_rgba(frame, x, y)?));
        }
    }
    Ok(image)
}

fn validate_ignore_regions(ignore_regions: &[Rect], compared_region: Rect) -> Result<()> {
    for region in ignore_regions {
        if region.width == 0 || region.height == 0 {
            return Err(PeekabooXError::new(
                "visual comparison ignore regions must be non-empty",
            ));
        }
        if region.x < 0 || region.y < 0 {
            return Err(PeekabooXError::new(
                "visual comparison ignore regions must not use negative coordinates",
            ));
        }
        region_end_i32(region.x, region.width)?;
        region_end_i32(region.y, region.height)?;
        if !rects_intersect(*region, compared_region) {
            continue;
        }
    }

    Ok(())
}

fn frame_region_patch(frame: &CaptureFrame, region: Rect) -> Result<(u32, Vec<u8>)> {
    validate_frame(frame, "patch source")?;
    let region = comparison_region(frame, Some(region))?;
    let bytes_per_pixel = bytes_per_pixel(frame.format);
    let patch_stride = usize::try_from(region.width)
        .ok()
        .and_then(|width| width.checked_mul(bytes_per_pixel))
        .ok_or_else(|| PeekabooXError::new("patch row size overflows usize"))?;
    let patch_len = usize::try_from(region.height)
        .ok()
        .and_then(|height| height.checked_mul(patch_stride))
        .ok_or_else(|| PeekabooXError::new("patch data length overflows usize"))?;
    let source_stride = usize::try_from(frame.stride)
        .map_err(|_| PeekabooXError::new("patch source stride overflows usize"))?;
    let mut data = Vec::with_capacity(patch_len);

    for y in region.y..region_end_i32(region.y, region.height)? {
        let row_offset = usize::try_from(y)
            .ok()
            .and_then(|row| row.checked_mul(source_stride))
            .ok_or_else(|| PeekabooXError::new("patch row offset overflows usize"))?;
        let column_offset = usize::try_from(region.x)
            .ok()
            .and_then(|column| column.checked_mul(bytes_per_pixel))
            .ok_or_else(|| PeekabooXError::new("patch column offset overflows usize"))?;
        let offset = row_offset
            .checked_add(column_offset)
            .ok_or_else(|| PeekabooXError::new("patch offset overflows usize"))?;
        data.extend_from_slice(
            frame
                .data
                .get(offset..offset + patch_stride)
                .ok_or_else(|| PeekabooXError::new("patch region exceeds frame data"))?,
        );
    }

    let patch_stride = u32::try_from(patch_stride)
        .map_err(|_| PeekabooXError::new("patch stride overflows u32"))?;
    Ok((patch_stride, data))
}

fn pixel_compare_channels(
    frame: &CaptureFrame,
    x: u32,
    y: u32,
    alpha_mode: VisualAlphaMode,
) -> Result<[u8; 4]> {
    let pixel = pixel_rgba(frame, x, y)?;
    Ok(match alpha_mode {
        VisualAlphaMode::Ignore => [pixel[0], pixel[1], pixel[2], 0],
        VisualAlphaMode::Compare => pixel,
    })
}

fn pixel_rgba(frame: &CaptureFrame, x: u32, y: u32) -> Result<[u8; 4]> {
    let bytes_per_pixel = bytes_per_pixel(frame.format);
    let offset = usize::try_from(y)
        .ok()
        .and_then(|row| row.checked_mul(usize::try_from(frame.stride).ok()?))
        .and_then(|row_offset| {
            usize::try_from(x)
                .ok()
                .and_then(|column| column.checked_mul(bytes_per_pixel))
                .and_then(|column_offset| row_offset.checked_add(column_offset))
        })
        .ok_or_else(|| PeekabooXError::new("pixel offset overflows usize"))?;
    let pixel = frame
        .data
        .get(offset..offset + bytes_per_pixel)
        .ok_or_else(|| PeekabooXError::new("pixel offset exceeds frame data"))?;

    Ok(match frame.format {
        PixelFormat::Bgra8 => [pixel[2], pixel[1], pixel[0], pixel[3]],
        PixelFormat::Rgba8 => [pixel[0], pixel[1], pixel[2], pixel[3]],
        PixelFormat::Rgb8 => [pixel[0], pixel[1], pixel[2], 255],
    })
}

fn pixel_rgb(frame: &CaptureFrame, x: u32, y: u32) -> Result<[u8; 3]> {
    let pixel = pixel_rgba(frame, x, y)?;
    Ok([pixel[0], pixel[1], pixel[2]])
}

fn bytes_per_pixel(format: PixelFormat) -> usize {
    match format {
        PixelFormat::Bgra8 | PixelFormat::Rgba8 => 4,
        PixelFormat::Rgb8 => 3,
    }
}

fn region_end_i32(origin: i32, length: u32) -> Result<i32> {
    i32::try_from(i64::from(origin) + i64::from(length))
        .map_err(|_| PeekabooXError::new("visual comparison region overflows i32"))
}

fn point_is_ignored(ignore_regions: &[Rect], x: i32, y: i32) -> Result<bool> {
    for region in ignore_regions {
        if rect_contains_point(*region, x, y)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn rect_contains_point(region: Rect, x: i32, y: i32) -> Result<bool> {
    let right = region_end_i32(region.x, region.width)?;
    let bottom = region_end_i32(region.y, region.height)?;

    Ok(x >= region.x && x < right && y >= region.y && y < bottom)
}

fn capture_to_temp_file(path: &Path) -> Result<()> {
    peekaboox_capture::capture_screen_to_file(path).map(|_| ())
}

fn capture_temp_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "peekaboox-ocr-{}-{}.png",
        std::process::id(),
        monotonic_ms()
    ))
}

fn ocr_preprocessed_temp_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "peekaboox-ocr-preprocessed-{}-{}.png",
        std::process::id(),
        monotonic_ms()
    ))
}

fn remove_temp_file(path: &Path) {
    if let Err(error) = std::fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        eprintln!(
            "failed to remove OCR temporary image {}: {error}",
            path.display()
        );
    }
}

fn monotonic_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

fn command_exists(command: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };

    std::env::split_paths(&paths).any(|path| path.join(command).is_file())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        HeuristicVisionBackend, IncrementalCaptureOptions, OcrBackend, OcrOptions,
        TesseractOcrBackend, UiElementDetectionOptions, UiElementSort, UiStateKind, UiStateOptions,
        VisionBackend, VisualAlphaMode, VisualCompareOptions, VisualSizePolicy, compare_frames,
        compare_image_files, detect_ui_elements, detect_ui_elements_from_image_file,
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
        let result =
            tesseract_result_from_tsv(SAMPLE_TSV, Some(Rect::new(0, 40, 100, 30))).unwrap();

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

        let error =
            compare_frames(&expected, &actual, &VisualCompareOptions::default()).unwrap_err();

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

        let diff =
            compare_image_files(&baseline, &changed, &VisualCompareOptions::default()).unwrap();
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
            incremental_capture_delta(None, &frame, 7, &IncrementalCaptureOptions::default())
                .unwrap();

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
                let offset = (u32::try_from(y).unwrap() * frame.stride
                    + u32::try_from(x).unwrap() * 3) as usize;
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
}
