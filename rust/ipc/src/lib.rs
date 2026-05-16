use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use peekaboox_core::{DesktopState, Point, Rect, Result, UiElement, WindowInfo};
use serde::{Deserialize, Serialize};

pub const API_VERSION: &str = "peekaboox.v1";
pub const DEFAULT_SOCKET_NAME: &str = "peekabooxd.sock";

fn default_visual_size_policy() -> String {
    "error".to_owned()
}

fn default_visual_alpha_mode() -> String {
    "ignore".to_owned()
}

fn default_ui_element_sort() -> String {
    "position".to_owned()
}

fn default_move_bounds_policy() -> String {
    "allow".to_owned()
}

fn default_input_backend() -> String {
    "auto".to_owned()
}

pub mod proto {
    tonic::include_proto!("peekaboox.v1");
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureTarget {
    FullScreen,
    Region(Rect),
    Window(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClickRequest {
    pub position: Option<Point>,
    pub semantic_selector: Option<String>,
    pub window_selector: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeTextRequest {
    pub text: String,
    pub typing_speed_chars_per_second: Option<u32>,
}

pub trait PeekabooXApi {
    fn list_windows(&self) -> Result<Vec<WindowInfo>>;
    fn find_element(&self, selector: &str) -> Result<Option<UiElement>>;
    fn get_desktop_state(&self) -> Result<DesktopState>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiRequestEnvelope {
    pub version: String,
    pub request: ApiRequest,
}

impl ApiRequestEnvelope {
    pub fn new(request: ApiRequest) -> Self {
        Self {
            version: API_VERSION.to_owned(),
            request,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum ApiRequest {
    Ping,
    Capture {
        output: String,
        #[serde(default)]
        region: Option<RectDto>,
        #[serde(default)]
        window_id: Option<String>,
        #[serde(default)]
        app: Option<String>,
        #[serde(default)]
        window_title: Option<String>,
        #[serde(default)]
        title_regex: Option<String>,
        #[serde(default)]
        format: Option<String>,
        #[serde(default)]
        no_overwrite: bool,
        #[serde(default)]
        include_semantic_tree: bool,
    },
    CaptureDelta {
        #[serde(default)]
        stream_id: Option<String>,
        #[serde(default)]
        reset: bool,
        #[serde(default)]
        region: Option<RectDto>,
        #[serde(default)]
        window_id: Option<String>,
        #[serde(default)]
        per_channel_threshold: u8,
        #[serde(default = "default_low_bandwidth")]
        low_bandwidth: bool,
    },
    CaptureBackends {
        #[serde(default = "default_capture_backends_output")]
        output: String,
        #[serde(default)]
        region: Option<RectDto>,
        #[serde(default)]
        diagnose: bool,
        #[serde(default)]
        probe: CaptureBackendProbeDto,
    },
    #[serde(rename = "probe_dmabuf")]
    ProbeDmaBuf {
        #[serde(default)]
        import_target: DmaBufImportTargetDto,
    },
    ListPlugins {
        #[serde(default)]
        paths: Vec<String>,
    },
    CallPluginTool {
        plugin_id: String,
        tool: String,
        #[serde(default)]
        arguments: serde_json::Value,
        #[serde(default)]
        paths: Vec<String>,
        #[serde(default = "default_plugin_timeout_ms")]
        timeout_ms: u64,
        #[serde(default = "default_plugin_max_output_bytes")]
        max_output_bytes: usize,
    },
    Click {
        x: i32,
        y: i32,
        button: MouseButtonDto,
        dry_run: bool,
    },
    MoveMouse {
        x: i32,
        y: i32,
        #[serde(default)]
        dry_run: bool,
        #[serde(default)]
        duration_ms: u64,
        #[serde(default)]
        steps: Option<u32>,
        #[serde(default = "default_move_bounds_policy")]
        bounds_policy: String,
        #[serde(default = "default_input_backend")]
        backend: String,
        #[serde(default)]
        restore: bool,
    },
    Drag {
        from_x: i32,
        from_y: i32,
        to_x: i32,
        to_y: i32,
        #[serde(default)]
        button: MouseButtonDto,
        #[serde(default = "default_drag_duration_ms")]
        duration_ms: u32,
        #[serde(default)]
        steps: Option<u32>,
        #[serde(default = "default_move_bounds_policy")]
        bounds_policy: String,
        #[serde(default = "default_input_backend")]
        backend: String,
        #[serde(default)]
        restore: bool,
        #[serde(default)]
        dry_run: bool,
    },
    TypeText {
        text: String,
        dry_run: bool,
    },
    PasteText {
        text: String,
        #[serde(default)]
        preserve_clipboard: bool,
        #[serde(default)]
        dry_run: bool,
    },
    Hotkey {
        keys: Vec<String>,
        #[serde(default)]
        dry_run: bool,
    },
    ListWindows {
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        app: Option<String>,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        title_regex: Option<String>,
        #[serde(default)]
        focused: bool,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        sort: Option<String>,
        #[serde(default)]
        backend: Option<String>,
        #[serde(default)]
        diagnose: bool,
    },
    FindElements {
        selector: String,
        #[serde(default)]
        vision_fallback: bool,
        #[serde(default)]
        app: Option<String>,
        #[serde(default)]
        window_title: Option<String>,
        #[serde(default)]
        window_id: Option<String>,
        #[serde(default)]
        vision_region: Option<RectDto>,
        #[serde(default)]
        vision_edge_threshold: Option<u8>,
        #[serde(default)]
        vision_min_width: Option<u32>,
        #[serde(default)]
        vision_min_height: Option<u32>,
        #[serde(default)]
        vision_min_component_pixels: Option<u32>,
        #[serde(default)]
        vision_max_elements: Option<u32>,
        #[serde(default)]
        vision_merge_distance: Option<u32>,
    },
    Ocr {
        #[serde(default)]
        image_path: Option<String>,
        #[serde(default)]
        region: Option<RectDto>,
        #[serde(default)]
        app: Option<String>,
        #[serde(default)]
        window_title: Option<String>,
        #[serde(default)]
        window_id: Option<String>,
        #[serde(default)]
        language: Option<String>,
        #[serde(default)]
        page_segmentation_mode: Option<u8>,
        #[serde(default)]
        engine_mode: Option<u8>,
        #[serde(default)]
        dpi: Option<u32>,
        #[serde(default)]
        min_confidence: Option<f32>,
        #[serde(default)]
        whitelist: Option<String>,
        #[serde(default)]
        config: Vec<String>,
        #[serde(default)]
        scale: Option<f32>,
        #[serde(default)]
        grayscale: bool,
        #[serde(default)]
        threshold: Option<u8>,
        #[serde(default)]
        invert: bool,
        #[serde(default)]
        contrast: Option<f32>,
        #[serde(default)]
        deskew: bool,
    },
    CompareImages {
        expected_path: String,
        actual_path: String,
        #[serde(default)]
        region: Option<RectDto>,
        #[serde(default)]
        ignore_regions: Vec<RectDto>,
        per_channel_threshold: u8,
        max_changed_ratio: f32,
        #[serde(default)]
        max_changed_pixels: Option<u64>,
        #[serde(default)]
        max_mean_absolute_error: Option<f32>,
        #[serde(default)]
        max_channel_delta: Option<u8>,
        #[serde(default = "default_visual_size_policy")]
        size_policy: String,
        #[serde(default = "default_visual_alpha_mode")]
        alpha_mode: String,
        #[serde(default)]
        diff_output: Option<String>,
    },
    DetectUiState {
        image_paths: Vec<String>,
        #[serde(default)]
        region: Option<RectDto>,
        #[serde(default)]
        ignore_regions: Vec<RectDto>,
        per_channel_threshold: u8,
        stable_max_changed_ratio: f32,
        #[serde(default)]
        stable_max_changed_pixels: Option<u64>,
        #[serde(default)]
        stable_max_mean_absolute_error: Option<f32>,
        #[serde(default)]
        stable_max_channel_delta: Option<u8>,
        loading_min_changed_ratio: f32,
        #[serde(default)]
        loading_min_changed_pixels: Option<u64>,
        required_stable_transitions: u32,
        #[serde(default = "default_visual_size_policy")]
        size_policy: String,
        #[serde(default = "default_visual_alpha_mode")]
        alpha_mode: String,
    },
    DetectUiElements {
        image_path: String,
        #[serde(default)]
        region: Option<RectDto>,
        #[serde(default)]
        ignore_regions: Vec<RectDto>,
        edge_threshold: u8,
        min_width: u32,
        min_height: u32,
        min_component_pixels: u32,
        #[serde(default)]
        min_confidence: Option<f32>,
        #[serde(default)]
        max_width: Option<u32>,
        #[serde(default)]
        max_height: Option<u32>,
        #[serde(default)]
        min_area: Option<u64>,
        #[serde(default)]
        max_area: Option<u64>,
        max_elements: u32,
        merge_distance: u32,
        #[serde(default)]
        padding: u32,
        #[serde(default = "default_ui_element_sort")]
        sort: String,
        #[serde(default)]
        mask_output_path: Option<String>,
        #[serde(default)]
        overlay_output_path: Option<String>,
    },
    DesktopFocus {
        app: String,
        #[serde(default = "default_true")]
        use_gnome_overview: bool,
        #[serde(default = "default_true")]
        launch_if_needed: bool,
        #[serde(default = "default_desktop_focus_wait_ms")]
        wait_after_focus_ms: u64,
        #[serde(default = "default_desktop_overview_wait_ms")]
        overview_wait_ms: u64,
        #[serde(default)]
        window_title: Option<String>,
        #[serde(default)]
        window_id: Option<String>,
        #[serde(default)]
        verify: bool,
    },
    DesktopLocate {
        app: String,
        target: String,
        #[serde(default)]
        image_path: Option<String>,
        #[serde(default = "default_true")]
        prefer_accessibility: bool,
        #[serde(default)]
        window_title: Option<String>,
        #[serde(default)]
        window_id: Option<String>,
    },
    DesktopClick {
        app: String,
        target: String,
        #[serde(default)]
        image_path: Option<String>,
        #[serde(default = "default_true")]
        prefer_accessibility: bool,
        #[serde(default)]
        window_title: Option<String>,
        #[serde(default)]
        button: MouseButtonDto,
        #[serde(default)]
        dry_run: bool,
        #[serde(default)]
        window_id: Option<String>,
        #[serde(default)]
        verify: bool,
    },
    DesktopDrag {
        app: String,
        target: String,
        #[serde(default)]
        image_path: Option<String>,
        #[serde(default = "default_true")]
        prefer_accessibility: bool,
        #[serde(default)]
        window_title: Option<String>,
        #[serde(default)]
        button: MouseButtonDto,
        #[serde(default = "default_desktop_drag_ratio")]
        from_ratio_x: f32,
        #[serde(default = "default_desktop_drag_ratio")]
        from_ratio_y: f32,
        #[serde(default = "default_desktop_drag_ratio")]
        to_ratio_x: f32,
        #[serde(default = "default_desktop_drag_ratio")]
        to_ratio_y: f32,
        #[serde(default = "default_desktop_drag_duration_ms")]
        duration_ms: u64,
        #[serde(default)]
        dry_run: bool,
        #[serde(default)]
        window_id: Option<String>,
        #[serde(default)]
        verify: bool,
    },
    DesktopTypeInto {
        app: String,
        target: String,
        text: String,
        #[serde(default)]
        image_path: Option<String>,
        #[serde(default = "default_true")]
        prefer_accessibility: bool,
        #[serde(default)]
        window_title: Option<String>,
        #[serde(default)]
        clear: bool,
        #[serde(default)]
        dry_run: bool,
        #[serde(default)]
        window_id: Option<String>,
        #[serde(default)]
        verify: bool,
    },
    DesktopAssert {
        app: String,
        target: String,
        #[serde(default)]
        image_path: Option<String>,
        #[serde(default = "default_true")]
        prefer_accessibility: bool,
        #[serde(default)]
        window_title: Option<String>,
        #[serde(default)]
        assertion: DesktopAssertionDto,
        #[serde(default)]
        expected_text: Option<String>,
        #[serde(default)]
        window_id: Option<String>,
    },
    DesktopProfiles {
        #[serde(default)]
        app: Option<String>,
        #[serde(default)]
        target: Option<String>,
        #[serde(default)]
        command: Option<String>,
        #[serde(default)]
        desktop_id: Option<String>,
        #[serde(default)]
        supports: Option<String>,
        #[serde(default)]
        check: bool,
        #[serde(default)]
        installed: bool,
        #[serde(default)]
        available: bool,
    },
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButtonDto {
    #[default]
    Left,
    Middle,
    Right,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopAssertionDto {
    #[default]
    Present,
    NotPresent,
    Active,
    NotActive,
    Contains,
    NotContains,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiResponseEnvelope {
    pub version: String,
    pub response: ApiResponse,
}

impl ApiResponseEnvelope {
    pub fn ok(result: ApiResult) -> Self {
        Self {
            version: API_VERSION.to_owned(),
            response: ApiResponse::Ok {
                result: Box::new(result),
            },
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            version: API_VERSION.to_owned(),
            response: ApiResponse::Error {
                message: message.into(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ApiResponse {
    Ok { result: Box<ApiResult> },
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "data", rename_all = "snake_case")]
pub enum ApiResult {
    Pong,
    Capture(CaptureResultDto),
    CaptureDelta(CaptureDeltaResultDto),
    CaptureBackends(CaptureBackendsResultDto),
    #[serde(rename = "dmabuf_probe")]
    DmaBufProbe(DmaBufProbeResultDto),
    Plugins(PluginListResultDto),
    PluginToolExecution(PluginToolExecutionResultDto),
    Click(ActionResultDto),
    MoveMouse(ActionResultDto),
    Drag(ActionResultDto),
    TypeText(ActionResultDto),
    PasteText(ActionResultDto),
    Hotkey(ActionResultDto),
    ListWindows(WindowListResultDto),
    FindElements(ElementListResultDto),
    Ocr(OcrResultDto),
    VisualDiff(VisualDiffDto),
    UiState(UiStateDto),
    DetectUiElements(ElementListResultDto),
    DesktopAction(DesktopActionResultDto),
    DesktopLocate(DesktopLocateResultDto),
    DesktopProfiles(DesktopProfilesResultDto),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptureResultDto {
    pub output_path: String,
    pub backend_name: String,
    pub backend_kind: String,
    pub bytes_written: u64,
    pub width: u32,
    pub height: u32,
    pub mime_type: String,
    pub capture_region: Option<RectDto>,
    pub window_id: Option<String>,
    pub window: Option<WindowDto>,
    pub captured_at_unix_ms: u64,
    pub source: String,
    pub semantic_tree: Vec<ElementDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptureDeltaResultDto {
    pub stream_id: String,
    pub sequence: u64,
    pub low_bandwidth: bool,
    pub frame_width: u32,
    pub frame_height: u32,
    pub pixel_format: String,
    pub full_frame: bool,
    pub capture_region: Option<RectDto>,
    pub changed_bounds: Option<RectDto>,
    pub changed_pixels: u64,
    pub changed_ratio: f32,
    pub patch_stride: u32,
    pub patch_base64: String,
    pub backend_name: String,
    pub backend_kind: String,
    pub captured_at_unix_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CaptureBackendProbeDto {
    #[default]
    None,
    File,
    Frame,
    Region,
    DmaBuf,
    All,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptureBackendsResultDto {
    pub session_type: String,
    pub desktop: Option<String>,
    pub pipewire_session_available: bool,
    pub pipewire_backend_feature_enabled: bool,
    pub egl_backend_feature_enabled: bool,
    pub output_path: String,
    pub region: Option<RectDto>,
    pub image_backends: Vec<CaptureBackendDto>,
    pub zero_copy_backends: Vec<ZeroCopyBackendDto>,
    #[serde(default)]
    pub probes: Vec<CaptureBackendProbeResultDto>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureBackendDto {
    pub name: String,
    pub backend_kind: String,
    pub command: Option<String>,
    pub available: bool,
    pub supports_output: bool,
    pub supports_file_capture: bool,
    pub supports_stdout_capture: bool,
    pub supports_stdout_region_capture: bool,
    pub selected: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZeroCopyBackendDto {
    pub name: String,
    pub backend_kind: String,
    pub transport: String,
    pub availability: String,
    pub selected: bool,
    pub pipewire_backend_feature_enabled: bool,
    pub egl_backend_feature_enabled: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptureBackendProbeResultDto {
    pub probe: String,
    pub ok: bool,
    pub backend_name: Option<String>,
    pub backend_kind: Option<String>,
    pub detail: String,
    pub output_path: Option<String>,
    pub bytes_written: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DmaBufImportTargetDto {
    #[default]
    Compute,
    Egl,
    EglTexture,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DmaBufProbeResultDto {
    pub import_target: DmaBufImportTargetDto,
    pub backend_name: String,
    pub stream_node_id: u32,
    pub pipewire_serial: Option<u64>,
    pub width: u32,
    pub height: u32,
    pub pixel_format: String,
    pub fourcc: u32,
    pub planes: usize,
    pub memory_layout: String,
    pub synchronization: String,
    pub egl_version: Option<String>,
    pub egl_modifiers: Option<bool>,
    pub texture_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginListResultDto {
    pub sdk_version: String,
    pub plugins: Vec<PluginDto>,
    pub errors: Vec<PluginDiscoveryErrorDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginDto {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub root_dir: String,
    pub manifest_path: String,
    pub capabilities: Vec<String>,
    pub entrypoint_kind: Option<String>,
    pub entrypoint_command: Vec<String>,
    pub tools: Vec<PluginToolDto>,
    pub metadata: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginToolDto {
    pub name: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub input_schema_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginDiscoveryErrorDto {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginToolExecutionResultDto {
    pub ok: bool,
    pub plugin_id: String,
    pub tool: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionResultDto {
    pub backend_name: String,
    pub backend_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopActionResultDto {
    pub app: String,
    pub action: String,
    pub detail: String,
    pub backend_name: String,
    #[serde(default)]
    pub verified: bool,
    #[serde(default)]
    pub verification_detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopLocateResultDto {
    pub app: String,
    pub target: String,
    pub point: PointDto,
    pub rect: Option<RectDto>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopProfilesResultDto {
    pub schema_version: String,
    pub count: usize,
    pub profiles: Vec<DesktopProfileDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopProfileDto {
    pub id: String,
    pub aliases: Vec<String>,
    pub search_name: String,
    pub desktop_ids: Vec<String>,
    pub commands: Vec<DesktopProfileCommandDto>,
    pub targets: Vec<DesktopProfileTargetDto>,
    pub availability: DesktopProfileAvailabilityDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopProfileCommandDto {
    pub program: String,
    pub args: Vec<String>,
    pub display: String,
    pub available: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopProfileTargetDto {
    pub name: String,
    pub supports: Vec<String>,
    pub sources: Vec<String>,
    pub can_locate: bool,
    pub can_click: bool,
    pub can_drag: bool,
    pub can_type: bool,
    pub can_assert_present: bool,
    pub can_assert_active: bool,
    pub can_assert_contains: bool,
    pub accessibility_selector: Option<String>,
    pub visual_layout: bool,
    pub visual_rect: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopProfileAvailabilityDto {
    pub checked: bool,
    pub installed: Option<bool>,
    pub command_available: Option<bool>,
    pub desktop_entry_available: Option<bool>,
    pub available_commands: Vec<String>,
    pub available_desktop_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PointDto {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowListResultDto {
    pub backend_name: String,
    pub backend_kind: String,
    pub warnings: Vec<String>,
    #[serde(default)]
    pub backend_reports: Vec<WindowBackendReportDto>,
    pub windows: Vec<WindowDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowBackendReportDto {
    pub backend_name: String,
    pub backend_kind: String,
    pub raw_window_count: usize,
    pub matched_window_count: usize,
    pub selected: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowDto {
    pub id: String,
    pub title: String,
    pub app_id: Option<String>,
    pub bounds: RectDto,
    pub focused: bool,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementListResultDto {
    pub backend_name: String,
    pub backend_kind: String,
    pub warnings: Vec<String>,
    #[serde(default)]
    pub cache_hit: bool,
    #[serde(default)]
    pub cache_age_ms: u128,
    #[serde(default)]
    pub vision_fallback_used: bool,
    pub elements: Vec<ElementDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementDto {
    pub id: String,
    pub role: String,
    pub label: Option<String>,
    pub bounds: RectDto,
    pub center: Option<PointDto>,
    pub confidence: f32,
    pub states: Vec<String>,
    pub window_id: Option<String>,
    pub window_title: Option<String>,
    pub app_id: Option<String>,
    pub parent_id: Option<String>,
    pub child_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcrResultDto {
    pub backend_name: String,
    pub text: String,
    pub blocks: Vec<OcrBlockDto>,
    pub words: Vec<OcrBlockDto>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcrBlockDto {
    pub text: String,
    pub element: ElementDto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisualDiffDto {
    pub compared_region: RectDto,
    pub compared_pixels: u64,
    pub changed_pixels: u64,
    pub changed_ratio: f32,
    pub mean_absolute_error: f32,
    pub max_channel_delta: u8,
    pub changed_bounds: Option<RectDto>,
    pub matches: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiStateDto {
    pub state: String,
    pub compared_transitions: u64,
    pub stable_transitions: u64,
    pub loading_transitions: u64,
    pub trailing_stable_transitions: u64,
    pub latest_diff: VisualDiffDto,
    pub max_changed_ratio: f32,
    pub mean_changed_ratio: f32,
    pub changed_bounds: Option<RectDto>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RectDto {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl From<Rect> for RectDto {
    fn from(rect: Rect) -> Self {
        Self {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        }
    }
}

impl From<RectDto> for Rect {
    fn from(rect: RectDto) -> Self {
        Self::new(rect.x, rect.y, rect.width, rect.height)
    }
}

impl From<Point> for PointDto {
    fn from(point: Point) -> Self {
        Self {
            x: point.x,
            y: point.y,
        }
    }
}

impl From<PointDto> for Point {
    fn from(point: PointDto) -> Self {
        Self::new(point.x, point.y)
    }
}

impl From<&UiElement> for ElementDto {
    fn from(element: &UiElement) -> Self {
        Self {
            id: element.id.clone(),
            role: element.role.clone(),
            label: element.label.clone(),
            bounds: RectDto::from(element.bounds),
            center: element
                .center
                .or_else(|| element.bounds.center())
                .map(PointDto::from),
            confidence: element.confidence,
            states: element.states.clone(),
            window_id: element.window_id.clone(),
            window_title: element.window_title.clone(),
            app_id: element.app_id.clone(),
            parent_id: element.parent_id.clone(),
            child_ids: element.child_ids.clone(),
        }
    }
}

impl From<&WindowInfo> for WindowDto {
    fn from(window: &WindowInfo) -> Self {
        Self {
            id: window.id.clone(),
            title: window.title.clone(),
            app_id: window.app_id.clone(),
            bounds: RectDto::from(window.bounds),
            focused: window.focused,
            state: format!("{:?}", window.state).to_ascii_lowercase(),
        }
    }
}

pub fn default_socket_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(DEFAULT_SOCKET_NAME)
}

pub fn send_request(
    socket_path: impl AsRef<Path>,
    request: ApiRequest,
) -> std::io::Result<ApiResponseEnvelope> {
    let mut stream = UnixStream::connect(socket_path)?;
    let payload = serde_json::to_vec(&ApiRequestEnvelope::new(request)).map_err(to_io_error)?;
    stream.write_all(&payload)?;
    stream.write_all(b"\n")?;
    stream.shutdown(std::net::Shutdown::Write)?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;

    serde_json::from_str(response.trim()).map_err(to_io_error)
}

pub fn encode_response(response: &ApiResponseEnvelope) -> std::io::Result<Vec<u8>> {
    let mut payload = serde_json::to_vec(response).map_err(to_io_error)?;
    payload.push(b'\n');
    Ok(payload)
}

pub fn decode_request(payload: &str) -> std::io::Result<ApiRequestEnvelope> {
    serde_json::from_str(payload.trim()).map_err(to_io_error)
}

fn to_io_error(error: serde_json::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, error)
}

fn default_low_bandwidth() -> bool {
    true
}

fn default_capture_backends_output() -> String {
    "screenshot.png".to_owned()
}

fn default_true() -> bool {
    true
}

fn default_drag_duration_ms() -> u32 {
    250
}

fn default_desktop_drag_duration_ms() -> u64 {
    250
}

fn default_desktop_drag_ratio() -> f32 {
    0.5
}

fn default_desktop_focus_wait_ms() -> u64 {
    1_000
}

fn default_desktop_overview_wait_ms() -> u64 {
    800
}

fn default_plugin_timeout_ms() -> u64 {
    10_000
}

fn default_plugin_max_output_bytes() -> usize {
    1_048_576
}

#[cfg(test)]
mod tests {
    use super::{
        API_VERSION, ApiRequest, ApiRequestEnvelope, ApiResponse, ApiResponseEnvelope, ApiResult,
        CaptureBackendProbeDto, DesktopAssertionDto, DmaBufImportTargetDto, DmaBufProbeResultDto,
        MouseButtonDto, PluginDiscoveryErrorDto, PluginDto, PluginListResultDto, PluginToolDto,
        PluginToolExecutionResultDto, decode_request, default_socket_path, encode_response,
    };

    #[test]
    fn api_version_is_namespaced() {
        assert_eq!(API_VERSION, "peekaboox.v1");
    }

    #[test]
    fn request_round_trips_as_json() {
        let request = ApiRequestEnvelope::new(ApiRequest::Click {
            x: 10,
            y: 20,
            button: MouseButtonDto::Left,
            dry_run: true,
        });
        let payload = serde_json::to_string(&request).unwrap();

        assert_eq!(decode_request(&payload).unwrap(), request);
    }

    #[test]
    fn move_mouse_request_round_trips_as_json() {
        let request = ApiRequestEnvelope::new(ApiRequest::MoveMouse {
            x: 10,
            y: 20,
            dry_run: true,
            duration_ms: 120,
            steps: Some(6),
            bounds_policy: "clamp".to_owned(),
            backend: "xdotool".to_owned(),
            restore: true,
        });
        let payload = serde_json::to_string(&request).unwrap();

        assert_eq!(decode_request(&payload).unwrap(), request);
    }

    #[test]
    fn drag_request_defaults_button_duration_and_dry_run() {
        let payload = r#"{"version":"peekaboox.v1","request":{"method":"drag","from_x":10,"from_y":20,"to_x":30,"to_y":40}}"#;
        let request = decode_request(payload).unwrap();

        assert_eq!(
            request,
            ApiRequestEnvelope::new(ApiRequest::Drag {
                from_x: 10,
                from_y: 20,
                to_x: 30,
                to_y: 40,
                button: MouseButtonDto::Left,
                duration_ms: 250,
                steps: None,
                bounds_policy: "allow".to_owned(),
                backend: "auto".to_owned(),
                restore: false,
                dry_run: false,
            })
        );
    }

    #[test]
    fn hotkey_request_round_trips_as_json() {
        let request = ApiRequestEnvelope::new(ApiRequest::Hotkey {
            keys: vec!["ctrl".to_owned(), "s".to_owned()],
            dry_run: true,
        });
        let payload = serde_json::to_string(&request).unwrap();

        assert_eq!(decode_request(&payload).unwrap(), request);
    }

    #[test]
    fn paste_text_request_round_trips_as_json() {
        let request = ApiRequestEnvelope::new(ApiRequest::PasteText {
            text: "hello".to_owned(),
            preserve_clipboard: true,
            dry_run: true,
        });
        let payload = serde_json::to_string(&request).unwrap();

        assert!(payload.contains(r#""method":"paste_text""#));
        assert!(payload.contains(r#""preserve_clipboard":true"#));
        assert_eq!(decode_request(&payload).unwrap(), request);
    }

    #[test]
    fn ocr_request_round_trips_as_json() {
        let request = ApiRequestEnvelope::new(ApiRequest::Ocr {
            image_path: Some("tests/fixtures/ocr/sample.png".to_owned()),
            region: Some(super::RectDto {
                x: 10,
                y: 20,
                width: 100,
                height: 40,
            }),
            app: Some("text-editor".to_owned()),
            window_title: Some("Invoice".to_owned()),
            window_id: Some("window-1".to_owned()),
            language: Some("eng".to_owned()),
            page_segmentation_mode: Some(6),
            engine_mode: Some(1),
            dpi: Some(300),
            min_confidence: Some(0.5),
            whitelist: Some("ABC123".to_owned()),
            config: vec!["preserve_interword_spaces=1".to_owned()],
            scale: Some(2.0),
            grayscale: true,
            threshold: Some(180),
            invert: true,
            contrast: Some(10.0),
            deskew: true,
        });
        let payload = serde_json::to_string(&request).unwrap();

        assert_eq!(decode_request(&payload).unwrap(), request);
    }

    #[test]
    fn capture_delta_request_round_trips_as_json() {
        let request = ApiRequestEnvelope::new(ApiRequest::CaptureDelta {
            stream_id: Some("agent-loop".to_owned()),
            reset: true,
            region: Some(super::RectDto {
                x: 10,
                y: 20,
                width: 100,
                height: 40,
            }),
            window_id: None,
            per_channel_threshold: 2,
            low_bandwidth: true,
        });
        let payload = serde_json::to_string(&request).unwrap();

        assert_eq!(decode_request(&payload).unwrap(), request);
    }

    #[test]
    fn capture_delta_request_defaults_stream_and_reset() {
        let payload = r#"{"version":"peekaboox.v1","request":{"method":"capture_delta"}}"#;
        let request = decode_request(payload).unwrap();

        assert_eq!(
            request,
            ApiRequestEnvelope::new(ApiRequest::CaptureDelta {
                stream_id: None,
                reset: false,
                region: None,
                window_id: None,
                per_channel_threshold: 0,
                low_bandwidth: true,
            })
        );
    }

    #[test]
    fn capture_backends_request_round_trips_as_json() {
        let request = ApiRequestEnvelope::new(ApiRequest::CaptureBackends {
            output: "target/backends/screen.xwd".to_owned(),
            region: Some(super::RectDto {
                x: 0,
                y: 0,
                width: 320,
                height: 180,
            }),
            diagnose: true,
            probe: CaptureBackendProbeDto::All,
        });
        let payload = serde_json::to_string(&request).unwrap();

        assert!(payload.contains(r#""method":"capture_backends""#));
        assert!(payload.contains(r#""probe":"all""#));
        assert_eq!(decode_request(&payload).unwrap(), request);
    }

    #[test]
    fn capture_backends_request_defaults_output_region_and_probe() {
        let payload = r#"{"version":"peekaboox.v1","request":{"method":"capture_backends"}}"#;
        let request = decode_request(payload).unwrap();

        assert_eq!(
            request,
            ApiRequestEnvelope::new(ApiRequest::CaptureBackends {
                output: "screenshot.png".to_owned(),
                region: None,
                diagnose: false,
                probe: CaptureBackendProbeDto::None,
            })
        );
    }

    #[test]
    fn capture_request_round_trips_region_and_window_target_fields() {
        let request = ApiRequestEnvelope::new(ApiRequest::Capture {
            output: "screen.png".to_owned(),
            region: Some(super::RectDto {
                x: 10,
                y: 20,
                width: 100,
                height: 40,
            }),
            window_id: None,
            app: None,
            window_title: None,
            title_regex: None,
            format: None,
            no_overwrite: false,
            include_semantic_tree: false,
        });
        let payload = serde_json::to_string(&request).unwrap();

        assert!(payload.contains(r#""method":"capture""#));
        assert!(payload.contains(r#""region""#));
        assert_eq!(decode_request(&payload).unwrap(), request);

        let window_request = ApiRequestEnvelope::new(ApiRequest::Capture {
            output: "window.png".to_owned(),
            region: None,
            window_id: Some("window-1".to_owned()),
            app: Some("calculator".to_owned()),
            window_title: Some("Calculator".to_owned()),
            title_regex: Some("Calc.*".to_owned()),
            format: Some("png".to_owned()),
            no_overwrite: true,
            include_semantic_tree: true,
        });
        let payload = serde_json::to_string(&window_request).unwrap();

        assert!(payload.contains(r#""window_id":"window-1""#));
        assert!(payload.contains(r#""app":"calculator""#));
        assert!(payload.contains(r#""window_title":"Calculator""#));
        assert!(payload.contains(r#""title_regex":"Calc.*""#));
        assert!(payload.contains(r#""format":"png""#));
        assert!(payload.contains(r#""no_overwrite":true"#));
        assert!(payload.contains(r#""include_semantic_tree":true"#));
        assert_eq!(decode_request(&payload).unwrap(), window_request);
    }

    #[test]
    fn desktop_request_round_trips_as_json() {
        let request = ApiRequestEnvelope::new(ApiRequest::DesktopTypeInto {
            app: "telegram".to_owned(),
            target: "search-input".to_owned(),
            text: "PeekabooX".to_owned(),
            image_path: None,
            prefer_accessibility: true,
            window_title: Some("Telegram".to_owned()),
            clear: true,
            dry_run: true,
            window_id: Some("window-1".to_owned()),
            verify: true,
        });
        let payload = serde_json::to_string(&request).unwrap();

        assert!(payload.contains(r#""method":"desktop_type_into""#));
        assert!(payload.contains(r#""window_id":"window-1""#));
        assert_eq!(decode_request(&payload).unwrap(), request);
    }

    #[test]
    fn desktop_assert_defaults_to_present() {
        let payload = r#"{"version":"peekaboox.v1","request":{"method":"desktop_assert","app":"telegram","target":"saved-messages"}}"#;
        let request = decode_request(payload).unwrap();

        assert_eq!(
            request,
            ApiRequestEnvelope::new(ApiRequest::DesktopAssert {
                app: "telegram".to_owned(),
                target: "saved-messages".to_owned(),
                image_path: None,
                prefer_accessibility: true,
                window_title: None,
                assertion: DesktopAssertionDto::Present,
                expected_text: None,
                window_id: None,
            })
        );
    }

    #[test]
    fn desktop_drag_defaults_ratios_button_duration_and_dry_run() {
        let payload = r#"{"version":"peekaboox.v1","request":{"method":"desktop_drag","app":"paint","target":"canvas"}}"#;
        let request = decode_request(payload).unwrap();

        assert_eq!(
            request,
            ApiRequestEnvelope::new(ApiRequest::DesktopDrag {
                app: "paint".to_owned(),
                target: "canvas".to_owned(),
                image_path: None,
                prefer_accessibility: true,
                window_title: None,
                button: MouseButtonDto::Left,
                from_ratio_x: 0.5,
                from_ratio_y: 0.5,
                to_ratio_x: 0.5,
                to_ratio_y: 0.5,
                duration_ms: 250,
                dry_run: false,
                window_id: None,
                verify: false,
            })
        );
    }

    #[test]
    fn probe_dmabuf_request_round_trips_as_json() {
        let request = ApiRequestEnvelope::new(ApiRequest::ProbeDmaBuf {
            import_target: DmaBufImportTargetDto::EglTexture,
        });
        let payload = serde_json::to_string(&request).unwrap();

        assert!(payload.contains(r#""method":"probe_dmabuf""#));
        assert!(payload.contains(r#""import_target":"egl_texture""#));
        assert_eq!(decode_request(&payload).unwrap(), request);
    }

    #[test]
    fn list_plugins_request_round_trips_as_json() {
        let request = ApiRequestEnvelope::new(ApiRequest::ListPlugins {
            paths: vec!["examples/plugins".to_owned()],
        });
        let payload = serde_json::to_string(&request).unwrap();

        assert!(payload.contains(r#""method":"list_plugins""#));
        assert!(payload.contains("examples/plugins"));
        assert_eq!(decode_request(&payload).unwrap(), request);
    }

    #[test]
    fn call_plugin_tool_request_defaults_execution_limits() {
        let payload = r#"{"version":"peekaboox.v1","request":{"method":"call_plugin_tool","plugin_id":"demo","tool":"demo.echo","arguments":{"text":"hello"}}}"#;
        let request = decode_request(payload).unwrap();

        assert_eq!(
            request,
            ApiRequestEnvelope::new(ApiRequest::CallPluginTool {
                plugin_id: "demo".to_owned(),
                tool: "demo.echo".to_owned(),
                arguments: serde_json::json!({"text": "hello"}),
                paths: Vec::new(),
                timeout_ms: 10_000,
                max_output_bytes: 1_048_576,
            })
        );
    }

    #[test]
    fn visual_compare_request_round_trips_as_json() {
        let request = ApiRequestEnvelope::new(ApiRequest::CompareImages {
            expected_path: "expected.png".to_owned(),
            actual_path: "actual.png".to_owned(),
            region: Some(super::RectDto {
                x: 10,
                y: 20,
                width: 100,
                height: 40,
            }),
            ignore_regions: vec![super::RectDto {
                x: 1,
                y: 2,
                width: 3,
                height: 4,
            }],
            per_channel_threshold: 2,
            max_changed_ratio: 0.01,
            max_changed_pixels: Some(12),
            max_mean_absolute_error: Some(2.5),
            max_channel_delta: Some(16),
            size_policy: "common-region".to_owned(),
            alpha_mode: "compare".to_owned(),
            diff_output: Some("diff.png".to_owned()),
        });
        let payload = serde_json::to_string(&request).unwrap();

        assert_eq!(decode_request(&payload).unwrap(), request);
    }

    #[test]
    fn find_elements_request_defaults_vision_fallback_to_false() {
        let payload = r#"{"version":"peekaboox.v1","request":{"method":"find_elements","selector":"role=button"}}"#;
        let request = decode_request(payload).unwrap();

        assert_eq!(
            request,
            ApiRequestEnvelope::new(ApiRequest::FindElements {
                selector: "role=button".to_owned(),
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
            })
        );
    }

    #[test]
    fn list_windows_request_defaults_to_unfiltered_query() {
        let payload = r#"{"version":"peekaboox.v1","request":{"method":"list_windows"}}"#;
        let request = decode_request(payload).unwrap();

        assert_eq!(
            request,
            ApiRequestEnvelope::new(ApiRequest::ListWindows {
                id: None,
                app: None,
                title: None,
                title_regex: None,
                focused: false,
                limit: None,
                sort: None,
                backend: None,
                diagnose: false,
            })
        );
    }

    #[test]
    fn list_windows_request_round_trips_as_json() {
        let request = ApiRequestEnvelope::new(ApiRequest::ListWindows {
            id: Some("window-1".to_owned()),
            app: Some("calculator".to_owned()),
            title: None,
            title_regex: Some("Calculator".to_owned()),
            focused: true,
            limit: Some(1),
            sort: Some("focused".to_owned()),
            backend: Some("xdotool".to_owned()),
            diagnose: true,
        });
        let payload = serde_json::to_string(&request).unwrap();

        assert_eq!(decode_request(&payload).unwrap(), request);
    }

    #[test]
    fn ui_state_request_round_trips_as_json() {
        let request = ApiRequestEnvelope::new(ApiRequest::DetectUiState {
            image_paths: vec!["first.png".to_owned(), "second.png".to_owned()],
            region: Some(super::RectDto {
                x: 10,
                y: 20,
                width: 100,
                height: 40,
            }),
            ignore_regions: vec![super::RectDto {
                x: 1,
                y: 2,
                width: 3,
                height: 4,
            }],
            per_channel_threshold: 2,
            stable_max_changed_ratio: 0.001,
            stable_max_changed_pixels: Some(3),
            stable_max_mean_absolute_error: Some(1.5),
            stable_max_channel_delta: Some(8),
            loading_min_changed_ratio: 0.02,
            loading_min_changed_pixels: Some(4),
            required_stable_transitions: 1,
            size_policy: "common-region".to_owned(),
            alpha_mode: "compare".to_owned(),
        });
        let payload = serde_json::to_string(&request).unwrap();

        assert_eq!(decode_request(&payload).unwrap(), request);
    }

    #[test]
    fn detect_ui_elements_request_round_trips_as_json() {
        let request = ApiRequestEnvelope::new(ApiRequest::DetectUiElements {
            image_path: "screen.png".to_owned(),
            region: Some(super::RectDto {
                x: 10,
                y: 20,
                width: 100,
                height: 40,
            }),
            ignore_regions: vec![super::RectDto {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            }],
            edge_threshold: 24,
            min_width: 8,
            min_height: 8,
            min_component_pixels: 12,
            min_confidence: Some(0.75),
            max_width: Some(300),
            max_height: Some(200),
            min_area: Some(64),
            max_area: Some(20_000),
            max_elements: 25,
            merge_distance: 2,
            padding: 3,
            sort: "area".to_owned(),
            mask_output_path: Some("target/mask.png".to_owned()),
            overlay_output_path: Some("target/overlay.png".to_owned()),
        });
        let payload = serde_json::to_string(&request).unwrap();

        assert_eq!(decode_request(&payload).unwrap(), request);
    }

    #[test]
    fn response_is_newline_terminated() {
        let response = ApiResponseEnvelope::ok(ApiResult::Pong);
        let payload = encode_response(&response).unwrap();

        assert_eq!(payload.last(), Some(&b'\n'));
    }

    #[test]
    fn dmabuf_probe_response_uses_public_method_name() {
        let response = ApiResponseEnvelope::ok(ApiResult::DmaBufProbe(DmaBufProbeResultDto {
            import_target: DmaBufImportTargetDto::EglTexture,
            backend_name: "egl-texture-dmabuf-import".to_owned(),
            stream_node_id: 7,
            pipewire_serial: Some(11),
            width: 800,
            height: 600,
            pixel_format: "rgba8".to_owned(),
            fourcc: 0x3432_4152,
            planes: 1,
            memory_layout: "single-plane".to_owned(),
            synchronization: "implicit".to_owned(),
            egl_version: Some("1.5".to_owned()),
            egl_modifiers: Some(true),
            texture_id: Some(3),
        }));
        let payload = String::from_utf8(encode_response(&response).unwrap()).unwrap();

        assert!(payload.contains(r#""method":"dmabuf_probe""#));
        assert!(payload.contains(r#""import_target":"egl_texture""#));
    }

    #[test]
    fn plugin_list_response_round_trips_as_json() {
        let response = ApiResponseEnvelope::ok(ApiResult::Plugins(PluginListResultDto {
            sdk_version: "peekaboox.plugin.v1".to_owned(),
            plugins: vec![PluginDto {
                id: "demo".to_owned(),
                name: "Demo".to_owned(),
                version: "1.0.0".to_owned(),
                description: Some("Demo plugin".to_owned()),
                root_dir: "examples/plugins/demo".to_owned(),
                manifest_path: "examples/plugins/demo/peekaboox.plugin.json".to_owned(),
                capabilities: vec!["observe".to_owned()],
                entrypoint_kind: Some("process".to_owned()),
                entrypoint_command: vec!["python3".to_owned(), "plugin.py".to_owned()],
                tools: vec![PluginToolDto {
                    name: "demo.inspect".to_owned(),
                    description: "Inspect demo state".to_owned(),
                    capabilities: vec!["observe".to_owned()],
                    input_schema_json: r#"{"type":"object"}"#.to_owned(),
                }],
                metadata: Default::default(),
            }],
            errors: vec![PluginDiscoveryErrorDto {
                path: "bad".to_owned(),
                message: "invalid manifest".to_owned(),
            }],
        }));
        let payload = encode_response(&response).unwrap();
        let decoded: ApiResponseEnvelope = serde_json::from_slice(&payload).unwrap();

        assert_eq!(decoded, response);
    }

    #[test]
    fn plugin_execution_response_round_trips_as_json() {
        let response = ApiResponseEnvelope::ok(ApiResult::PluginToolExecution(
            PluginToolExecutionResultDto {
                ok: true,
                plugin_id: "demo".to_owned(),
                tool: "demo.echo".to_owned(),
                exit_code: 0,
                stdout: r#"{"result":{"answer":42}}"#.to_owned(),
                stderr: String::new(),
                result: Some(serde_json::json!({"answer": 42})),
                error: None,
            },
        ));
        let payload = encode_response(&response).unwrap();
        let decoded: ApiResponseEnvelope = serde_json::from_slice(&payload).unwrap();

        assert_eq!(decoded, response);
    }

    #[test]
    fn error_response_carries_message() {
        let response = ApiResponseEnvelope::error("no backend");

        assert_eq!(
            response.response,
            ApiResponse::Error {
                message: "no backend".to_owned()
            }
        );
    }

    #[test]
    fn default_socket_uses_expected_name() {
        assert!(default_socket_path().ends_with(super::DEFAULT_SOCKET_NAME));
    }

    #[test]
    fn protobuf_contract_is_generated() {
        let request = super::proto::ListWindowsRequest::default();

        assert!(request.id.is_none());
    }
}
