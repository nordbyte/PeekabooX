# ruff: noqa: F401

import json
import sys
import unittest
from importlib.util import find_spec
from io import StringIO
from pathlib import Path
from tempfile import TemporaryDirectory
from textwrap import dedent
from unittest.mock import patch

import peekaboox.agent.runtime as agent_runtime_module
import peekaboox.mcp.server as mcp_server_module
from peekaboox.agent import AgentRuntime, PreflightError, VerificationResult
from peekaboox.client import (
    ActionResult,
    CaptureBackend,
    CaptureBackendProbeResult,
    CaptureBackendsResult,
    CaptureDeltaResult,
    CaptureMetadata,
    CaptureScreenResult,
    DesktopActionResult,
    DesktopLocateResult,
    DesktopProfile,
    DesktopProfileAvailability,
    DesktopProfileCommand,
    DesktopProfilesResult,
    DesktopProfileTarget,
    DesktopState,
    DetectUiElementsResult,
    DmaBufProbeResult,
    OcrBlock,
    OcrResult,
    PeekabooXClient,
    Rect,
    UiElement,
    UiStateResult,
    VisualDiffResult,
    WindowBackendReport,
    WindowInfo,
    WindowListResult,
    ZeroCopyBackend,
)
from peekaboox.doctor import DoctorCategory, DoctorCheck, DoctorResult, run_doctor
from peekaboox.mcp import McpServer
from peekaboox.mcp.server import create_server
from peekaboox.memory import MemoryStore, SemanticDesktopGraph, SQLiteMemoryStore
from peekaboox.planning import PlanningEngine, WorkflowRefinementRequest, WorkflowReplanningRequest
from peekaboox.plugins import (
    PLUGIN_MANIFEST_FILE,
    PLUGIN_SDK_VERSION,
    discover_plugins,
    execute_plugin_tool,
    trust_plugin,
    verify_plugin_trust,
)
from peekaboox.security import (
    Capability,
    CapabilityDeniedError,
    CapabilityPolicy,
    CapabilityProfile,
    ConfirmationDeniedError,
    ConfirmationPolicy,
    ConfirmationRequiredError,
    DangerousAction,
    JsonlAuditLogger,
    capability_profile,
)
from peekaboox.workflows import (
    WORKFLOW_SCHEMA_VERSION,
    Workflow,
    WorkflowRecorder,
    WorkflowStep,
    create_workflow_bundle,
    dump_workflow_text,
    load_workflow_file,
    load_workflow_text,
    workflow_json_schema,
)


class FakeClient:
    def __init__(self) -> None:
        self.clicked_at: tuple[int, int] | None = None
        self.moved_to: tuple[int, int] | None = None
        self.dragged: tuple[int, int, int, int, str, int] | None = None
        self.hotkeys: list[tuple[str, ...]] = []
        self.last_vision_fallback = False
        self.last_click_options: dict[str, object] = {}
        self.typed_text: str | None = None
        self.pasted_text: str | None = None
        self.preserve_clipboard: bool | None = None
        self.last_type_options: dict[str, object] = {}
        self.last_paste_options: dict[str, object] = {}
        self.last_hotkey_options: dict[str, object] = {}
        self.last_find_selector: str | None = None
        self.desktop_calls: list[tuple[str, dict[str, object]]] = []
        self.last_window_query: dict[str, object] | None = None
        self.last_window_result_query: dict[str, object] | None = None
        self.last_capture: dict[str, object] | None = None

    def capture_screen(
        self,
        include_semantic_tree: bool = False,
        region: Rect | None = None,
        window_id: str | None = None,
    ) -> CaptureScreenResult:
        self.last_capture = {
            "include_semantic_tree": include_semantic_tree,
            "region": region,
            "window_id": window_id,
        }
        semantic_tree = (
            self._submit_button(),
        ) if include_semantic_tree else ()
        return CaptureScreenResult(
            image=b"png",
            mime_type="image/png",
            semantic_tree=semantic_tree,
            metadata=CaptureMetadata(
                width=800,
                height=600,
                backend="fake",
                captured_at_unix_ms=123,
            ),
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
        return CaptureDeltaResult(
            stream_id=stream_id,
            sequence=1 if reset else 2,
            low_bandwidth=low_bandwidth,
            full_frame=reset,
            frame_width=800,
            frame_height=600,
            pixel_format="rgba8",
            capture_region=region,
            changed_bounds=region or Rect(x=10, y=20, width=30, height=40),
            changed_pixels=1200,
            changed_ratio=0.0025,
            patch_stride=120,
            patch=b"patch",
            metadata=CaptureMetadata(
                width=800,
                height=600,
                backend="fake",
                captured_at_unix_ms=124,
            ),
        )

    def capture_backends(
        self,
        output: str = "screenshot.png",
        region: Rect | None = None,
        diagnose: bool = False,
        probe: str = "none",
    ) -> CaptureBackendsResult:
        return CaptureBackendsResult(
            session_type="wayland",
            desktop="GNOME",
            pipewire_session_available=True,
            pipewire_backend_feature_enabled=True,
            egl_backend_feature_enabled=False,
            output_path=str(output),
            region=region,
            image_backends=(
                CaptureBackend(
                    name="portal",
                    backend_kind="wayland",
                    command=None,
                    available=True,
                    supports_output=True,
                    supports_file_capture=True,
                    supports_stdout_capture=True,
                    supports_stdout_region_capture=True,
                    selected=True,
                    reason=None,
                ),
            ),
            zero_copy_backends=(
                ZeroCopyBackend(
                    name="pipewire",
                    backend_kind="wayland",
                    transport="dmabuf",
                    availability="available",
                    selected=True,
                    pipewire_backend_feature_enabled=True,
                    egl_backend_feature_enabled=False,
                    reason=None,
                ),
            ),
            probes=(
                CaptureBackendProbeResult(
                    probe=probe,
                    ok=True,
                    backend_name="portal",
                    backend_kind="wayland",
                    detail="captured 320x180",
                    output_path=str(output) if probe == "file" else None,
                    bytes_written=1234 if probe == "file" else None,
                    width=region.width if region else 320,
                    height=region.height if region else 180,
                ),
            )
            if probe != "none"
            else (),
            warnings=() if diagnose else ("diagnostics disabled",),
        )

    def probe_dmabuf(self, import_target: str = "compute") -> DmaBufProbeResult:
        return DmaBufProbeResult(
            import_target=import_target,
            backend_name="fake-dmabuf",
            stream_node_id=7,
            pipewire_serial=11,
            width=800,
            height=600,
            pixel_format="rgba8",
            fourcc=875713112,
            planes=1,
            memory_layout="single-plane",
            synchronization="implicit",
            egl_version=None,
            egl_modifiers=None,
            texture_id=None,
        )

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
        self.last_window_query = {
            "id": id,
            "app": app,
            "title": title,
            "title_regex": title_regex,
            "focused": focused,
            "limit": limit,
            "sort": sort,
            "backend": backend,
            "diagnose": diagnose,
        }
        return (
            WindowInfo(
                id="window-1",
                title="Terminal",
                app_id="org.example.Terminal",
                bounds=Rect(x=1, y=2, width=800, height=600),
                focused=True,
                state="normal",
            ),
        )

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
        self.last_window_result_query = {
            "id": id,
            "app": app,
            "title": title,
            "title_regex": title_regex,
            "focused": focused,
            "limit": limit,
            "sort": sort,
            "backend": backend,
            "diagnose": diagnose,
        }
        return WindowListResult(
            backend_name="fake",
            backend_kind="mock",
            warnings=("fallback used",),
            backend_reports=(
                WindowBackendReport(
                    backend_name="fake",
                    backend_kind="mock",
                    raw_window_count=1,
                    matched_window_count=1,
                    selected=True,
                    error=None,
                ),
            ),
            windows=self.list_windows(
                id=id,
                app=app,
                title=title,
                title_regex=title_regex,
                focused=focused,
                limit=limit,
                sort=sort,
                backend=backend,
                diagnose=diagnose,
            ),
        )

    def click(
        self,
        x: int | None = None,
        y: int | None = None,
        semantic_selector: str | None = None,
        vision_fallback: bool = False,
        **kwargs,
    ) -> ActionResult:
        self.last_vision_fallback = vision_fallback
        self.last_click_options = kwargs
        if semantic_selector is not None:
            self.clicked_at = None
            return ActionResult(ok=True, message=f"clicked {semantic_selector}")
        assert x is not None
        assert y is not None
        self.clicked_at = (x, y)
        return ActionResult(ok=True, message="clicked")

    def click_selector(
        self,
        selector: str,
        vision_fallback: bool = False,
        **kwargs,
    ) -> ActionResult:
        return self.click(
            semantic_selector=selector,
            vision_fallback=vision_fallback,
            **kwargs,
        )

    def move_mouse(self, x: int, y: int) -> ActionResult:
        self.moved_to = (x, y)
        return ActionResult(ok=True, message="moved")

    def drag(
        self,
        from_x: int,
        from_y: int,
        to_x: int,
        to_y: int,
        *,
        button: str = "left",
        duration_ms: int = 250,
    ) -> ActionResult:
        self.dragged = (from_x, from_y, to_x, to_y, button, duration_ms)
        return ActionResult(ok=True, message="dragged")

    def ocr_screen(
        self,
        region: Rect | None = None,
        language: str | None = None,
        **kwargs,
    ) -> OcrResult:
        block = OcrBlock(
            text="Submit",
            element=UiElement(
                id="ocr:1:2:3:4",
                role="text",
                label="Submit",
                bounds=region or Rect(x=1, y=2, width=3, height=4),
                confidence=1.0,
            ),
        )
        return OcrResult(
            backend_name="fake",
            text="Submit",
            blocks=(block,),
            words=(block,),
            warnings=(),
        )

    def ocr_region(self, region: Rect, language: str | None = None, **kwargs) -> OcrResult:
        return self.ocr_screen(region=region, language=language, **kwargs)

    def compare_images(
        self,
        expected_image: bytes,
        actual_image: bytes,
        region: Rect | None = None,
        ignore_regions=None,
        per_channel_threshold: int | None = None,
        max_changed_ratio: float | None = None,
        max_changed_pixels: int | None = None,
        max_mean_absolute_error: float | None = None,
        max_channel_delta: int | None = None,
        size_policy: str | None = None,
        alpha: str | None = None,
    ) -> VisualDiffResult:
        return VisualDiffResult(
            compared_region=region or Rect(x=0, y=0, width=1, height=1),
            compared_pixels=1,
            changed_pixels=0,
            changed_ratio=0.0,
            mean_absolute_error=0.0,
            max_channel_delta=0,
            changed_bounds=None,
            matches=True,
        )

    def compare_image_files(
        self,
        expected_path: str,
        actual_path: str,
        region: Rect | None = None,
        ignore_regions=None,
        per_channel_threshold: int | None = None,
        max_changed_ratio: float | None = None,
        max_changed_pixels: int | None = None,
        max_mean_absolute_error: float | None = None,
        max_channel_delta: int | None = None,
        size_policy: str | None = None,
        alpha: str | None = None,
    ) -> VisualDiffResult:
        return self.compare_images(
            b"expected",
            b"actual",
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
        images: tuple[bytes, ...] | list[bytes],
        region: Rect | None = None,
        ignore_regions=None,
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
        return UiStateResult(
            state="stable",
            compared_transitions=1,
            stable_transitions=1,
            loading_transitions=0,
            trailing_stable_transitions=1,
            latest_diff=self.compare_images(b"first", b"second", region=region),
            max_changed_ratio=0.0,
            mean_changed_ratio=0.0,
            changed_bounds=None,
        )

    def detect_ui_state_from_image_files(
        self,
        image_paths: tuple[str, ...] | list[str],
        region: Rect | None = None,
        ignore_regions=None,
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
        return self.detect_ui_state(
            [b"first", b"second"],
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
        ignore_regions: tuple[Rect, ...] | None = None,
        min_confidence: float | None = None,
        max_width: int | None = None,
        max_height: int | None = None,
        min_area: int | None = None,
        max_area: int | None = None,
        padding: int | None = None,
        sort: str | None = None,
        mask_output_path: str | None = None,
        overlay_output_path: str | None = None,
    ) -> DetectUiElementsResult:
        return DetectUiElementsResult(
            backend_name="heuristic_vision",
            backend_kind="vision",
            warnings=(),
            elements=(
                UiElement(
                    id="vision:0:1:2:3:4",
                    role="visual-region",
                    label=None,
                    bounds=region or Rect(x=1, y=2, width=3, height=4),
                    confidence=0.86,
                    states=("visible",),
                ),
            ),
        )

    def detect_ui_elements_from_image_file(
        self,
        image_path: str,
        region: Rect | None = None,
        edge_threshold: int | None = None,
        min_width: int | None = None,
        min_height: int | None = None,
        min_component_pixels: int | None = None,
        max_elements: int | None = None,
        merge_distance: int | None = None,
        ignore_regions: tuple[Rect, ...] | None = None,
        min_confidence: float | None = None,
        max_width: int | None = None,
        max_height: int | None = None,
        min_area: int | None = None,
        max_area: int | None = None,
        padding: int | None = None,
        sort: str | None = None,
        mask_output_path: str | None = None,
        overlay_output_path: str | None = None,
    ) -> DetectUiElementsResult:
        return self.detect_ui_elements(
            b"image",
            region=region,
            edge_threshold=edge_threshold,
            min_width=min_width,
            min_height=min_height,
            min_component_pixels=min_component_pixels,
            max_elements=max_elements,
            merge_distance=merge_distance,
            ignore_regions=ignore_regions,
            min_confidence=min_confidence,
            max_width=max_width,
            max_height=max_height,
            min_area=min_area,
            max_area=max_area,
            padding=padding,
            sort=sort,
            mask_output_path=mask_output_path,
            overlay_output_path=overlay_output_path,
        )

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
        self.typed_text = text
        self.last_type_options = {
            "typing_speed_chars_per_second": typing_speed_chars_per_second,
            "dry_run": dry_run,
            "backend": backend,
            "delay_ms": delay_ms,
            "key_delay_ms": key_delay_ms,
        }
        return ActionResult(ok=True, message=f"typed {len(text)} chars")

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
        self.pasted_text = text
        self.preserve_clipboard = preserve_clipboard
        self.last_paste_options = {
            "dry_run": dry_run,
            "clipboard_backend": clipboard_backend,
            "hotkey_backend": hotkey_backend,
            "delay_ms": delay_ms,
            "restore_delay_ms": restore_delay_ms,
            "restore_policy": restore_policy,
        }
        return ActionResult(ok=True, message=f"pasted {len(text)} chars")

    def hotkey(
        self,
        keys: list[str] | tuple[str, ...] | str,
        *,
        dry_run: bool = False,
        backend: str | None = None,
        delay_ms: int | None = None,
        key_delay_ms: int | None = None,
        repeat: int | None = None,
        interval_ms: int | None = None,
        release_before: bool = False,
        release_after: bool = False,
    ) -> ActionResult:
        if isinstance(keys, str):
            key_values = tuple(keys.split("+"))
        else:
            key_values = tuple(keys)
        self.hotkeys.append(key_values)
        self.last_hotkey_options = {
            "dry_run": dry_run,
            "backend": backend,
            "delay_ms": delay_ms,
            "key_delay_ms": key_delay_ms,
            "repeat": repeat,
            "interval_ms": interval_ms,
            "release_before": release_before,
            "release_after": release_after,
        }
        return ActionResult(ok=True, message="hotkey")

    def find_element(self, selector: str, vision_fallback: bool = False) -> tuple[UiElement, ...]:
        self.last_find_selector = selector
        self.last_vision_fallback = vision_fallback
        return (self._submit_button(),)

    def get_desktop_state(self) -> DesktopState:
        return DesktopState(
            active_window=self.list_windows()[0],
            windows=self.list_windows(),
            elements=(self._submit_button(),),
        )

    def desktop_focus(self, app: str, **kwargs) -> DesktopActionResult:
        self.desktop_calls.append(("focus", {"app": app, **kwargs}))
        return DesktopActionResult(
            app=app,
            action="focus",
            detail="focused",
            backend_name="fake-desktop",
            verified=True,
            verification_detail="window fake-window is focused",
            focus_diagnostics=["windows: selected fake-window", "verify: fake-window focused"],
        )

    def desktop_locate(self, app: str, target: str, **kwargs) -> DesktopLocateResult:
        self.desktop_calls.append(("locate", {"app": app, "target": target, **kwargs}))
        return DesktopLocateResult(
            app=app,
            target=target,
            x=10,
            y=20,
            rect=Rect(x=1, y=2, width=30, height=40),
            source="fake",
        )

    def desktop_profiles(self, app: str | None = None, **kwargs) -> DesktopProfilesResult:
        self.desktop_calls.append(("profiles", {"app": app, **kwargs}))
        profiles = (
            DesktopProfile(
                id="telegram",
                aliases=("telegram", "telegram-desktop", "org.telegram.desktop"),
                search_name="Telegram",
                desktop_ids=("telegram-desktop", "org.telegram.desktop"),
                commands=(
                    DesktopProfileCommand(
                        program="telegram-desktop",
                        args=(),
                        display="telegram-desktop",
                        available=None,
                    ),
                    DesktopProfileCommand(
                        program="flatpak",
                        args=("run", "org.telegram.desktop"),
                        display="flatpak run org.telegram.desktop",
                        available=None,
                    ),
                ),
                targets=(
                    DesktopProfileTarget(
                        name="search-input",
                        supports=("locate", "click", "type-into", "assert-contains"),
                        sources=("visual-layout",),
                        can_locate=True,
                        can_click=True,
                        can_drag=False,
                        can_type=True,
                        can_assert_present=True,
                        can_assert_active=False,
                        can_assert_contains=True,
                        accessibility_selector=None,
                        visual_layout=True,
                        visual_rect=True,
                    ),
                    DesktopProfileTarget(
                        name="message-input",
                        supports=("locate", "click", "type-into", "assert-contains"),
                        sources=("visual-layout",),
                        can_locate=True,
                        can_click=True,
                        can_drag=False,
                        can_type=True,
                        can_assert_present=True,
                        can_assert_active=False,
                        can_assert_contains=True,
                        accessibility_selector=None,
                        visual_layout=True,
                        visual_rect=True,
                    ),
                ),
                availability=DesktopProfileAvailability(
                    checked=False,
                    installed=None,
                    command_available=None,
                    desktop_entry_available=None,
                    available_commands=(),
                    available_desktop_ids=(),
                ),
            ),
        )
        if app and app != "telegram":
            profiles = ()
        return DesktopProfilesResult(
            schema_version="desktop-profiles.v1",
            count=len(profiles),
            profiles=profiles,
        )

    def desktop_click(self, app: str, target: str, **kwargs) -> DesktopActionResult:
        self.desktop_calls.append(("click", {"app": app, "target": target, **kwargs}))
        return DesktopActionResult(
            app=app,
            action="click",
            detail=f"clicked {target}",
            backend_name="fake-desktop",
            verified=bool(kwargs.get("verify", False)),
            verification_detail="target still present" if kwargs.get("verify", False) else None,
            focus_diagnostics=["windows: selected fake-window", "verify: fake-window focused"],
        )

    def desktop_drag(self, app: str, target: str, **kwargs) -> DesktopActionResult:
        self.desktop_calls.append(("drag", {"app": app, "target": target, **kwargs}))
        return DesktopActionResult(
            app=app,
            action="drag",
            detail=f"dragged {target}",
            backend_name="fake-desktop",
            verified=bool(kwargs.get("verify", False)),
            verification_detail="target still present" if kwargs.get("verify", False) else None,
            focus_diagnostics=["windows: selected fake-window", "verify: fake-window focused"],
        )

    def desktop_type_into(self, app: str, target: str, text: str, **kwargs) -> DesktopActionResult:
        self.desktop_calls.append(
            ("type_into", {"app": app, "target": target, "text": text, **kwargs})
        )
        return DesktopActionResult(
            app=app,
            action="type-into",
            detail=f"typed into {target}",
            backend_name="fake-desktop",
            verified=bool(kwargs.get("verify", False)),
            verification_detail="target contains typed text" if kwargs.get("verify", False) else None,
            focus_diagnostics=["windows: selected fake-window", "verify: fake-window focused"],
        )

    def desktop_assert(self, app: str, target: str, **kwargs) -> DesktopActionResult:
        self.desktop_calls.append(("assert", {"app": app, "target": target, **kwargs}))
        return DesktopActionResult(
            app=app,
            action="assert",
            detail=f"asserted {target}",
            backend_name="fake-desktop",
        )

    def _submit_button(self) -> UiElement:
        return UiElement(
            id="button-1",
            role="push button",
            label="Submit",
            bounds=Rect(x=10, y=20, width=90, height=30),
            confidence=1.0,
            states=("enabled", "visible"),
        )


class FlakyActionClient(FakeClient):
    def __init__(self, failures_before_success: int) -> None:
        super().__init__()
        self.failures_before_success = failures_before_success
        self.click_calls = 0

    def click(
        self,
        x: int | None = None,
        y: int | None = None,
        semantic_selector: str | None = None,
        vision_fallback: bool = False,
        **kwargs,
    ) -> ActionResult:
        self.click_calls += 1
        if self.click_calls <= self.failures_before_success:
            return ActionResult(ok=False, message="target not ready")
        return super().click(
            x=x,
            y=y,
            semantic_selector=semantic_selector,
            vision_fallback=vision_fallback,
            **kwargs,
        )


class MovedSubmitClient(FakeClient):
    def _submit_button(self) -> UiElement:
        return UiElement(
            id="button-1",
            role="push button",
            label="Submit",
            bounds=Rect(x=100, y=200, width=90, height=30),
            confidence=1.0,
            states=("enabled", "visible"),
        )


class SemanticClickMissClient(FakeClient):
    def __init__(self) -> None:
        super().__init__()
        self.semantic_click_calls = 0

    def click(
        self,
        x: int | None = None,
        y: int | None = None,
        semantic_selector: str | None = None,
        vision_fallback: bool = False,
        **kwargs,
    ) -> ActionResult:
        if semantic_selector is not None:
            self.semantic_click_calls += 1
            self.last_vision_fallback = vision_fallback
            self.last_click_options = kwargs
            return ActionResult(ok=False, message="selector miss")
        return super().click(
            x=x,
            y=y,
            semantic_selector=semantic_selector,
            vision_fallback=vision_fallback,
            **kwargs,
        )


class VisionFallbackFindClient(FakeClient):
    def find_element(self, selector: str, vision_fallback: bool = False) -> tuple[UiElement, ...]:
        self.last_find_selector = selector
        self.last_vision_fallback = vision_fallback
        return (self._submit_button(),) if vision_fallback else ()

    def get_desktop_state(self) -> DesktopState:
        return DesktopState(
            active_window=self.list_windows()[0],
            windows=self.list_windows(),
            elements=(),
        )


def _protobuf_available() -> bool:
    try:
        return find_spec("grpc") is not None and find_spec("google.protobuf") is not None
    except ModuleNotFoundError:
        return False
