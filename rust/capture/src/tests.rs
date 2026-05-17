use std::collections::HashSet;
use std::os::fd::IntoRawFd;
use std::path::Path;

use dbus::arg::{PropMap, Variant};
use image::ImageEncoder;

use super::{
    CaptureBackend, CaptureEnvironment, CaptureTool, DRM_FORMAT_MOD_INVALID, DmaBufFrameCandidate,
    DmaBufFrameDescriptor, DmaBufImportTarget, DmaBufMemoryLayout, DmaBufPlaneCandidate,
    DmaBufPlaneDescriptor, DmaBufSynchronization, SessionType, UnimplementedCaptureBackend,
    ZeroCopyAvailability, capture_backend_capabilities, crop_frame, decode_image_bytes,
    detect_pipewire_session_from, dmabuf_descriptor_from_candidate, file_uri_to_path, fourcc_code,
    grim_region_geometry, import_dmabuf_frame, portal_first_stream, portal_first_stream_node_id,
    portal_request_path_from_unique_name, portal_result_object_path,
    prepare_dmabuf_import_descriptor, select_backend, select_frame_backend,
    select_region_frame_backend, select_zero_copy_backend, validate_region_capture_frame,
    x11_region_geometry, zero_copy_capture_capabilities,
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
fn selects_xwd_for_xwd_output_when_other_x11_backends_exist() {
    let environment = environment(SessionType::X11, None, ["scrot", "xwd"]);

    let backend = select_backend(&environment, Path::new("screenshot.xwd")).unwrap();
    let names = super::candidate_backends(&environment, Path::new("screenshot.xwd"))
        .iter()
        .map(|backend| backend.name())
        .collect::<Vec<_>>();

    assert_eq!(backend.tool, CaptureTool::Xwd);
    assert_eq!(names, vec!["xwd"]);
}

#[test]
fn does_not_select_portal_for_xwd_output() {
    let environment = environment(SessionType::Wayland, Some("sway"), ["grim"]);

    let backend = select_backend(&environment, Path::new("screenshot.xwd"));

    assert!(backend.is_none());
}

#[test]
fn x11_candidates_are_not_duplicated() {
    let environment = environment(SessionType::X11, None, ["scrot", "maim"]);

    let backends = super::candidate_backends(&environment, Path::new("screenshot.png"));
    let names = backends
        .iter()
        .map(|backend| backend.name())
        .collect::<Vec<_>>();

    assert_eq!(names, vec!["scrot", "maim"]);
}

#[test]
fn diagnostics_include_missing_and_unsupported_backends() {
    let environment = environment(SessionType::X11, None, ["xwd"]);

    let capabilities = capture_backend_capabilities(&environment, Path::new("screenshot.png"));
    let xwd = capabilities
        .iter()
        .find(|capability| capability.name == "xwd")
        .unwrap();
    let grim = capabilities
        .iter()
        .find(|capability| capability.name == "grim")
        .unwrap();

    assert!(xwd.available);
    assert!(!xwd.supports_output);
    assert_eq!(
        xwd.reason.as_deref(),
        Some("xwd only supports .xwd output for file capture")
    );
    assert_eq!(
        grim.reason.as_deref(),
        Some("not considered for current session")
    );
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

#[cfg(feature = "pipewire-backend")]
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

#[cfg(feature = "pipewire-backend")]
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

#[cfg(not(feature = "pipewire-backend"))]
#[test]
fn reports_missing_pipewire_backend_when_feature_is_disabled() {
    let environment = environment_with_pipewire(SessionType::Wayland, Some("GNOME"), []);

    let capability = zero_copy_capture_capabilities(&environment)
        .into_iter()
        .next()
        .unwrap();

    assert_eq!(
        capability.availability,
        ZeroCopyAvailability::MissingPipeWireBackend
    );
    assert!(select_zero_copy_backend(&environment).is_none());
}

#[test]
fn pipewire_session_detection_ignores_command_presence_only() {
    assert!(!detect_pipewire_session_from(false, false));
    assert!(detect_pipewire_session_from(true, false));
    assert!(detect_pipewire_session_from(false, true));
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
fn derives_portal_request_path_before_method_call() {
    let path = portal_request_path_from_unique_name(":1.42", "peekaboox_token").unwrap();

    assert_eq!(
        path.to_string(),
        "/org/freedesktop/portal/desktop/request/1_42/peekaboox_token"
    );
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
fn closes_candidate_plane_fds_when_validation_fails() {
    let valid_fd = owned_test_fd();
    let invalid_fd = owned_test_fd();

    let error = dmabuf_descriptor_from_candidate(DmaBufFrameCandidate {
        width: 1920,
        height: 1080,
        format: PixelFormat::Bgra8,
        fourcc: fourcc_code(b'X', b'R', b'2', b'4'),
        modifier: DRM_FORMAT_MOD_INVALID,
        planes: vec![
            DmaBufPlaneCandidate {
                fd: valid_fd,
                offset: 128,
                stride: 7680,
            },
            DmaBufPlaneCandidate {
                fd: invalid_fd,
                offset: 128,
                stride: 0,
            },
        ],
    })
    .unwrap_err();

    assert!(error.message().contains("zero stride"));
    assert!(!fd_is_open(valid_fd));
    assert!(!fd_is_open(invalid_fd));
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
    assert_ne!(import.planes[0].fd, fd);
    assert!(fd_is_open(import.planes[0].fd));
    assert_eq!(import.planes[0].offset, 128);
    assert_eq!(import.planes[0].stride, 7680);
    assert_eq!(import.planes[0].modifier, DRM_FORMAT_MOD_INVALID);
}

#[test]
fn import_descriptor_keeps_fd_valid_after_source_descriptor_drops() {
    let descriptor = single_plane_dmabuf_descriptor();
    let source_fd = descriptor.planes[0].fd;
    let import =
        prepare_dmabuf_import_descriptor(&descriptor, DmaBufImportTarget::Compute).unwrap();
    let import_fd = import.planes[0].fd;

    drop(descriptor);

    assert!(!fd_is_open(source_fd));
    assert!(fd_is_open(import_fd));
    drop(import);
    assert!(!fd_is_open(import_fd));
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

fn fd_is_open(fd: i32) -> bool {
    if fd < 0 {
        return false;
    }
    let borrowed = unsafe { std::os::fd::BorrowedFd::borrow_raw(fd) };
    borrowed.try_clone_to_owned().is_ok()
}
