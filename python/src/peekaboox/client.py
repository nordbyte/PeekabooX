from __future__ import annotations

import json
from dataclasses import dataclass, field
from importlib import import_module
from os import PathLike
from types import ModuleType
from typing import Any, Iterable, Sequence


DEFAULT_GRPC_TARGET = "127.0.0.1:47777"


class MissingGrpcDependencyError(ImportError):
    """Raised when the optional gRPC runtime dependencies are unavailable."""


class PeekabooXClientError(RuntimeError):
    """Raised when the daemon returns a gRPC error."""


@dataclass(frozen=True, slots=True)
class Rect:
    x: int
    y: int
    width: int
    height: int


@dataclass(frozen=True, slots=True)
class Point:
    x: int
    y: int


@dataclass(frozen=True, slots=True)
class WindowInfo:
    id: str
    title: str
    app_id: str | None
    bounds: Rect
    focused: bool
    state: str


@dataclass(frozen=True, slots=True)
class WindowBackendReport:
    backend_name: str
    backend_kind: str
    raw_window_count: int
    matched_window_count: int
    selected: bool
    error: str | None


@dataclass(frozen=True, slots=True)
class WindowListResult:
    backend_name: str
    backend_kind: str
    warnings: tuple[str, ...]
    backend_reports: tuple[WindowBackendReport, ...]
    windows: tuple[WindowInfo, ...]


@dataclass(frozen=True, slots=True)
class UiElement:
    id: str
    role: str
    label: str | None
    bounds: Rect
    confidence: float
    center: Point | None = None
    states: tuple[str, ...] = ()
    window_id: str | None = None
    window_title: str | None = None
    app_id: str | None = None
    parent_id: str | None = None
    child_ids: tuple[str, ...] = ()


@dataclass(frozen=True, slots=True)
class CaptureMetadata:
    width: int
    height: int
    backend: str
    captured_at_unix_ms: int


@dataclass(frozen=True, slots=True)
class CaptureScreenResult:
    image: bytes
    mime_type: str
    semantic_tree: tuple[UiElement, ...]
    metadata: CaptureMetadata | None


@dataclass(frozen=True, slots=True)
class CaptureDeltaResult:
    stream_id: str
    sequence: int
    low_bandwidth: bool
    full_frame: bool
    frame_width: int
    frame_height: int
    pixel_format: str
    capture_region: Rect | None
    changed_bounds: Rect | None
    changed_pixels: int
    changed_ratio: float
    patch_stride: int
    patch: bytes
    metadata: CaptureMetadata | None


@dataclass(frozen=True, slots=True)
class CaptureBackend:
    name: str
    backend_kind: str
    command: str | None
    available: bool
    supports_output: bool
    supports_file_capture: bool
    supports_stdout_capture: bool
    supports_stdout_region_capture: bool
    selected: bool
    reason: str | None


@dataclass(frozen=True, slots=True)
class ZeroCopyBackend:
    name: str
    backend_kind: str
    transport: str
    availability: str
    selected: bool
    pipewire_backend_feature_enabled: bool
    egl_backend_feature_enabled: bool
    reason: str | None


@dataclass(frozen=True, slots=True)
class CaptureBackendProbeResult:
    probe: str
    ok: bool
    backend_name: str | None
    backend_kind: str | None
    detail: str
    output_path: str | None
    bytes_written: int | None
    width: int | None
    height: int | None


@dataclass(frozen=True, slots=True)
class CaptureBackendsResult:
    session_type: str
    desktop: str | None
    pipewire_session_available: bool
    pipewire_backend_feature_enabled: bool
    egl_backend_feature_enabled: bool
    output_path: str
    region: Rect | None
    image_backends: tuple[CaptureBackend, ...]
    zero_copy_backends: tuple[ZeroCopyBackend, ...]
    probes: tuple[CaptureBackendProbeResult, ...]
    warnings: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class OcrBlock:
    text: str
    element: UiElement


@dataclass(frozen=True, slots=True)
class OcrResult:
    backend_name: str
    text: str
    blocks: tuple[OcrBlock, ...]
    words: tuple[OcrBlock, ...]
    warnings: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class VisualDiffResult:
    compared_region: Rect
    compared_pixels: int
    changed_pixels: int
    changed_ratio: float
    mean_absolute_error: float
    max_channel_delta: int
    changed_bounds: Rect | None
    matches: bool


@dataclass(frozen=True, slots=True)
class UiStateResult:
    state: str
    compared_transitions: int
    stable_transitions: int
    loading_transitions: int
    trailing_stable_transitions: int
    latest_diff: VisualDiffResult
    max_changed_ratio: float
    mean_changed_ratio: float
    changed_bounds: Rect | None


@dataclass(frozen=True, slots=True)
class DetectUiElementsResult:
    backend_name: str
    backend_kind: str
    warnings: tuple[str, ...]
    elements: tuple[UiElement, ...]


@dataclass(frozen=True, slots=True)
class FindElementResult:
    backend_name: str
    backend_kind: str
    warnings: tuple[str, ...]
    elements: tuple[UiElement, ...]
    cache_hit: bool = False
    cache_age_ms: int = 0
    vision_fallback_used: bool = False


@dataclass(frozen=True, slots=True)
class ActionResult:
    ok: bool
    message: str
    backend_name: str | None = None
    backend_kind: str | None = None


@dataclass(frozen=True, slots=True)
class DesktopActionResult:
    app: str
    action: str
    detail: str
    backend_name: str
    verified: bool = False
    verification_detail: str | None = None


@dataclass(frozen=True, slots=True)
class DesktopLocateResult:
    app: str
    target: str
    x: int
    y: int
    rect: Rect | None
    source: str


@dataclass(frozen=True, slots=True)
class DesktopProfileCommand:
    program: str
    args: tuple[str, ...]
    display: str
    available: bool | None = None


@dataclass(frozen=True, slots=True)
class DesktopProfileTarget:
    name: str
    supports: tuple[str, ...]
    sources: tuple[str, ...]
    can_locate: bool
    can_click: bool
    can_drag: bool
    can_type: bool
    can_assert_present: bool
    can_assert_active: bool
    can_assert_contains: bool
    accessibility_selector: str | None
    visual_layout: bool
    visual_rect: bool


@dataclass(frozen=True, slots=True)
class DesktopProfileAvailability:
    checked: bool
    installed: bool | None
    command_available: bool | None
    desktop_entry_available: bool | None
    available_commands: tuple[str, ...]
    available_desktop_ids: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class DesktopProfile:
    id: str
    aliases: tuple[str, ...]
    search_name: str
    desktop_ids: tuple[str, ...]
    commands: tuple[DesktopProfileCommand, ...]
    targets: tuple[DesktopProfileTarget, ...]
    availability: DesktopProfileAvailability


@dataclass(frozen=True, slots=True)
class DesktopProfilesResult:
    schema_version: str
    count: int
    profiles: tuple[DesktopProfile, ...]


@dataclass(frozen=True, slots=True)
class DesktopState:
    active_window: WindowInfo | None
    windows: tuple[WindowInfo, ...]
    elements: tuple[UiElement, ...]


@dataclass(frozen=True, slots=True)
class DmaBufProbeResult:
    import_target: str
    backend_name: str
    stream_node_id: int
    pipewire_serial: int | None
    width: int
    height: int
    pixel_format: str
    fourcc: int
    planes: int
    memory_layout: str
    synchronization: str
    egl_version: str | None
    egl_modifiers: bool | None
    texture_id: int | None


@dataclass(frozen=True, slots=True)
class PluginTool:
    name: str
    description: str
    capabilities: tuple[str, ...]
    input_schema: dict[str, Any]


@dataclass(frozen=True, slots=True)
class Plugin:
    id: str
    name: str
    version: str
    description: str | None
    root_dir: str
    manifest_path: str
    capabilities: tuple[str, ...]
    entrypoint_kind: str | None
    entrypoint_command: tuple[str, ...]
    tools: tuple[PluginTool, ...]
    metadata: dict[str, str]


@dataclass(frozen=True, slots=True)
class PluginDiscoveryError:
    path: str
    message: str


@dataclass(frozen=True, slots=True)
class PluginListResult:
    sdk_version: str
    plugins: tuple[Plugin, ...]
    errors: tuple[PluginDiscoveryError, ...]


@dataclass(frozen=True, slots=True)
class PluginToolExecutionResult:
    ok: bool
    plugin_id: str
    tool: str
    exit_code: int
    stdout: str
    stderr: str
    result: Any | None
    error: str | None


@dataclass(slots=True)
class PeekabooXClient:
    """Synchronous Python client for the local PeekabooX daemon gRPC API."""

    target: str = DEFAULT_GRPC_TARGET
    timeout_seconds: float = 5.0
    max_receive_message_bytes: int = 64 * 1024 * 1024
    max_send_message_bytes: int = 64 * 1024 * 1024
    stub: Any | None = field(default=None, repr=False)
    messages: ModuleType | Any | None = field(default=None, repr=False)
    _grpc: ModuleType | Any | None = field(default=None, init=False, repr=False)
    _channel: Any | None = field(default=None, init=False, repr=False)

    def __post_init__(self) -> None:
        if self.stub is not None and self.messages is not None:
            return

        grpc_module, messages, services = _load_grpc_modules()
        self._grpc = grpc_module
        if self.messages is None:
            self.messages = messages
        if self.stub is None:
            self._channel = grpc_module.insecure_channel(
                self.target,
                options=[
                    ("grpc.max_receive_message_length", self.max_receive_message_bytes),
                    ("grpc.max_send_message_length", self.max_send_message_bytes),
                ],
            )
            self.stub = services.PeekabooXStub(self._channel)

    def close(self) -> None:
        if self._channel is not None:
            close = getattr(self._channel, "close", None)
            if close is not None:
                close()

    def capture_screen(
        self,
        include_semantic_tree: bool = False,
        region: Rect | None = None,
        window_id: str | None = None,
    ) -> CaptureScreenResult:
        request = self.messages.CaptureScreenRequest(
            target=_capture_target(self.messages, region=region, window_id=window_id),
            include_semantic_tree=include_semantic_tree,
        )
        response = self._call("CaptureScreen", request)
        metadata = _message_field(response, "metadata")
        return CaptureScreenResult(
            image=response.image,
            mime_type=response.mime_type,
            semantic_tree=tuple(_ui_element_from_proto(element) for element in response.semantic_tree),
            metadata=_capture_metadata_from_proto(metadata) if metadata is not None else None,
        )

    def capture_delta(
        self,
        stream_id: str = "default",
        reset: bool = False,
        region: Rect | None = None,
        window_id: str | None = None,
        per_channel_threshold: int | None = None,
        low_bandwidth: bool = True,
    ) -> CaptureDeltaResult:
        request_kwargs: dict[str, Any] = {
            "stream_id": stream_id,
            "reset": reset,
            "target": _capture_target(self.messages, region=region, window_id=window_id),
        }
        if per_channel_threshold is not None:
            request_kwargs["per_channel_threshold"] = per_channel_threshold
        if _message_accepts_field(self.messages.CaptureDeltaRequest, "low_bandwidth"):
            request_kwargs["low_bandwidth"] = low_bandwidth
        response = self._call("CaptureDelta", self.messages.CaptureDeltaRequest(**request_kwargs))
        return _capture_delta_from_proto(response)

    def capture_backends(
        self,
        output: str | PathLike[str] = "screenshot.png",
        region: Rect | None = None,
        diagnose: bool = False,
        probe: str = "none",
    ) -> CaptureBackendsResult:
        request_kwargs: dict[str, Any] = {
            "output": str(output),
            "diagnose": diagnose,
            "probe": _capture_backend_probe_value(self.messages, probe),
        }
        if region is not None:
            request_kwargs["region"] = _rect_to_proto(self.messages, region)
        response = self._call(
            "CaptureBackends",
            self.messages.CaptureBackendsRequest(**request_kwargs),
        )
        return _capture_backends_from_proto(response)

    def ocr_screen(
        self,
        region: Rect | None = None,
        language: str | None = None,
        image_path: str | PathLike[str] | None = None,
        window_id: str | None = None,
        window_title: str | None = None,
        app: str | None = None,
        page_segmentation_mode: int | None = None,
        engine_mode: int | None = None,
        dpi: int | None = None,
        min_confidence: float | None = None,
        whitelist: str | None = None,
        config: Sequence[str] = (),
        scale: float | None = None,
        grayscale: bool = False,
        threshold: int | None = None,
        invert: bool = False,
        contrast: float | None = None,
        deskew: bool = False,
    ) -> OcrResult:
        request_kwargs: dict[str, Any] = {}
        if region is not None:
            request_kwargs["region"] = _rect_to_proto(self.messages, region)
        if language is not None:
            request_kwargs["language"] = language
        optional_values = {
            "image_path": str(image_path) if image_path is not None else None,
            "window_id": window_id,
            "window_title": window_title,
            "app": app,
            "page_segmentation_mode": page_segmentation_mode,
            "engine_mode": engine_mode,
            "dpi": dpi,
            "min_confidence": min_confidence,
            "whitelist": whitelist,
            "scale": scale,
            "threshold": threshold,
            "contrast": contrast,
        }
        for field_name, value in optional_values.items():
            if value is not None and _message_accepts_field(
                self.messages.OcrScreenRequest, field_name
            ):
                request_kwargs[field_name] = value
        if config and _message_accepts_field(self.messages.OcrScreenRequest, "config"):
            request_kwargs["config"] = list(config)
        for field_name, value in {
            "grayscale": grayscale,
            "invert": invert,
            "deskew": deskew,
        }.items():
            if value and _message_accepts_field(self.messages.OcrScreenRequest, field_name):
                request_kwargs[field_name] = value
        response = self._call("OcrScreen", self.messages.OcrScreenRequest(**request_kwargs))
        return _ocr_result_from_proto(response)

    def ocr_region(
        self,
        region: Rect,
        language: str | None = None,
        **kwargs: Any,
    ) -> OcrResult:
        return self.ocr_screen(region=region, language=language, **kwargs)

    def compare_images(
        self,
        expected_image: bytes,
        actual_image: bytes,
        region: Rect | None = None,
        ignore_regions: Iterable[Rect] | None = None,
        per_channel_threshold: int | None = None,
        max_changed_ratio: float | None = None,
        max_changed_pixels: int | None = None,
        max_mean_absolute_error: float | None = None,
        max_channel_delta: int | None = None,
        size_policy: str | None = None,
        alpha: str | None = None,
    ) -> VisualDiffResult:
        request_kwargs: dict[str, Any] = {
            "expected_image": expected_image,
            "actual_image": actual_image,
        }
        if region is not None:
            request_kwargs["region"] = _rect_to_proto(self.messages, region)
        if ignore_regions is not None:
            request_kwargs["ignore_regions"] = [
                _rect_to_proto(self.messages, ignore_region)
                for ignore_region in ignore_regions
            ]
        if per_channel_threshold is not None:
            request_kwargs["per_channel_threshold"] = per_channel_threshold
        if max_changed_ratio is not None:
            request_kwargs["max_changed_ratio"] = max_changed_ratio
        if max_changed_pixels is not None:
            request_kwargs["max_changed_pixels"] = max_changed_pixels
        if max_mean_absolute_error is not None:
            request_kwargs["max_mean_absolute_error"] = max_mean_absolute_error
        if max_channel_delta is not None:
            request_kwargs["max_channel_delta"] = max_channel_delta
        if size_policy is not None:
            request_kwargs["size_policy"] = size_policy
        if alpha is not None:
            request_kwargs["alpha"] = alpha
        response = self._call(
            "CompareImages",
            self.messages.CompareImagesRequest(**request_kwargs),
        )
        return _visual_diff_from_proto(response)

    def compare_image_files(
        self,
        expected_path: str | PathLike[str],
        actual_path: str | PathLike[str],
        region: Rect | None = None,
        ignore_regions: Iterable[Rect] | None = None,
        per_channel_threshold: int | None = None,
        max_changed_ratio: float | None = None,
        max_changed_pixels: int | None = None,
        max_mean_absolute_error: float | None = None,
        max_channel_delta: int | None = None,
        size_policy: str | None = None,
        alpha: str | None = None,
    ) -> VisualDiffResult:
        with open(expected_path, "rb") as expected_file:
            expected_image = expected_file.read()
        with open(actual_path, "rb") as actual_file:
            actual_image = actual_file.read()
        return self.compare_images(
            expected_image,
            actual_image,
            region=region,
            ignore_regions=ignore_regions,
            per_channel_threshold=per_channel_threshold,
            max_changed_ratio=max_changed_ratio,
            max_changed_pixels=max_changed_pixels,
            max_mean_absolute_error=max_mean_absolute_error,
            max_channel_delta=max_channel_delta,
            size_policy=size_policy,
            alpha=alpha,
        )

    def detect_ui_state(
        self,
        images: Sequence[bytes],
        region: Rect | None = None,
        ignore_regions: Iterable[Rect] | None = None,
        per_channel_threshold: int | None = None,
        stable_max_changed_ratio: float | None = None,
        stable_max_changed_pixels: int | None = None,
        stable_max_mean_absolute_error: float | None = None,
        stable_max_channel_delta: int | None = None,
        loading_min_changed_ratio: float | None = None,
        loading_min_changed_pixels: int | None = None,
        required_stable_transitions: int | None = None,
        size_policy: str | None = None,
        alpha: str | None = None,
    ) -> UiStateResult:
        request_kwargs: dict[str, Any] = {"images": list(images)}
        if region is not None:
            request_kwargs["region"] = _rect_to_proto(self.messages, region)
        if ignore_regions is not None:
            request_kwargs["ignore_regions"] = [
                _rect_to_proto(self.messages, ignore_region)
                for ignore_region in ignore_regions
            ]
        if per_channel_threshold is not None:
            request_kwargs["per_channel_threshold"] = per_channel_threshold
        if stable_max_changed_ratio is not None:
            request_kwargs["stable_max_changed_ratio"] = stable_max_changed_ratio
        if stable_max_changed_pixels is not None:
            request_kwargs["stable_max_changed_pixels"] = stable_max_changed_pixels
        if stable_max_mean_absolute_error is not None:
            request_kwargs["stable_max_mean_absolute_error"] = stable_max_mean_absolute_error
        if stable_max_channel_delta is not None:
            request_kwargs["stable_max_channel_delta"] = stable_max_channel_delta
        if loading_min_changed_ratio is not None:
            request_kwargs["loading_min_changed_ratio"] = loading_min_changed_ratio
        if loading_min_changed_pixels is not None:
            request_kwargs["loading_min_changed_pixels"] = loading_min_changed_pixels
        if required_stable_transitions is not None:
            request_kwargs["required_stable_transitions"] = required_stable_transitions
        if size_policy is not None:
            request_kwargs["size_policy"] = size_policy
        if alpha is not None:
            request_kwargs["alpha"] = alpha
        response = self._call(
            "DetectUiState",
            self.messages.DetectUiStateRequest(**request_kwargs),
        )
        return _ui_state_from_proto(response)

    def detect_ui_state_from_image_files(
        self,
        image_paths: Sequence[str | PathLike[str]],
        region: Rect | None = None,
        ignore_regions: Iterable[Rect] | None = None,
        per_channel_threshold: int | None = None,
        stable_max_changed_ratio: float | None = None,
        stable_max_changed_pixels: int | None = None,
        stable_max_mean_absolute_error: float | None = None,
        stable_max_channel_delta: int | None = None,
        loading_min_changed_ratio: float | None = None,
        loading_min_changed_pixels: int | None = None,
        required_stable_transitions: int | None = None,
        size_policy: str | None = None,
        alpha: str | None = None,
    ) -> UiStateResult:
        images: list[bytes] = []
        for image_path in image_paths:
            with open(image_path, "rb") as image_file:
                images.append(image_file.read())
        return self.detect_ui_state(
            images,
            region=region,
            ignore_regions=ignore_regions,
            per_channel_threshold=per_channel_threshold,
            stable_max_changed_ratio=stable_max_changed_ratio,
            stable_max_changed_pixels=stable_max_changed_pixels,
            stable_max_mean_absolute_error=stable_max_mean_absolute_error,
            stable_max_channel_delta=stable_max_channel_delta,
            loading_min_changed_ratio=loading_min_changed_ratio,
            loading_min_changed_pixels=loading_min_changed_pixels,
            required_stable_transitions=required_stable_transitions,
            size_policy=size_policy,
            alpha=alpha,
        )

    def detect_ui_elements(
        self,
        image: bytes,
        region: Rect | None = None,
        edge_threshold: int | None = None,
        min_width: int | None = None,
        min_height: int | None = None,
        min_component_pixels: int | None = None,
        max_elements: int | None = None,
        merge_distance: int | None = None,
        ignore_regions: Iterable[Rect] | None = None,
        min_confidence: float | None = None,
        max_width: int | None = None,
        max_height: int | None = None,
        min_area: int | None = None,
        max_area: int | None = None,
        padding: int | None = None,
        sort: str | None = None,
        mask_output_path: str | PathLike[str] | None = None,
        overlay_output_path: str | PathLike[str] | None = None,
    ) -> DetectUiElementsResult:
        request_kwargs: dict[str, Any] = {"image": image}
        if region is not None:
            request_kwargs["region"] = _rect_to_proto(self.messages, region)
        if ignore_regions is not None:
            request_kwargs["ignore_regions"] = [
                _rect_to_proto(self.messages, ignore_region)
                for ignore_region in ignore_regions
            ]
        if edge_threshold is not None:
            request_kwargs["edge_threshold"] = edge_threshold
        if min_width is not None:
            request_kwargs["min_width"] = min_width
        if min_height is not None:
            request_kwargs["min_height"] = min_height
        if min_component_pixels is not None:
            request_kwargs["min_component_pixels"] = min_component_pixels
        if min_confidence is not None:
            request_kwargs["min_confidence"] = min_confidence
        if max_width is not None:
            request_kwargs["max_width"] = max_width
        if max_height is not None:
            request_kwargs["max_height"] = max_height
        if min_area is not None:
            request_kwargs["min_area"] = min_area
        if max_area is not None:
            request_kwargs["max_area"] = max_area
        if max_elements is not None:
            request_kwargs["max_elements"] = max_elements
        if merge_distance is not None:
            request_kwargs["merge_distance"] = merge_distance
        if padding is not None:
            request_kwargs["padding"] = padding
        if sort is not None:
            request_kwargs["sort"] = sort
        if mask_output_path is not None:
            request_kwargs["mask_output_path"] = str(mask_output_path)
        if overlay_output_path is not None:
            request_kwargs["overlay_output_path"] = str(overlay_output_path)
        response = self._call(
            "DetectUiElements",
            self.messages.DetectUiElementsRequest(**request_kwargs),
        )
        return _detect_ui_elements_from_proto(response)

    def detect_ui_elements_from_image_file(
        self,
        image_path: str | PathLike[str],
        region: Rect | None = None,
        edge_threshold: int | None = None,
        min_width: int | None = None,
        min_height: int | None = None,
        min_component_pixels: int | None = None,
        max_elements: int | None = None,
        merge_distance: int | None = None,
        ignore_regions: Iterable[Rect] | None = None,
        min_confidence: float | None = None,
        max_width: int | None = None,
        max_height: int | None = None,
        min_area: int | None = None,
        max_area: int | None = None,
        padding: int | None = None,
        sort: str | None = None,
        mask_output_path: str | PathLike[str] | None = None,
        overlay_output_path: str | PathLike[str] | None = None,
    ) -> DetectUiElementsResult:
        with open(image_path, "rb") as image_file:
            image = image_file.read()
        return self.detect_ui_elements(
            image,
            region=region,
            ignore_regions=ignore_regions,
            edge_threshold=edge_threshold,
            min_width=min_width,
            min_height=min_height,
            min_component_pixels=min_component_pixels,
            min_confidence=min_confidence,
            max_width=max_width,
            max_height=max_height,
            min_area=min_area,
            max_area=max_area,
            max_elements=max_elements,
            merge_distance=merge_distance,
            padding=padding,
            sort=sort,
            mask_output_path=mask_output_path,
            overlay_output_path=overlay_output_path,
        )

    def click(
        self,
        x: int | None = None,
        y: int | None = None,
        semantic_selector: str | None = None,
        vision_fallback: bool = False,
        *,
        button: str = "left",
        dry_run: bool = False,
        bounds_policy: str | None = None,
        backend: str | None = None,
        restore: bool = False,
        region: Rect | None = None,
        ratio_x: float | None = None,
        ratio_y: float | None = None,
        window_id: str | None = None,
        app: str | None = None,
        window_title: str | None = None,
        title_regex: str | None = None,
    ) -> ActionResult:
        if bounds_policy is not None and bounds_policy not in {
            "allow",
            "clamp",
            "fail",
            "fail-out-of-bounds",
        }:
            raise ValueError("bounds_policy must be allow, clamp, fail, or fail-out-of-bounds")
        if backend is not None and backend not in {"auto", "uinput", "ydotool", "xdotool"}:
            raise ValueError("backend must be auto, uinput, ydotool, or xdotool")
        for name, value in (("ratio_x", ratio_x), ("ratio_y", ratio_y)):
            if value is not None and (
                not float(value) == float(value) or not 0.0 <= float(value) <= 1.0
            ):
                raise ValueError(f"{name} must be between 0.0 and 1.0")

        has_coordinates = x is not None or y is not None
        has_selector = semantic_selector is not None
        has_scope = any(
            value is not None
            for value in (region, ratio_x, ratio_y, window_id, app, window_title, title_regex)
        )
        if sum(1 for value in (has_coordinates, has_selector, has_scope) if value) != 1:
            raise ValueError(
                "provide exactly one click target: coordinates, semantic_selector, or region/ratio/window scope"
            )

        request_kwargs: dict[str, Any] = {
            "vision_fallback": vision_fallback,
            "button": _mouse_button_to_proto(self.messages, button),
            "dry_run": dry_run,
            "restore": restore,
        }
        if has_coordinates:
            if x is None or y is None:
                raise ValueError("x and y are required for coordinate clicks")
            request_kwargs["coordinates"] = self.messages.Point(x=x, y=y)
        if semantic_selector is not None:
            request_kwargs["semantic_selector"] = semantic_selector
        if bounds_policy is not None:
            request_kwargs["bounds_policy"] = bounds_policy
        if backend is not None:
            request_kwargs["backend"] = backend
        if region is not None:
            request_kwargs["region"] = _rect_to_proto(self.messages, region)
        if ratio_x is not None:
            request_kwargs["ratio_x"] = ratio_x
        if ratio_y is not None:
            request_kwargs["ratio_y"] = ratio_y
        if window_id is not None:
            request_kwargs["window_id"] = window_id
        if app is not None:
            request_kwargs["app"] = app
        if window_title is not None:
            request_kwargs["window_title"] = window_title
        if title_regex is not None:
            request_kwargs["title_regex"] = title_regex
        request = self.messages.ClickRequest(**request_kwargs)
        return _action_result_from_proto(self._call("Click", request))

    def click_selector(
        self,
        selector: str,
        vision_fallback: bool = False,
        **kwargs: Any,
    ) -> ActionResult:
        return self.click(
            semantic_selector=selector,
            vision_fallback=vision_fallback,
            **kwargs,
        )

    def move_mouse(
        self,
        x: int | None = None,
        y: int | None = None,
        *,
        relative_x: int | None = None,
        relative_y: int | None = None,
        region: Rect | None = None,
        ratio_x: float | None = None,
        ratio_y: float | None = None,
        window_id: str | None = None,
        app: str | None = None,
        window_title: str | None = None,
        title_regex: str | None = None,
        dry_run: bool = False,
        duration_ms: int | None = None,
        steps: int | None = None,
        bounds_policy: str | None = None,
        backend: str | None = None,
        restore: bool = False,
    ) -> ActionResult:
        if duration_ms is not None and duration_ms < 0:
            raise ValueError("duration_ms must be non-negative")
        if steps is not None and steps <= 0:
            raise ValueError("steps must be greater than zero")
        if bounds_policy is not None and bounds_policy not in {
            "allow",
            "clamp",
            "fail",
            "fail-out-of-bounds",
        }:
            raise ValueError("bounds_policy must be allow, clamp, fail, or fail-out-of-bounds")
        if backend is not None and backend not in {"auto", "uinput", "ydotool", "xdotool"}:
            raise ValueError("backend must be auto, uinput, ydotool, or xdotool")
        for name, value in (("ratio_x", ratio_x), ("ratio_y", ratio_y)):
            if value is not None and (not float(value) == float(value) or not 0.0 <= value <= 1.0):
                raise ValueError(f"{name} must be between 0.0 and 1.0")

        has_coordinates = x is not None or y is not None
        has_relative = relative_x is not None or relative_y is not None
        has_scope = any(
            value is not None
            for value in (region, ratio_x, ratio_y, window_id, app, window_title, title_regex)
        )
        if sum(1 for value in (has_coordinates, has_relative, has_scope) if value) != 1:
            raise ValueError("provide exactly one move target")

        request_kwargs: dict[str, Any] = {
            "dry_run": dry_run,
            "restore": restore,
        }
        if has_coordinates:
            if x is None or y is None:
                raise ValueError("x and y are required for coordinate moves")
            request_kwargs["coordinates"] = self.messages.Point(x=x, y=y)
        if has_relative:
            if relative_x is None or relative_y is None:
                raise ValueError("relative_x and relative_y are required for relative moves")
            request_kwargs["relative"] = self.messages.Point(x=relative_x, y=relative_y)
        if region is not None:
            request_kwargs["region"] = _rect_to_proto(self.messages, region)
        if ratio_x is not None:
            request_kwargs["ratio_x"] = ratio_x
        if ratio_y is not None:
            request_kwargs["ratio_y"] = ratio_y
        if window_id is not None:
            request_kwargs["window_id"] = window_id
        if app is not None:
            request_kwargs["app"] = app
        if window_title is not None:
            request_kwargs["window_title"] = window_title
        if title_regex is not None:
            request_kwargs["title_regex"] = title_regex
        if duration_ms is not None:
            request_kwargs["duration_ms"] = duration_ms
        if steps is not None:
            request_kwargs["steps"] = steps
        if bounds_policy is not None:
            request_kwargs["bounds_policy"] = bounds_policy
        if backend is not None:
            request_kwargs["backend"] = backend

        request = self.messages.MoveMouseRequest(**request_kwargs)
        return _action_result_from_proto(self._call("MoveMouse", request))

    def drag(
        self,
        from_x: int | None = None,
        from_y: int | None = None,
        to_x: int | None = None,
        to_y: int | None = None,
        *,
        button: str = "left",
        duration_ms: int = 250,
        dry_run: bool = False,
        steps: int | None = None,
        bounds_policy: str | None = None,
        backend: str | None = None,
        restore: bool = False,
        from_current: bool = False,
        from_ratio: tuple[float, float] | None = None,
        to_ratio: tuple[float, float] | None = None,
        region: Rect | None = None,
        window_id: str | None = None,
        app: str | None = None,
        window_title: str | None = None,
        title_regex: str | None = None,
    ) -> ActionResult:
        if duration_ms < 0:
            raise ValueError("duration_ms must be non-negative")
        if steps is not None and steps <= 0:
            raise ValueError("steps must be greater than zero")
        if bounds_policy is not None and bounds_policy not in {
            "allow",
            "clamp",
            "fail",
            "fail-out-of-bounds",
        }:
            raise ValueError("bounds_policy must be allow, clamp, fail, or fail-out-of-bounds")
        if backend is not None and backend not in {"auto", "uinput", "xdotool"}:
            raise ValueError("backend must be auto, uinput, or xdotool")
        if from_ratio is not None:
            _validate_ratio_pair("from_ratio", from_ratio)
        if to_ratio is not None:
            _validate_ratio_pair("to_ratio", to_ratio)

        has_from_coordinates = from_x is not None or from_y is not None
        has_to_coordinates = to_x is not None or to_y is not None
        if sum(1 for value in (has_from_coordinates, from_current, from_ratio is not None) if value) != 1:
            raise ValueError("provide exactly one drag from endpoint")
        if sum(1 for value in (has_to_coordinates, to_ratio is not None) if value) != 1:
            raise ValueError("provide exactly one drag to endpoint")
        has_scope = any(
            value is not None
            for value in (region, window_id, app, window_title, title_regex)
        )
        if has_scope and from_ratio is None and to_ratio is None:
            raise ValueError("drag region/window scope requires from_ratio or to_ratio")

        request_kwargs: dict[str, Any] = {
            "button": _mouse_button_to_proto(self.messages, button),
            "duration_ms": duration_ms,
            "dry_run": dry_run,
            "restore": restore,
            "from_current": from_current,
        }
        if has_from_coordinates:
            if from_x is None or from_y is None:
                raise ValueError("from_x and from_y are required for coordinate drags")
            request_kwargs["from"] = self.messages.Point(x=from_x, y=from_y)
        if has_to_coordinates:
            if to_x is None or to_y is None:
                raise ValueError("to_x and to_y are required for coordinate drags")
            request_kwargs["to"] = self.messages.Point(x=to_x, y=to_y)
        if steps is not None:
            request_kwargs["steps"] = steps
        if bounds_policy is not None:
            request_kwargs["bounds_policy"] = bounds_policy
        if backend is not None:
            request_kwargs["backend"] = backend
        if region is not None:
            request_kwargs["region"] = _rect_to_proto(self.messages, region)
        if window_id is not None:
            request_kwargs["window_id"] = window_id
        if app is not None:
            request_kwargs["app"] = app
        if window_title is not None:
            request_kwargs["window_title"] = window_title
        if title_regex is not None:
            request_kwargs["title_regex"] = title_regex
        if from_ratio is not None:
            request_kwargs["from_ratio_x"] = from_ratio[0]
            request_kwargs["from_ratio_y"] = from_ratio[1]
        if to_ratio is not None:
            request_kwargs["to_ratio_x"] = to_ratio[0]
            request_kwargs["to_ratio_y"] = to_ratio[1]

        request = self.messages.DragRequest(**request_kwargs)
        return _action_result_from_proto(self._call("Drag", request))

    def type_text(
        self,
        text: str,
        typing_speed_chars_per_second: int | None = None,
        *,
        dry_run: bool = False,
        backend: str | None = None,
        delay_ms: int | None = None,
        key_delay_ms: int | None = None,
    ) -> ActionResult:
        if typing_speed_chars_per_second is not None and typing_speed_chars_per_second <= 0:
            raise ValueError("typing_speed_chars_per_second must be greater than zero")
        if delay_ms is not None and delay_ms < 0:
            raise ValueError("delay_ms must be non-negative")
        if key_delay_ms is not None and key_delay_ms < 0:
            raise ValueError("key_delay_ms must be non-negative")
        if typing_speed_chars_per_second is not None and key_delay_ms is not None:
            raise ValueError("typing_speed_chars_per_second cannot be combined with key_delay_ms")
        if backend is not None and backend not in {"auto", "wtype", "ydotool", "xdotool"}:
            raise ValueError("backend must be auto, wtype, ydotool, or xdotool")

        request_kwargs: dict[str, Any] = {"text": text, "dry_run": dry_run}
        if typing_speed_chars_per_second is not None:
            request_kwargs["typing_speed_chars_per_second"] = typing_speed_chars_per_second
        if backend is not None:
            request_kwargs["backend"] = backend
        if delay_ms is not None:
            request_kwargs["delay_ms"] = delay_ms
        if key_delay_ms is not None:
            request_kwargs["key_delay_ms"] = key_delay_ms
        request = self.messages.TypeTextRequest(**request_kwargs)
        return _action_result_from_proto(self._call("TypeText", request))

    def paste_text(
        self,
        text: str,
        preserve_clipboard: bool = False,
        *,
        dry_run: bool = False,
        clipboard_backend: str | None = None,
        hotkey_backend: str | None = None,
        delay_ms: int | None = None,
        restore_delay_ms: int | None = None,
        restore_policy: str | None = None,
    ) -> ActionResult:
        if clipboard_backend is not None and clipboard_backend not in {
            "auto",
            "wl-copy",
            "xclip",
            "xsel",
        }:
            raise ValueError("clipboard_backend must be auto, wl-copy, xclip, or xsel")
        if hotkey_backend is not None and hotkey_backend not in {
            "auto",
            "ydotool",
            "xdotool",
        }:
            raise ValueError("hotkey_backend must be auto, ydotool, or xdotool")
        if delay_ms is not None and delay_ms < 0:
            raise ValueError("delay_ms must be non-negative")
        if restore_delay_ms is not None and restore_delay_ms < 0:
            raise ValueError("restore_delay_ms must be non-negative")
        if restore_policy is not None and restore_policy not in {
            "strict",
            "best-effort",
            "off",
        }:
            raise ValueError("restore_policy must be strict, best-effort, or off")

        request_kwargs: dict[str, Any] = {
            "text": text,
            "preserve_clipboard": preserve_clipboard,
            "dry_run": dry_run,
        }
        if clipboard_backend is not None:
            request_kwargs["clipboard_backend"] = clipboard_backend
        if hotkey_backend is not None:
            request_kwargs["hotkey_backend"] = hotkey_backend
        if delay_ms is not None:
            request_kwargs["delay_ms"] = delay_ms
        if restore_delay_ms is not None:
            request_kwargs["restore_delay_ms"] = restore_delay_ms
        if restore_policy is not None:
            request_kwargs["restore_policy"] = restore_policy
        request = self.messages.PasteTextRequest(**request_kwargs)
        return _action_result_from_proto(self._call("PasteText", request))

    def hotkey(self, keys: Sequence[str] | str) -> ActionResult:
        if isinstance(keys, str):
            key_values = [keys]
        else:
            key_values = list(keys)
        if not key_values or any(not str(key).strip() for key in key_values):
            raise ValueError("hotkey requires at least one non-empty key")
        request = self.messages.HotkeyRequest(keys=[str(key) for key in key_values])
        return _action_result_from_proto(self._call("Hotkey", request))

    def find_elements(
        self,
        selector: str,
        vision_fallback: bool = False,
        app: str | None = None,
        window_title: str | None = None,
        window_id: str | None = None,
        vision_region: Rect | None = None,
        vision_edge_threshold: int | None = None,
        vision_min_width: int | None = None,
        vision_min_height: int | None = None,
        vision_min_component_pixels: int | None = None,
        vision_max_elements: int | None = None,
        vision_merge_distance: int | None = None,
    ) -> FindElementResult:
        request_kwargs: dict[str, Any] = {
            "selector": selector,
            "vision_fallback": vision_fallback,
        }
        if app is not None:
            request_kwargs["app"] = app
        if window_title is not None:
            request_kwargs["window_title"] = window_title
        if window_id is not None:
            request_kwargs["window_id"] = window_id
        if vision_region is not None:
            request_kwargs["vision_region"] = _rect_to_proto(self.messages, vision_region)
        if vision_edge_threshold is not None:
            request_kwargs["vision_edge_threshold"] = vision_edge_threshold
        if vision_min_width is not None:
            request_kwargs["vision_min_width"] = vision_min_width
        if vision_min_height is not None:
            request_kwargs["vision_min_height"] = vision_min_height
        if vision_min_component_pixels is not None:
            request_kwargs["vision_min_component_pixels"] = vision_min_component_pixels
        if vision_max_elements is not None:
            request_kwargs["vision_max_elements"] = vision_max_elements
        if vision_merge_distance is not None:
            request_kwargs["vision_merge_distance"] = vision_merge_distance
        response = self._call(
            "FindElement",
            self.messages.FindElementRequest(**request_kwargs),
        )
        return FindElementResult(
            backend_name=getattr(response, "backend_name", ""),
            backend_kind=getattr(response, "backend_kind", ""),
            warnings=tuple(getattr(response, "warnings", ())),
            elements=tuple(_ui_element_from_proto(element) for element in response.elements),
            cache_hit=bool(getattr(response, "cache_hit", False)),
            cache_age_ms=int(getattr(response, "cache_age_ms", 0)),
            vision_fallback_used=bool(getattr(response, "vision_fallback_used", False)),
        )

    def find_element(
        self,
        selector: str,
        vision_fallback: bool = False,
        app: str | None = None,
        window_title: str | None = None,
        window_id: str | None = None,
        vision_region: Rect | None = None,
        vision_edge_threshold: int | None = None,
        vision_min_width: int | None = None,
        vision_min_height: int | None = None,
        vision_min_component_pixels: int | None = None,
        vision_max_elements: int | None = None,
        vision_merge_distance: int | None = None,
    ) -> tuple[UiElement, ...]:
        return self.find_elements(
            selector,
            vision_fallback=vision_fallback,
            app=app,
            window_title=window_title,
            window_id=window_id,
            vision_region=vision_region,
            vision_edge_threshold=vision_edge_threshold,
            vision_min_width=vision_min_width,
            vision_min_height=vision_min_height,
            vision_min_component_pixels=vision_min_component_pixels,
            vision_max_elements=vision_max_elements,
            vision_merge_distance=vision_merge_distance,
        ).elements

    def list_windows(
        self,
        *,
        id: str | None = None,
        app: str | None = None,
        title: str | None = None,
        title_regex: str | None = None,
        focused: bool = False,
        limit: int | None = None,
        sort: str | None = None,
        backend: str | None = None,
        diagnose: bool = False,
    ) -> tuple[WindowInfo, ...]:
        return self.list_windows_result(
            id=id,
            app=app,
            title=title,
            title_regex=title_regex,
            focused=focused,
            limit=limit,
            sort=sort,
            backend=backend,
            diagnose=diagnose,
        ).windows

    def list_windows_result(
        self,
        *,
        id: str | None = None,
        app: str | None = None,
        title: str | None = None,
        title_regex: str | None = None,
        focused: bool = False,
        limit: int | None = None,
        sort: str | None = None,
        backend: str | None = None,
        diagnose: bool = False,
    ) -> WindowListResult:
        request = _list_windows_request(
            self.messages,
            id=id,
            app=app,
            title=title,
            title_regex=title_regex,
            focused=focused,
            limit=limit,
            sort=sort,
            backend=backend,
            diagnose=diagnose,
        )
        response = self._call("ListWindows", request)
        return _window_list_result_from_proto(response)

    def get_desktop_state(self) -> DesktopState:
        response = self._call("GetDesktopState", self.messages.GetDesktopStateRequest())
        active_window = _message_field(response, "active_window")
        return DesktopState(
            active_window=_window_from_proto(active_window) if active_window is not None else None,
            windows=tuple(_window_from_proto(window) for window in response.windows),
            elements=tuple(_ui_element_from_proto(element) for element in response.elements),
        )

    def desktop_profiles(
        self,
        app: str | None = None,
        *,
        target: str | None = None,
        command: str | None = None,
        desktop_id: str | None = None,
        supports: str | None = None,
        check: bool = False,
        installed: bool = False,
        available: bool = False,
    ) -> DesktopProfilesResult:
        request_kwargs: dict[str, Any] = {
            "check": check,
            "installed": installed,
            "available": available,
        }
        if app is not None:
            request_kwargs["app"] = app
        if target is not None:
            request_kwargs["target"] = target
        if command is not None:
            request_kwargs["command"] = command
        if desktop_id is not None:
            request_kwargs["desktop_id"] = desktop_id
        if supports is not None:
            request_kwargs["supports"] = supports
        return _desktop_profiles_from_proto(
            self._call("DesktopProfiles", self.messages.DesktopProfilesRequest(**request_kwargs))
        )

    def desktop_focus(
        self,
        app: str,
        *,
        use_gnome_overview: bool = True,
        launch_if_needed: bool = True,
        wait_after_focus_ms: int = 1_000,
        overview_wait_ms: int = 800,
        window_title: str | None = None,
        window_id: str | None = None,
        verify: bool = False,
    ) -> DesktopActionResult:
        request_kwargs: dict[str, Any] = {
            "app": app,
            "use_gnome_overview": use_gnome_overview,
            "launch_if_needed": launch_if_needed,
            "wait_after_focus_ms": wait_after_focus_ms,
            "overview_wait_ms": overview_wait_ms,
            "verify": verify,
        }
        if window_title is not None:
            request_kwargs["window_title"] = window_title
        if window_id is not None:
            request_kwargs["window_id"] = window_id
        return _desktop_action_from_proto(
            self._call("DesktopFocus", self.messages.DesktopFocusRequest(**request_kwargs))
        )

    def desktop_locate(
        self,
        app: str,
        target: str,
        *,
        image_path: str | PathLike[str] | None = None,
        prefer_accessibility: bool = True,
        window_title: str | None = None,
        window_id: str | None = None,
    ) -> DesktopLocateResult:
        request_kwargs: dict[str, Any] = {
            "app": app,
            "target": target,
            "prefer_accessibility": prefer_accessibility,
        }
        if image_path is not None:
            request_kwargs["image_path"] = str(image_path)
        if window_title is not None:
            request_kwargs["window_title"] = window_title
        if window_id is not None:
            request_kwargs["window_id"] = window_id
        return _desktop_locate_from_proto(
            self._call("DesktopLocate", self.messages.DesktopLocateRequest(**request_kwargs))
        )

    def desktop_click(
        self,
        app: str,
        target: str,
        *,
        image_path: str | PathLike[str] | None = None,
        prefer_accessibility: bool = True,
        window_title: str | None = None,
        window_id: str | None = None,
        button: str = "left",
        dry_run: bool = False,
        verify: bool = False,
    ) -> DesktopActionResult:
        request_kwargs: dict[str, Any] = {
            "app": app,
            "target": target,
            "prefer_accessibility": prefer_accessibility,
            "button": _mouse_button_to_proto(self.messages, button),
            "dry_run": dry_run,
            "verify": verify,
        }
        if image_path is not None:
            request_kwargs["image_path"] = str(image_path)
        if window_title is not None:
            request_kwargs["window_title"] = window_title
        if window_id is not None:
            request_kwargs["window_id"] = window_id
        return _desktop_action_from_proto(
            self._call("DesktopClick", self.messages.DesktopClickRequest(**request_kwargs))
        )

    def desktop_drag(
        self,
        app: str,
        target: str,
        *,
        image_path: str | PathLike[str] | None = None,
        prefer_accessibility: bool = True,
        window_title: str | None = None,
        window_id: str | None = None,
        button: str = "left",
        from_ratio: tuple[float, float] = (0.5, 0.5),
        to_ratio: tuple[float, float] = (0.5, 0.5),
        duration_ms: int = 250,
        dry_run: bool = False,
        verify: bool = False,
    ) -> DesktopActionResult:
        _validate_ratio_pair("from_ratio", from_ratio)
        _validate_ratio_pair("to_ratio", to_ratio)
        if duration_ms < 0:
            raise ValueError("duration_ms must be non-negative")
        request_kwargs: dict[str, Any] = {
            "app": app,
            "target": target,
            "prefer_accessibility": prefer_accessibility,
            "button": _mouse_button_to_proto(self.messages, button),
            "from_ratio_x": from_ratio[0],
            "from_ratio_y": from_ratio[1],
            "to_ratio_x": to_ratio[0],
            "to_ratio_y": to_ratio[1],
            "duration_ms": duration_ms,
            "dry_run": dry_run,
            "verify": verify,
        }
        if image_path is not None:
            request_kwargs["image_path"] = str(image_path)
        if window_title is not None:
            request_kwargs["window_title"] = window_title
        if window_id is not None:
            request_kwargs["window_id"] = window_id
        return _desktop_action_from_proto(
            self._call("DesktopDrag", self.messages.DesktopDragRequest(**request_kwargs))
        )

    def desktop_type_into(
        self,
        app: str,
        target: str,
        text: str,
        *,
        image_path: str | PathLike[str] | None = None,
        prefer_accessibility: bool = True,
        window_title: str | None = None,
        window_id: str | None = None,
        clear: bool = False,
        dry_run: bool = False,
        verify: bool = False,
    ) -> DesktopActionResult:
        request_kwargs: dict[str, Any] = {
            "app": app,
            "target": target,
            "text": text,
            "prefer_accessibility": prefer_accessibility,
            "clear": clear,
            "dry_run": dry_run,
            "verify": verify,
        }
        if image_path is not None:
            request_kwargs["image_path"] = str(image_path)
        if window_title is not None:
            request_kwargs["window_title"] = window_title
        if window_id is not None:
            request_kwargs["window_id"] = window_id
        return _desktop_action_from_proto(
            self._call("DesktopTypeInto", self.messages.DesktopTypeIntoRequest(**request_kwargs))
        )

    def desktop_assert(
        self,
        app: str,
        target: str,
        *,
        assertion: str = "present",
        expected_text: str | None = None,
        image_path: str | PathLike[str] | None = None,
        prefer_accessibility: bool = True,
        window_title: str | None = None,
        window_id: str | None = None,
    ) -> DesktopActionResult:
        request_kwargs: dict[str, Any] = {
            "app": app,
            "target": target,
            "prefer_accessibility": prefer_accessibility,
            "assertion": _desktop_assertion_to_proto(
                self.messages,
                assertion,
                expected_text=expected_text,
            ),
        }
        if expected_text is not None:
            request_kwargs["expected_text"] = expected_text
        if image_path is not None:
            request_kwargs["image_path"] = str(image_path)
        if window_title is not None:
            request_kwargs["window_title"] = window_title
        if window_id is not None:
            request_kwargs["window_id"] = window_id
        return _desktop_action_from_proto(
            self._call("DesktopAssert", self.messages.DesktopAssertRequest(**request_kwargs))
        )

    def probe_dmabuf(self, import_target: str = "compute") -> DmaBufProbeResult:
        request = self.messages.ProbeDmaBufRequest(
            import_target=_dmabuf_import_target_to_proto(self.messages, import_target),
        )
        return _dmabuf_probe_from_proto(self._call("ProbeDmaBuf", request))

    def list_plugins(self, paths: Sequence[str | PathLike[str]] = ()) -> PluginListResult:
        request = self.messages.ListPluginsRequest(paths=[str(path) for path in paths])
        return _plugin_list_from_proto(self._call("ListPlugins", request))

    def call_plugin_tool(
        self,
        plugin_id: str,
        tool: str,
        arguments: dict[str, Any] | None = None,
        *,
        paths: Sequence[str | PathLike[str]] = (),
        timeout_seconds: float = 10.0,
        max_output_bytes: int = 1_048_576,
    ) -> PluginToolExecutionResult:
        if timeout_seconds <= 0:
            raise ValueError("timeout_seconds must be positive")
        if max_output_bytes < 0:
            raise ValueError("max_output_bytes must be non-negative")
        request = self.messages.CallPluginToolRequest(
            plugin_id=plugin_id,
            tool=tool,
            arguments_json=json.dumps(arguments or {}),
            paths=[str(path) for path in paths],
            timeout_ms=max(1, int(timeout_seconds * 1000)),
            max_output_bytes=max_output_bytes,
        )
        return _plugin_execution_from_proto(self._call("CallPluginTool", request))

    def _call(self, method_name: str, request: Any) -> Any:
        method = getattr(self.stub, method_name)
        try:
            return method(request, timeout=self.timeout_seconds)
        except Exception as error:
            if self._grpc is not None and isinstance(error, self._grpc.RpcError):
                code = error.code()
                code_name = getattr(code, "name", str(code))
                details = error.details() or "no details"
                raise PeekabooXClientError(
                    f"{method_name} failed with {code_name}: {details}"
                ) from error
            raise


def _load_grpc_modules() -> tuple[ModuleType, ModuleType, ModuleType]:
    try:
        grpc_module = import_module("grpc")
        messages = import_module("peekaboox.v1.peekaboox_pb2")
        services = import_module("peekaboox.v1.peekaboox_pb2_grpc")
    except ModuleNotFoundError as error:
        raise MissingGrpcDependencyError(
            "PeekabooXClient requires grpcio and protobuf; install the Python package with its runtime dependencies"
        ) from error
    return grpc_module, messages, services


def _capture_target(messages: Any, region: Rect | None = None, window_id: str | None = None) -> Any:
    if region is not None and window_id is not None and window_id.strip():
        raise ValueError("provide either region or window_id, not both")
    if region is not None:
        return messages.CaptureTarget(region=_rect_to_proto(messages, region))
    if window_id is not None:
        window_id = window_id.strip()
        if not window_id:
            raise ValueError("window_id must not be empty")
        return messages.CaptureTarget(window_id=window_id)
    return messages.CaptureTarget(full_screen=True)


def _list_windows_request(
    messages: Any,
    *,
    id: str | None,
    app: str | None,
    title: str | None,
    title_regex: str | None,
    focused: bool,
    limit: int | None,
    sort: str | None,
    backend: str | None,
    diagnose: bool,
) -> Any:
    if limit is not None and limit <= 0:
        raise ValueError("limit must be greater than zero")

    kwargs: dict[str, Any] = {
        "focused": focused,
        "diagnose": diagnose,
    }
    for key, value in {
        "id": id,
        "app": app,
        "title": title,
        "title_regex": title_regex,
        "sort": sort,
        "backend": backend,
    }.items():
        value = _clean_optional_string(value)
        if value is not None:
            kwargs[key] = value
    if limit is not None:
        kwargs["limit"] = limit

    return messages.ListWindowsRequest(**kwargs)


def _window_list_result_from_proto(response: Any) -> WindowListResult:
    return WindowListResult(
        backend_name=response.backend_name,
        backend_kind=response.backend_kind,
        warnings=tuple(response.warnings),
        backend_reports=tuple(
            _window_backend_report_from_proto(report) for report in response.backend_reports
        ),
        windows=tuple(_window_from_proto(window) for window in response.windows),
    )


def _window_backend_report_from_proto(report: Any) -> WindowBackendReport:
    return WindowBackendReport(
        backend_name=report.backend_name,
        backend_kind=report.backend_kind,
        raw_window_count=report.raw_window_count,
        matched_window_count=report.matched_window_count,
        selected=report.selected,
        error=_optional_scalar(report, "error"),
    )


def _clean_optional_string(value: str | None) -> str | None:
    if value is None:
        return None
    value = value.strip()
    return value or None


def _window_from_proto(window: Any) -> WindowInfo:
    bounds = _message_field(window, "bounds")
    return WindowInfo(
        id=window.id,
        title=window.title,
        app_id=_optional_scalar(window, "app_id"),
        bounds=_rect_from_proto(bounds),
        focused=window.focused,
        state=window.state,
    )


def _ui_element_from_proto(element: Any) -> UiElement:
    bounds = _message_field(element, "bounds")
    center = _message_field(element, "center")
    return UiElement(
        id=element.id,
        role=element.role,
        label=_optional_scalar(element, "label"),
        bounds=_rect_from_proto(bounds),
        confidence=element.confidence,
        center=_point_from_proto(center),
        states=tuple(element.states),
        window_id=_optional_scalar(element, "window_id"),
        window_title=_optional_scalar(element, "window_title"),
        app_id=_optional_scalar(element, "app_id"),
        parent_id=_optional_scalar(element, "parent_id"),
        child_ids=tuple(getattr(element, "child_ids", ())),
    )


def _ocr_result_from_proto(response: Any) -> OcrResult:
    return OcrResult(
        backend_name=response.backend_name,
        text=response.text,
        blocks=tuple(_ocr_block_from_proto(block) for block in response.blocks),
        words=tuple(
            _ocr_block_from_proto(word)
            for word in getattr(response, "words", ())
        ),
        warnings=tuple(response.warnings),
    )


def _ocr_block_from_proto(block: Any) -> OcrBlock:
    element = _message_field(block, "element")
    return OcrBlock(
        text=block.text,
        element=_ui_element_from_proto(element),
    )


def _visual_diff_from_proto(response: Any) -> VisualDiffResult:
    changed_bounds = _message_field(response, "changed_bounds")
    compared_region = _message_field(response, "compared_region")
    return VisualDiffResult(
        compared_region=_rect_from_proto(compared_region),
        compared_pixels=response.compared_pixels,
        changed_pixels=response.changed_pixels,
        changed_ratio=response.changed_ratio,
        mean_absolute_error=response.mean_absolute_error,
        max_channel_delta=response.max_channel_delta,
        changed_bounds=_rect_from_proto(changed_bounds) if changed_bounds is not None else None,
        matches=response.matches,
    )


def _capture_delta_from_proto(response: Any) -> CaptureDeltaResult:
    changed_bounds = _message_field(response, "changed_bounds")
    capture_region = _message_field(response, "capture_region")
    metadata = _message_field(response, "metadata")
    return CaptureDeltaResult(
        stream_id=response.stream_id,
        sequence=response.sequence,
        low_bandwidth=bool(getattr(response, "low_bandwidth", True)),
        full_frame=response.full_frame,
        frame_width=response.frame_width,
        frame_height=response.frame_height,
        pixel_format=_pixel_format_name(response.pixel_format),
        capture_region=_rect_from_proto(capture_region) if capture_region is not None else None,
        changed_bounds=_rect_from_proto(changed_bounds) if changed_bounds is not None else None,
        changed_pixels=response.changed_pixels,
        changed_ratio=response.changed_ratio,
        patch_stride=response.patch_stride,
        patch=response.patch,
        metadata=_capture_metadata_from_proto(metadata) if metadata is not None else None,
    )


def _capture_backends_from_proto(response: Any) -> CaptureBackendsResult:
    region = _message_field(response, "region")
    return CaptureBackendsResult(
        session_type=response.session_type,
        desktop=_optional_scalar(response, "desktop"),
        pipewire_session_available=response.pipewire_session_available,
        pipewire_backend_feature_enabled=response.pipewire_backend_feature_enabled,
        egl_backend_feature_enabled=response.egl_backend_feature_enabled,
        output_path=response.output_path,
        region=_rect_from_proto(region) if region is not None else None,
        image_backends=tuple(
            _capture_backend_from_proto(backend) for backend in response.image_backends
        ),
        zero_copy_backends=tuple(
            _zero_copy_backend_from_proto(backend) for backend in response.zero_copy_backends
        ),
        probes=tuple(_capture_backend_probe_from_proto(probe) for probe in response.probes),
        warnings=tuple(response.warnings),
    )


def _capture_backend_from_proto(backend: Any) -> CaptureBackend:
    return CaptureBackend(
        name=backend.name,
        backend_kind=backend.backend_kind,
        command=_optional_scalar(backend, "command"),
        available=backend.available,
        supports_output=backend.supports_output,
        supports_file_capture=backend.supports_file_capture,
        supports_stdout_capture=backend.supports_stdout_capture,
        supports_stdout_region_capture=backend.supports_stdout_region_capture,
        selected=backend.selected,
        reason=_optional_scalar(backend, "reason"),
    )


def _zero_copy_backend_from_proto(backend: Any) -> ZeroCopyBackend:
    return ZeroCopyBackend(
        name=backend.name,
        backend_kind=backend.backend_kind,
        transport=backend.transport,
        availability=backend.availability,
        selected=backend.selected,
        pipewire_backend_feature_enabled=backend.pipewire_backend_feature_enabled,
        egl_backend_feature_enabled=backend.egl_backend_feature_enabled,
        reason=_optional_scalar(backend, "reason"),
    )


def _capture_backend_probe_from_proto(probe: Any) -> CaptureBackendProbeResult:
    return CaptureBackendProbeResult(
        probe=probe.probe,
        ok=probe.ok,
        backend_name=_optional_scalar(probe, "backend_name"),
        backend_kind=_optional_scalar(probe, "backend_kind"),
        detail=probe.detail,
        output_path=_optional_scalar(probe, "output_path"),
        bytes_written=_optional_int(probe, "bytes_written"),
        width=_optional_int(probe, "width"),
        height=_optional_int(probe, "height"),
    )


def _pixel_format_name(value: int) -> str:
    return {
        1: "rgb8",
        2: "rgba8",
        3: "bgra8",
    }.get(value, "unspecified")


def _ui_state_from_proto(response: Any) -> UiStateResult:
    changed_bounds = _message_field(response, "changed_bounds")
    latest_diff = _message_field(response, "latest_diff")
    return UiStateResult(
        state=_ui_state_name(response.state),
        compared_transitions=response.compared_transitions,
        stable_transitions=response.stable_transitions,
        loading_transitions=response.loading_transitions,
        trailing_stable_transitions=response.trailing_stable_transitions,
        latest_diff=_visual_diff_from_proto(latest_diff),
        max_changed_ratio=response.max_changed_ratio,
        mean_changed_ratio=response.mean_changed_ratio,
        changed_bounds=_rect_from_proto(changed_bounds) if changed_bounds is not None else None,
    )


def _ui_state_name(value: int) -> str:
    return {
        1: "stable",
        2: "loading",
        3: "changing",
    }.get(value, "unspecified")


def _detect_ui_elements_from_proto(response: Any) -> DetectUiElementsResult:
    return DetectUiElementsResult(
        backend_name=response.backend_name,
        backend_kind=response.backend_kind,
        warnings=tuple(response.warnings),
        elements=tuple(_ui_element_from_proto(element) for element in response.elements),
    )


def _dmabuf_probe_from_proto(response: Any) -> DmaBufProbeResult:
    return DmaBufProbeResult(
        import_target=_dmabuf_import_target_name(response.import_target),
        backend_name=response.backend_name,
        stream_node_id=response.stream_node_id,
        pipewire_serial=_optional_int(response, "pipewire_serial"),
        width=response.width,
        height=response.height,
        pixel_format=response.pixel_format,
        fourcc=response.fourcc,
        planes=response.planes,
        memory_layout=response.memory_layout,
        synchronization=response.synchronization,
        egl_version=_optional_scalar(response, "egl_version"),
        egl_modifiers=_optional_bool(response, "egl_modifiers"),
        texture_id=_optional_int(response, "texture_id"),
    )


def _plugin_list_from_proto(response: Any) -> PluginListResult:
    return PluginListResult(
        sdk_version=response.sdk_version,
        plugins=tuple(_plugin_from_proto(plugin) for plugin in response.plugins),
        errors=tuple(
            PluginDiscoveryError(path=error.path, message=error.message)
            for error in response.errors
        ),
    )


def _plugin_from_proto(plugin: Any) -> Plugin:
    return Plugin(
        id=plugin.id,
        name=plugin.name,
        version=plugin.version,
        description=_optional_scalar(plugin, "description"),
        root_dir=plugin.root_dir,
        manifest_path=plugin.manifest_path,
        capabilities=tuple(plugin.capabilities),
        entrypoint_kind=_optional_scalar(plugin, "entrypoint_kind"),
        entrypoint_command=tuple(plugin.entrypoint_command),
        tools=tuple(_plugin_tool_from_proto(tool) for tool in plugin.tools),
        metadata=dict(plugin.metadata),
    )


def _plugin_tool_from_proto(tool: Any) -> PluginTool:
    try:
        input_schema = json.loads(tool.input_schema_json or "{}")
    except json.JSONDecodeError:
        input_schema = {}
    if not isinstance(input_schema, dict):
        input_schema = {}
    return PluginTool(
        name=tool.name,
        description=tool.description,
        capabilities=tuple(tool.capabilities),
        input_schema=input_schema,
    )


def _plugin_execution_from_proto(response: Any) -> PluginToolExecutionResult:
    result_json = _optional_scalar(response, "result_json")
    result = None
    if result_json is not None:
        try:
            result = json.loads(result_json)
        except json.JSONDecodeError:
            result = result_json
    return PluginToolExecutionResult(
        ok=response.ok,
        plugin_id=response.plugin_id,
        tool=response.tool,
        exit_code=response.exit_code,
        stdout=response.stdout,
        stderr=response.stderr,
        result=result,
        error=_optional_scalar(response, "error"),
    )


def _desktop_action_from_proto(response: Any) -> DesktopActionResult:
    return DesktopActionResult(
        app=response.app,
        action=response.action,
        detail=response.detail,
        backend_name=response.backend_name,
        verified=getattr(response, "verified", False),
        verification_detail=_optional_scalar(response, "verification_detail"),
    )


def _desktop_locate_from_proto(response: Any) -> DesktopLocateResult:
    point = _message_field(response, "point")
    rect = _message_field(response, "rect")
    return DesktopLocateResult(
        app=response.app,
        target=response.target,
        x=point.x if point is not None else 0,
        y=point.y if point is not None else 0,
        rect=_rect_from_proto(rect) if rect is not None else None,
        source=response.source,
    )


def _desktop_profiles_from_proto(response: Any) -> DesktopProfilesResult:
    return DesktopProfilesResult(
        schema_version=response.schema_version,
        count=int(response.count),
        profiles=tuple(_desktop_profile_from_proto(profile) for profile in response.profiles),
    )


def _desktop_profile_from_proto(profile: Any) -> DesktopProfile:
    availability = _message_field(profile, "availability")
    return DesktopProfile(
        id=profile.id,
        aliases=tuple(profile.aliases),
        search_name=profile.search_name,
        desktop_ids=tuple(profile.desktop_ids),
        commands=tuple(
            DesktopProfileCommand(
                program=command.program,
                args=tuple(command.args),
                display=command.display,
                available=_optional_bool(command, "available"),
            )
            for command in profile.commands
        ),
        targets=tuple(
            DesktopProfileTarget(
                name=target.name,
                supports=tuple(target.supports),
                sources=tuple(target.sources),
                can_locate=target.can_locate,
                can_click=target.can_click,
                can_drag=target.can_drag,
                can_type=target.can_type,
                can_assert_present=target.can_assert_present,
                can_assert_active=target.can_assert_active,
                can_assert_contains=target.can_assert_contains,
                accessibility_selector=_optional_scalar(target, "accessibility_selector"),
                visual_layout=target.visual_layout,
                visual_rect=target.visual_rect,
            )
            for target in profile.targets
        ),
        availability=_desktop_profile_availability_from_proto(availability),
    )


def _desktop_profile_availability_from_proto(
    availability: Any | None,
) -> DesktopProfileAvailability:
    if availability is None:
        return DesktopProfileAvailability(
            checked=False,
            installed=None,
            command_available=None,
            desktop_entry_available=None,
            available_commands=(),
            available_desktop_ids=(),
        )
    return DesktopProfileAvailability(
        checked=availability.checked,
        installed=_optional_bool(availability, "installed"),
        command_available=_optional_bool(availability, "command_available"),
        desktop_entry_available=_optional_bool(availability, "desktop_entry_available"),
        available_commands=tuple(availability.available_commands),
        available_desktop_ids=tuple(availability.available_desktop_ids),
    )


def _rect_from_proto(rect: Any | None) -> Rect:
    if rect is None:
        return Rect(x=0, y=0, width=0, height=0)
    return Rect(x=rect.x, y=rect.y, width=rect.width, height=rect.height)


def _point_from_proto(point: Any | None) -> Point | None:
    if point is None:
        return None
    return Point(x=point.x, y=point.y)


def _rect_to_proto(messages: Any, rect: Rect) -> Any:
    return messages.Rect(x=rect.x, y=rect.y, width=rect.width, height=rect.height)


def _mouse_button_to_proto(messages: Any, button: str) -> int:
    normalized = button.strip().casefold().replace("-", "_")
    names = {
        "left": "MOUSE_BUTTON_LEFT",
        "middle": "MOUSE_BUTTON_MIDDLE",
        "right": "MOUSE_BUTTON_RIGHT",
    }
    try:
        name = names[normalized]
    except KeyError as error:
        raise ValueError("button must be left, middle, or right") from error

    if hasattr(messages, name):
        return int(getattr(messages, name))
    enum = getattr(messages, "MouseButton", None)
    if enum is not None and hasattr(enum, "Value"):
        return int(enum.Value(name))
    return {"MOUSE_BUTTON_LEFT": 1, "MOUSE_BUTTON_MIDDLE": 2, "MOUSE_BUTTON_RIGHT": 3}[name]


def _desktop_assertion_to_proto(
    messages: Any,
    assertion: str,
    *,
    expected_text: str | None = None,
) -> int:
    normalized = assertion.strip().casefold().replace("-", "_")
    names = {
        "present": "DESKTOP_ASSERTION_KIND_PRESENT",
        "not_present": "DESKTOP_ASSERTION_KIND_NOT_PRESENT",
        "active": "DESKTOP_ASSERTION_KIND_ACTIVE",
        "not_active": "DESKTOP_ASSERTION_KIND_NOT_ACTIVE",
        "contains": "DESKTOP_ASSERTION_KIND_CONTAINS",
        "not_contains": "DESKTOP_ASSERTION_KIND_NOT_CONTAINS",
    }
    try:
        name = names[normalized]
    except KeyError as error:
        raise ValueError(
            "assertion must be present, not_present, active, not_active, contains, or not_contains"
        ) from error
    if normalized in {"contains", "not_contains"} and not (expected_text or "").strip():
        raise ValueError(f"assertion {normalized} requires expected_text")

    if hasattr(messages, name):
        return int(getattr(messages, name))
    enum = getattr(messages, "DesktopAssertionKind", None)
    if enum is not None and hasattr(enum, "Value"):
        return int(enum.Value(name))
    return {
        "DESKTOP_ASSERTION_KIND_PRESENT": 1,
        "DESKTOP_ASSERTION_KIND_NOT_PRESENT": 2,
        "DESKTOP_ASSERTION_KIND_ACTIVE": 3,
        "DESKTOP_ASSERTION_KIND_NOT_ACTIVE": 4,
        "DESKTOP_ASSERTION_KIND_CONTAINS": 5,
        "DESKTOP_ASSERTION_KIND_NOT_CONTAINS": 6,
    }[name]


def _validate_ratio_pair(name: str, value: tuple[float, float]) -> None:
    if len(value) != 2:
        raise ValueError(f"{name} must contain exactly two values")
    for index, part in enumerate(value):
        if not 0.0 <= float(part) <= 1.0:
            raise ValueError(f"{name}[{index}] must be between 0.0 and 1.0")


def _dmabuf_import_target_to_proto(messages: Any, import_target: str) -> int:
    normalized = import_target.strip().casefold().replace("-", "_")
    names = {
        "compute": "DMA_BUF_IMPORT_TARGET_COMPUTE",
        "egl": "DMA_BUF_IMPORT_TARGET_EGL",
        "egl_texture": "DMA_BUF_IMPORT_TARGET_EGL_TEXTURE",
    }
    try:
        name = names[normalized]
    except KeyError as error:
        raise ValueError("import_target must be compute, egl, or egl_texture") from error
    if hasattr(messages, name):
        return int(getattr(messages, name))
    enum = getattr(messages, "DmaBufImportTarget", None)
    if enum is not None and hasattr(enum, "Value"):
        return int(enum.Value(name))
    return {
        "DMA_BUF_IMPORT_TARGET_COMPUTE": 1,
        "DMA_BUF_IMPORT_TARGET_EGL": 2,
        "DMA_BUF_IMPORT_TARGET_EGL_TEXTURE": 3,
    }[name]


def _capture_backend_probe_value(messages: Any, probe: str) -> int:
    normalized = probe.strip().casefold().replace("-", "_")
    names = {
        "none": "CAPTURE_BACKEND_PROBE_NONE",
        "file": "CAPTURE_BACKEND_PROBE_FILE",
        "frame": "CAPTURE_BACKEND_PROBE_FRAME",
        "region": "CAPTURE_BACKEND_PROBE_REGION",
        "dmabuf": "CAPTURE_BACKEND_PROBE_DMA_BUF",
        "dma_buf": "CAPTURE_BACKEND_PROBE_DMA_BUF",
        "all": "CAPTURE_BACKEND_PROBE_ALL",
    }
    try:
        name = names[normalized]
    except KeyError as error:
        raise ValueError("probe must be none, file, frame, region, dmabuf, or all") from error
    if hasattr(messages, name):
        return int(getattr(messages, name))
    enum = getattr(messages, "CaptureBackendProbe", None)
    if enum is not None and hasattr(enum, "Value"):
        return int(enum.Value(name))
    return {
        "CAPTURE_BACKEND_PROBE_NONE": 1,
        "CAPTURE_BACKEND_PROBE_FILE": 2,
        "CAPTURE_BACKEND_PROBE_FRAME": 3,
        "CAPTURE_BACKEND_PROBE_REGION": 4,
        "CAPTURE_BACKEND_PROBE_DMA_BUF": 5,
        "CAPTURE_BACKEND_PROBE_ALL": 6,
    }[name]


def _dmabuf_import_target_name(value: int) -> str:
    return {
        1: "compute",
        2: "egl",
        3: "egl_texture",
    }.get(value, "unspecified")


def _capture_metadata_from_proto(metadata: Any) -> CaptureMetadata:
    return CaptureMetadata(
        width=metadata.width,
        height=metadata.height,
        backend=metadata.backend,
        captured_at_unix_ms=metadata.captured_at_unix_ms,
    )


def _message_accepts_field(message_type: Any, field_name: str) -> bool:
    descriptor = getattr(message_type, "DESCRIPTOR", None)
    fields = getattr(descriptor, "fields_by_name", {})
    return field_name in fields


def _action_result_from_proto(response: Any) -> ActionResult:
    return ActionResult(
        ok=response.ok,
        message=response.message,
        backend_name=_optional_scalar(response, "backend_name"),
        backend_kind=_optional_scalar(response, "backend_kind"),
    )


def _optional_scalar(message: Any, field_name: str) -> str | None:
    if _has_field(message, field_name):
        return getattr(message, field_name)
    return None


def _optional_int(message: Any, field_name: str) -> int | None:
    if _has_field(message, field_name):
        return int(getattr(message, field_name))
    return None


def _optional_bool(message: Any, field_name: str) -> bool | None:
    if _has_field(message, field_name):
        return bool(getattr(message, field_name))
    return None


def _message_field(message: Any, field_name: str) -> Any | None:
    value = getattr(message, field_name, None)
    if value is None:
        return None
    if _has_field(message, field_name):
        return value
    return None


def _has_field(message: Any, field_name: str) -> bool:
    has_field = getattr(message, "HasField", None)
    if has_field is None:
        return getattr(message, field_name, None) is not None
    try:
        return bool(has_field(field_name))
    except ValueError:
        return getattr(message, field_name, None) is not None
