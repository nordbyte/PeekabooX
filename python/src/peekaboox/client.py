from __future__ import annotations

from dataclasses import dataclass, field
from importlib import import_module
from os import PathLike
from types import ModuleType
from typing import Any, Sequence


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
class WindowInfo:
    id: str
    title: str
    app_id: str | None
    bounds: Rect
    focused: bool
    state: str


@dataclass(frozen=True, slots=True)
class UiElement:
    id: str
    role: str
    label: str | None
    bounds: Rect
    confidence: float
    states: tuple[str, ...] = ()


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
class OcrBlock:
    text: str
    element: UiElement


@dataclass(frozen=True, slots=True)
class OcrResult:
    backend_name: str
    text: str
    blocks: tuple[OcrBlock, ...]
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
class ActionResult:
    ok: bool
    message: str


@dataclass(frozen=True, slots=True)
class DesktopState:
    active_window: WindowInfo | None
    windows: tuple[WindowInfo, ...]
    elements: tuple[UiElement, ...]


@dataclass(slots=True)
class PeekabooXClient:
    """Synchronous Python client for the local PeekabooX daemon gRPC API."""

    target: str = DEFAULT_GRPC_TARGET
    timeout_seconds: float = 5.0
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
            self._channel = grpc_module.insecure_channel(self.target)
            self.stub = services.PeekabooXStub(self._channel)

    def close(self) -> None:
        if self._channel is not None:
            close = getattr(self._channel, "close", None)
            if close is not None:
                close()

    def capture_screen(self, include_semantic_tree: bool = False) -> CaptureScreenResult:
        request = self.messages.CaptureScreenRequest(
            target=self.messages.CaptureTarget(full_screen=True),
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
        per_channel_threshold: int | None = None,
        low_bandwidth: bool = True,
    ) -> CaptureDeltaResult:
        request_kwargs: dict[str, Any] = {
            "stream_id": stream_id,
            "reset": reset,
            "target": self.messages.CaptureTarget(full_screen=True),
        }
        if region is not None:
            request_kwargs["target"] = self.messages.CaptureTarget(
                region=_rect_to_proto(self.messages, region)
            )
        if per_channel_threshold is not None:
            request_kwargs["per_channel_threshold"] = per_channel_threshold
        if _message_accepts_field(self.messages.CaptureDeltaRequest, "low_bandwidth"):
            request_kwargs["low_bandwidth"] = low_bandwidth
        response = self._call("CaptureDelta", self.messages.CaptureDeltaRequest(**request_kwargs))
        return _capture_delta_from_proto(response)

    def ocr_screen(
        self,
        region: Rect | None = None,
        language: str | None = None,
    ) -> OcrResult:
        request_kwargs: dict[str, Any] = {}
        if region is not None:
            request_kwargs["region"] = _rect_to_proto(self.messages, region)
        if language is not None:
            request_kwargs["language"] = language
        response = self._call("OcrScreen", self.messages.OcrScreenRequest(**request_kwargs))
        return _ocr_result_from_proto(response)

    def ocr_region(self, region: Rect, language: str | None = None) -> OcrResult:
        return self.ocr_screen(region=region, language=language)

    def compare_images(
        self,
        expected_image: bytes,
        actual_image: bytes,
        region: Rect | None = None,
        per_channel_threshold: int | None = None,
        max_changed_ratio: float | None = None,
    ) -> VisualDiffResult:
        request_kwargs: dict[str, Any] = {
            "expected_image": expected_image,
            "actual_image": actual_image,
        }
        if region is not None:
            request_kwargs["region"] = _rect_to_proto(self.messages, region)
        if per_channel_threshold is not None:
            request_kwargs["per_channel_threshold"] = per_channel_threshold
        if max_changed_ratio is not None:
            request_kwargs["max_changed_ratio"] = max_changed_ratio
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
        per_channel_threshold: int | None = None,
        max_changed_ratio: float | None = None,
    ) -> VisualDiffResult:
        with open(expected_path, "rb") as expected_file:
            expected_image = expected_file.read()
        with open(actual_path, "rb") as actual_file:
            actual_image = actual_file.read()
        return self.compare_images(
            expected_image,
            actual_image,
            region=region,
            per_channel_threshold=per_channel_threshold,
            max_changed_ratio=max_changed_ratio,
        )

    def detect_ui_state(
        self,
        images: Sequence[bytes],
        region: Rect | None = None,
        per_channel_threshold: int | None = None,
        stable_max_changed_ratio: float | None = None,
        loading_min_changed_ratio: float | None = None,
        required_stable_transitions: int | None = None,
    ) -> UiStateResult:
        request_kwargs: dict[str, Any] = {"images": list(images)}
        if region is not None:
            request_kwargs["region"] = _rect_to_proto(self.messages, region)
        if per_channel_threshold is not None:
            request_kwargs["per_channel_threshold"] = per_channel_threshold
        if stable_max_changed_ratio is not None:
            request_kwargs["stable_max_changed_ratio"] = stable_max_changed_ratio
        if loading_min_changed_ratio is not None:
            request_kwargs["loading_min_changed_ratio"] = loading_min_changed_ratio
        if required_stable_transitions is not None:
            request_kwargs["required_stable_transitions"] = required_stable_transitions
        response = self._call(
            "DetectUiState",
            self.messages.DetectUiStateRequest(**request_kwargs),
        )
        return _ui_state_from_proto(response)

    def detect_ui_state_from_image_files(
        self,
        image_paths: Sequence[str | PathLike[str]],
        region: Rect | None = None,
        per_channel_threshold: int | None = None,
        stable_max_changed_ratio: float | None = None,
        loading_min_changed_ratio: float | None = None,
        required_stable_transitions: int | None = None,
    ) -> UiStateResult:
        images: list[bytes] = []
        for image_path in image_paths:
            with open(image_path, "rb") as image_file:
                images.append(image_file.read())
        return self.detect_ui_state(
            images,
            region=region,
            per_channel_threshold=per_channel_threshold,
            stable_max_changed_ratio=stable_max_changed_ratio,
            loading_min_changed_ratio=loading_min_changed_ratio,
            required_stable_transitions=required_stable_transitions,
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
    ) -> DetectUiElementsResult:
        request_kwargs: dict[str, Any] = {"image": image}
        if region is not None:
            request_kwargs["region"] = _rect_to_proto(self.messages, region)
        if edge_threshold is not None:
            request_kwargs["edge_threshold"] = edge_threshold
        if min_width is not None:
            request_kwargs["min_width"] = min_width
        if min_height is not None:
            request_kwargs["min_height"] = min_height
        if min_component_pixels is not None:
            request_kwargs["min_component_pixels"] = min_component_pixels
        if max_elements is not None:
            request_kwargs["max_elements"] = max_elements
        if merge_distance is not None:
            request_kwargs["merge_distance"] = merge_distance
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
    ) -> DetectUiElementsResult:
        with open(image_path, "rb") as image_file:
            image = image_file.read()
        return self.detect_ui_elements(
            image,
            region=region,
            edge_threshold=edge_threshold,
            min_width=min_width,
            min_height=min_height,
            min_component_pixels=min_component_pixels,
            max_elements=max_elements,
            merge_distance=merge_distance,
        )

    def click(
        self,
        x: int | None = None,
        y: int | None = None,
        semantic_selector: str | None = None,
        vision_fallback: bool = False,
    ) -> ActionResult:
        if semantic_selector is not None:
            if x is not None or y is not None:
                raise ValueError("provide either coordinates or semantic_selector, not both")
            request = self.messages.ClickRequest(
                semantic_selector=semantic_selector,
                vision_fallback=vision_fallback,
            )
        else:
            if x is None or y is None:
                raise ValueError("x and y are required for coordinate clicks")
            request = self.messages.ClickRequest(
                coordinates=self.messages.Point(x=x, y=y),
                vision_fallback=vision_fallback,
            )
        return _action_result_from_proto(self._call("Click", request))

    def click_selector(self, selector: str, vision_fallback: bool = False) -> ActionResult:
        return self.click(semantic_selector=selector, vision_fallback=vision_fallback)

    def type_text(
        self,
        text: str,
        typing_speed_chars_per_second: int | None = None,
    ) -> ActionResult:
        request_kwargs: dict[str, Any] = {"text": text}
        if typing_speed_chars_per_second is not None:
            request_kwargs["typing_speed_chars_per_second"] = typing_speed_chars_per_second
        request = self.messages.TypeTextRequest(**request_kwargs)
        return _action_result_from_proto(self._call("TypeText", request))

    def find_element(self, selector: str, vision_fallback: bool = False) -> tuple[UiElement, ...]:
        response = self._call(
            "FindElement",
            self.messages.FindElementRequest(
                selector=selector,
                vision_fallback=vision_fallback,
            ),
        )
        return tuple(_ui_element_from_proto(element) for element in response.elements)

    def list_windows(self) -> tuple[WindowInfo, ...]:
        response = self._call("ListWindows", self.messages.ListWindowsRequest())
        return tuple(_window_from_proto(window) for window in response.windows)

    def get_desktop_state(self) -> DesktopState:
        response = self._call("GetDesktopState", self.messages.GetDesktopStateRequest())
        active_window = _message_field(response, "active_window")
        return DesktopState(
            active_window=_window_from_proto(active_window) if active_window is not None else None,
            windows=tuple(_window_from_proto(window) for window in response.windows),
            elements=tuple(_ui_element_from_proto(element) for element in response.elements),
        )

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
    return UiElement(
        id=element.id,
        role=element.role,
        label=_optional_scalar(element, "label"),
        bounds=_rect_from_proto(bounds),
        confidence=element.confidence,
        states=tuple(element.states),
    )


def _ocr_result_from_proto(response: Any) -> OcrResult:
    return OcrResult(
        backend_name=response.backend_name,
        text=response.text,
        blocks=tuple(_ocr_block_from_proto(block) for block in response.blocks),
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


def _rect_from_proto(rect: Any | None) -> Rect:
    if rect is None:
        return Rect(x=0, y=0, width=0, height=0)
    return Rect(x=rect.x, y=rect.y, width=rect.width, height=rect.height)


def _rect_to_proto(messages: Any, rect: Rect) -> Any:
    return messages.Rect(x=rect.x, y=rect.y, width=rect.width, height=rect.height)


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
    return ActionResult(ok=response.ok, message=response.message)


def _optional_scalar(message: Any, field_name: str) -> str | None:
    if _has_field(message, field_name):
        return getattr(message, field_name)
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
