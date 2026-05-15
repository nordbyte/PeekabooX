use std::collections::HashSet;
use std::ffi::OsStr;
use std::io::Cursor;
use std::os::fd::{AsRawFd, FromRawFd};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use dbus::arg::{PropMap, RefArg, Variant};
use dbus::blocking::Connection;
use dbus::message::MatchRule;
use image::codecs::png::PngEncoder;
use image::{ColorType, DynamicImage, ImageEncoder, ImageReader};
use peekaboox_core::{BackendKind, CaptureFrame, PeekabooXError, PixelFormat, Rect, Result};

pub trait CaptureBackend {
    fn capture_screen(&self) -> Result<CaptureFrame>;
    fn capture_region(&self, region: Rect) -> Result<CaptureFrame>;
}

#[derive(Debug, Default)]
pub struct UnimplementedCaptureBackend;

impl CaptureBackend for UnimplementedCaptureBackend {
    fn capture_screen(&self) -> Result<CaptureFrame> {
        Err(PeekabooXError::new(
            "screen capture backend is unavailable in this environment",
        ))
    }

    fn capture_region(&self, _region: Rect) -> Result<CaptureFrame> {
        Err(PeekabooXError::new(
            "region capture backend is unavailable in this environment",
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionType {
    Wayland,
    X11,
    Unknown,
}

impl SessionType {
    fn from_value(value: Option<&str>) -> Self {
        match value.unwrap_or_default().to_ascii_lowercase().as_str() {
            "wayland" => Self::Wayland,
            "x11" => Self::X11,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureEnvironment {
    pub session_type: SessionType,
    pub current_desktop: Option<String>,
    pub pipewire_session_available: bool,
    pub commands: HashSet<String>,
}

impl CaptureEnvironment {
    pub fn detect() -> Self {
        let command_names = [
            "gdbus",
            "grim",
            "gnome-screenshot",
            "pipewire",
            "pw-cli",
            "spectacle",
            "scrot",
            "maim",
            "import",
            "wireplumber",
            "xwd",
        ];
        let commands = command_names
            .into_iter()
            .filter(|command| command_exists(command))
            .map(str::to_owned)
            .collect();

        Self {
            session_type: SessionType::from_value(
                std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
            ),
            current_desktop: std::env::var("XDG_CURRENT_DESKTOP").ok(),
            pipewire_session_available: detect_pipewire_session(&commands),
            commands,
        }
    }

    fn has_command(&self, command: &str) -> bool {
        self.commands.contains(command)
    }

    fn is_gnome(&self) -> bool {
        self.current_desktop
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .contains("gnome")
    }

    fn is_kde(&self) -> bool {
        self.current_desktop
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .contains("kde")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureTool {
    XdgDesktopPortal,
    GnomeShellScreenshot,
    Grim,
    GnomeScreenshot,
    Spectacle,
    Scrot,
    Maim,
    ImageMagickImport,
    Xwd,
}

impl CaptureTool {
    pub fn backend_kind(self) -> BackendKind {
        match self {
            Self::XdgDesktopPortal => BackendKind::Portal,
            Self::GnomeShellScreenshot | Self::GnomeScreenshot | Self::Spectacle => {
                BackendKind::Portal
            }
            Self::Grim => BackendKind::Wayland,
            Self::Scrot | Self::Maim | Self::ImageMagickImport | Self::Xwd => BackendKind::X11,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::XdgDesktopPortal => "xdg-desktop-portal",
            Self::GnomeShellScreenshot => "gnome-shell-screenshot",
            Self::Grim => "grim",
            Self::GnomeScreenshot => "gnome-screenshot",
            Self::Spectacle => "spectacle",
            Self::Scrot => "scrot",
            Self::Maim => "maim",
            Self::ImageMagickImport => "imagemagick-import",
            Self::Xwd => "xwd",
        }
    }

    fn command(self) -> &'static str {
        match self {
            Self::XdgDesktopPortal => "",
            Self::GnomeShellScreenshot => "gdbus",
            Self::Grim => "grim",
            Self::GnomeScreenshot => "gnome-screenshot",
            Self::Spectacle => "spectacle",
            Self::Scrot => "scrot",
            Self::Maim => "maim",
            Self::ImageMagickImport => "import",
            Self::Xwd => "xwd",
        }
    }

    fn is_available(self, environment: &CaptureEnvironment) -> bool {
        self == Self::XdgDesktopPortal || environment.has_command(self.command())
    }

    fn supports_output(self, output: &Path) -> bool {
        if self != Self::Xwd {
            return true;
        }

        output
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|extension| extension.eq_ignore_ascii_case("xwd"))
    }

    fn supports_stdout_capture(self) -> bool {
        matches!(self, Self::Grim | Self::Maim | Self::ImageMagickImport)
    }

    fn supports_stdout_region_capture(self) -> bool {
        matches!(self, Self::Grim | Self::Maim | Self::ImageMagickImport)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedCaptureBackend {
    pub tool: CaptureTool,
    pub session_type: SessionType,
}

impl DetectedCaptureBackend {
    pub fn name(&self) -> &'static str {
        self.tool.name()
    }

    pub fn backend_kind(&self) -> BackendKind {
        self.tool.backend_kind()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureFileMetadata {
    pub output_path: PathBuf,
    pub backend_name: String,
    pub backend_kind: BackendKind,
    pub bytes_written: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureFrameSource {
    DirectStdout,
    DmaBufZeroCopy,
    FileFallback,
    FullFrameCrop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureFrameMetadata {
    pub frame: CaptureFrame,
    pub backend_name: String,
    pub backend_kind: BackendKind,
    pub source: CaptureFrameSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZeroCopyTransport {
    XdgDesktopPortalScreenCastPipeWireDmaBuf,
}

impl ZeroCopyTransport {
    pub fn name(self) -> &'static str {
        match self {
            Self::XdgDesktopPortalScreenCastPipeWireDmaBuf => {
                "xdg-desktop-portal-screencast-pipewire-dmabuf"
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZeroCopyAvailability {
    Available,
    MissingPipeWireSession,
    UnsupportedSession,
}

impl ZeroCopyAvailability {
    pub fn is_available(self) -> bool {
        self == Self::Available
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZeroCopyCaptureCapability {
    pub backend_name: String,
    pub backend_kind: BackendKind,
    pub transport: ZeroCopyTransport,
    pub availability: ZeroCopyAvailability,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedZeroCopyBackend {
    pub transport: ZeroCopyTransport,
    pub backend_kind: BackendKind,
}

impl DetectedZeroCopyBackend {
    pub fn name(&self) -> &'static str {
        self.transport.name()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct DmaBufPlaneDescriptor {
    pub fd: i32,
    pub offset: u32,
    pub stride: u32,
    pub modifier: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DmaBufFrameDescriptor {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub fourcc: u32,
    pub planes: Vec<DmaBufPlaneDescriptor>,
}

impl Drop for DmaBufFrameDescriptor {
    fn drop(&mut self) {
        for plane in &mut self.planes {
            close_raw_fd(&mut plane.fd);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaBufImportTarget {
    Egl,
    Vulkan,
    Compute,
}

impl DmaBufImportTarget {
    pub fn name(self) -> &'static str {
        match self {
            Self::Egl => "egl",
            Self::Vulkan => "vulkan",
            Self::Compute => "compute",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaBufSynchronization {
    Implicit,
}

impl DmaBufSynchronization {
    pub fn name(self) -> &'static str {
        match self {
            Self::Implicit => "implicit",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaBufMemoryLayout {
    SinglePlane,
    MultiPlane,
}

impl DmaBufMemoryLayout {
    pub fn name(self) -> &'static str {
        match self {
            Self::SinglePlane => "single-plane",
            Self::MultiPlane => "multi-plane",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmaBufPlaneImportDescriptor {
    pub plane_index: usize,
    pub fd: i32,
    pub offset: u32,
    pub stride: u32,
    pub modifier: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmaBufFrameImportDescriptor {
    pub target: DmaBufImportTarget,
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub fourcc: u32,
    pub memory_layout: DmaBufMemoryLayout,
    pub synchronization: DmaBufSynchronization,
    pub planes: Vec<DmaBufPlaneImportDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedDmaBufFrame {
    pub backend_name: String,
    pub backend_kind: DmaBufImportTarget,
    pub descriptor: DmaBufFrameImportDescriptor,
}

pub trait DmaBufFrameImporter {
    fn backend_name(&self) -> &str;
    fn import_target(&self) -> DmaBufImportTarget;
    fn import_frame(&self, descriptor: &DmaBufFrameDescriptor) -> Result<ImportedDmaBufFrame>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatingDmaBufImporter {
    target: DmaBufImportTarget,
    backend_name: String,
}

impl ValidatingDmaBufImporter {
    pub fn new(target: DmaBufImportTarget) -> Self {
        Self::named(target, format!("dmabuf-import-{}", target.name()))
    }

    pub fn named(target: DmaBufImportTarget, backend_name: impl Into<String>) -> Self {
        Self {
            target,
            backend_name: backend_name.into(),
        }
    }
}

impl Default for ValidatingDmaBufImporter {
    fn default() -> Self {
        Self::new(DmaBufImportTarget::Compute)
    }
}

impl DmaBufFrameImporter for ValidatingDmaBufImporter {
    fn backend_name(&self) -> &str {
        &self.backend_name
    }

    fn import_target(&self) -> DmaBufImportTarget {
        self.target
    }

    fn import_frame(&self, descriptor: &DmaBufFrameDescriptor) -> Result<ImportedDmaBufFrame> {
        let import_descriptor = prepare_dmabuf_import_descriptor(descriptor, self.target)?;
        Ok(ImportedDmaBufFrame {
            backend_name: self.backend_name.clone(),
            backend_kind: self.target,
            descriptor: import_descriptor,
        })
    }
}

pub fn import_dmabuf_frame(
    descriptor: &DmaBufFrameDescriptor,
    target: DmaBufImportTarget,
) -> Result<ImportedDmaBufFrame> {
    ValidatingDmaBufImporter::new(target).import_frame(descriptor)
}

pub fn prepare_dmabuf_import_descriptor(
    descriptor: &DmaBufFrameDescriptor,
    target: DmaBufImportTarget,
) -> Result<DmaBufFrameImportDescriptor> {
    validate_dmabuf_frame_descriptor(descriptor)?;

    let memory_layout = if descriptor.planes.len() == 1 {
        DmaBufMemoryLayout::SinglePlane
    } else {
        DmaBufMemoryLayout::MultiPlane
    };
    let planes = descriptor
        .planes
        .iter()
        .enumerate()
        .map(|(plane_index, plane)| DmaBufPlaneImportDescriptor {
            plane_index,
            fd: plane.fd,
            offset: plane.offset,
            stride: plane.stride,
            modifier: plane.modifier,
        })
        .collect();

    Ok(DmaBufFrameImportDescriptor {
        target,
        width: descriptor.width,
        height: descriptor.height,
        format: descriptor.format,
        fourcc: descriptor.fourcc,
        memory_layout,
        synchronization: DmaBufSynchronization::Implicit,
        planes,
    })
}

pub struct PipeWireScreenCastStream {
    pub session_handle: String,
    pub stream_node_id: u32,
    pub pipewire_serial: Option<u64>,
    pub pipewire_fd: dbus::arg::OwnedFd,
    pub backend_name: String,
    pub backend_kind: BackendKind,
    _portal_connection: Connection,
}

impl std::fmt::Debug for PipeWireScreenCastStream {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PipeWireScreenCastStream")
            .field("session_handle", &self.session_handle)
            .field("stream_node_id", &self.stream_node_id)
            .field("pipewire_serial", &self.pipewire_serial)
            .field("pipewire_raw_fd", &self.pipewire_raw_fd())
            .field("backend_name", &self.backend_name)
            .field("backend_kind", &self.backend_kind)
            .finish_non_exhaustive()
    }
}

impl PipeWireScreenCastStream {
    pub fn pipewire_raw_fd(&self) -> i32 {
        self.pipewire_fd.as_raw_fd()
    }
}

pub const DRM_FORMAT_MOD_INVALID: u64 = (1_u64 << 56) - 1;

const DRM_FORMAT_XRGB8888: u32 = fourcc_code(b'X', b'R', b'2', b'4');
const DRM_FORMAT_ARGB8888: u32 = fourcc_code(b'A', b'R', b'2', b'4');
const DRM_FORMAT_XBGR8888: u32 = fourcc_code(b'X', b'B', b'2', b'4');
const DRM_FORMAT_ABGR8888: u32 = fourcc_code(b'A', b'B', b'2', b'4');
const DRM_FORMAT_RGB888: u32 = fourcc_code(b'R', b'G', b'2', b'4');

fn validate_dmabuf_frame_descriptor(descriptor: &DmaBufFrameDescriptor) -> Result<()> {
    if descriptor.width == 0 || descriptor.height == 0 {
        return Err(PeekabooXError::new(format!(
            "invalid DMA-BUF frame dimensions {}x{}",
            descriptor.width, descriptor.height
        )));
    }

    let expected_plane_count = dmabuf_fourcc_plane_count(descriptor.fourcc)
        .ok_or_else(|| unsupported_dmabuf_fourcc_error(descriptor.fourcc))?;
    if descriptor.planes.len() != expected_plane_count {
        return Err(PeekabooXError::new(format!(
            "DMA-BUF frame exposes {} plane(s), expected {} for fourcc 0x{:08x}",
            descriptor.planes.len(),
            expected_plane_count,
            descriptor.fourcc
        )));
    }

    if !dmabuf_pixel_format_matches_fourcc(descriptor.format, descriptor.fourcc) {
        return Err(PeekabooXError::new(format!(
            "DMA-BUF pixel format {:?} does not match fourcc 0x{:08x}",
            descriptor.format, descriptor.fourcc
        )));
    }

    let row_bytes = u64::from(descriptor.width)
        .checked_mul(dmabuf_bytes_per_pixel(descriptor.format) as u64)
        .ok_or_else(|| PeekabooXError::new("DMA-BUF frame row size overflows u64"))?;

    for (index, plane) in descriptor.planes.iter().enumerate() {
        if plane.fd < 0 {
            return Err(PeekabooXError::new(format!(
                "DMA-BUF plane {index} has an invalid file descriptor"
            )));
        }
        if plane.stride == 0 {
            return Err(PeekabooXError::new(format!(
                "DMA-BUF plane {index} has zero stride"
            )));
        }
        if u64::from(plane.stride) < row_bytes {
            return Err(PeekabooXError::new(format!(
                "DMA-BUF plane {index} stride {} is smaller than row width {row_bytes}",
                plane.stride
            )));
        }

        let rows_before_last = u64::from(descriptor.height.saturating_sub(1));
        let last_row_offset = u64::from(plane.offset)
            .checked_add(
                rows_before_last
                    .checked_mul(u64::from(plane.stride))
                    .ok_or_else(|| {
                        PeekabooXError::new(format!(
                            "DMA-BUF plane {index} row offset overflows u64"
                        ))
                    })?,
            )
            .ok_or_else(|| {
                PeekabooXError::new(format!("DMA-BUF plane {index} row offset overflows u64"))
            })?;
        last_row_offset.checked_add(row_bytes).ok_or_else(|| {
            PeekabooXError::new(format!("DMA-BUF plane {index} byte range overflows u64"))
        })?;
    }

    Ok(())
}

fn dmabuf_fourcc_plane_count(fourcc: u32) -> Option<usize> {
    match fourcc {
        DRM_FORMAT_XRGB8888 | DRM_FORMAT_ARGB8888 | DRM_FORMAT_XBGR8888 | DRM_FORMAT_ABGR8888
        | DRM_FORMAT_RGB888 => Some(1),
        _ => None,
    }
}

fn dmabuf_pixel_format_matches_fourcc(format: PixelFormat, fourcc: u32) -> bool {
    match format {
        PixelFormat::Bgra8 => matches!(fourcc, DRM_FORMAT_XRGB8888 | DRM_FORMAT_ARGB8888),
        PixelFormat::Rgba8 => matches!(fourcc, DRM_FORMAT_XBGR8888 | DRM_FORMAT_ABGR8888),
        PixelFormat::Rgb8 => fourcc == DRM_FORMAT_RGB888,
    }
}

fn dmabuf_bytes_per_pixel(format: PixelFormat) -> usize {
    match format {
        PixelFormat::Bgra8 | PixelFormat::Rgba8 => 4,
        PixelFormat::Rgb8 => 3,
    }
}

fn unsupported_dmabuf_fourcc_error(fourcc: u32) -> PeekabooXError {
    PeekabooXError::new(format!(
        "unsupported DMA-BUF fourcc 0x{fourcc:08x}; supported import formats are XR24, AR24, XB24, AB24, and RG24"
    ))
}

fn close_raw_fd(fd: &mut i32) {
    if *fd < 0 {
        return;
    }

    let owned_fd = *fd;
    *fd = -1;
    unsafe {
        drop(std::fs::File::from_raw_fd(owned_fd));
    }
}

#[cfg(any(test, feature = "pipewire-backend"))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct DmaBufPlaneCandidate {
    fd: i32,
    offset: u32,
    stride: u32,
}

#[cfg(any(test, feature = "pipewire-backend"))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct DmaBufFrameCandidate {
    width: u32,
    height: u32,
    format: PixelFormat,
    fourcc: u32,
    modifier: u64,
    planes: Vec<DmaBufPlaneCandidate>,
}

#[cfg(any(test, feature = "pipewire-backend"))]
fn dmabuf_descriptor_from_candidate(
    candidate: DmaBufFrameCandidate,
) -> Result<DmaBufFrameDescriptor> {
    if candidate.width == 0 || candidate.height == 0 {
        return Err(PeekabooXError::new(format!(
            "invalid DMA-BUF frame dimensions {}x{}",
            candidate.width, candidate.height
        )));
    }

    if candidate.planes.is_empty() {
        return Err(PeekabooXError::new(
            "PipeWire DMA-BUF frame did not expose any planes",
        ));
    }

    let mut planes = Vec::with_capacity(candidate.planes.len());
    for (index, plane) in candidate.planes.into_iter().enumerate() {
        if plane.fd < 0 {
            return Err(PeekabooXError::new(format!(
                "PipeWire DMA-BUF plane {index} has an invalid file descriptor"
            )));
        }
        if plane.stride == 0 {
            return Err(PeekabooXError::new(format!(
                "PipeWire DMA-BUF plane {index} has zero stride"
            )));
        }

        planes.push(DmaBufPlaneDescriptor {
            fd: plane.fd,
            offset: plane.offset,
            stride: plane.stride,
            modifier: candidate.modifier,
        });
    }

    Ok(DmaBufFrameDescriptor {
        width: candidate.width,
        height: candidate.height,
        format: candidate.format,
        fourcc: candidate.fourcc,
        planes,
    })
}

const fn fourcc_code(a: u8, b: u8, c: u8, d: u8) -> u32 {
    (a as u32) | ((b as u32) << 8) | ((c as u32) << 16) | ((d as u32) << 24)
}

#[cfg(feature = "egl-backend")]
pub use egl_importer::{
    EglDmaBufImporter, EglImportedDmaBufFrame, EglTextureDmaBufImporter,
    EglTextureImportedDmaBufFrame,
};

#[cfg(feature = "egl-backend")]
mod egl_importer {
    use super::*;
    use std::ffi::{CStr, c_char, c_void};
    use std::ptr::NonNull;
    use std::rc::Rc;

    type EglBoolean = u32;
    type EglClientBuffer = *mut c_void;
    type EglConfig = *mut c_void;
    type EglContext = *mut c_void;
    type EglDisplay = *mut c_void;
    type EglEnum = u32;
    type EglImage = *mut c_void;
    type EglInt = i32;
    type EglNativeDisplay = *mut c_void;
    type EglSurface = *mut c_void;
    type GlEnum = u32;
    type GlInt = i32;
    type GlSizei = i32;
    type GlUInt = u32;

    type GlEglImageTargetTexture2dProc = unsafe extern "C" fn(GlEnum, *mut c_void);

    type EglCreateImageProc = unsafe extern "C" fn(
        EglDisplay,
        EglContext,
        EglEnum,
        EglClientBuffer,
        *const EglInt,
    ) -> EglImage;
    type EglDestroyImageProc = unsafe extern "C" fn(EglDisplay, EglImage) -> EglBoolean;
    type EglGetPlatformDisplayProc =
        unsafe extern "C" fn(EglEnum, *mut c_void, *const EglInt) -> EglDisplay;

    const EGL_FALSE: EglBoolean = 0;
    const EGL_NO_DISPLAY: EglDisplay = std::ptr::null_mut();
    const EGL_NO_CONTEXT: EglContext = std::ptr::null_mut();
    const EGL_NO_CLIENT_BUFFER: EglClientBuffer = std::ptr::null_mut();
    const EGL_NO_IMAGE: EglImage = std::ptr::null_mut();
    const EGL_NO_SURFACE: EglSurface = std::ptr::null_mut();
    const EGL_DEFAULT_DISPLAY: EglNativeDisplay = std::ptr::null_mut();
    const EGL_NONE: EglInt = 0x3038;
    const EGL_RED_SIZE: EglInt = 0x3024;
    const EGL_GREEN_SIZE: EglInt = 0x3023;
    const EGL_BLUE_SIZE: EglInt = 0x3022;
    const EGL_ALPHA_SIZE: EglInt = 0x3021;
    const EGL_SURFACE_TYPE: EglInt = 0x3033;
    const EGL_PBUFFER_BIT: EglInt = 0x0001;
    const EGL_RENDERABLE_TYPE: EglInt = 0x3040;
    const EGL_OPENGL_ES2_BIT: EglInt = 0x0004;
    const EGL_EXTENSIONS: EglInt = 0x3055;
    const EGL_WIDTH: EglInt = 0x3057;
    const EGL_HEIGHT: EglInt = 0x3056;
    const EGL_CONTEXT_CLIENT_VERSION: EglInt = 0x3098;
    const EGL_OPENGL_ES_API: EglEnum = 0x30a0;
    const EGL_PLATFORM_SURFACELESS_MESA: EglEnum = 0x31dd;
    const EGL_LINUX_DMA_BUF_EXT: EglEnum = 0x3270;
    const EGL_LINUX_DRM_FOURCC_EXT: EglInt = 0x3271;

    const EGL_DMA_BUF_PLANE_FD_EXT: [EglInt; 4] = [0x3272, 0x3275, 0x3278, 0x3440];
    const EGL_DMA_BUF_PLANE_OFFSET_EXT: [EglInt; 4] = [0x3273, 0x3276, 0x3279, 0x3441];
    const EGL_DMA_BUF_PLANE_PITCH_EXT: [EglInt; 4] = [0x3274, 0x3277, 0x327a, 0x3442];
    const EGL_DMA_BUF_PLANE_MODIFIER_LO_EXT: [EglInt; 4] = [0x3443, 0x3445, 0x3447, 0x3449];
    const EGL_DMA_BUF_PLANE_MODIFIER_HI_EXT: [EglInt; 4] = [0x3444, 0x3446, 0x3448, 0x344a];

    const EGL_EXT_IMAGE_DMA_BUF_IMPORT: &str = "EGL_EXT_image_dma_buf_import";
    const EGL_EXT_IMAGE_DMA_BUF_IMPORT_MODIFIERS: &str = "EGL_EXT_image_dma_buf_import_modifiers";
    const EGL_KHR_SURFACELESS_CONTEXT: &str = "EGL_KHR_surfaceless_context";

    const GL_NO_ERROR: GlEnum = 0;
    const GL_EXTENSIONS: GlEnum = 0x1f03;
    const GL_TEXTURE_2D: GlEnum = 0x0de1;
    const GL_TEXTURE_MAG_FILTER: GlEnum = 0x2800;
    const GL_TEXTURE_MIN_FILTER: GlEnum = 0x2801;
    const GL_TEXTURE_WRAP_S: GlEnum = 0x2802;
    const GL_TEXTURE_WRAP_T: GlEnum = 0x2803;
    const GL_NEAREST: GlInt = 0x2600;
    const GL_CLAMP_TO_EDGE: GlInt = 0x812f;

    const GL_OES_EGL_IMAGE: &str = "GL_OES_EGL_image";

    #[link(name = "EGL")]
    unsafe extern "C" {
        fn eglGetDisplay(display_id: EglNativeDisplay) -> EglDisplay;
        fn eglInitialize(dpy: EglDisplay, major: *mut EglInt, minor: *mut EglInt) -> EglBoolean;
        fn eglTerminate(dpy: EglDisplay) -> EglBoolean;
        fn eglGetError() -> EglInt;
        fn eglQueryString(dpy: EglDisplay, name: EglInt) -> *const c_char;
        fn eglGetProcAddress(procname: *const c_char) -> *const c_void;
        fn eglBindAPI(api: EglEnum) -> EglBoolean;
        fn eglChooseConfig(
            dpy: EglDisplay,
            attrib_list: *const EglInt,
            configs: *mut EglConfig,
            config_size: EglInt,
            num_config: *mut EglInt,
        ) -> EglBoolean;
        fn eglCreateContext(
            dpy: EglDisplay,
            config: EglConfig,
            share_context: EglContext,
            attrib_list: *const EglInt,
        ) -> EglContext;
        fn eglDestroyContext(dpy: EglDisplay, ctx: EglContext) -> EglBoolean;
        fn eglCreatePbufferSurface(
            dpy: EglDisplay,
            config: EglConfig,
            attrib_list: *const EglInt,
        ) -> EglSurface;
        fn eglDestroySurface(dpy: EglDisplay, surface: EglSurface) -> EglBoolean;
        fn eglMakeCurrent(
            dpy: EglDisplay,
            draw: EglSurface,
            read: EglSurface,
            ctx: EglContext,
        ) -> EglBoolean;
    }

    #[link(name = "GLESv2")]
    unsafe extern "C" {
        fn glGenTextures(n: GlSizei, textures: *mut GlUInt);
        fn glDeleteTextures(n: GlSizei, textures: *const GlUInt);
        fn glBindTexture(target: GlEnum, texture: GlUInt);
        fn glTexParameteri(target: GlEnum, pname: GlEnum, param: GlInt);
        fn glGetError() -> GlEnum;
        fn glGetString(name: GlEnum) -> *const u8;
    }

    #[derive(Clone)]
    pub struct EglDmaBufImporter {
        display: Rc<EglDisplayInner>,
    }

    impl EglDmaBufImporter {
        pub fn new() -> Result<Self> {
            Ok(Self {
                display: Rc::new(EglDisplayInner::open()?),
            })
        }

        pub fn egl_version(&self) -> (i32, i32) {
            self.display.egl_version
        }

        pub fn supports_modifiers(&self) -> bool {
            self.display.supports_modifiers
        }

        pub fn import_image(
            &self,
            descriptor: &DmaBufFrameDescriptor,
        ) -> Result<EglImportedDmaBufFrame> {
            let import_descriptor =
                prepare_dmabuf_import_descriptor(descriptor, DmaBufImportTarget::Egl)?;
            self.import_descriptor(&import_descriptor)
        }

        pub fn import_descriptor(
            &self,
            descriptor: &DmaBufFrameImportDescriptor,
        ) -> Result<EglImportedDmaBufFrame> {
            if descriptor.target != DmaBufImportTarget::Egl {
                return Err(PeekabooXError::new(format!(
                    "EGL importer requires an egl import descriptor, got {}",
                    descriptor.target.name()
                )));
            }

            let attributes = egl_dma_buf_attributes(descriptor, self.display.supports_modifiers)?;
            let image = unsafe {
                (self.display.create_image)(
                    self.display.display,
                    EGL_NO_CONTEXT,
                    EGL_LINUX_DMA_BUF_EXT,
                    EGL_NO_CLIENT_BUFFER,
                    attributes.as_ptr(),
                )
            };
            if image == EGL_NO_IMAGE {
                return Err(egl_last_error("eglCreateImageKHR DMA-BUF import failed"));
            }

            Ok(EglImportedDmaBufFrame {
                backend_name: "egl-dmabuf-import".to_owned(),
                descriptor: descriptor.clone(),
                image,
                display: self.display.clone(),
            })
        }
    }

    impl std::fmt::Debug for EglDmaBufImporter {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("EglDmaBufImporter")
                .field("egl_version", &self.display.egl_version)
                .field("supports_modifiers", &self.display.supports_modifiers)
                .finish_non_exhaustive()
        }
    }

    pub struct EglImportedDmaBufFrame {
        pub backend_name: String,
        pub descriptor: DmaBufFrameImportDescriptor,
        image: EglImage,
        display: Rc<EglDisplayInner>,
    }

    impl EglImportedDmaBufFrame {
        pub fn native_image_handle(&self) -> *mut c_void {
            self.image.cast()
        }
    }

    impl std::fmt::Debug for EglImportedDmaBufFrame {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("EglImportedDmaBufFrame")
                .field("backend_name", &self.backend_name)
                .field("descriptor", &self.descriptor)
                .field("native_image_handle", &self.native_image_handle())
                .finish_non_exhaustive()
        }
    }

    impl Drop for EglImportedDmaBufFrame {
        fn drop(&mut self) {
            if self.image != EGL_NO_IMAGE {
                let _ = unsafe { (self.display.destroy_image)(self.display.display, self.image) };
                self.image = EGL_NO_IMAGE;
            }
        }
    }

    #[derive(Clone)]
    pub struct EglTextureDmaBufImporter {
        image_importer: EglDmaBufImporter,
        context: Rc<EglGlesContext>,
        image_target_texture_2d: GlEglImageTargetTexture2dProc,
    }

    impl EglTextureDmaBufImporter {
        pub fn new() -> Result<Self> {
            let image_importer = EglDmaBufImporter::new()?;
            let context = Rc::new(EglGlesContext::new(image_importer.display.clone())?);
            context.make_current()?;
            if !gl_extension_present(GL_OES_EGL_IMAGE) {
                return Err(PeekabooXError::new(format!(
                    "GLES context does not advertise {GL_OES_EGL_IMAGE}"
                )));
            }
            let image_target_texture_2d = load_gl_egl_image_target_texture_2d_proc()?;

            Ok(Self {
                image_importer,
                context,
                image_target_texture_2d,
            })
        }

        pub fn egl_version(&self) -> (i32, i32) {
            self.image_importer.egl_version()
        }

        pub fn supports_modifiers(&self) -> bool {
            self.image_importer.supports_modifiers()
        }

        pub fn import_texture(
            &self,
            descriptor: &DmaBufFrameDescriptor,
        ) -> Result<EglTextureImportedDmaBufFrame> {
            self.context.make_current()?;
            let image = self.image_importer.import_image(descriptor)?;
            let texture_id = match create_gles_texture_from_egl_image(
                self.context.as_ref(),
                self.image_target_texture_2d,
                image.native_image_handle(),
            ) {
                Ok(texture_id) => texture_id,
                Err(error) => {
                    drop(image);
                    return Err(error);
                }
            };

            Ok(EglTextureImportedDmaBufFrame {
                backend_name: "egl-texture-dmabuf-import".to_owned(),
                descriptor: image.descriptor.clone(),
                texture_id,
                context: self.context.clone(),
                image,
            })
        }
    }

    impl std::fmt::Debug for EglTextureDmaBufImporter {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("EglTextureDmaBufImporter")
                .field("egl_version", &self.egl_version())
                .field("supports_modifiers", &self.supports_modifiers())
                .finish_non_exhaustive()
        }
    }

    pub struct EglTextureImportedDmaBufFrame {
        pub backend_name: String,
        pub descriptor: DmaBufFrameImportDescriptor,
        texture_id: GlUInt,
        context: Rc<EglGlesContext>,
        image: EglImportedDmaBufFrame,
    }

    impl EglTextureImportedDmaBufFrame {
        pub fn texture_id(&self) -> u32 {
            self.texture_id
        }

        pub fn native_image_handle(&self) -> *mut c_void {
            self.image.native_image_handle()
        }
    }

    impl std::fmt::Debug for EglTextureImportedDmaBufFrame {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("EglTextureImportedDmaBufFrame")
                .field("backend_name", &self.backend_name)
                .field("descriptor", &self.descriptor)
                .field("texture_id", &self.texture_id)
                .field("native_image_handle", &self.native_image_handle())
                .finish_non_exhaustive()
        }
    }

    impl Drop for EglTextureImportedDmaBufFrame {
        fn drop(&mut self) {
            if self.texture_id != 0 {
                if self.context.make_current().is_ok() {
                    let texture_id = self.texture_id;
                    unsafe {
                        glDeleteTextures(1, &texture_id);
                    }
                }
                self.texture_id = 0;
            }
        }
    }

    struct EglDisplayInner {
        display: EglDisplay,
        egl_version: (i32, i32),
        supports_modifiers: bool,
        supports_surfaceless_context: bool,
        create_image: EglCreateImageProc,
        destroy_image: EglDestroyImageProc,
    }

    impl EglDisplayInner {
        fn open() -> Result<Self> {
            let mut errors = Vec::new();

            if let Some(get_platform_display) = load_get_platform_display_proc() {
                let attributes = [EGL_NONE];
                let display = unsafe {
                    get_platform_display(
                        EGL_PLATFORM_SURFACELESS_MESA,
                        EGL_DEFAULT_DISPLAY.cast(),
                        attributes.as_ptr(),
                    )
                };
                match Self::from_display(display, "surfaceless EGL display") {
                    Ok(display) => return Ok(display),
                    Err(error) => errors.push(error.message().to_owned()),
                }
            }

            let display = unsafe { eglGetDisplay(EGL_DEFAULT_DISPLAY) };
            match Self::from_display(display, "default EGL display") {
                Ok(display) => Ok(display),
                Err(error) => {
                    errors.push(error.message().to_owned());
                    Err(PeekabooXError::new(format!(
                        "failed to initialize EGL DMA-BUF importer: {}",
                        errors.join("; ")
                    )))
                }
            }
        }

        fn from_display(display: EglDisplay, label: &str) -> Result<Self> {
            if display == EGL_NO_DISPLAY {
                return Err(egl_last_error(format!("{label} unavailable")));
            }

            let mut major = 0;
            let mut minor = 0;
            let initialized = unsafe { eglInitialize(display, &mut major, &mut minor) };
            if initialized == EGL_FALSE {
                return Err(egl_last_error(format!("eglInitialize failed for {label}")));
            }

            let extensions = egl_query_extensions(display);
            if !egl_extension_present(&extensions, EGL_EXT_IMAGE_DMA_BUF_IMPORT) {
                let _ = unsafe { eglTerminate(display) };
                return Err(PeekabooXError::new(format!(
                    "{label} does not advertise {EGL_EXT_IMAGE_DMA_BUF_IMPORT}"
                )));
            }

            let create_image = match load_create_image_proc() {
                Ok(create_image) => create_image,
                Err(error) => {
                    let _ = unsafe { eglTerminate(display) };
                    return Err(error);
                }
            };
            let destroy_image = match load_destroy_image_proc() {
                Ok(destroy_image) => destroy_image,
                Err(error) => {
                    let _ = unsafe { eglTerminate(display) };
                    return Err(error);
                }
            };

            Ok(Self {
                display,
                egl_version: (major, minor),
                supports_modifiers: egl_extension_present(
                    &extensions,
                    EGL_EXT_IMAGE_DMA_BUF_IMPORT_MODIFIERS,
                ),
                supports_surfaceless_context: egl_extension_present(
                    &extensions,
                    EGL_KHR_SURFACELESS_CONTEXT,
                ),
                create_image,
                destroy_image,
            })
        }
    }

    impl Drop for EglDisplayInner {
        fn drop(&mut self) {
            if self.display != EGL_NO_DISPLAY {
                let _ = unsafe { eglTerminate(self.display) };
                self.display = EGL_NO_DISPLAY;
            }
        }
    }

    struct EglGlesContext {
        display: Rc<EglDisplayInner>,
        context: EglContext,
        surface: EglSurface,
    }

    impl EglGlesContext {
        fn new(display: Rc<EglDisplayInner>) -> Result<Self> {
            let bound = unsafe { eglBindAPI(EGL_OPENGL_ES_API) };
            if bound == EGL_FALSE {
                return Err(egl_last_error("eglBindAPI(EGL_OPENGL_ES_API) failed"));
            }

            let config = choose_gles_config(display.display, true).or_else(|error| {
                if display.supports_surfaceless_context {
                    choose_gles_config(display.display, false)
                } else {
                    Err(error)
                }
            })?;
            let surface = create_gles_surface(
                display.display,
                config,
                display.supports_surfaceless_context,
            )?;
            let context_attributes = [EGL_CONTEXT_CLIENT_VERSION, 2, EGL_NONE];
            let context = unsafe {
                eglCreateContext(
                    display.display,
                    config,
                    EGL_NO_CONTEXT,
                    context_attributes.as_ptr(),
                )
            };
            if context == EGL_NO_CONTEXT {
                if surface != EGL_NO_SURFACE {
                    let _ = unsafe { eglDestroySurface(display.display, surface) };
                }
                return Err(egl_last_error("eglCreateContext for GLES2 failed"));
            }

            let context = Self {
                display,
                context,
                surface,
            };
            if let Err(error) = context.make_current() {
                drop(context);
                return Err(error);
            }

            Ok(context)
        }

        fn make_current(&self) -> Result<()> {
            let made_current = unsafe {
                eglMakeCurrent(
                    self.display.display,
                    self.surface,
                    self.surface,
                    self.context,
                )
            };
            if made_current == EGL_FALSE {
                return Err(egl_last_error(
                    "eglMakeCurrent for GLES texture import failed",
                ));
            }

            Ok(())
        }
    }

    impl Drop for EglGlesContext {
        fn drop(&mut self) {
            if self.display.display != EGL_NO_DISPLAY {
                let _ = unsafe {
                    eglMakeCurrent(
                        self.display.display,
                        EGL_NO_SURFACE,
                        EGL_NO_SURFACE,
                        EGL_NO_CONTEXT,
                    )
                };
                if self.context != EGL_NO_CONTEXT {
                    let _ = unsafe { eglDestroyContext(self.display.display, self.context) };
                    self.context = EGL_NO_CONTEXT;
                }
                if self.surface != EGL_NO_SURFACE {
                    let _ = unsafe { eglDestroySurface(self.display.display, self.surface) };
                    self.surface = EGL_NO_SURFACE;
                }
            }
        }
    }

    fn choose_gles_config(display: EglDisplay, require_pbuffer: bool) -> Result<EglConfig> {
        let mut attributes = vec![
            EGL_RENDERABLE_TYPE,
            EGL_OPENGL_ES2_BIT,
            EGL_RED_SIZE,
            8,
            EGL_GREEN_SIZE,
            8,
            EGL_BLUE_SIZE,
            8,
            EGL_ALPHA_SIZE,
            8,
        ];
        if require_pbuffer {
            attributes.extend([EGL_SURFACE_TYPE, EGL_PBUFFER_BIT]);
        }
        attributes.push(EGL_NONE);

        let mut config = std::ptr::null_mut();
        let mut config_count = 0;
        let selected = unsafe {
            eglChooseConfig(
                display,
                attributes.as_ptr(),
                &mut config,
                1,
                &mut config_count,
            )
        };
        if selected == EGL_FALSE || config_count == 0 || config.is_null() {
            return Err(egl_last_error("eglChooseConfig for GLES2 failed"));
        }

        Ok(config)
    }

    fn create_gles_surface(
        display: EglDisplay,
        config: EglConfig,
        supports_surfaceless_context: bool,
    ) -> Result<EglSurface> {
        let surface_attributes = [EGL_WIDTH, 1, EGL_HEIGHT, 1, EGL_NONE];
        let surface =
            unsafe { eglCreatePbufferSurface(display, config, surface_attributes.as_ptr()) };
        if surface != EGL_NO_SURFACE {
            return Ok(surface);
        }

        if supports_surfaceless_context {
            return Ok(EGL_NO_SURFACE);
        }

        Err(egl_last_error("eglCreatePbufferSurface for GLES2 failed"))
    }

    fn egl_dma_buf_attributes(
        descriptor: &DmaBufFrameImportDescriptor,
        supports_modifiers: bool,
    ) -> Result<Vec<EglInt>> {
        if descriptor.planes.len() > EGL_DMA_BUF_PLANE_FD_EXT.len() {
            return Err(PeekabooXError::new(format!(
                "EGL DMA-BUF import supports at most {} planes, got {}",
                EGL_DMA_BUF_PLANE_FD_EXT.len(),
                descriptor.planes.len()
            )));
        }

        let mut attributes = vec![
            EGL_WIDTH,
            u32_to_egl_int("EGL DMA-BUF width", descriptor.width)?,
            EGL_HEIGHT,
            u32_to_egl_int("EGL DMA-BUF height", descriptor.height)?,
            EGL_LINUX_DRM_FOURCC_EXT,
            descriptor.fourcc as EglInt,
        ];

        for plane in &descriptor.planes {
            let Some(fd_attr) = EGL_DMA_BUF_PLANE_FD_EXT.get(plane.plane_index) else {
                return Err(PeekabooXError::new(format!(
                    "EGL DMA-BUF plane index {} is out of range",
                    plane.plane_index
                )));
            };

            attributes.extend([
                *fd_attr,
                plane.fd,
                EGL_DMA_BUF_PLANE_OFFSET_EXT[plane.plane_index],
                u32_to_egl_int("EGL DMA-BUF plane offset", plane.offset)?,
                EGL_DMA_BUF_PLANE_PITCH_EXT[plane.plane_index],
                u32_to_egl_int("EGL DMA-BUF plane stride", plane.stride)?,
            ]);

            if plane.modifier != DRM_FORMAT_MOD_INVALID {
                if !supports_modifiers {
                    return Err(PeekabooXError::new(
                        "EGL DMA-BUF import requires explicit modifier support",
                    ));
                }

                attributes.extend([
                    EGL_DMA_BUF_PLANE_MODIFIER_LO_EXT[plane.plane_index],
                    (plane.modifier as u32) as EglInt,
                    EGL_DMA_BUF_PLANE_MODIFIER_HI_EXT[plane.plane_index],
                    ((plane.modifier >> 32) as u32) as EglInt,
                ]);
            }
        }

        attributes.push(EGL_NONE);
        Ok(attributes)
    }

    fn u32_to_egl_int(name: &str, value: u32) -> Result<EglInt> {
        EglInt::try_from(value)
            .map_err(|_| PeekabooXError::new(format!("{name} value {value} overflows EGLint")))
    }

    fn create_gles_texture_from_egl_image(
        context: &EglGlesContext,
        image_target_texture_2d: GlEglImageTargetTexture2dProc,
        image: *mut c_void,
    ) -> Result<GlUInt> {
        context.make_current()?;
        let mut texture_id = 0;
        unsafe {
            glGenTextures(1, &mut texture_id);
        }
        check_gl_error("glGenTextures for EGLImage import")?;
        if texture_id == 0 {
            return Err(PeekabooXError::new(
                "glGenTextures returned texture id 0 for EGLImage import",
            ));
        }

        let bind_result =
            bind_egl_image_to_gles_texture(texture_id, image_target_texture_2d, image);
        if let Err(error) = bind_result {
            unsafe {
                glDeleteTextures(1, &texture_id);
            }
            return Err(error);
        }

        Ok(texture_id)
    }

    fn bind_egl_image_to_gles_texture(
        texture_id: GlUInt,
        image_target_texture_2d: GlEglImageTargetTexture2dProc,
        image: *mut c_void,
    ) -> Result<()> {
        unsafe {
            glBindTexture(GL_TEXTURE_2D, texture_id);
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_NEAREST);
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_NEAREST);
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);
            image_target_texture_2d(GL_TEXTURE_2D, image);
        }
        check_gl_error("glEGLImageTargetTexture2DOES")
    }

    fn gl_extension_present(extension: &str) -> bool {
        let pointer = unsafe { glGetString(GL_EXTENSIONS) };
        if pointer.is_null() {
            return false;
        }

        let extensions = unsafe { CStr::from_ptr(pointer.cast()) }.to_string_lossy();
        extension_list_contains(&extensions, extension)
    }

    fn load_gl_egl_image_target_texture_2d_proc() -> Result<GlEglImageTargetTexture2dProc> {
        let pointer = egl_proc_address(b"glEGLImageTargetTexture2DOES\0").ok_or_else(|| {
            PeekabooXError::new("GLES did not expose glEGLImageTargetTexture2DOES")
        })?;
        Ok(unsafe {
            std::mem::transmute::<*mut c_void, GlEglImageTargetTexture2dProc>(pointer.as_ptr())
        })
    }

    fn check_gl_error(context: impl AsRef<str>) -> Result<()> {
        let error = unsafe { glGetError() };
        if error == GL_NO_ERROR {
            Ok(())
        } else {
            Err(PeekabooXError::new(format!(
                "{}: GL error 0x{error:04x}",
                context.as_ref()
            )))
        }
    }

    fn egl_query_extensions(display: EglDisplay) -> String {
        let pointer = unsafe { eglQueryString(display, EGL_EXTENSIONS) };
        if pointer.is_null() {
            return String::new();
        }

        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned()
    }

    fn egl_extension_present(extensions: &str, extension: &str) -> bool {
        extension_list_contains(extensions, extension)
    }

    fn extension_list_contains(extensions: &str, extension: &str) -> bool {
        extensions
            .split_ascii_whitespace()
            .any(|candidate| candidate == extension)
    }

    fn load_get_platform_display_proc() -> Option<EglGetPlatformDisplayProc> {
        let pointer = egl_proc_address(b"eglGetPlatformDisplayEXT\0")?;
        Some(unsafe {
            std::mem::transmute::<*mut c_void, EglGetPlatformDisplayProc>(pointer.as_ptr())
        })
    }

    fn load_create_image_proc() -> Result<EglCreateImageProc> {
        let pointer = egl_proc_address(b"eglCreateImageKHR\0")
            .or_else(|| egl_proc_address(b"eglCreateImage\0"))
            .ok_or_else(|| {
                PeekabooXError::new(
                    "EGL did not expose eglCreateImageKHR or eglCreateImage for DMA-BUF import",
                )
            })?;
        Ok(unsafe { std::mem::transmute::<*mut c_void, EglCreateImageProc>(pointer.as_ptr()) })
    }

    fn load_destroy_image_proc() -> Result<EglDestroyImageProc> {
        let pointer = egl_proc_address(b"eglDestroyImageKHR\0")
            .or_else(|| egl_proc_address(b"eglDestroyImage\0"))
            .ok_or_else(|| {
                PeekabooXError::new(
                    "EGL did not expose eglDestroyImageKHR or eglDestroyImage for DMA-BUF cleanup",
                )
            })?;
        Ok(unsafe { std::mem::transmute::<*mut c_void, EglDestroyImageProc>(pointer.as_ptr()) })
    }

    fn egl_proc_address(name: &'static [u8]) -> Option<NonNull<c_void>> {
        let pointer = unsafe { eglGetProcAddress(name.as_ptr().cast()) };
        NonNull::new(pointer.cast_mut())
    }

    fn egl_last_error(context: impl AsRef<str>) -> PeekabooXError {
        let error = unsafe { eglGetError() };
        PeekabooXError::new(format!("{}: EGL error 0x{error:04x}", context.as_ref()))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn builds_egl_attributes_without_invalid_modifier() {
            let descriptor = egl_import_descriptor(DRM_FORMAT_MOD_INVALID);

            let attributes = egl_dma_buf_attributes(&descriptor, false).unwrap();

            assert_eq!(
                attributes,
                vec![
                    EGL_WIDTH,
                    1920,
                    EGL_HEIGHT,
                    1080,
                    EGL_LINUX_DRM_FOURCC_EXT,
                    fourcc_code(b'X', b'R', b'2', b'4') as EglInt,
                    EGL_DMA_BUF_PLANE_FD_EXT[0],
                    12,
                    EGL_DMA_BUF_PLANE_OFFSET_EXT[0],
                    128,
                    EGL_DMA_BUF_PLANE_PITCH_EXT[0],
                    7680,
                    EGL_NONE,
                ]
            );
        }

        #[test]
        fn builds_egl_attributes_with_explicit_modifier() {
            let descriptor = egl_import_descriptor(0x0102_0304_0506_0708);

            let attributes = egl_dma_buf_attributes(&descriptor, true).unwrap();

            assert_eq!(
                attributes,
                vec![
                    EGL_WIDTH,
                    1920,
                    EGL_HEIGHT,
                    1080,
                    EGL_LINUX_DRM_FOURCC_EXT,
                    fourcc_code(b'X', b'R', b'2', b'4') as EglInt,
                    EGL_DMA_BUF_PLANE_FD_EXT[0],
                    12,
                    EGL_DMA_BUF_PLANE_OFFSET_EXT[0],
                    128,
                    EGL_DMA_BUF_PLANE_PITCH_EXT[0],
                    7680,
                    EGL_DMA_BUF_PLANE_MODIFIER_LO_EXT[0],
                    0x0506_0708,
                    EGL_DMA_BUF_PLANE_MODIFIER_HI_EXT[0],
                    0x0102_0304,
                    EGL_NONE,
                ]
            );
        }

        #[test]
        fn rejects_explicit_modifier_when_egl_extension_is_missing() {
            let descriptor = egl_import_descriptor(0x0102_0304_0506_0708);

            let error = egl_dma_buf_attributes(&descriptor, false).unwrap_err();

            assert!(error.message().contains("modifier support"));
        }

        #[test]
        fn detects_egl_extension_as_ascii_word() {
            assert!(egl_extension_present(
                "EGL_EXT_alpha EGL_EXT_image_dma_buf_import EGL_EXT_beta",
                EGL_EXT_IMAGE_DMA_BUF_IMPORT
            ));
            assert!(!egl_extension_present(
                "EGL_EXT_image_dma_buf_import_modifiers",
                EGL_EXT_IMAGE_DMA_BUF_IMPORT
            ));
        }

        fn egl_import_descriptor(modifier: u64) -> DmaBufFrameImportDescriptor {
            DmaBufFrameImportDescriptor {
                target: DmaBufImportTarget::Egl,
                width: 1920,
                height: 1080,
                format: PixelFormat::Bgra8,
                fourcc: fourcc_code(b'X', b'R', b'2', b'4'),
                memory_layout: DmaBufMemoryLayout::SinglePlane,
                synchronization: DmaBufSynchronization::Implicit,
                planes: vec![DmaBufPlaneImportDescriptor {
                    plane_index: 0,
                    fd: 12,
                    offset: 128,
                    stride: 7680,
                    modifier,
                }],
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct CommandCaptureBackend;

impl CommandCaptureBackend {
    pub fn detect_backend_for(&self, output: &Path) -> Result<DetectedCaptureBackend> {
        let environment = CaptureEnvironment::detect();
        candidate_backends(&environment, output)
            .into_iter()
            .next()
            .ok_or_else(|| {
            PeekabooXError::new(format!(
                "no supported screenshot backend found for {:?}; install gdbus, grim, gnome-screenshot, spectacle, scrot, maim, imagemagick import, or use .xwd output with xwd",
                environment.session_type
            ))
        })
    }

    pub fn capture_screen_to_file(&self, output: impl AsRef<Path>) -> Result<CaptureFileMetadata> {
        let output = absolute_output_path(output.as_ref())?;
        prepare_output_parent(&output)?;

        let environment = CaptureEnvironment::detect();
        let backends = candidate_backends(&environment, &output);
        if backends.is_empty() {
            return Err(PeekabooXError::new(format!(
                "no supported screenshot backend found for {:?}; install xdg-desktop-portal, gdbus, grim, gnome-screenshot, spectacle, scrot, maim, imagemagick import, or use .xwd output with xwd",
                environment.session_type
            )));
        }

        let mut selected_backend = None;
        let mut errors = Vec::new();

        for backend in backends {
            match run_capture_tool(backend.tool, &output) {
                Ok(()) => {
                    selected_backend = Some(backend);
                    break;
                }
                Err(error) => errors.push(format!("{}: {}", backend.name(), error.message())),
            }
        }

        let Some(backend) = selected_backend else {
            return Err(PeekabooXError::new(format!(
                "all screenshot backends failed: {}",
                errors.join("; ")
            )));
        };

        let bytes_written = std::fs::metadata(&output)
            .map_err(|error| {
                PeekabooXError::new(format!(
                    "screenshot backend {} did not create {}: {error}",
                    backend.name(),
                    output.display()
                ))
            })?
            .len();

        if bytes_written == 0 {
            return Err(PeekabooXError::new(format!(
                "screenshot backend {} created an empty file at {}",
                backend.name(),
                output.display()
            )));
        }

        Ok(CaptureFileMetadata {
            output_path: output,
            backend_name: backend.name().to_owned(),
            backend_kind: backend.backend_kind(),
            bytes_written,
        })
    }

    pub fn capture_screen_frame(&self) -> Result<CaptureFrameMetadata> {
        let environment = CaptureEnvironment::detect();
        let mut direct_errors = Vec::new();

        for backend in candidate_frame_backends(&environment) {
            match run_capture_tool_stdout(backend.tool).and_then(|bytes| decode_image_bytes(&bytes))
            {
                Ok(frame) => {
                    return Ok(CaptureFrameMetadata {
                        frame,
                        backend_name: backend.name().to_owned(),
                        backend_kind: backend.backend_kind(),
                        source: CaptureFrameSource::DirectStdout,
                    });
                }
                Err(error) => {
                    direct_errors.push(format!("{}: {}", backend.name(), error.message()))
                }
            }
        }

        let output = frame_capture_temp_path();
        let file_metadata = self.capture_screen_to_file(&output).map_err(|error| {
            if direct_errors.is_empty() {
                error
            } else {
                PeekabooXError::new(format!(
                    "direct frame capture failed: {}; file fallback failed: {}",
                    direct_errors.join("; "),
                    error.message()
                ))
            }
        })?;
        let output_path = file_metadata.output_path.clone();
        let frame = load_image_file(&output_path).map_err(|error| {
            PeekabooXError::new(format!("failed to decode captured image: {error}"))
        });
        remove_best_effort(&output_path, "capture frame fallback screenshot");
        let frame = frame?;

        Ok(CaptureFrameMetadata {
            frame,
            backend_name: file_metadata.backend_name,
            backend_kind: file_metadata.backend_kind,
            source: CaptureFrameSource::FileFallback,
        })
    }

    pub fn capture_region_frame(&self, region: Rect) -> Result<CaptureFrameMetadata> {
        validate_region(region)?;
        let environment = CaptureEnvironment::detect();
        let mut direct_errors = Vec::new();

        for backend in candidate_region_frame_backends(&environment) {
            match run_capture_tool_stdout_region(backend.tool, region)
                .and_then(|bytes| decode_image_bytes(&bytes))
                .and_then(|frame| validate_region_capture_frame(frame, region))
            {
                Ok(frame) => {
                    return Ok(CaptureFrameMetadata {
                        frame,
                        backend_name: backend.name().to_owned(),
                        backend_kind: backend.backend_kind(),
                        source: CaptureFrameSource::DirectStdout,
                    });
                }
                Err(error) => {
                    direct_errors.push(format!("{}: {}", backend.name(), error.message()))
                }
            }
        }

        let mut metadata = self.capture_screen_frame().map_err(|error| {
            if direct_errors.is_empty() {
                error
            } else {
                PeekabooXError::new(format!(
                    "direct region capture failed: {}; full-frame fallback failed: {}",
                    direct_errors.join("; "),
                    error.message()
                ))
            }
        })?;
        metadata.frame = crop_frame(&metadata.frame, region)?;
        metadata.source = CaptureFrameSource::FullFrameCrop;
        Ok(metadata)
    }
}

impl CaptureBackend for CommandCaptureBackend {
    fn capture_screen(&self) -> Result<CaptureFrame> {
        self.capture_screen_frame().map(|metadata| metadata.frame)
    }

    fn capture_region(&self, region: Rect) -> Result<CaptureFrame> {
        self.capture_region_frame(region)
            .map(|metadata| metadata.frame)
    }
}

#[derive(Debug, Default)]
pub struct DmaBufCaptureBackend;

impl DmaBufCaptureBackend {
    pub fn detect_backend(&self) -> Result<DetectedZeroCopyBackend> {
        let environment = CaptureEnvironment::detect();
        select_zero_copy_backend(&environment)
            .ok_or_else(|| PeekabooXError::new(zero_copy_unavailable_message(&environment)))
    }

    pub fn open_screen_cast_stream(&self) -> Result<PipeWireScreenCastStream> {
        let backend = self.detect_backend()?;
        open_pipewire_screencast_stream(backend)
    }

    pub fn capture_screen_dmabuf(&self) -> Result<DmaBufFrameDescriptor> {
        let stream = self.open_screen_cast_stream()?;
        pipewire_consumer::capture_first_dmabuf_frame(stream)
    }
}

pub fn capture_screen_to_file(output: impl AsRef<Path>) -> Result<CaptureFileMetadata> {
    CommandCaptureBackend.capture_screen_to_file(output)
}

pub fn capture_screen_frame() -> Result<CaptureFrameMetadata> {
    CommandCaptureBackend.capture_screen_frame()
}

pub fn capture_region_frame(region: Rect) -> Result<CaptureFrameMetadata> {
    CommandCaptureBackend.capture_region_frame(region)
}

pub fn encode_frame_png(frame: &CaptureFrame) -> Result<Vec<u8>> {
    let rgba = frame_to_rgba_bytes(frame)?;
    let mut output = Vec::new();
    PngEncoder::new(&mut output)
        .write_image(&rgba, frame.width, frame.height, ColorType::Rgba8.into())
        .map_err(|error| PeekabooXError::new(format!("failed to encode frame as PNG: {error}")))?;
    Ok(output)
}

pub fn write_frame_png(frame: &CaptureFrame, output: impl AsRef<Path>) -> Result<u64> {
    let output = absolute_output_path(output.as_ref())?;
    prepare_output_parent(&output)?;
    let png = encode_frame_png(frame)?;
    std::fs::write(&output, &png).map_err(|error| {
        PeekabooXError::new(format!("failed to write {}: {error}", output.display()))
    })?;
    Ok(png.len() as u64)
}

pub fn capture_region_to_file(
    region: Rect,
    output: impl AsRef<Path>,
) -> Result<CaptureFileMetadata> {
    let metadata = capture_region_frame(region)?;
    let output = absolute_output_path(output.as_ref())?;
    let bytes_written = write_frame_png(&metadata.frame, &output)?;
    Ok(CaptureFileMetadata {
        output_path: output,
        backend_name: metadata.backend_name,
        backend_kind: metadata.backend_kind,
        bytes_written,
    })
}

pub fn capture_screen_dmabuf() -> Result<DmaBufFrameDescriptor> {
    DmaBufCaptureBackend.capture_screen_dmabuf()
}

pub fn open_pipewire_screencast() -> Result<PipeWireScreenCastStream> {
    DmaBufCaptureBackend.open_screen_cast_stream()
}

pub fn capture_pipewire_dmabuf_frame(
    stream: PipeWireScreenCastStream,
) -> Result<DmaBufFrameDescriptor> {
    pipewire_consumer::capture_first_dmabuf_frame(stream)
}

#[cfg(not(feature = "pipewire-backend"))]
mod pipewire_consumer {
    use super::*;

    pub(super) fn capture_first_dmabuf_frame(
        stream: PipeWireScreenCastStream,
    ) -> Result<DmaBufFrameDescriptor> {
        Err(PeekabooXError::new(format!(
            "opened PipeWire ScreenCast stream node {} via {}, but this build was compiled without the `pipewire-backend` feature required to consume DMA-BUF buffers",
            stream.stream_node_id, stream.backend_name
        )))
    }
}

#[cfg(feature = "pipewire-backend")]
mod pipewire_consumer {
    use super::*;
    use libspa_sys as spa_sys;
    use pipewire as pw;
    use pw::{properties::properties, spa};
    use std::cell::{Cell, RefCell};
    use std::os::fd::{BorrowedFd, IntoRawFd, OwnedFd};
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    #[derive(Debug, Clone, Copy)]
    struct StreamVideoFormat {
        width: u32,
        height: u32,
        pixel_format: PixelFormat,
        fourcc: u32,
        modifier: u64,
        plane_count: usize,
    }

    #[derive(Debug, Default)]
    struct ConsumerState {
        format: Option<StreamVideoFormat>,
        result: Option<std::result::Result<DmaBufFrameDescriptor, String>>,
    }

    impl ConsumerState {
        fn finish_ok(&mut self, descriptor: DmaBufFrameDescriptor) {
            if self.result.is_none() {
                self.result = Some(Ok(descriptor));
            }
        }

        fn finish_err(&mut self, message: impl Into<String>) {
            if self.result.is_none() {
                self.result = Some(Err(message.into()));
            }
        }
    }

    pub(super) fn capture_first_dmabuf_frame(
        screen_cast: PipeWireScreenCastStream,
    ) -> Result<DmaBufFrameDescriptor> {
        let mainloop = pw::main_loop::MainLoopRc::new(None)
            .map_err(|error| pipewire_error("create PipeWire main loop", error))?;
        let context = pw::context::ContextRc::new(&mainloop, None)
            .map_err(|error| pipewire_error("create PipeWire context", error))?;
        let pipewire_fd = duplicate_pipewire_fd(&screen_cast)?;
        let core = context
            .connect_fd_rc(pipewire_fd, None)
            .map_err(|error| pipewire_error("connect to portal PipeWire remote", error))?;
        pipewire_roundtrip(&mainloop, &core, "initialize portal PipeWire remote")?;

        let state = Rc::new(RefCell::new(ConsumerState::default()));
        let target_object = screen_cast.pipewire_serial.map(|serial| serial.to_string());
        let stream_properties = if let Some(target_object) = target_object.as_deref() {
            properties! {
                *pw::keys::MEDIA_TYPE => "Video",
                *pw::keys::MEDIA_CATEGORY => "Capture",
                *pw::keys::MEDIA_ROLE => "Screen",
                *pw::keys::TARGET_OBJECT => target_object,
            }
        } else {
            properties! {
                *pw::keys::MEDIA_TYPE => "Video",
                *pw::keys::MEDIA_CATEGORY => "Capture",
                *pw::keys::MEDIA_ROLE => "Screen",
            }
        };
        let capture_stream =
            pw::stream::StreamBox::new(&core, "peekaboox-dmabuf-capture", stream_properties)
                .map_err(|error| pipewire_error("create PipeWire capture stream", error))?;

        let state_loop = mainloop.clone();
        let param_loop = mainloop.clone();
        let process_loop = mainloop.clone();
        let _listener = capture_stream
            .add_local_listener_with_user_data(state.clone())
            .state_changed(move |_, state, old, new| match new {
                pw::stream::StreamState::Error(error) => {
                    state
                        .borrow_mut()
                        .finish_err(format!("PipeWire stream entered error state: {error}"));
                    state_loop.quit();
                }
                pw::stream::StreamState::Unconnected
                    if old != pw::stream::StreamState::Unconnected =>
                {
                    state
                        .borrow_mut()
                        .finish_err("PipeWire stream disconnected before a DMA-BUF frame arrived");
                    state_loop.quit();
                }
                _ => {}
            })
            .param_changed(move |stream, state, id, param| {
                if id != spa::param::ParamType::Format.as_raw() {
                    return;
                }
                if let Err(error) = negotiate_dmabuf_buffers(stream, state, param) {
                    state.borrow_mut().finish_err(error.message().to_owned());
                    param_loop.quit();
                }
            })
            .process(move |stream, state| {
                if collect_first_frame(stream, state) {
                    process_loop.quit();
                }
            })
            .register()
            .map_err(|error| pipewire_error("register PipeWire stream listener", error))?;

        let initial_format_param = build_initial_format_param()?;
        let initial_format_pod = spa::pod::Pod::from_bytes(&initial_format_param)
            .ok_or_else(|| PeekabooXError::new("failed to build PipeWire EnumFormat parameter"))?;
        let mut params = [initial_format_pod];
        let target_node_id = if target_object.is_some() {
            None
        } else {
            Some(screen_cast.stream_node_id)
        };
        capture_stream
            .connect(
                spa::utils::Direction::Input,
                target_node_id,
                pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::DONT_RECONNECT,
                &mut params,
            )
            .map_err(|error| {
                pipewire_error(
                    format!(
                        "connect PipeWire stream to portal node {}",
                        screen_cast.stream_node_id
                    ),
                    error,
                )
            })?;

        wait_for_frame_result(&mainloop, state, Duration::from_secs(5))
    }

    fn pipewire_roundtrip(
        mainloop: &pw::main_loop::MainLoopRc,
        core: &pw::core::Core,
        operation: &str,
    ) -> Result<()> {
        let done = Rc::new(Cell::new(false));
        let done_callback = done.clone();
        let pending = core
            .sync(0)
            .map_err(|error| pipewire_error(operation, error))?;
        let _listener = core
            .add_listener_local()
            .done(move |id, seq| {
                if id == pw::core::PW_ID_CORE && seq == pending {
                    done_callback.set(true);
                }
            })
            .register();

        let started = Instant::now();
        let timeout = Duration::from_secs(2);
        while !done.get() && started.elapsed() < timeout {
            let remaining = timeout.saturating_sub(started.elapsed());
            let step = remaining.min(Duration::from_millis(50));
            let dispatched = mainloop.loop_().iterate(step);
            if dispatched < 0 {
                return Err(PeekabooXError::new(format!(
                    "PipeWire main loop iteration failed with status {dispatched}"
                )));
            }
        }

        if done.get() {
            Ok(())
        } else {
            Err(PeekabooXError::new(format!(
                "timed out waiting to {operation}"
            )))
        }
    }

    fn wait_for_frame_result(
        mainloop: &pw::main_loop::MainLoopRc,
        state: Rc<RefCell<ConsumerState>>,
        timeout: Duration,
    ) -> Result<DmaBufFrameDescriptor> {
        let started = Instant::now();
        while state.borrow().result.is_none() && started.elapsed() < timeout {
            let remaining = timeout.saturating_sub(started.elapsed());
            let step = remaining.min(Duration::from_millis(50));
            let dispatched = mainloop.loop_().iterate(step);
            if dispatched < 0 {
                return Err(PeekabooXError::new(format!(
                    "PipeWire main loop iteration failed with status {dispatched}"
                )));
            }
        }

        let result = state.borrow_mut().result.take().ok_or_else(|| {
            PeekabooXError::new(format!(
                "timed out after {}ms waiting for a PipeWire DMA-BUF frame",
                timeout.as_millis()
            ))
        })?;

        result.map_err(PeekabooXError::new)
    }

    fn duplicate_pipewire_fd(stream: &PipeWireScreenCastStream) -> Result<OwnedFd> {
        let raw_fd = stream.pipewire_raw_fd();
        if raw_fd < 0 {
            return Err(PeekabooXError::new(
                "portal returned an invalid PipeWire remote file descriptor",
            ));
        }

        // The D-Bus OwnedFd remains owned by PipeWireScreenCastStream; the
        // PipeWire context needs its own fd because connect_fd_rc consumes it.
        let borrowed = unsafe { BorrowedFd::borrow_raw(raw_fd) };
        borrowed.try_clone_to_owned().map_err(|error| {
            PeekabooXError::new(format!(
                "failed to duplicate PipeWire remote file descriptor: {error}"
            ))
        })
    }

    fn negotiate_dmabuf_buffers(
        stream: &pw::stream::Stream,
        state: &Rc<RefCell<ConsumerState>>,
        param: Option<&spa::pod::Pod>,
    ) -> Result<()> {
        let Some(param) = param else {
            return Ok(());
        };

        let (media_type, media_subtype) =
            spa::param::format_utils::parse_format(param).map_err(|error| {
                PeekabooXError::new(format!(
                    "failed to parse PipeWire format parameter: {error}"
                ))
            })?;
        if media_type != spa::param::format::MediaType::Video
            || media_subtype != spa::param::format::MediaSubtype::Raw
        {
            return Ok(());
        }

        let format = parse_video_format(param)?;
        state.borrow_mut().format = Some(format);

        let buffer_param = build_dmabuf_buffer_param(format)?;
        let buffer_pod = spa::pod::Pod::from_bytes(&buffer_param).ok_or_else(|| {
            PeekabooXError::new("failed to build PipeWire DMA-BUF Buffers parameter")
        })?;
        let mut params = [buffer_pod];
        stream
            .update_params(&mut params)
            .map_err(|error| pipewire_error("update PipeWire DMA-BUF buffer parameters", error))
    }

    fn parse_video_format(param: &spa::pod::Pod) -> Result<StreamVideoFormat> {
        let mut info = spa::param::video::VideoInfoRaw::default();
        info.parse(param).map_err(|error| {
            PeekabooXError::new(format!(
                "failed to parse PipeWire raw video format: {error}"
            ))
        })?;

        let size = info.size();
        let (pixel_format, fourcc, plane_count) = video_format_descriptor(info.format())?;
        let modifier = if info
            .flags()
            .contains(spa::param::video::VideoFlags::MODIFIER)
        {
            info.modifier()
        } else {
            DRM_FORMAT_MOD_INVALID
        };

        Ok(StreamVideoFormat {
            width: size.width,
            height: size.height,
            pixel_format,
            fourcc,
            modifier,
            plane_count,
        })
    }

    fn collect_first_frame(
        stream: &pw::stream::Stream,
        state: &Rc<RefCell<ConsumerState>>,
    ) -> bool {
        let Some(mut buffer) = stream.dequeue_buffer() else {
            return false;
        };

        let Some(format) = state.borrow().format else {
            state
                .borrow_mut()
                .finish_err("PipeWire delivered a buffer before negotiating a video format");
            return true;
        };

        match descriptor_from_pipewire_buffer(format, &mut buffer) {
            Ok(descriptor) => state.borrow_mut().finish_ok(descriptor),
            Err(error) => state.borrow_mut().finish_err(error.message().to_owned()),
        }
        true
    }

    fn descriptor_from_pipewire_buffer(
        format: StreamVideoFormat,
        buffer: &mut pw::buffer::Buffer<'_>,
    ) -> Result<DmaBufFrameDescriptor> {
        let datas = buffer.datas_mut();
        if datas.is_empty() {
            return Err(PeekabooXError::new(
                "PipeWire buffer did not expose any data planes",
            ));
        }
        if datas[0].type_() != spa::buffer::DataType::DmaBuf {
            return Err(PeekabooXError::new(format!(
                "PipeWire returned {:?} data instead of DMA-BUF",
                datas[0].type_()
            )));
        }

        let mut planes = Vec::with_capacity(format.plane_count);
        for data in datas
            .iter()
            .filter(|data| data.type_() == spa::buffer::DataType::DmaBuf)
        {
            if planes.len() == format.plane_count {
                break;
            }

            let chunk = data.chunk();
            let stride = u32::try_from(chunk.stride()).map_err(|_| {
                PeekabooXError::new(format!(
                    "PipeWire DMA-BUF plane {} has negative stride {}",
                    planes.len(),
                    chunk.stride()
                ))
            })?;

            let fd = data.fd();
            let owned_fd = if fd >= 0 {
                let borrowed = unsafe { BorrowedFd::borrow_raw(fd) };
                borrowed.try_clone_to_owned().map_err(|error| {
                    PeekabooXError::new(format!(
                        "failed to duplicate PipeWire DMA-BUF plane fd: {error}"
                    ))
                })?
            } else {
                return Err(PeekabooXError::new(format!(
                    "PipeWire DMA-BUF plane {} has an invalid file descriptor",
                    planes.len()
                )));
            };

            planes.push(DmaBufPlaneCandidate {
                fd: owned_fd.into_raw_fd(),
                offset: chunk.offset(),
                stride,
            });
        }

        if planes.len() < format.plane_count {
            return Err(PeekabooXError::new(format!(
                "PipeWire DMA-BUF buffer exposed {} plane(s), expected at least {}",
                planes.len(),
                format.plane_count
            )));
        }

        dmabuf_descriptor_from_candidate(DmaBufFrameCandidate {
            width: format.width,
            height: format.height,
            format: format.pixel_format,
            fourcc: format.fourcc,
            modifier: format.modifier,
            planes,
        })
    }

    fn build_initial_format_param() -> Result<Vec<u8>> {
        let object = spa::pod::object!(
            spa::utils::SpaTypes::ObjectParamFormat,
            spa::param::ParamType::EnumFormat,
            spa::pod::property!(
                spa::param::format::FormatProperties::MediaType,
                Id,
                spa::param::format::MediaType::Video
            ),
            spa::pod::property!(
                spa::param::format::FormatProperties::MediaSubtype,
                Id,
                spa::param::format::MediaSubtype::Raw
            ),
            spa::pod::property!(
                spa::param::format::FormatProperties::VideoFormat,
                Choice,
                Enum,
                Id,
                spa::param::video::VideoFormat::BGRx,
                spa::param::video::VideoFormat::BGRx,
                spa::param::video::VideoFormat::BGRA,
                spa::param::video::VideoFormat::RGBx,
                spa::param::video::VideoFormat::RGBA,
                spa::param::video::VideoFormat::RGB,
            ),
            spa::pod::property!(
                spa::param::format::FormatProperties::VideoModifier,
                Long,
                DRM_FORMAT_MOD_INVALID as i64
            ),
            spa::pod::property!(
                spa::param::format::FormatProperties::VideoSize,
                Choice,
                Range,
                Rectangle,
                spa::utils::Rectangle {
                    width: 1920,
                    height: 1080
                },
                spa::utils::Rectangle {
                    width: 1,
                    height: 1
                },
                spa::utils::Rectangle {
                    width: 8192,
                    height: 8192
                }
            ),
            spa::pod::property!(
                spa::param::format::FormatProperties::VideoFramerate,
                Choice,
                Range,
                Fraction,
                spa::utils::Fraction { num: 30, denom: 1 },
                spa::utils::Fraction { num: 0, denom: 1 },
                spa::utils::Fraction { num: 240, denom: 1 }
            ),
        );
        serialize_pod_object(object)
    }

    fn build_dmabuf_buffer_param(format: StreamVideoFormat) -> Result<Vec<u8>> {
        let data_type_mask = 1_i32
            .checked_shl(spa::buffer::DataType::DmaBuf.as_raw())
            .ok_or_else(|| PeekabooXError::new("invalid PipeWire DMA-BUF data type mask"))?;
        let object = spa::pod::Object {
            type_: spa::utils::SpaTypes::ObjectParamBuffers.as_raw(),
            id: spa::param::ParamType::Buffers.as_raw(),
            properties: vec![
                spa::pod::Property::new(
                    spa_sys::SPA_PARAM_BUFFERS_buffers,
                    spa::pod::Value::Int(3),
                ),
                spa::pod::Property::new(
                    spa_sys::SPA_PARAM_BUFFERS_blocks,
                    spa::pod::Value::Int(format.plane_count as i32),
                ),
                spa::pod::Property::new(
                    spa_sys::SPA_PARAM_BUFFERS_dataType,
                    spa::pod::Value::Int(data_type_mask),
                ),
            ],
        };
        serialize_pod_object(object)
    }

    fn serialize_pod_object(object: spa::pod::Object) -> Result<Vec<u8>> {
        spa::pod::serialize::PodSerializer::serialize(
            std::io::Cursor::new(Vec::new()),
            &spa::pod::Value::Object(object),
        )
        .map(|serialized| serialized.0.into_inner())
        .map_err(|error| PeekabooXError::new(format!("failed to serialize PipeWire POD: {error}")))
    }

    fn video_format_descriptor(
        format: spa::param::video::VideoFormat,
    ) -> Result<(PixelFormat, u32, usize)> {
        if format == spa::param::video::VideoFormat::BGRx {
            Ok((PixelFormat::Bgra8, fourcc_code(b'X', b'R', b'2', b'4'), 1))
        } else if format == spa::param::video::VideoFormat::BGRA {
            Ok((PixelFormat::Bgra8, fourcc_code(b'A', b'R', b'2', b'4'), 1))
        } else if format == spa::param::video::VideoFormat::RGBx {
            Ok((PixelFormat::Rgba8, fourcc_code(b'X', b'B', b'2', b'4'), 1))
        } else if format == spa::param::video::VideoFormat::RGBA {
            Ok((PixelFormat::Rgba8, fourcc_code(b'A', b'B', b'2', b'4'), 1))
        } else if format == spa::param::video::VideoFormat::RGB {
            Ok((PixelFormat::Rgb8, fourcc_code(b'R', b'G', b'2', b'4'), 1))
        } else {
            Err(PeekabooXError::new(format!(
                "unsupported PipeWire DMA-BUF video format {format:?}"
            )))
        }
    }

    fn pipewire_error(context: impl AsRef<str>, error: impl std::fmt::Display) -> PeekabooXError {
        PeekabooXError::new(format!("failed to {}: {error}", context.as_ref()))
    }
}

pub fn select_backend(
    environment: &CaptureEnvironment,
    output: &Path,
) -> Option<DetectedCaptureBackend> {
    candidate_backends(environment, output).into_iter().next()
}

pub fn select_frame_backend(environment: &CaptureEnvironment) -> Option<DetectedCaptureBackend> {
    candidate_frame_backends(environment).into_iter().next()
}

pub fn select_region_frame_backend(
    environment: &CaptureEnvironment,
) -> Option<DetectedCaptureBackend> {
    candidate_region_frame_backends(environment)
        .into_iter()
        .next()
}

pub fn select_zero_copy_backend(
    environment: &CaptureEnvironment,
) -> Option<DetectedZeroCopyBackend> {
    zero_copy_capture_capabilities(environment)
        .into_iter()
        .find(|capability| capability.availability.is_available())
        .map(|capability| DetectedZeroCopyBackend {
            transport: capability.transport,
            backend_kind: capability.backend_kind,
        })
}

pub fn zero_copy_capture_capabilities(
    environment: &CaptureEnvironment,
) -> Vec<ZeroCopyCaptureCapability> {
    let availability = if environment.session_type != SessionType::Wayland {
        ZeroCopyAvailability::UnsupportedSession
    } else if !environment.pipewire_session_available {
        ZeroCopyAvailability::MissingPipeWireSession
    } else {
        ZeroCopyAvailability::Available
    };

    vec![ZeroCopyCaptureCapability {
        backend_name: ZeroCopyTransport::XdgDesktopPortalScreenCastPipeWireDmaBuf
            .name()
            .to_owned(),
        backend_kind: BackendKind::Portal,
        transport: ZeroCopyTransport::XdgDesktopPortalScreenCastPipeWireDmaBuf,
        availability,
    }]
}

fn zero_copy_unavailable_message(environment: &CaptureEnvironment) -> String {
    let detail = zero_copy_capture_capabilities(environment)
        .into_iter()
        .map(|capability| {
            format!(
                "{}: {:?}",
                capability.transport.name(),
                capability.availability
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    format!("no DMA-BUF zero-copy capture backend available ({detail})")
}

const PORTAL_DESTINATION: &str = "org.freedesktop.portal.Desktop";
const PORTAL_OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";
const PORTAL_SCREENCAST_INTERFACE: &str = "org.freedesktop.portal.ScreenCast";
const PORTAL_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

fn open_pipewire_screencast_stream(
    backend: DetectedZeroCopyBackend,
) -> Result<PipeWireScreenCastStream> {
    let connection = Connection::new_session().map_err(|error| {
        PeekabooXError::new(format!("failed to connect to session bus: {error}"))
    })?;
    let session_handle = screencast_create_session(&connection)?;
    screencast_select_sources(&connection, &session_handle)?;
    let stream = screencast_start(&connection, &session_handle)?;
    let pipewire_fd = screencast_open_pipewire_remote(&connection, &session_handle)?;

    Ok(PipeWireScreenCastStream {
        session_handle,
        stream_node_id: stream.node_id,
        pipewire_serial: stream.pipewire_serial,
        pipewire_fd,
        backend_name: backend.name().to_owned(),
        backend_kind: backend.backend_kind,
        _portal_connection: connection,
    })
}

fn screencast_create_session(connection: &Connection) -> Result<String> {
    let proxy = portal_proxy(connection);
    let mut options = portal_request_options("peekaboox_screencast_create");
    options.insert(
        "session_handle_token".to_owned(),
        Variant(Box::new(portal_token("peekaboox_screencast_session"))),
    );

    let (handle,): (dbus::Path<'static>,) = proxy
        .method_call(PORTAL_SCREENCAST_INTERFACE, "CreateSession", (options,))
        .map_err(|error| {
            PeekabooXError::new(format!(
                "xdg-desktop-portal ScreenCast.CreateSession failed: {error}"
            ))
        })?;
    let results = wait_for_portal_request(connection, handle, "ScreenCast.CreateSession")?;
    portal_result_object_path(&results, "session_handle")
}

fn screencast_select_sources(connection: &Connection, session_handle: &str) -> Result<()> {
    let proxy = portal_proxy(connection);
    let mut options = portal_request_options("peekaboox_screencast_select");
    options.insert("types".to_owned(), Variant(Box::new(1_u32)));
    options.insert("multiple".to_owned(), Variant(Box::new(false)));
    options.insert("cursor_mode".to_owned(), Variant(Box::new(2_u32)));
    let session = portal_path(session_handle)?;

    let (handle,): (dbus::Path<'static>,) = proxy
        .method_call(
            PORTAL_SCREENCAST_INTERFACE,
            "SelectSources",
            (session, options),
        )
        .map_err(|error| {
            PeekabooXError::new(format!(
                "xdg-desktop-portal ScreenCast.SelectSources failed: {error}"
            ))
        })?;
    wait_for_portal_request(connection, handle, "ScreenCast.SelectSources")?;
    Ok(())
}

fn screencast_start(
    connection: &Connection,
    session_handle: &str,
) -> Result<PortalScreenCastStream> {
    let proxy = portal_proxy(connection);
    let options = portal_request_options("peekaboox_screencast_start");
    let session = portal_path(session_handle)?;

    let (handle,): (dbus::Path<'static>,) = proxy
        .method_call(PORTAL_SCREENCAST_INTERFACE, "Start", (session, "", options))
        .map_err(|error| {
            PeekabooXError::new(format!(
                "xdg-desktop-portal ScreenCast.Start failed: {error}"
            ))
        })?;
    let results = wait_for_portal_request(connection, handle, "ScreenCast.Start")?;
    portal_first_stream(&results)
}

fn screencast_open_pipewire_remote(
    connection: &Connection,
    session_handle: &str,
) -> Result<dbus::arg::OwnedFd> {
    let proxy = portal_proxy(connection);
    let session = portal_path(session_handle)?;
    let options = PropMap::new();
    let (pipewire_fd,): (dbus::arg::OwnedFd,) = proxy
        .method_call(
            PORTAL_SCREENCAST_INTERFACE,
            "OpenPipeWireRemote",
            (session, options),
        )
        .map_err(|error| {
            PeekabooXError::new(format!(
                "xdg-desktop-portal ScreenCast.OpenPipeWireRemote failed: {error}"
            ))
        })?;
    Ok(pipewire_fd)
}

fn portal_proxy(connection: &Connection) -> dbus::blocking::Proxy<'_, &Connection> {
    connection.with_proxy(
        PORTAL_DESTINATION,
        PORTAL_OBJECT_PATH,
        PORTAL_REQUEST_TIMEOUT,
    )
}

fn portal_request_options(handle_prefix: &str) -> PropMap {
    let mut options = PropMap::new();
    options.insert(
        "handle_token".to_owned(),
        Variant(Box::new(portal_token(handle_prefix))),
    );
    options
}

fn portal_token(prefix: &str) -> String {
    format!(
        "{}_{}_{}",
        prefix,
        std::process::id(),
        monotonic_token_component()
    )
}

fn portal_path(value: &str) -> Result<dbus::Path<'static>> {
    dbus::Path::new(value.to_owned())
        .map(dbus::Path::into_static)
        .map_err(|error| {
            PeekabooXError::new(format!("invalid portal object path {value}: {error}"))
        })
}

fn wait_for_portal_request(
    connection: &Connection,
    handle: dbus::Path<'static>,
    operation: &str,
) -> Result<PropMap> {
    let (sender, receiver) = mpsc::channel();
    let match_rule =
        MatchRule::new_signal("org.freedesktop.portal.Request", "Response").with_path(handle);
    let _match_token = connection
        .add_match(
            match_rule,
            move |(response, results): (u32, PropMap), _connection: &Connection, _message| {
                let _ = sender.send((response, results));
                true
            },
        )
        .map_err(|error| {
            PeekabooXError::new(format!(
                "failed to subscribe to xdg-desktop-portal {operation} response: {error}"
            ))
        })?;

    let deadline = Instant::now() + PORTAL_REQUEST_TIMEOUT;
    while Instant::now() < deadline {
        connection
            .process(Duration::from_millis(250))
            .map_err(|error| {
                PeekabooXError::new(format!(
                    "failed while waiting for xdg-desktop-portal {operation} response: {error}"
                ))
            })?;

        if let Ok((response, results)) = receiver.try_recv() {
            return match response {
                0 => Ok(results),
                1 => Err(PeekabooXError::new(format!(
                    "xdg-desktop-portal {operation} request was cancelled"
                ))),
                other => Err(PeekabooXError::new(format!(
                    "xdg-desktop-portal {operation} request failed with response code {other}"
                ))),
            };
        }
    }

    Err(PeekabooXError::new(format!(
        "timed out waiting for xdg-desktop-portal {operation} response"
    )))
}

fn portal_result_object_path(results: &PropMap, key: &str) -> Result<String> {
    let value = portal_result(results, key)?;
    if let Some(path) = dbus::arg::cast::<dbus::Path<'static>>(value) {
        return Ok(path.to_string());
    }
    if let Some(path) = value.as_str() {
        return Ok(path.to_owned());
    }
    Err(PeekabooXError::new(format!(
        "xdg-desktop-portal response field {key:?} was not an object path"
    )))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PortalScreenCastStream {
    node_id: u32,
    pipewire_serial: Option<u64>,
}

#[cfg(test)]
fn portal_first_stream_node_id(results: &PropMap) -> Result<u32> {
    portal_first_stream(results).map(|stream| stream.node_id)
}

fn portal_first_stream(results: &PropMap) -> Result<PortalScreenCastStream> {
    let streams = portal_result(results, "streams")?;
    let mut streams = streams.as_iter().ok_or_else(|| {
        PeekabooXError::new("xdg-desktop-portal Start response field \"streams\" was not an array")
    })?;
    let stream = streams.next().ok_or_else(|| {
        PeekabooXError::new("xdg-desktop-portal Start response did not include any streams")
    })?;
    let mut fields = stream
        .as_iter()
        .ok_or_else(|| PeekabooXError::new("xdg-desktop-portal stream entry was not a struct"))?;
    let node_id = fields
        .next()
        .and_then(RefArg::as_u64)
        .ok_or_else(|| PeekabooXError::new("xdg-desktop-portal stream entry missed node id"))?;
    let node_id = u32::try_from(node_id)
        .map_err(|_| PeekabooXError::new("xdg-desktop-portal stream node id overflows u32"))?;
    let pipewire_serial = fields.next().and_then(portal_stream_pipewire_serial);

    Ok(PortalScreenCastStream {
        node_id,
        pipewire_serial,
    })
}

fn portal_stream_pipewire_serial(properties: &(dyn RefArg + '_)) -> Option<u64> {
    let mut fields = properties.as_iter()?;
    while let Some(key_arg) = fields.next() {
        let key = key_arg.as_str();
        let value = fields.next()?;
        if key != Some("pipewire-serial") {
            continue;
        }
        if let Some(serial) = portal_refarg_u64(value) {
            return Some(serial);
        }
    }
    None
}

fn portal_refarg_u64(value: &(dyn RefArg + '_)) -> Option<u64> {
    if let Some(value) = value.as_u64() {
        return Some(value);
    }

    value.as_iter()?.find_map(portal_refarg_u64)
}

fn portal_result<'a>(results: &'a PropMap, key: &str) -> Result<&'a (dyn RefArg + 'static)> {
    results
        .get(key)
        .map(|value| value.0.as_ref())
        .ok_or_else(|| {
            PeekabooXError::new(format!(
                "xdg-desktop-portal response did not include field {key:?}"
            ))
        })
}

pub fn candidate_backends(
    environment: &CaptureEnvironment,
    output: &Path,
) -> Vec<DetectedCaptureBackend> {
    let mut candidates = Vec::new();

    if environment.session_type == SessionType::Wayland {
        candidates.push(CaptureTool::XdgDesktopPortal);
        if environment.is_gnome() {
            candidates.push(CaptureTool::GnomeShellScreenshot);
        }
        candidates.push(CaptureTool::Grim);
        if environment.is_kde() {
            candidates.push(CaptureTool::Spectacle);
        }
    }

    if environment.session_type == SessionType::X11 {
        candidates.extend([
            CaptureTool::GnomeScreenshot,
            CaptureTool::Spectacle,
            CaptureTool::Scrot,
            CaptureTool::Maim,
            CaptureTool::ImageMagickImport,
            CaptureTool::Xwd,
        ]);
    }

    candidates.extend([
        CaptureTool::GnomeScreenshot,
        CaptureTool::Spectacle,
        CaptureTool::Scrot,
        CaptureTool::Maim,
        CaptureTool::ImageMagickImport,
        CaptureTool::Xwd,
    ]);

    candidates
        .into_iter()
        .filter_map(|tool| {
            if tool.is_available(environment) && tool.supports_output(output) {
                Some(DetectedCaptureBackend {
                    tool,
                    session_type: environment.session_type,
                })
            } else {
                None
            }
        })
        .collect()
}

fn candidate_frame_backends(environment: &CaptureEnvironment) -> Vec<DetectedCaptureBackend> {
    candidate_backends(environment, Path::new("screenshot.png"))
        .into_iter()
        .filter(|backend| backend.tool.supports_stdout_capture())
        .collect()
}

fn candidate_region_frame_backends(
    environment: &CaptureEnvironment,
) -> Vec<DetectedCaptureBackend> {
    candidate_backends(environment, Path::new("screenshot.png"))
        .into_iter()
        .filter(|backend| backend.tool.supports_stdout_region_capture())
        .collect()
}

fn run_capture_tool(tool: CaptureTool, output: &Path) -> Result<()> {
    if tool == CaptureTool::XdgDesktopPortal {
        return capture_with_xdg_desktop_portal(output);
    }

    let status_output = match tool {
        CaptureTool::XdgDesktopPortal => unreachable!("handled before command dispatch"),
        CaptureTool::GnomeShellScreenshot => Command::new("gdbus")
            .args([
                "call",
                "--session",
                "--dest",
                "org.gnome.Shell.Screenshot",
                "--object-path",
                "/org/gnome/Shell/Screenshot",
                "--method",
                "org.gnome.Shell.Screenshot.Screenshot",
                "false",
                "false",
            ])
            .arg(output)
            .output(),
        CaptureTool::Grim => Command::new("grim").arg(output).output(),
        CaptureTool::GnomeScreenshot => Command::new("gnome-screenshot")
            .arg("-f")
            .arg(output)
            .output(),
        CaptureTool::Spectacle => Command::new("spectacle")
            .args(["-b", "-n", "-o"])
            .arg(output)
            .output(),
        CaptureTool::Scrot => Command::new("scrot").arg(output).output(),
        CaptureTool::Maim => Command::new("maim").arg(output).output(),
        CaptureTool::ImageMagickImport => Command::new("import")
            .args(["-window", "root"])
            .arg(output)
            .output(),
        CaptureTool::Xwd => Command::new("xwd")
            .args(["-root", "-silent", "-out"])
            .arg(output)
            .output(),
    }?;

    if !status_output.status.success() {
        return Err(PeekabooXError::new(format!(
            "screenshot backend {} failed with status {}; stderr: {}",
            tool.name(),
            status_output.status,
            String::from_utf8_lossy(&status_output.stderr).trim()
        )));
    }

    if tool == CaptureTool::GnomeShellScreenshot
        && !String::from_utf8_lossy(&status_output.stdout).contains("(true,")
    {
        return Err(PeekabooXError::new(format!(
            "GNOME Shell screenshot API returned an unsuccessful response: {}",
            String::from_utf8_lossy(&status_output.stdout).trim()
        )));
    }

    Ok(())
}

fn run_capture_tool_stdout(tool: CaptureTool) -> Result<Vec<u8>> {
    let status_output = match tool {
        CaptureTool::Grim => Command::new("grim").arg("-").output(),
        CaptureTool::Maim => Command::new("maim").output(),
        CaptureTool::ImageMagickImport => Command::new("import")
            .args(["-window", "root", "png:-"])
            .output(),
        other => {
            return Err(PeekabooXError::new(format!(
                "screenshot backend {} does not support stdout frame capture",
                other.name()
            )));
        }
    }?;

    if !status_output.status.success() {
        return Err(PeekabooXError::new(format!(
            "screenshot backend {} stdout capture failed with status {}; stderr: {}",
            tool.name(),
            status_output.status,
            String::from_utf8_lossy(&status_output.stderr).trim()
        )));
    }
    if status_output.stdout.is_empty() {
        return Err(PeekabooXError::new(format!(
            "screenshot backend {} produced empty stdout",
            tool.name()
        )));
    }

    Ok(status_output.stdout)
}

fn run_capture_tool_stdout_region(tool: CaptureTool, region: Rect) -> Result<Vec<u8>> {
    validate_region(region)?;
    let status_output = match tool {
        CaptureTool::Grim => Command::new("grim")
            .args(["-g", &grim_region_geometry(region), "-"])
            .output(),
        CaptureTool::Maim => Command::new("maim")
            .args(["-g", &x11_region_geometry(region)])
            .output(),
        CaptureTool::ImageMagickImport => Command::new("import")
            .args([
                "-window",
                "root",
                "-crop",
                &x11_region_geometry(region),
                "png:-",
            ])
            .output(),
        other => {
            return Err(PeekabooXError::new(format!(
                "screenshot backend {} does not support stdout region capture",
                other.name()
            )));
        }
    }?;

    if !status_output.status.success() {
        return Err(PeekabooXError::new(format!(
            "screenshot backend {} stdout region capture failed with status {}; stderr: {}",
            tool.name(),
            status_output.status,
            String::from_utf8_lossy(&status_output.stderr).trim()
        )));
    }
    if status_output.stdout.is_empty() {
        return Err(PeekabooXError::new(format!(
            "screenshot backend {} produced empty region stdout",
            tool.name()
        )));
    }

    Ok(status_output.stdout)
}

fn grim_region_geometry(region: Rect) -> String {
    format!(
        "{},{} {}x{}",
        region.x, region.y, region.width, region.height
    )
}

fn x11_region_geometry(region: Rect) -> String {
    format!(
        "{}x{}+{}+{}",
        region.width, region.height, region.x, region.y
    )
}

fn capture_with_xdg_desktop_portal(output: &Path) -> Result<()> {
    let connection = Connection::new_session().map_err(|error| {
        PeekabooXError::new(format!("failed to connect to session bus: {error}"))
    })?;

    let token = format!(
        "peekaboox{}_{}",
        std::process::id(),
        monotonic_token_component()
    );

    let proxy = connection.with_proxy(
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        Duration::from_secs(5),
    );

    let mut options = PropMap::new();
    options.insert("handle_token".to_owned(), Variant(Box::new(token)));
    options.insert("interactive".to_owned(), Variant(Box::new(false)));

    let (handle,): (dbus::Path<'static>,) = proxy
        .method_call(
            "org.freedesktop.portal.Screenshot",
            "Screenshot",
            ("", options),
        )
        .map_err(|error| {
            PeekabooXError::new(format!(
                "xdg-desktop-portal Screenshot call failed: {error}"
            ))
        })?;

    let (sender, receiver) = mpsc::channel();
    let match_rule =
        MatchRule::new_signal("org.freedesktop.portal.Request", "Response").with_path(handle);

    let _match_token = connection
        .add_match(
            match_rule,
            move |(response, results): (u32, PropMap), _connection: &Connection, _message| {
                let uri = results
                    .get("uri")
                    .and_then(|value| value.0.as_str())
                    .map(str::to_owned);
                let _ = sender.send((response, uri));
                true
            },
        )
        .map_err(|error| {
            PeekabooXError::new(format!(
                "failed to subscribe to xdg-desktop-portal response: {error}"
            ))
        })?;

    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        connection
            .process(Duration::from_millis(250))
            .map_err(|error| {
                PeekabooXError::new(format!(
                    "failed while waiting for xdg-desktop-portal response: {error}"
                ))
            })?;

        if let Ok((response, uri)) = receiver.try_recv() {
            return handle_portal_response(response, uri, output);
        }
    }

    Err(PeekabooXError::new(
        "timed out waiting for xdg-desktop-portal screenshot response",
    ))
}

fn handle_portal_response(response: u32, uri: Option<String>, output: &Path) -> Result<()> {
    match response {
        0 => {
            let uri = uri.ok_or_else(|| {
                PeekabooXError::new("xdg-desktop-portal response did not include a screenshot URI")
            })?;
            let source = file_uri_to_path(&uri)?;
            std::fs::copy(&source, output).map_err(|error| {
                PeekabooXError::new(format!(
                    "failed to copy portal screenshot from {} to {}: {error}",
                    source.display(),
                    output.display()
                ))
            })?;
            Ok(())
        }
        1 => Err(PeekabooXError::new(
            "xdg-desktop-portal screenshot request was cancelled",
        )),
        other => Err(PeekabooXError::new(format!(
            "xdg-desktop-portal screenshot request failed with response code {other}"
        ))),
    }
}

fn load_image_file(path: &Path) -> Result<CaptureFrame> {
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

fn decode_image_bytes(bytes: &[u8]) -> Result<CaptureFrame> {
    let image = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| PeekabooXError::new(format!("failed to detect image format: {error}")))?
        .decode()
        .map_err(|error| PeekabooXError::new(format!("failed to decode image bytes: {error}")))?;

    Ok(capture_frame_from_image(image))
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

fn crop_frame(frame: &CaptureFrame, region: Rect) -> Result<CaptureFrame> {
    validate_frame(frame, "crop source")?;
    validate_region(region)?;
    if i64::from(region.x) + i64::from(region.width) > i64::from(frame.width)
        || i64::from(region.y) + i64::from(region.height) > i64::from(frame.height)
    {
        return Err(PeekabooXError::new(
            "capture region exceeds full-frame capture bounds",
        ));
    }

    let bytes_per_pixel = bytes_per_pixel(frame.format);
    let row_bytes = usize::try_from(region.width)
        .ok()
        .and_then(|width| width.checked_mul(bytes_per_pixel))
        .ok_or_else(|| PeekabooXError::new("capture region row size overflows usize"))?;
    let output_stride = u32::try_from(row_bytes)
        .map_err(|_| PeekabooXError::new("capture region stride overflows u32"))?;
    let output_len = usize::try_from(region.height)
        .ok()
        .and_then(|height| height.checked_mul(row_bytes))
        .ok_or_else(|| PeekabooXError::new("capture region data length overflows usize"))?;
    let source_stride = usize::try_from(frame.stride)
        .map_err(|_| PeekabooXError::new("capture source stride overflows usize"))?;
    let mut data = Vec::with_capacity(output_len);

    for y in region.y..region_end_i32(region.y, region.height)? {
        let row_offset = usize::try_from(y)
            .ok()
            .and_then(|row| row.checked_mul(source_stride))
            .ok_or_else(|| PeekabooXError::new("capture region row offset overflows usize"))?;
        let column_offset = usize::try_from(region.x)
            .ok()
            .and_then(|column| column.checked_mul(bytes_per_pixel))
            .ok_or_else(|| PeekabooXError::new("capture region column offset overflows usize"))?;
        let offset = row_offset
            .checked_add(column_offset)
            .ok_or_else(|| PeekabooXError::new("capture region offset overflows usize"))?;
        data.extend_from_slice(
            frame
                .data
                .get(offset..offset + row_bytes)
                .ok_or_else(|| PeekabooXError::new("capture region exceeds frame data"))?,
        );
    }

    Ok(CaptureFrame {
        width: region.width,
        height: region.height,
        stride: output_stride,
        format: frame.format,
        data,
    })
}

fn frame_to_rgba_bytes(frame: &CaptureFrame) -> Result<Vec<u8>> {
    validate_frame(frame, "PNG encode source")?;
    let width = usize::try_from(frame.width)
        .map_err(|_| PeekabooXError::new("frame width overflows usize"))?;
    let height = usize::try_from(frame.height)
        .map_err(|_| PeekabooXError::new("frame height overflows usize"))?;
    let stride = usize::try_from(frame.stride)
        .map_err(|_| PeekabooXError::new("frame stride overflows usize"))?;
    let bytes_per_pixel = bytes_per_pixel(frame.format);
    let row_bytes = width
        .checked_mul(bytes_per_pixel)
        .ok_or_else(|| PeekabooXError::new("frame row size overflows usize"))?;
    let output_len = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| PeekabooXError::new("PNG output size overflows usize"))?;
    let mut rgba = Vec::with_capacity(output_len);

    for y in 0..height {
        let row_offset = y
            .checked_mul(stride)
            .ok_or_else(|| PeekabooXError::new("frame row offset overflows usize"))?;
        let row_end = row_offset
            .checked_add(row_bytes)
            .ok_or_else(|| PeekabooXError::new("frame row end overflows usize"))?;
        let row = frame
            .data
            .get(row_offset..row_end)
            .ok_or_else(|| PeekabooXError::new("frame row exceeds frame data"))?;
        for pixel in row.chunks_exact(bytes_per_pixel) {
            match frame.format {
                PixelFormat::Rgb8 => rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]),
                PixelFormat::Rgba8 => rgba.extend_from_slice(pixel),
                PixelFormat::Bgra8 => {
                    rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
                }
            }
        }
    }

    Ok(rgba)
}

fn validate_frame(frame: &CaptureFrame, name: &str) -> Result<()> {
    if frame.width == 0 || frame.height == 0 {
        return Err(PeekabooXError::new(format!(
            "{name} frame dimensions must be greater than zero"
        )));
    }
    let bytes_per_pixel = bytes_per_pixel(frame.format);
    let row_bytes = usize::try_from(frame.width)
        .ok()
        .and_then(|width| width.checked_mul(bytes_per_pixel))
        .ok_or_else(|| PeekabooXError::new(format!("{name} frame row size overflows usize")))?;
    let stride = usize::try_from(frame.stride)
        .map_err(|_| PeekabooXError::new(format!("{name} frame stride overflows usize")))?;
    if stride < row_bytes {
        return Err(PeekabooXError::new(format!(
            "{name} frame stride {} is smaller than row width {row_bytes}",
            frame.stride
        )));
    }
    let required_len = usize::try_from(frame.height)
        .ok()
        .and_then(|height| height.checked_sub(1))
        .and_then(|rows_before_last| rows_before_last.checked_mul(stride))
        .and_then(|prefix| prefix.checked_add(row_bytes))
        .ok_or_else(|| PeekabooXError::new(format!("{name} frame data length overflows usize")))?;
    if frame.data.len() < required_len {
        return Err(PeekabooXError::new(format!(
            "{name} frame data is too short: expected at least {required_len} bytes, got {}",
            frame.data.len()
        )));
    }

    Ok(())
}

fn validate_region(region: Rect) -> Result<()> {
    if region.width == 0 || region.height == 0 {
        return Err(PeekabooXError::new(
            "capture region dimensions must be greater than zero",
        ));
    }
    if region.x < 0 || region.y < 0 {
        return Err(PeekabooXError::new(
            "capture region origin must be non-negative",
        ));
    }
    region_end_i32(region.x, region.width)?;
    region_end_i32(region.y, region.height)?;
    Ok(())
}

fn validate_region_capture_frame(frame: CaptureFrame, region: Rect) -> Result<CaptureFrame> {
    if frame.width == region.width && frame.height == region.height {
        return Ok(frame);
    }

    Err(PeekabooXError::new(format!(
        "region capture returned {}x{} but expected {}x{}",
        frame.width, frame.height, region.width, region.height
    )))
}

fn bytes_per_pixel(format: PixelFormat) -> usize {
    match format {
        PixelFormat::Bgra8 | PixelFormat::Rgba8 => 4,
        PixelFormat::Rgb8 => 3,
    }
}

fn region_end_i32(origin: i32, length: u32) -> Result<i32> {
    i32::try_from(i64::from(origin) + i64::from(length))
        .map_err(|_| PeekabooXError::new("capture region overflows i32"))
}

fn file_uri_to_path(uri: &str) -> Result<PathBuf> {
    let Some(path) = uri.strip_prefix("file://") else {
        return Err(PeekabooXError::new(format!(
            "xdg-desktop-portal returned a non-file URI: {uri}"
        )));
    };

    Ok(PathBuf::from(percent_decode(path)?))
}

fn percent_decode(value: &str) -> Result<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(PeekabooXError::new(format!(
                    "invalid percent encoding in file URI path: {value}"
                )));
            }

            let high = hex_value(bytes[index + 1])?;
            let low = hex_value(bytes[index + 2])?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }

    String::from_utf8(decoded)
        .map_err(|error| PeekabooXError::new(format!("file URI path is not UTF-8: {error}")))
}

fn hex_value(value: u8) -> Result<u8> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(PeekabooXError::new(format!(
            "invalid hex digit in percent encoding: {}",
            value as char
        ))),
    }
}

fn monotonic_token_component() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

fn frame_capture_temp_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "peekaboox-frame-capture-{}-{}.png",
        std::process::id(),
        monotonic_token_component()
    ))
}

fn absolute_output_path(output: &Path) -> Result<PathBuf> {
    if output.is_absolute() {
        Ok(output.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(output))
    }
}

fn prepare_output_parent(output: &Path) -> Result<()> {
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }

    Ok(())
}

fn remove_best_effort(path: &Path, description: &str) {
    if let Err(error) = std::fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        eprintln!("failed to remove {description} {}: {error}", path.display());
    }
}

fn detect_pipewire_session(commands: &HashSet<String>) -> bool {
    if commands.contains("pw-cli") || commands.contains("pipewire") {
        return true;
    }
    if std::env::var_os("PIPEWIRE_REMOTE").is_some() {
        return true;
    }

    pipewire_runtime_socket().is_some_and(|socket| socket.exists())
}

fn pipewire_runtime_socket() -> Option<PathBuf> {
    std::env::var_os("PIPEWIRE_RUNTIME_DIR")
        .or_else(|| std::env::var_os("XDG_RUNTIME_DIR"))
        .map(PathBuf::from)
        .map(|runtime_dir| runtime_dir.join("pipewire-0"))
}

fn command_exists(command: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };

    std::env::split_paths(&paths).any(|path| path.join(command).is_file())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::os::fd::IntoRawFd;
    use std::path::Path;

    use dbus::arg::{PropMap, Variant};
    use image::ImageEncoder;

    use super::{
        CaptureBackend, CaptureEnvironment, CaptureTool, DRM_FORMAT_MOD_INVALID,
        DmaBufFrameCandidate, DmaBufFrameDescriptor, DmaBufImportTarget, DmaBufMemoryLayout,
        DmaBufPlaneCandidate, DmaBufPlaneDescriptor, DmaBufSynchronization, SessionType,
        UnimplementedCaptureBackend, ZeroCopyAvailability, crop_frame, decode_image_bytes,
        dmabuf_descriptor_from_candidate, file_uri_to_path, fourcc_code, grim_region_geometry,
        import_dmabuf_frame, portal_first_stream, portal_first_stream_node_id,
        portal_result_object_path, prepare_dmabuf_import_descriptor, select_backend,
        select_frame_backend, select_region_frame_backend, select_zero_copy_backend,
        validate_region_capture_frame, x11_region_geometry, zero_copy_capture_capabilities,
    };
    use peekaboox_core::{CaptureFrame, PixelFormat, Rect};

    #[test]
    fn unimplemented_backend_returns_typed_error() {
        let backend = UnimplementedCaptureBackend;
        let error = backend.capture_screen().unwrap_err();

        assert!(error.message().contains("capture backend"));
    }

    #[test]
    fn selects_xdg_desktop_portal_first_on_gnome_wayland() {
        let environment = environment(SessionType::Wayland, Some("ubuntu:GNOME"), ["gdbus"]);

        let backend = select_backend(&environment, Path::new("screenshot.png")).unwrap();

        assert_eq!(backend.tool, CaptureTool::XdgDesktopPortal);
    }

    #[test]
    fn selects_xdg_desktop_portal_first_on_non_gnome_wayland() {
        let environment = environment(SessionType::Wayland, Some("sway"), ["grim"]);

        let backend = select_backend(&environment, Path::new("screenshot.png")).unwrap();

        assert_eq!(backend.tool, CaptureTool::XdgDesktopPortal);
    }

    #[test]
    fn rejects_xwd_for_png_output() {
        let environment = environment(SessionType::X11, None, ["xwd"]);

        let backend = select_backend(&environment, Path::new("screenshot.png"));

        assert!(backend.is_none());
    }

    #[test]
    fn allows_xwd_for_xwd_output() {
        let environment = environment(SessionType::X11, None, ["xwd"]);

        let backend = select_backend(&environment, Path::new("screenshot.xwd")).unwrap();

        assert_eq!(backend.tool, CaptureTool::Xwd);
    }

    #[test]
    fn selects_stdout_frame_backend_when_available() {
        let environment = environment(SessionType::Wayland, Some("sway"), ["grim"]);

        let backend = select_frame_backend(&environment).unwrap();

        assert_eq!(backend.tool, CaptureTool::Grim);
    }

    #[test]
    fn skips_file_only_backends_for_stdout_frame_capture() {
        let environment = environment(SessionType::Wayland, Some("GNOME"), ["gdbus"]);

        assert!(select_frame_backend(&environment).is_none());
    }

    #[test]
    fn selects_stdout_region_frame_backend_when_available() {
        let environment = environment(SessionType::X11, None, ["maim"]);

        let backend = select_region_frame_backend(&environment).unwrap();

        assert_eq!(backend.tool, CaptureTool::Maim);
    }

    #[test]
    fn selects_zero_copy_backend_on_wayland_with_pipewire() {
        let environment = environment_with_pipewire(SessionType::Wayland, Some("GNOME"), []);

        let backend = select_zero_copy_backend(&environment).unwrap();

        assert_eq!(
            backend.name(),
            "xdg-desktop-portal-screencast-pipewire-dmabuf"
        );
        assert_eq!(backend.backend_kind, peekaboox_core::BackendKind::Portal);
    }

    #[test]
    fn reports_missing_pipewire_for_wayland_zero_copy() {
        let environment = environment(SessionType::Wayland, Some("sway"), []);

        let capability = zero_copy_capture_capabilities(&environment)
            .into_iter()
            .next()
            .unwrap();

        assert_eq!(
            capability.availability,
            ZeroCopyAvailability::MissingPipeWireSession
        );
        assert!(select_zero_copy_backend(&environment).is_none());
    }

    #[test]
    fn skips_zero_copy_backend_on_x11() {
        let environment = environment_with_pipewire(SessionType::X11, None, ["pw-cli"]);

        let capability = zero_copy_capture_capabilities(&environment)
            .into_iter()
            .next()
            .unwrap();

        assert_eq!(
            capability.availability,
            ZeroCopyAvailability::UnsupportedSession
        );
        assert!(select_zero_copy_backend(&environment).is_none());
    }

    #[test]
    fn reads_portal_session_handle_from_response_results() {
        let mut results = PropMap::new();
        results.insert(
            "session_handle".to_owned(),
            Variant(Box::new(
                dbus::Path::new("/org/freedesktop/portal/desktop/session/1").unwrap(),
            )),
        );

        let session_handle = portal_result_object_path(&results, "session_handle").unwrap();

        assert_eq!(session_handle, "/org/freedesktop/portal/desktop/session/1");
    }

    #[test]
    fn reads_portal_stream_node_id_from_response_results() {
        let mut results = PropMap::new();
        let streams: Vec<(u32, PropMap)> = vec![(42, PropMap::new())];
        results.insert("streams".to_owned(), Variant(Box::new(streams)));

        let node_id = portal_first_stream_node_id(&results).unwrap();

        assert_eq!(node_id, 42);
    }

    #[test]
    fn reads_portal_stream_pipewire_serial_from_response_results() {
        let mut stream_properties = PropMap::new();
        stream_properties.insert("pipewire-serial".to_owned(), Variant(Box::new(9001_u64)));
        let streams: Vec<(u32, PropMap)> = vec![(42, stream_properties)];
        let mut results = PropMap::new();
        results.insert("streams".to_owned(), Variant(Box::new(streams)));

        let stream = portal_first_stream(&results).unwrap();

        assert_eq!(stream.node_id, 42);
        assert_eq!(stream.pipewire_serial, Some(9001));
    }

    #[test]
    fn uses_linux_drm_invalid_modifier_value() {
        assert_eq!(DRM_FORMAT_MOD_INVALID, 0x00ff_ffff_ffff_ffff);
    }

    #[test]
    fn builds_dmabuf_descriptor_from_pipewire_plane_metadata() {
        let fd = owned_test_fd();
        let descriptor = dmabuf_descriptor_from_candidate(DmaBufFrameCandidate {
            width: 1920,
            height: 1080,
            format: PixelFormat::Bgra8,
            fourcc: fourcc_code(b'X', b'R', b'2', b'4'),
            modifier: DRM_FORMAT_MOD_INVALID,
            planes: vec![DmaBufPlaneCandidate {
                fd,
                offset: 128,
                stride: 7680,
            }],
        })
        .unwrap();

        assert_eq!(descriptor.width, 1920);
        assert_eq!(descriptor.height, 1080);
        assert_eq!(descriptor.format, PixelFormat::Bgra8);
        assert_eq!(descriptor.fourcc, fourcc_code(b'X', b'R', b'2', b'4'));
        assert_eq!(descriptor.planes.len(), 1);
        assert_eq!(descriptor.planes[0].fd, fd);
        assert_eq!(descriptor.planes[0].offset, 128);
        assert_eq!(descriptor.planes[0].stride, 7680);
        assert_eq!(descriptor.planes[0].modifier, DRM_FORMAT_MOD_INVALID);
    }

    #[test]
    fn rejects_dmabuf_descriptor_without_planes() {
        let error = dmabuf_descriptor_from_candidate(DmaBufFrameCandidate {
            width: 1920,
            height: 1080,
            format: PixelFormat::Bgra8,
            fourcc: fourcc_code(b'X', b'R', b'2', b'4'),
            modifier: DRM_FORMAT_MOD_INVALID,
            planes: Vec::new(),
        })
        .unwrap_err();

        assert!(error.message().contains("did not expose any planes"));
    }

    #[test]
    fn prepares_dmabuf_import_descriptor_for_compute_backend() {
        let descriptor = single_plane_dmabuf_descriptor();
        let fd = descriptor.planes[0].fd;

        let import =
            prepare_dmabuf_import_descriptor(&descriptor, DmaBufImportTarget::Compute).unwrap();

        assert_eq!(import.target, DmaBufImportTarget::Compute);
        assert_eq!(import.width, 1920);
        assert_eq!(import.height, 1080);
        assert_eq!(import.format, PixelFormat::Bgra8);
        assert_eq!(import.fourcc, fourcc_code(b'X', b'R', b'2', b'4'));
        assert_eq!(import.memory_layout, DmaBufMemoryLayout::SinglePlane);
        assert_eq!(import.synchronization, DmaBufSynchronization::Implicit);
        assert_eq!(import.planes.len(), 1);
        assert_eq!(import.planes[0].plane_index, 0);
        assert_eq!(import.planes[0].fd, fd);
        assert_eq!(import.planes[0].offset, 128);
        assert_eq!(import.planes[0].stride, 7680);
        assert_eq!(import.planes[0].modifier, DRM_FORMAT_MOD_INVALID);
    }

    #[test]
    fn imports_dmabuf_frame_with_named_target_backend() {
        let descriptor = single_plane_dmabuf_descriptor();

        let imported = import_dmabuf_frame(&descriptor, DmaBufImportTarget::Vulkan).unwrap();

        assert_eq!(imported.backend_name, "dmabuf-import-vulkan");
        assert_eq!(imported.backend_kind, DmaBufImportTarget::Vulkan);
        assert_eq!(imported.descriptor.target, DmaBufImportTarget::Vulkan);
    }

    #[test]
    fn rejects_dmabuf_import_when_fourcc_does_not_match_pixel_format() {
        let mut descriptor = single_plane_dmabuf_descriptor();
        descriptor.fourcc = fourcc_code(b'R', b'G', b'2', b'4');

        let error =
            prepare_dmabuf_import_descriptor(&descriptor, DmaBufImportTarget::Compute).unwrap_err();

        assert!(error.message().contains("does not match"));
    }

    #[test]
    fn rejects_dmabuf_import_when_stride_is_too_small() {
        let mut descriptor = single_plane_dmabuf_descriptor();
        descriptor.planes[0].stride = 4;

        let error =
            prepare_dmabuf_import_descriptor(&descriptor, DmaBufImportTarget::Compute).unwrap_err();

        assert!(error.message().contains("smaller than row width"));
    }

    #[test]
    fn formats_region_geometry_for_capture_tools() {
        let region = Rect::new(10, 20, 300, 120);

        assert_eq!(grim_region_geometry(region), "10,20 300x120");
        assert_eq!(x11_region_geometry(region), "300x120+10+20");
    }

    #[test]
    fn decodes_png_bytes_to_rgba_frame() {
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&[1, 2, 3, 4], 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();

        let frame = decode_image_bytes(&png).unwrap();

        assert_eq!(frame.width, 1);
        assert_eq!(frame.height, 1);
        assert_eq!(frame.stride, 4);
        assert_eq!(frame.data, vec![1, 2, 3, 4]);
    }

    #[test]
    fn crops_frame_region_with_preserved_format() {
        let frame = CaptureFrame {
            width: 3,
            height: 2,
            stride: 9,
            format: PixelFormat::Rgb8,
            data: vec![
                1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18,
            ],
        };

        let cropped = crop_frame(&frame, Rect::new(1, 0, 2, 2)).unwrap();

        assert_eq!(cropped.width, 2);
        assert_eq!(cropped.height, 2);
        assert_eq!(cropped.stride, 6);
        assert_eq!(cropped.format, PixelFormat::Rgb8);
        assert_eq!(cropped.data, vec![4, 5, 6, 7, 8, 9, 13, 14, 15, 16, 17, 18]);
    }

    #[test]
    fn rejects_invalid_capture_region() {
        let backend = super::CommandCaptureBackend;
        let error = backend
            .capture_region(Rect::new(-1, 0, 10, 10))
            .unwrap_err();

        assert!(error.message().contains("non-negative"));
    }

    #[test]
    fn rejects_mismatched_region_capture_frame_dimensions() {
        let frame = CaptureFrame {
            width: 9,
            height: 10,
            stride: 36,
            format: PixelFormat::Rgba8,
            data: vec![0; 9 * 10 * 4],
        };
        let error = validate_region_capture_frame(frame, Rect::new(0, 0, 10, 10)).unwrap_err();

        assert!(error.message().contains("expected 10x10"));
    }

    #[test]
    fn decodes_file_uri_paths() {
        let path = file_uri_to_path("file:///tmp/PeekabooX%20Screenshot.png").unwrap();

        assert_eq!(path, Path::new("/tmp/PeekabooX Screenshot.png"));
    }

    fn environment<const N: usize>(
        session_type: SessionType,
        current_desktop: Option<&str>,
        commands: [&str; N],
    ) -> CaptureEnvironment {
        CaptureEnvironment {
            session_type,
            current_desktop: current_desktop.map(str::to_owned),
            pipewire_session_available: false,
            commands: commands
                .into_iter()
                .map(str::to_owned)
                .collect::<HashSet<_>>(),
        }
    }

    fn environment_with_pipewire<const N: usize>(
        session_type: SessionType,
        current_desktop: Option<&str>,
        commands: [&str; N],
    ) -> CaptureEnvironment {
        let mut environment = environment(session_type, current_desktop, commands);
        environment.pipewire_session_available = true;
        environment
    }

    fn single_plane_dmabuf_descriptor() -> DmaBufFrameDescriptor {
        DmaBufFrameDescriptor {
            width: 1920,
            height: 1080,
            format: PixelFormat::Bgra8,
            fourcc: fourcc_code(b'X', b'R', b'2', b'4'),
            planes: vec![DmaBufPlaneDescriptor {
                fd: owned_test_fd(),
                offset: 128,
                stride: 7680,
                modifier: DRM_FORMAT_MOD_INVALID,
            }],
        }
    }

    fn owned_test_fd() -> i32 {
        std::fs::File::open("/dev/null").unwrap().into_raw_fd()
    }
}
