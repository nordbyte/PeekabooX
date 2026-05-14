use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use peekaboox_core::{DesktopState, Point, Rect, Result, UiElement, WindowInfo};
use serde::{Deserialize, Serialize};

pub const API_VERSION: &str = "peekaboox.v1";
pub const DEFAULT_SOCKET_NAME: &str = "peekabooxd.sock";

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
    },
    CaptureDelta {
        #[serde(default)]
        stream_id: Option<String>,
        #[serde(default)]
        reset: bool,
        region: Option<RectDto>,
        #[serde(default)]
        per_channel_threshold: u8,
        #[serde(default = "default_low_bandwidth")]
        low_bandwidth: bool,
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
        dry_run: bool,
    },
    TypeText {
        text: String,
        dry_run: bool,
    },
    Hotkey {
        keys: Vec<String>,
        #[serde(default)]
        dry_run: bool,
    },
    ListWindows,
    FindElements {
        selector: String,
        #[serde(default)]
        vision_fallback: bool,
    },
    Ocr {
        region: Option<RectDto>,
        language: Option<String>,
    },
    CompareImages {
        expected_path: String,
        actual_path: String,
        region: Option<RectDto>,
        per_channel_threshold: u8,
        max_changed_ratio: f32,
    },
    DetectUiState {
        image_paths: Vec<String>,
        region: Option<RectDto>,
        per_channel_threshold: u8,
        stable_max_changed_ratio: f32,
        loading_min_changed_ratio: f32,
        required_stable_transitions: u32,
    },
    DetectUiElements {
        image_path: String,
        region: Option<RectDto>,
        edge_threshold: u8,
        min_width: u32,
        min_height: u32,
        min_component_pixels: u32,
        max_elements: u32,
        merge_distance: u32,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiResponseEnvelope {
    pub version: String,
    pub response: ApiResponse,
}

impl ApiResponseEnvelope {
    pub fn ok(result: ApiResult) -> Self {
        Self {
            version: API_VERSION.to_owned(),
            response: ApiResponse::Ok { result },
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
    Ok { result: ApiResult },
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "data", rename_all = "snake_case")]
pub enum ApiResult {
    Pong,
    Capture(CaptureResultDto),
    CaptureDelta(CaptureDeltaResultDto),
    #[serde(rename = "dmabuf_probe")]
    DmaBufProbe(DmaBufProbeResultDto),
    Plugins(PluginListResultDto),
    Click(ActionResultDto),
    MoveMouse(ActionResultDto),
    Drag(ActionResultDto),
    TypeText(ActionResultDto),
    Hotkey(ActionResultDto),
    ListWindows(WindowListResultDto),
    FindElements(ElementListResultDto),
    Ocr(OcrResultDto),
    VisualDiff(VisualDiffDto),
    UiState(UiStateDto),
    DetectUiElements(ElementListResultDto),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureResultDto {
    pub output_path: String,
    pub backend_name: String,
    pub backend_kind: String,
    pub bytes_written: u64,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionResultDto {
    pub backend_name: String,
    pub backend_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowListResultDto {
    pub backend_name: String,
    pub backend_kind: String,
    pub warnings: Vec<String>,
    pub windows: Vec<WindowDto>,
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
    pub elements: Vec<ElementDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementDto {
    pub id: String,
    pub role: String,
    pub label: Option<String>,
    pub bounds: RectDto,
    pub confidence: f32,
    pub states: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcrResultDto {
    pub backend_name: String,
    pub text: String,
    pub blocks: Vec<OcrBlockDto>,
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

impl From<&UiElement> for ElementDto {
    fn from(element: &UiElement) -> Self {
        Self {
            id: element.id.clone(),
            role: element.role.clone(),
            label: element.label.clone(),
            bounds: RectDto::from(element.bounds),
            confidence: element.confidence,
            states: element.states.clone(),
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

fn default_drag_duration_ms() -> u32 {
    250
}

#[cfg(test)]
mod tests {
    use super::{
        API_VERSION, ApiRequest, ApiRequestEnvelope, ApiResponse, ApiResponseEnvelope, ApiResult,
        DmaBufImportTargetDto, DmaBufProbeResultDto, MouseButtonDto, PluginDiscoveryErrorDto,
        PluginDto, PluginListResultDto, PluginToolDto, decode_request, default_socket_path,
        encode_response,
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
    fn ocr_request_round_trips_as_json() {
        let request = ApiRequestEnvelope::new(ApiRequest::Ocr {
            region: Some(super::RectDto {
                x: 10,
                y: 20,
                width: 100,
                height: 40,
            }),
            language: Some("eng".to_owned()),
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
                per_channel_threshold: 0,
                low_bandwidth: true,
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
            per_channel_threshold: 2,
            max_changed_ratio: 0.01,
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
            })
        );
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
            per_channel_threshold: 2,
            stable_max_changed_ratio: 0.001,
            loading_min_changed_ratio: 0.02,
            required_stable_transitions: 1,
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
            edge_threshold: 24,
            min_width: 8,
            min_height: 8,
            min_component_pixels: 12,
            max_elements: 25,
            merge_distance: 2,
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
        let request = super::proto::ListWindowsRequest {};

        assert_eq!(std::mem::size_of_val(&request), 0);
    }
}
