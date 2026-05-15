pub type Result<T> = std::result::Result<T, PeekabooXError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeekabooXError {
    message: String,
}

impl PeekabooXError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for PeekabooXError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PeekabooXError {}

impl From<std::io::Error> for PeekabooXError {
    fn from(error: std::io::Error) -> Self {
        Self::new(error.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Wayland,
    X11,
    Portal,
    AtSpi,
    Uinput,
    Vision,
    Mock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn center(self) -> Option<Point> {
        if self.width == 0 || self.height == 0 {
            return None;
        }

        let x = i64::from(self.x) + i64::from(self.width / 2);
        let y = i64::from(self.y) + i64::from(self.height / 2);

        Some(Point::new(i32::try_from(x).ok()?, i32::try_from(y).ok()?))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureFrame {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: PixelFormat,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Bgra8,
    Rgba8,
    Rgb8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    pub id: String,
    pub title: String,
    pub app_id: Option<String>,
    pub bounds: Rect,
    pub focused: bool,
    pub state: WindowState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowState {
    Normal,
    Minimized,
    Maximized,
    Fullscreen,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiElement {
    pub id: String,
    pub role: String,
    pub label: Option<String>,
    pub bounds: Rect,
    pub center: Option<Point>,
    pub confidence: f32,
    pub states: Vec<String>,
    pub window_id: Option<String>,
    pub window_title: Option<String>,
    pub app_id: Option<String>,
    pub parent_id: Option<String>,
    pub child_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DesktopState {
    pub active_window: Option<WindowInfo>,
    pub windows: Vec<WindowInfo>,
    pub elements: Vec<UiElement>,
}
