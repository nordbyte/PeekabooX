import json
import sys
import unittest
from io import StringIO
from importlib.util import find_spec
from pathlib import Path
from tempfile import TemporaryDirectory
from textwrap import dedent
from unittest.mock import patch

import peekaboox.agent.runtime as agent_runtime_module
from peekaboox.agent import AgentRuntime, PreflightError, VerificationResult
from peekaboox.client import (
    ActionResult,
    CaptureBackend,
    CaptureBackendProbeResult,
    CaptureBackendsResult,
    CaptureDeltaResult,
    CaptureMetadata,
    CaptureScreenResult,
    DetectUiElementsResult,
    DesktopActionResult,
    DesktopLocateResult,
    DesktopState,
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
from peekaboox.memory import MemoryStore, SQLiteMemoryStore, SemanticDesktopGraph
from peekaboox.mcp import McpServer
from peekaboox.mcp.server import create_server
from peekaboox.planning import PlanningEngine, WorkflowRefinementRequest, WorkflowReplanningRequest
from peekaboox.plugins import (
    PLUGIN_MANIFEST_FILE,
    PLUGIN_SDK_VERSION,
    discover_plugins,
    execute_plugin_tool,
)
from peekaboox.security import (
    Capability,
    CapabilityDeniedError,
    CapabilityProfile,
    CapabilityPolicy,
    ConfirmationDeniedError,
    ConfirmationPolicy,
    ConfirmationRequiredError,
    DangerousAction,
    JsonlAuditLogger,
    capability_profile,
)
from peekaboox.workflows import (
    Workflow,
    WorkflowRecorder,
    WorkflowStep,
    dump_workflow_text,
    load_workflow_file,
    load_workflow_text,
)


class FakeClient:
    def __init__(self) -> None:
        self.clicked_at: tuple[int, int] | None = None
        self.moved_to: tuple[int, int] | None = None
        self.dragged: tuple[int, int, int, int, str, int] | None = None
        self.hotkeys: list[tuple[str, ...]] = []
        self.last_vision_fallback = False
        self.typed_text: str | None = None
        self.pasted_text: str | None = None
        self.preserve_clipboard: bool | None = None
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
    ) -> ActionResult:
        self.last_vision_fallback = vision_fallback
        if semantic_selector is not None:
            self.clicked_at = None
            return ActionResult(ok=True, message=f"clicked {semantic_selector}")
        assert x is not None
        assert y is not None
        self.clicked_at = (x, y)
        return ActionResult(ok=True, message="clicked")

    def click_selector(self, selector: str, vision_fallback: bool = False) -> ActionResult:
        return self.click(semantic_selector=selector, vision_fallback=vision_fallback)

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
        per_channel_threshold: int | None = None,
        max_changed_ratio: float | None = None,
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
        per_channel_threshold: int | None = None,
        max_changed_ratio: float | None = None,
    ) -> VisualDiffResult:
        return self.compare_images(
            b"expected",
            b"actual",
            region=region,
            per_channel_threshold=per_channel_threshold,
            max_changed_ratio=max_changed_ratio,
        )

    def detect_ui_state(
        self,
        images: tuple[bytes, ...] | list[bytes],
        region: Rect | None = None,
        per_channel_threshold: int | None = None,
        stable_max_changed_ratio: float | None = None,
        loading_min_changed_ratio: float | None = None,
        required_stable_transitions: int | None = None,
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
        per_channel_threshold: int | None = None,
        stable_max_changed_ratio: float | None = None,
        loading_min_changed_ratio: float | None = None,
        required_stable_transitions: int | None = None,
    ) -> UiStateResult:
        return self.detect_ui_state(
            [b"first", b"second"],
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
        )

    def type_text(
        self,
        text: str,
        typing_speed_chars_per_second: int | None = None,
    ) -> ActionResult:
        self.typed_text = text
        return ActionResult(ok=True, message=f"typed {len(text)} chars")

    def paste_text(self, text: str, preserve_clipboard: bool = False) -> ActionResult:
        self.pasted_text = text
        self.preserve_clipboard = preserve_clipboard
        return ActionResult(ok=True, message=f"pasted {len(text)} chars")

    def hotkey(self, keys: list[str] | tuple[str, ...] | str) -> ActionResult:
        if isinstance(keys, str):
            key_values = tuple(keys.split("+"))
        else:
            key_values = tuple(keys)
        self.hotkeys.append(key_values)
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

    def desktop_click(self, app: str, target: str, **kwargs) -> DesktopActionResult:
        self.desktop_calls.append(("click", {"app": app, "target": target, **kwargs}))
        return DesktopActionResult(
            app=app,
            action="click",
            detail=f"clicked {target}",
            backend_name="fake-desktop",
        )

    def desktop_drag(self, app: str, target: str, **kwargs) -> DesktopActionResult:
        self.desktop_calls.append(("drag", {"app": app, "target": target, **kwargs}))
        return DesktopActionResult(
            app=app,
            action="drag",
            detail=f"dragged {target}",
            backend_name="fake-desktop",
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
    ) -> ActionResult:
        self.click_calls += 1
        if self.click_calls <= self.failures_before_success:
            return ActionResult(ok=False, message="target not ready")
        return super().click(
            x=x,
            y=y,
            semantic_selector=semantic_selector,
            vision_fallback=vision_fallback,
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
    ) -> ActionResult:
        if semantic_selector is not None:
            self.semantic_click_calls += 1
            self.last_vision_fallback = vision_fallback
            return ActionResult(ok=False, message="selector miss")
        return super().click(
            x=x,
            y=y,
            semantic_selector=semantic_selector,
            vision_fallback=vision_fallback,
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


class RuntimeTests(unittest.TestCase):
    def test_agent_runtime_rejects_empty_goals(self) -> None:
        runtime = AgentRuntime()

        with self.assertRaisesRegex(ValueError, "goal"):
            runtime.plan(" ")

    def test_run_doctor_maps_cli_json(self) -> None:
        payload = {
            "status": "ok",
            "categories": [
                {
                    "name": "desktop",
                    "status": "ok",
                    "severity": "info",
                    "ok_count": 1,
                    "warn_count": 0,
                    "fail_count": 0,
                    "total_count": 1,
                },
                {
                    "name": "ocr",
                    "status": "warn",
                    "severity": "warning",
                    "ok_count": 0,
                    "warn_count": 1,
                    "fail_count": 0,
                    "total_count": 1,
                },
            ],
            "checks": [
                {
                    "name": "display-server",
                    "category": "desktop",
                    "status": "ok",
                    "severity": "info",
                    "detail": "display ready",
                },
                {
                    "name": "ocr",
                    "category": "ocr",
                    "status": "warn",
                    "severity": "warning",
                    "detail": "tesseract missing",
                },
            ],
        }
        script = f"import json; print(json.dumps({payload!r}))"

        result = run_doctor(command=(sys.executable, "-c", script), strict=True)

        self.assertEqual(result.status, "ok")
        self.assertTrue(result.strict)
        self.assertEqual(result.ok_count, 1)
        self.assertEqual(result.warn_count, 1)
        self.assertEqual(result.fail_count, 0)
        self.assertEqual(result.checks[0].name, "display-server")
        self.assertEqual(result.checks[0].category, "desktop")
        self.assertEqual(result.checks[1].severity, "warning")
        self.assertEqual([category.name for category in result.categories], ["desktop", "ocr"])
        self.assertEqual(result.categories[1].status, "warn")

    def test_mcp_server_registers_default_tools(self) -> None:
        server = McpServer()

        server.register_default_tools()

        self.assertIn("capture_screen", server.tools)
        self.assertIn("capture_delta", server.tools)
        self.assertIn("capture_backends", server.tools)
        self.assertIn("doctor", server.tools)
        self.assertIn("preflight", server.tools)
        self.assertIn("probe_dmabuf", server.tools)
        self.assertIn("get_desktop_state", server.tools)
        self.assertIn("find_element", server.tools)
        self.assertIn("click", server.tools)
        self.assertIn("move_mouse", server.tools)
        self.assertIn("drag", server.tools)
        self.assertIn("desktop_focus", server.tools)
        self.assertIn("desktop_locate", server.tools)
        self.assertIn("desktop_click", server.tools)
        self.assertIn("desktop_drag", server.tools)
        self.assertIn("desktop_type_into", server.tools)
        self.assertIn("desktop_assert", server.tools)
        self.assertIn("paste_text", server.tools)
        self.assertIn("execute_goal", server.tools)
        self.assertIn("generate_workflow", server.tools)
        self.assertIn("save_generated_workflow", server.tools)
        self.assertIn("refine_workflow", server.tools)
        self.assertIn("save_refined_workflow", server.tools)
        self.assertIn("execute_workflow", server.tools)
        self.assertIn("execute_workflow_file", server.tools)
        self.assertIn("start_workflow_recording", server.tools)
        self.assertIn("stop_workflow_recording", server.tools)
        self.assertIn("get_recorded_workflow", server.tools)
        self.assertIn("save_recorded_workflow", server.tools)
        self.assertIn("ingest_desktop_snapshot", server.tools)
        self.assertIn("latest_desktop_snapshot", server.tools)
        self.assertIn("record_desktop_event", server.tools)
        self.assertIn("desktop_graph_status", server.tools)
        self.assertIn("refresh_desktop_graph", server.tools)
        self.assertIn("query_desktop_graph", server.tools)
        self.assertIn("query_desktop_edges", server.tools)
        self.assertIn("find_elements", server.tools)
        self.assertIn("elements", server.tools)
        self.assertIn("vision_elements", server.tools)
        self.assertIn("ocr", server.tools)
        self.assertIn("ocr_image", server.tools)
        self.assertIn("capture_dmabuf", server.tools)
        self.assertIn("desktop_profiles", server.tools)
        self.assertIn("plan", server.tools)
        self.assertIn("plan_workflow", server.tools)
        self.assertIn("replan_workflow", server.tools)
        self.assertIn("load_workflow_file", server.tools)
        self.assertIn("capability_audit", server.tools)
        self.assertIn("confirmation_audit", server.tools)
        self.assertIn("preflight_audit", server.tools)
        self.assertIn("hotkey", server.tools)
        self.assertIn("vision_fallback", server.tools["find_element"].input_schema["properties"])
        self.assertIn("outputSchema", server.tools["capture_screen"].descriptor())
        self.assertIn("annotations", server.tools["click"].descriptor())
        self.assertTrue(server.tools["capture_screen"].descriptor()["annotations"]["readOnlyHint"])
        self.assertTrue(server.tools["click"].descriptor()["annotations"]["destructiveHint"])
        capture_schema = server.tools["capture_screen"].input_schema["properties"]
        self.assertIn("app", capture_schema)
        self.assertIn("window_title", capture_schema)
        self.assertIn("title_regex", capture_schema)
        window_schema = server.tools["list_windows"].input_schema["properties"]
        self.assertIn("title_regex", window_schema)
        self.assertIn("diagnose", window_schema)
        self.assertEqual(window_schema["limit"]["minimum"], 1)

    def test_agent_runtime_delegates_to_daemon_client(self) -> None:
        fake_client = FakeClient()
        runtime = AgentRuntime(client=fake_client)

        self.assertEqual(runtime.list_windows()[0].title, "Terminal")
        self.assertEqual(
            runtime.list_windows(app="Terminal", focused=True, limit=1, sort="focused")[0].title,
            "Terminal",
        )
        self.assertIsNotNone(fake_client.last_window_query)
        self.assertEqual(fake_client.last_window_query["app"], "Terminal")
        self.assertTrue(fake_client.last_window_query["focused"])
        self.assertEqual(fake_client.last_window_query["limit"], 1)
        self.assertEqual(runtime.list_windows_result(diagnose=True).backend_name, "fake")
        self.assertIsNotNone(fake_client.last_window_result_query)
        self.assertTrue(fake_client.last_window_result_query["diagnose"])
        self.assertTrue(runtime.click(10, 20).ok)
        self.assertEqual(fake_client.clicked_at, (10, 20))
        self.assertTrue(runtime.move_mouse(30, 40).ok)
        self.assertEqual(fake_client.moved_to, (30, 40))
        self.assertTrue(runtime.drag(1, 2, 3, 4, button="middle", duration_ms=500).ok)
        self.assertEqual(fake_client.dragged, (1, 2, 3, 4, "middle", 500))
        self.assertTrue(runtime.hotkey(["ctrl", "s"]).ok)
        self.assertEqual(fake_client.hotkeys[-1], ("ctrl", "s"))
        self.assertEqual(runtime.ocr_region(Rect(x=1, y=2, width=3, height=4)).text, "Submit")
        self.assertEqual(runtime.capture_delta(stream_id="agent-loop").stream_id, "agent-loop")
        self.assertEqual(runtime.capture_backends(probe="file").probes[0].probe, "file")
        self.assertTrue(runtime.compare_images(b"a", b"b").matches)
        self.assertEqual(runtime.detect_ui_state([b"a", b"b"]).state, "stable")
        self.assertEqual(runtime.detect_ui_elements(b"image").elements[0].role, "visual-region")
        self.assertEqual(runtime.desktop_focus("telegram").action, "focus")
        self.assertEqual(runtime.desktop_locate("telegram", "search-input").x, 10)
        self.assertEqual(
            runtime.desktop_click("telegram", "search-input", dry_run=True).action,
            "click",
        )
        self.assertEqual(
            runtime.desktop_type_into(
                "telegram",
                "search-input",
                "PeekabooX",
                dry_run=True,
            ).action,
            "type-into",
        )
        self.assertEqual(
            runtime.desktop_assert("telegram", "saved-messages").action,
            "assert",
        )

    def test_agent_runtime_runs_doctor_through_observe_capability(self) -> None:
        runtime = AgentRuntime(client=FakeClient())
        expected = DoctorResult(
            status="ok",
            checks=(
                DoctorCheck(
                    name="display-server",
                    status="ok",
                    detail="WAYLAND_DISPLAY=wayland-0",
                ),
                DoctorCheck(
                    name="ocr",
                    status="warn",
                    detail="tesseract not available",
                ),
            ),
            ok_count=1,
            warn_count=1,
            fail_count=0,
            exit_code=0,
        )

        with patch("peekaboox.agent.runtime.run_doctor", return_value=expected) as run:
            result = runtime.doctor(strict=True, timeout_seconds=2.5)

        self.assertEqual(result, expected)
        run.assert_called_once_with(strict=True, timeout_seconds=2.5)
        self.assertEqual(runtime.capability_audit()[-1].capability, Capability.OBSERVE)
        self.assertEqual(runtime.capability_audit()[-1].operation, "doctor")

    def test_agent_runtime_preflight_blocks_unusable_input(self) -> None:
        fake_client = FakeClient()
        runtime = AgentRuntime(client=fake_client, preflight_mode="strict")
        doctor = DoctorResult(
            status="fail",
            checks=(
                DoctorCheck(
                    name="input-click",
                    status="fail",
                    detail="no input backend candidate detected",
                ),
            ),
            categories=(
                DoctorCategory(
                    name="input",
                    status="fail",
                    severity="error",
                    ok_count=0,
                    warn_count=0,
                    fail_count=1,
                    total_count=1,
                ),
            ),
            ok_count=0,
            warn_count=0,
            fail_count=1,
            exit_code=0,
        )

        with patch("peekaboox.agent.runtime.run_doctor", return_value=doctor) as run:
            with self.assertRaisesRegex(PreflightError, "input"):
                runtime.click(10, 20)

        run.assert_called_once_with(strict=False, timeout_seconds=30.0)
        self.assertIsNone(fake_client.clicked_at)

    def test_agent_runtime_preflight_allows_warnings_and_caches_doctor(self) -> None:
        fake_client = FakeClient()
        runtime = AgentRuntime(client=fake_client, preflight_mode="strict")
        doctor = DoctorResult(
            status="ok",
            checks=(
                DoctorCheck(
                    name="input-click",
                    status="warn",
                    detail="only fallback input backend available",
                ),
            ),
            categories=(
                DoctorCategory(
                    name="input",
                    status="warn",
                    severity="warning",
                    ok_count=0,
                    warn_count=1,
                    fail_count=0,
                    total_count=1,
                ),
            ),
            ok_count=0,
            warn_count=1,
            fail_count=0,
            exit_code=0,
        )

        with patch("peekaboox.agent.runtime.run_doctor", return_value=doctor) as run:
            runtime.move_mouse(30, 40)
            runtime.hotkey("ctrl+s")

        run.assert_called_once_with(strict=False, timeout_seconds=30.0)
        self.assertEqual(fake_client.moved_to, (30, 40))
        self.assertEqual(fake_client.hotkeys[-1], ("ctrl", "s"))

    def test_agent_runtime_preflight_keeps_diagnostics_available(self) -> None:
        runtime = AgentRuntime(client=FakeClient(), preflight_mode="strict")

        with patch("peekaboox.agent.runtime.run_doctor") as run:
            result = runtime.capture_backends(diagnose=True, probe="none")

        run.assert_not_called()
        self.assertEqual(result.image_backends[0].name, "portal")

    def test_execute_workflow_returns_preflight_failure_before_actions(self) -> None:
        fake_client = FakeClient()
        runtime = AgentRuntime(client=fake_client, preflight_mode="strict")
        workflow = Workflow(
            name="capture then click",
            steps=(
                WorkflowStep(action="capture_screen"),
                WorkflowStep(action="click", x=10, y=20),
            ),
        )
        doctor = DoctorResult(
            status="fail",
            checks=(
                DoctorCheck(
                    name="capture-file",
                    status="fail",
                    detail="no backend candidate detected",
                ),
            ),
            categories=(
                DoctorCategory(
                    name="desktop",
                    status="ok",
                    severity="info",
                    ok_count=1,
                    warn_count=0,
                    fail_count=0,
                    total_count=1,
                ),
                DoctorCategory(
                    name="capture",
                    status="fail",
                    severity="error",
                    ok_count=0,
                    warn_count=0,
                    fail_count=1,
                    total_count=1,
                ),
                DoctorCategory(
                    name="input",
                    status="ok",
                    severity="info",
                    ok_count=1,
                    warn_count=0,
                    fail_count=0,
                    total_count=1,
                ),
            ),
            ok_count=2,
            warn_count=0,
            fail_count=1,
            exit_code=0,
        )

        with patch("peekaboox.agent.runtime.run_doctor", return_value=doctor):
            result = runtime.execute_workflow(workflow)

        self.assertFalse(result.ok)
        self.assertEqual(result.steps, ())
        self.assertEqual(result.recovery["next_action"], "run_doctor")
        self.assertEqual(result.recovery["retryable"], False)
        self.assertEqual(result.recovery["preflight"]["blocked_categories"], ["capture"])
        self.assertIsNone(fake_client.clicked_at)

    def test_capability_policy_blocks_direct_runtime_actions_and_audits(self) -> None:
        policy = CapabilityPolicy.allow_only([Capability.OBSERVE])
        runtime = AgentRuntime(client=FakeClient(), capability_policy=policy)

        self.assertEqual(runtime.list_windows()[0].title, "Terminal")
        with self.assertRaises(CapabilityDeniedError):
            runtime.click(10, 20)

        audit = runtime.capability_audit()
        self.assertEqual(audit[0].capability, Capability.OBSERVE)
        self.assertTrue(audit[0].allowed)
        self.assertEqual(audit[-1].capability, Capability.CLICK)
        self.assertFalse(audit[-1].allowed)

    def test_capability_policy_blocks_memory_writes(self) -> None:
        runtime = AgentRuntime(
            client=FakeClient(),
            capability_policy=CapabilityPolicy.deny([Capability.MEMORY_WRITE]),
        )

        with self.assertRaises(CapabilityDeniedError):
            runtime.ingest_desktop_snapshot()

        audit = runtime.capability_audit()
        self.assertEqual(audit[0].operation, "ingest_desktop_snapshot")
        self.assertFalse(audit[0].allowed)

    def test_capability_policy_blocks_workflow_execution(self) -> None:
        runtime = AgentRuntime(
            client=FakeClient(),
            capability_policy=CapabilityPolicy.deny([Capability.WORKFLOW_EXECUTE]),
        )
        workflow = Workflow(name="blocked", steps=[WorkflowStep(action="observe")])

        with self.assertRaises(CapabilityDeniedError):
            runtime.execute_workflow(workflow)

        audit = runtime.capability_audit()
        self.assertEqual(audit[0].capability, Capability.WORKFLOW_EXECUTE)
        self.assertFalse(audit[0].allowed)

    def test_capability_policy_blocks_ocr_region(self) -> None:
        runtime = AgentRuntime(
            client=FakeClient(),
            capability_policy=CapabilityPolicy.deny([Capability.VISION]),
        )

        with self.assertRaises(CapabilityDeniedError):
            runtime.ocr_region(Rect(x=1, y=2, width=3, height=4))

        self.assertEqual(runtime.capability_audit()[0].capability, Capability.VISION)

    def test_capability_policy_profiles_define_reusable_allowlists(self) -> None:
        policy = CapabilityPolicy.from_profile(CapabilityProfile.OBSERVE)

        self.assertTrue(policy.allows(Capability.OBSERVE))
        self.assertTrue(policy.allows(Capability.VISION))
        self.assertTrue(policy.allows(Capability.MEMORY_READ))
        self.assertFalse(policy.allows(Capability.CLICK))
        self.assertFalse(policy.allows(Capability.TYPE_TEXT))
        self.assertFalse(policy.allows(Capability.MEMORY_WRITE))
        self.assertTrue(policy.allows(Capability.PLUGIN_READ))
        self.assertFalse(policy.allows(Capability.PLUGIN_EXECUTE))
        self.assertEqual(capability_profile("read-only").name, CapabilityProfile.OBSERVE)
        with self.assertRaises(ValueError):
            CapabilityPolicy.from_profile("unknown")

    def test_plugin_discovery_loads_manifest_tools(self) -> None:
        with TemporaryDirectory() as tmpdir:
            plugin_dir = Path(tmpdir) / "plugins" / "demo"
            plugin_dir.mkdir(parents=True)
            (plugin_dir / PLUGIN_MANIFEST_FILE).write_text(
                json.dumps(
                    {
                        "schema_version": PLUGIN_SDK_VERSION,
                        "id": "demo.plugin",
                        "name": "Demo Plugin",
                        "version": "1.0.0",
                        "capabilities": ["observe"],
                        "entrypoint": {
                            "kind": "process",
                            "command": ["python3", "plugin.py"],
                        },
                        "tools": [
                            {
                                "name": "demo.inspect",
                                "description": "Inspect demo state",
                                "capabilities": ["observe"],
                                "input_schema": {"type": "object"},
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )

            result = discover_plugins([Path(tmpdir) / "plugins"])

        self.assertEqual(result.errors, ())
        self.assertEqual(result.plugins[0].manifest.id, "demo.plugin")
        self.assertEqual(result.plugins[0].manifest.tools[0].name, "demo.inspect")

    def test_agent_runtime_lists_plugins_with_capability_gate(self) -> None:
        with TemporaryDirectory() as tmpdir:
            plugin_dir = Path(tmpdir) / "demo"
            plugin_dir.mkdir()
            (plugin_dir / PLUGIN_MANIFEST_FILE).write_text(
                json.dumps(
                    {
                        "schema_version": PLUGIN_SDK_VERSION,
                        "id": "runtime.demo",
                        "name": "Runtime Demo",
                        "version": "1.0.0",
                    }
                ),
                encoding="utf-8",
            )
            runtime = AgentRuntime(client=FakeClient(), plugin_paths=(Path(tmpdir),))

            result = runtime.list_plugins()

        self.assertEqual(result.plugins[0].manifest.id, "runtime.demo")
        self.assertEqual(runtime.capability_audit()[0].capability, Capability.PLUGIN_READ)

    def test_agent_runtime_executes_process_plugin_tool(self) -> None:
        with TemporaryDirectory() as tmpdir:
            plugin_dir = Path(tmpdir) / "demo"
            plugin_dir.mkdir()
            script = plugin_dir / "plugin.py"
            script.write_text(
                "import json, sys\n"
                "request = json.load(sys.stdin)\n"
                "json.dump({'ok': True, 'result': {'tool': request['tool'], 'answer': 42}}, sys.stdout)\n",
                encoding="utf-8",
            )
            (plugin_dir / PLUGIN_MANIFEST_FILE).write_text(
                json.dumps(
                    {
                        "schema_version": PLUGIN_SDK_VERSION,
                        "id": "exec.demo",
                        "name": "Exec Demo",
                        "version": "1.0.0",
                        "entrypoint": {
                            "kind": "process",
                            "command": [sys.executable, "plugin.py"],
                        },
                        "tools": [
                            {
                                "name": "exec.answer",
                                "description": "Return a test answer",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )
            runtime = AgentRuntime(client=FakeClient(), plugin_paths=(Path(tmpdir),))

            result = runtime.call_plugin_tool("exec.demo", "exec.answer")

        self.assertTrue(result.ok)
        self.assertEqual(result.result["answer"], 42)
        self.assertEqual(runtime.capability_audit()[0].capability, Capability.PLUGIN_EXECUTE)

    def test_process_plugin_tool_validates_input_schema(self) -> None:
        with TemporaryDirectory() as tmpdir:
            plugin_dir = Path(tmpdir) / "demo"
            plugin_dir.mkdir()
            (plugin_dir / "plugin.py").write_text(
                "import json, sys\njson.dump({'ok': True}, sys.stdout)\n",
                encoding="utf-8",
            )
            (plugin_dir / PLUGIN_MANIFEST_FILE).write_text(
                json.dumps(
                    {
                        "schema_version": PLUGIN_SDK_VERSION,
                        "id": "schema.demo",
                        "name": "Schema Demo",
                        "version": "1.0.0",
                        "entrypoint": {
                            "kind": "process",
                            "command": [sys.executable, "plugin.py"],
                        },
                        "tools": [
                            {
                                "name": "schema.echo",
                                "description": "Echo",
                                "input_schema": {
                                    "type": "object",
                                    "required": ["text"],
                                    "properties": {"text": {"type": "string"}},
                                    "additionalProperties": False,
                                },
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )
            plugin = discover_plugins([Path(tmpdir)]).plugins[0]

            with self.assertRaisesRegex(ValueError, "required field"):
                execute_plugin_tool(plugin, "schema.echo", {})

            with self.assertRaisesRegex(ValueError, "additional property"):
                execute_plugin_tool(plugin, "schema.echo", {"text": "ok", "extra": True})

    def test_capability_policy_can_load_profile_from_env(self) -> None:
        with patch.dict("os.environ", {"PEEKABOOX_CAPABILITY_PROFILE": "plan"}):
            policy = CapabilityPolicy.from_env()

        self.assertTrue(policy.allows(Capability.WORKFLOW_GENERATE))
        self.assertTrue(policy.allows(Capability.MEMORY_WRITE))
        self.assertFalse(policy.allows(Capability.CLICK))

    @unittest.skipUnless(_protobuf_available(), "protobuf runtime dependencies are not installed")
    def test_agent_runtime_connect_accepts_capability_policy(self) -> None:
        runtime = AgentRuntime.connect(
            capability_policy=CapabilityPolicy.deny([Capability.CLICK])
        )

        with self.assertRaises(CapabilityDeniedError):
            runtime.click(10, 20)

        self.assertEqual(runtime.capability_audit()[0].capability, Capability.CLICK)

    @unittest.skipUnless(_protobuf_available(), "protobuf runtime dependencies are not installed")
    def test_agent_runtime_connect_accepts_capability_profile(self) -> None:
        runtime = AgentRuntime.connect(capability_profile=CapabilityProfile.OBSERVE)

        with self.assertRaises(CapabilityDeniedError):
            runtime.click(10, 20)

        self.assertEqual(runtime.capability_audit()[0].capability, Capability.CLICK)
        self.assertFalse(runtime.capability_audit()[0].allowed)

    def test_agent_cli_prints_help_without_command(self) -> None:
        output = StringIO()
        with patch("sys.stdout", output):
            exit_code = agent_runtime_module.main([])

        self.assertEqual(exit_code, 0)
        self.assertIn("peekaboox-agent", output.getvalue())
        self.assertNotIn("scaffold", output.getvalue())

    def test_agent_cli_lists_plugins_as_json(self) -> None:
        output = StringIO()
        with patch("sys.stdout", output):
            exit_code = agent_runtime_module.main(
                ["plugins", "--path", "examples/plugins"]
            )

        self.assertEqual(exit_code, 0)
        payload = json.loads(output.getvalue())
        self.assertEqual(payload["sdk_version"], PLUGIN_SDK_VERSION)
        self.assertEqual(payload["plugins"][0]["manifest"]["id"], "org.peekaboox.examples.system-info")

    def test_agent_cli_lists_filtered_windows_as_json(self) -> None:
        fake_client = FakeClient()
        output = StringIO()
        with (
            patch("sys.stdout", output),
            patch(
                "peekaboox.agent.runtime.AgentRuntime.connect",
                return_value=AgentRuntime(client=fake_client),
            ),
        ):
            exit_code = agent_runtime_module.main(
                [
                    "windows",
                    "--app",
                    "Terminal",
                    "--focused",
                    "--limit",
                    "1",
                    "--sort",
                    "focused",
                    "--backend",
                    "at-spi",
                ]
            )

        payload = json.loads(output.getvalue())
        self.assertEqual(exit_code, 0)
        self.assertEqual(payload[0]["title"], "Terminal")
        self.assertIsNotNone(fake_client.last_window_query)
        self.assertEqual(fake_client.last_window_query["app"], "Terminal")
        self.assertTrue(fake_client.last_window_query["focused"])
        self.assertEqual(fake_client.last_window_query["limit"], 1)
        self.assertEqual(fake_client.last_window_query["sort"], "focused")
        self.assertEqual(fake_client.last_window_query["backend"], "at-spi")

    def test_agent_cli_passes_preflight_options_to_runtime(self) -> None:
        fake_client = FakeClient()
        output = StringIO()
        with (
            patch("sys.stdout", output),
            patch(
                "peekaboox.agent.runtime.AgentRuntime.connect",
                return_value=AgentRuntime(client=fake_client),
            ) as connect,
        ):
            exit_code = agent_runtime_module.main(
                [
                    "--preflight-mode",
                    "strict",
                    "--preflight-timeout",
                    "2.5",
                    "windows",
                    "--diagnose",
                ]
            )

        self.assertEqual(exit_code, 0)
        connect.assert_called_once()
        self.assertEqual(connect.call_args.kwargs["preflight_mode"], "strict")
        self.assertEqual(connect.call_args.kwargs["preflight_timeout_seconds"], 2.5)

    def test_agent_cli_preflight_prints_json_result(self) -> None:
        output = StringIO()
        doctor = DoctorResult(
            status="ok",
            checks=(
                DoctorCheck(
                    name="capture-frame",
                    status="warn",
                    detail="no direct backend candidate detected",
                ),
            ),
            categories=(
                DoctorCategory(
                    name="desktop",
                    status="ok",
                    severity="info",
                    ok_count=1,
                    warn_count=0,
                    fail_count=0,
                    total_count=1,
                ),
                DoctorCategory(
                    name="capture",
                    status="warn",
                    severity="warning",
                    ok_count=0,
                    warn_count=1,
                    fail_count=0,
                    total_count=1,
                ),
            ),
            ok_count=1,
            warn_count=1,
            fail_count=0,
            exit_code=0,
        )

        with (
            patch("sys.stdout", output),
            patch("peekaboox.agent.runtime.run_doctor", return_value=doctor) as run,
        ):
            exit_code = agent_runtime_module.main(
                [
                    "--preflight-mode",
                    "strict",
                    "--preflight-timeout",
                    "2.5",
                    "preflight",
                    "desktop",
                    "capture",
                    "--operation",
                    "capture_screen",
                    "--timeout",
                    "1.5",
                ]
            )

        payload = json.loads(output.getvalue())
        self.assertEqual(exit_code, 0)
        run.assert_called_once_with(strict=False, timeout_seconds=1.5)
        self.assertTrue(payload["ok"])
        self.assertEqual(payload["required_categories"], ["desktop", "capture"])
        self.assertEqual(payload["warning_categories"], ["capture"])
        self.assertEqual(payload["operation"], "capture_screen")

    def test_agent_cli_preflight_require_returns_failure_for_blocked_category(self) -> None:
        output = StringIO()
        doctor = DoctorResult(
            status="fail",
            checks=(
                DoctorCheck(
                    name="display-server",
                    status="fail",
                    detail="neither WAYLAND_DISPLAY nor DISPLAY is set",
                ),
            ),
            categories=(
                DoctorCategory(
                    name="desktop",
                    status="fail",
                    severity="error",
                    ok_count=0,
                    warn_count=0,
                    fail_count=1,
                    total_count=1,
                ),
            ),
            ok_count=0,
            warn_count=0,
            fail_count=1,
            exit_code=0,
        )

        with (
            patch("sys.stdout", output),
            patch("peekaboox.agent.runtime.run_doctor", return_value=doctor),
        ):
            exit_code = agent_runtime_module.main(
                ["preflight", "desktop", "--operation", "list_windows", "--require"]
            )

        payload = json.loads(output.getvalue())
        self.assertEqual(exit_code, 1)
        self.assertFalse(payload["ok"])
        self.assertEqual(payload["blocked_categories"], ["desktop"])

    def test_agent_cli_windows_diagnose_prints_metadata(self) -> None:
        fake_client = FakeClient()
        output = StringIO()
        with (
            patch("sys.stdout", output),
            patch(
                "peekaboox.agent.runtime.AgentRuntime.connect",
                return_value=AgentRuntime(client=fake_client),
            ),
        ):
            exit_code = agent_runtime_module.main(
                ["windows", "--title-regex", "Term.*", "--diagnose"]
            )

        payload = json.loads(output.getvalue())
        self.assertEqual(exit_code, 0)
        self.assertEqual(payload["backend_name"], "fake")
        self.assertTrue(payload["backend_reports"][0]["selected"])
        self.assertIsNotNone(fake_client.last_window_result_query)
        self.assertEqual(fake_client.last_window_result_query["title_regex"], "Term.*")
        self.assertTrue(fake_client.last_window_result_query["diagnose"])

    @unittest.skipUnless(_protobuf_available(), "protobuf runtime dependencies are not installed")
    def test_agent_runtime_connect_rejects_policy_and_profile_together(self) -> None:
        with self.assertRaises(ValueError):
            AgentRuntime.connect(
                capability_policy=CapabilityPolicy.allow_all(),
                capability_profile=CapabilityProfile.OBSERVE,
            )

    def test_confirmation_policy_blocks_dangerous_actions_without_confirmer(self) -> None:
        runtime = AgentRuntime(
            client=FakeClient(),
            confirmation_policy=ConfirmationPolicy.require_for([DangerousAction.CLICK]),
        )

        with self.assertRaises(ConfirmationRequiredError):
            runtime.click(10, 20)

        audit = runtime.confirmation_audit()
        self.assertEqual(audit[0].action, DangerousAction.CLICK)
        self.assertEqual(audit[0].operation, "click")
        self.assertFalse(audit[0].confirmed)

    def test_confirmation_policy_allows_confirmed_dangerous_actions(self) -> None:
        fake_client = FakeClient()
        runtime = AgentRuntime(
            client=fake_client,
            confirmation_policy=ConfirmationPolicy.require_for(
                [DangerousAction.CLICK],
                confirmer=lambda request: request.metadata["x"] == 10,
            ),
        )

        result = runtime.click(10, 20)

        self.assertTrue(result.ok)
        self.assertEqual(fake_client.clicked_at, (10, 20))
        self.assertTrue(runtime.confirmation_audit()[0].confirmed)

    def test_confirmation_policy_denies_workflow_execution_before_steps(self) -> None:
        fake_client = FakeClient()
        runtime = AgentRuntime(
            client=fake_client,
            confirmation_policy=ConfirmationPolicy.require_for(
                [DangerousAction.WORKFLOW_EXECUTE],
                confirmer=lambda _request: False,
            ),
        )
        workflow = Workflow(name="blocked", steps=[WorkflowStep(action="click", x=10, y=20)])

        with self.assertRaises(ConfirmationDeniedError):
            runtime.execute_workflow(workflow)

        self.assertIsNone(fake_client.clicked_at)
        audit = runtime.confirmation_audit()
        self.assertEqual(audit[0].action, DangerousAction.WORKFLOW_EXECUTE)
        self.assertEqual(audit[0].metadata["workflow"], "blocked")
        self.assertFalse(audit[0].confirmed)

    def test_runtime_persists_capability_and_confirmation_audit_events(self) -> None:
        with TemporaryDirectory() as tmpdir:
            audit_path = Path(tmpdir) / "runtime-audit.jsonl"
            fake_client = FakeClient()
            runtime = AgentRuntime(
                client=fake_client,
                audit_logger=JsonlAuditLogger(audit_path),
                confirmation_policy=ConfirmationPolicy.require_for(
                    [DangerousAction.CLICK],
                    confirmer=lambda _request: True,
                ),
            )

            runtime.list_windows()
            runtime.click(10, 20)

            records = [
                json.loads(line)
                for line in audit_path.read_text(encoding="utf-8").splitlines()
            ]

        self.assertEqual(records[0]["event"], "capability")
        self.assertEqual(records[0]["status"], "ok")
        self.assertEqual(records[0]["details"]["capability"], Capability.OBSERVE)
        self.assertTrue(
            any(
                record["event"] == "confirmation"
                and record["status"] == "confirmed"
                and record["details"]["action"] == DangerousAction.CLICK
                for record in records
            )
        )

    def test_runtime_persists_preflight_audit_events(self) -> None:
        warning_doctor = DoctorResult(
            status="ok",
            checks=(
                DoctorCheck(
                    name="input-click",
                    status="warn",
                    detail="only fallback input backend available",
                ),
            ),
            categories=(
                DoctorCategory(
                    name="input",
                    status="warn",
                    severity="warning",
                    ok_count=0,
                    warn_count=1,
                    fail_count=0,
                    total_count=1,
                ),
            ),
            ok_count=0,
            warn_count=1,
            fail_count=0,
            exit_code=0,
        )
        blocked_doctor = DoctorResult(
            status="fail",
            checks=(
                DoctorCheck(
                    name="display-server",
                    status="fail",
                    detail="neither WAYLAND_DISPLAY nor DISPLAY is set",
                ),
            ),
            categories=(
                DoctorCategory(
                    name="desktop",
                    status="fail",
                    severity="error",
                    ok_count=0,
                    warn_count=0,
                    fail_count=1,
                    total_count=1,
                ),
            ),
            ok_count=0,
            warn_count=0,
            fail_count=1,
            exit_code=0,
        )

        with TemporaryDirectory() as tmpdir:
            audit_path = Path(tmpdir) / "runtime-audit.jsonl"
            runtime = AgentRuntime(
                audit_logger=JsonlAuditLogger(audit_path),
                preflight_mode="strict",
            )
            with patch(
                "peekaboox.agent.runtime.run_doctor",
                side_effect=(warning_doctor, blocked_doctor),
            ):
                runtime.preflight("input", operation="hotkey")
                with self.assertRaises(PreflightError):
                    runtime.require_preflight("desktop", operation="list_windows", refresh=True)

            audit_events = runtime.preflight_audit()
            records = [
                json.loads(line)
                for line in audit_path.read_text(encoding="utf-8").splitlines()
            ]

        self.assertEqual([event.status for event in audit_events], ["warning", "blocked"])
        self.assertEqual(audit_events[0].operation, "hotkey")
        self.assertEqual(audit_events[0].warning_categories, ("input",))
        self.assertEqual(audit_events[1].blocked_categories, ("desktop",))
        preflight_records = [record for record in records if record["event"] == "preflight"]
        self.assertEqual(
            [record["status"] for record in preflight_records],
            ["warning", "blocked"],
        )
        self.assertEqual(preflight_records[0]["details"]["operation"], "hotkey")
        self.assertEqual(preflight_records[0]["details"]["mode"], "strict")
        self.assertEqual(preflight_records[0]["details"]["warning_categories"], ["input"])
        self.assertEqual(preflight_records[1]["details"]["blocked_categories"], ["desktop"])
        self.assertIn("preflight blocked list_windows", preflight_records[1]["error"])

    def test_semantic_desktop_graph_ingests_and_serializes_state(self) -> None:
        graph = SemanticDesktopGraph()

        snapshot = graph.ingest_desktop_state(
            FakeClient().get_desktop_state(),
            snapshot_id="snapshot:test",
            captured_at_unix_ms=123,
        )

        nodes_by_id = {node.id: node for node in snapshot.nodes}
        self.assertEqual(snapshot.active_window_id, "window:window-1")
        self.assertEqual(nodes_by_id["window:window-1"].label, "Terminal")
        self.assertEqual(nodes_by_id["element:button-1"].role, "push button")
        self.assertEqual(
            graph.find_nodes(kind="element", label_contains="submit")[0].id,
            "element:button-1",
        )
        self.assertTrue(
            any(
                edge.kind == "contains"
                and edge.source == "window:window-1"
                and edge.target == "element:button-1"
                for edge in snapshot.edges
            )
        )

        restored = SemanticDesktopGraph.from_json(graph.to_json())

        self.assertEqual(restored.latest_snapshot().id, "snapshot:test")
        self.assertEqual(
            restored.find_nodes(kind="window", label_contains="terminal")[0].bounds.width,
            800,
        )

    def test_semantic_desktop_graph_queries_nodes_and_edges(self) -> None:
        graph = SemanticDesktopGraph()
        graph.ingest_desktop_state(
            FakeClient().get_desktop_state(),
            snapshot_id="snapshot:query",
            captured_at_unix_ms=234,
        )

        elements = graph.find_nodes(
            kind="element",
            attribute_equals={"element_id": "button-1"},
            contained_by="window-1",
        )
        contains_edges = graph.query_edges(
            source="window:window-1",
            target="element:button-1",
            kind="contains",
        )

        self.assertEqual(elements[0].label, "Submit")
        self.assertEqual(graph.node_by_id("element:button-1").role, "push button")
        self.assertEqual(len(contains_edges), 1)

    def test_memory_store_exports_and_imports_desktop_graph(self) -> None:
        store = MemoryStore()
        store.put("last_goal", "submit")

        store.ingest_desktop_state(
            FakeClient().get_desktop_state(),
            snapshot_id="snapshot:memory",
            captured_at_unix_ms=456,
        )
        payload = store.export_desktop_graph()
        restored = MemoryStore()
        restored.import_desktop_graph(payload)

        self.assertEqual(store.get("last_goal"), "submit")
        self.assertEqual(restored.latest_desktop_snapshot().id, "snapshot:memory")

    def test_memory_store_records_events_and_invalidates_desktop_graph(self) -> None:
        store = MemoryStore()
        store.ingest_desktop_state(
            FakeClient().get_desktop_state(),
            snapshot_id="snapshot:event",
            captured_at_unix_ms=456,
        )

        update = store.record_desktop_event(
            kind="window.focused",
            source="accessibility",
            target_id="window-1",
            occurred_at_unix_ms=457,
        )
        status = store.desktop_graph_status()

        self.assertTrue(update.stale)
        self.assertTrue(status.stale)
        self.assertEqual(status.latest_snapshot_id, "snapshot:event")
        self.assertEqual(status.event_count, 1)
        self.assertEqual(status.invalidation_count, 1)
        self.assertEqual(update.invalidation.invalidated_snapshot_id, "snapshot:event")
        self.assertIn("window:window-1", update.invalidation.affected_node_ids)
        self.assertIn("element:button-1", update.invalidation.affected_node_ids)

    def test_memory_store_event_with_state_refreshes_desktop_graph(self) -> None:
        store = MemoryStore()

        update = store.record_desktop_event(
            kind="capture.updated",
            source="capture",
            state=FakeClient().get_desktop_state(),
            snapshot_id="snapshot:event-refresh",
            occurred_at_unix_ms=458,
        )

        self.assertFalse(update.stale)
        self.assertEqual(update.snapshot.id, "snapshot:event-refresh")
        self.assertFalse(store.desktop_graph_status().stale)

    def test_memory_store_finds_cached_elements_by_semantic_selector(self) -> None:
        store = MemoryStore()
        store.ingest_desktop_state(
            FakeClient().get_desktop_state(),
            snapshot_id="snapshot:cache",
            captured_at_unix_ms=459,
        )

        role_label = store.find_cached_elements("role=push button,label=submit")
        state_bounds = store.find_cached_elements(
            "state=enabled,bounds=10,20,90,30,confidence>=0.9"
        )
        point = store.find_cached_elements("contains=55,35")

        self.assertEqual(role_label[0].id, "button-1")
        self.assertEqual(state_bounds[0].label, "Submit")
        self.assertEqual(point[0].role, "push button")

    def test_sqlite_memory_store_persists_values_and_desktop_graph(self) -> None:
        with TemporaryDirectory() as tmpdir:
            database_path = Path(tmpdir) / "memory.sqlite3"
            store = SQLiteMemoryStore(database_path)
            store.put("last_goal", "submit")
            store.ingest_desktop_state(
                FakeClient().get_desktop_state(),
                snapshot_id="snapshot:sqlite",
                captured_at_unix_ms=567,
            )
            store.close()

            restored = SQLiteMemoryStore(database_path)

            self.assertEqual(restored.get("last_goal"), "submit")
            self.assertEqual(restored.latest_desktop_snapshot().id, "snapshot:sqlite")
            self.assertEqual(
                restored.query_desktop_nodes(kind="element", contained_by="window-1")[0].label,
                "Submit",
            )
            restored.close()

    def test_sqlite_memory_store_persists_desktop_events_and_invalidations(self) -> None:
        with TemporaryDirectory() as tmpdir:
            database_path = Path(tmpdir) / "memory.sqlite3"
            store = SQLiteMemoryStore(database_path)
            store.ingest_desktop_state(
                FakeClient().get_desktop_state(),
                snapshot_id="snapshot:sqlite-event",
                captured_at_unix_ms=568,
            )
            store.record_desktop_event(
                kind="accessibility.element.changed",
                source="accessibility",
                target_id="button-1",
                occurred_at_unix_ms=569,
            )
            store.close()

            restored = SQLiteMemoryStore(database_path)
            status = restored.desktop_graph_status()

            self.assertTrue(status.stale)
            self.assertEqual(status.event_count, 1)
            self.assertEqual(status.invalidation_count, 1)
            self.assertEqual(status.last_event.kind, "accessibility.element.changed")
            self.assertEqual(
                status.last_invalidation.affected_node_ids,
                ("element:button-1",),
            )
            restored.close()

    def test_agent_runtime_ingests_desktop_snapshots(self) -> None:
        runtime = AgentRuntime(client=FakeClient())

        snapshot = runtime.ingest_desktop_snapshot(
            snapshot_id="snapshot:runtime",
            captured_at_unix_ms=789,
        )

        self.assertEqual(snapshot.active_window_id, "window:window-1")
        self.assertEqual(runtime.latest_desktop_snapshot().id, "snapshot:runtime")

    def test_agent_runtime_records_verification_snapshots(self) -> None:
        runtime = AgentRuntime(client=FakeClient())

        result = runtime.execute_step(
            WorkflowStep(action="click", selector="role=push button,label=Submit")
        )

        self.assertTrue(result.ok)
        self.assertEqual(runtime.latest_desktop_snapshot().active_window_id, "window:window-1")

    def test_agent_runtime_refreshes_stale_desktop_graph_before_query(self) -> None:
        runtime = AgentRuntime(client=FakeClient())
        runtime.ingest_desktop_snapshot(snapshot_id="snapshot:stale")
        runtime.record_desktop_event(
            kind="accessibility.element.changed",
            source="accessibility",
            target_id="button-1",
        )

        nodes = runtime.query_desktop_graph(
            kind="element",
            label_contains="submit",
            refresh_if_stale=True,
        )

        self.assertEqual(nodes[0].id, "element:button-1")
        self.assertFalse(runtime.desktop_graph_status().stale)

    def test_agent_runtime_uses_fresh_graph_for_find_element(self) -> None:
        fake_client = FakeClient()
        runtime = AgentRuntime(client=fake_client)
        runtime.ingest_desktop_snapshot(snapshot_id="snapshot:find-cache")
        fake_client.last_find_selector = None

        elements = runtime.find_element("role=push button,label=submit")

        self.assertEqual(elements[0].id, "button-1")
        self.assertIsNone(fake_client.last_find_selector)

    def test_agent_runtime_falls_back_to_daemon_on_cached_selector_miss(self) -> None:
        fake_client = FakeClient()
        runtime = AgentRuntime(client=fake_client)
        runtime.ingest_desktop_snapshot(snapshot_id="snapshot:find-miss")

        elements = runtime.find_element("label=Cancel")

        self.assertEqual(elements[0].label, "Submit")
        self.assertEqual(fake_client.last_find_selector, "label=Cancel")

    def test_agent_runtime_skips_find_cache_when_graph_is_stale(self) -> None:
        fake_client = FakeClient()
        runtime = AgentRuntime(client=fake_client)
        runtime.ingest_desktop_snapshot(snapshot_id="snapshot:find-stale")
        runtime.record_desktop_event(kind="window.focused", target_id="window-1")

        elements = runtime.find_element("role=push button,label=submit")

        self.assertEqual(elements[0].id, "button-1")
        self.assertEqual(fake_client.last_find_selector, "role=push button,label=submit")

    def test_agent_runtime_uses_fresh_graph_for_semantic_click_center(self) -> None:
        fake_client = FakeClient()
        runtime = AgentRuntime(client=fake_client)
        runtime.ingest_desktop_snapshot(snapshot_id="snapshot:click-cache")

        result = runtime.click_selector("role=push button,label=submit")

        self.assertTrue(result.ok)
        self.assertEqual(fake_client.clicked_at, (55, 35))

    def test_agent_runtime_records_coordinate_clicks_as_semantic_selectors(self) -> None:
        fake_client = FakeClient()
        runtime = AgentRuntime(client=fake_client)

        runtime.start_recording("semantic-click")
        runtime.click(55, 35)
        workflow = runtime.stop_recording()

        recorded_step = workflow.steps[0]
        self.assertEqual(recorded_step.selector, "role=push button,label=Submit")
        self.assertIsNone(recorded_step.x)
        self.assertIsNone(recorded_step.y)
        self.assertEqual(fake_client.clicked_at, (55, 35))

        moved_client = MovedSubmitClient()
        replay_runtime = AgentRuntime(client=moved_client)
        replay_runtime.ingest_desktop_snapshot(snapshot_id="snapshot:moved")
        replay_result = replay_runtime.execute_workflow(workflow)

        self.assertTrue(replay_result.ok)
        self.assertEqual(moved_client.clicked_at, (145, 215))

    def test_agent_runtime_executes_workflow_with_verification(self) -> None:
        fake_client = FakeClient()
        runtime = AgentRuntime(client=fake_client)
        workflow = Workflow(
            name="submit",
            steps=[
                WorkflowStep(action="find_element", selector="role=push button,label=Submit"),
                WorkflowStep(
                    action="click",
                    selector="role=push button,label=Submit",
                    vision_fallback=True,
                ),
                WorkflowStep(action="type_text", value="Hello"),
            ],
        )

        result = runtime.execute_workflow(workflow)

        self.assertTrue(result.ok)
        self.assertEqual(len(result.steps), 3)
        self.assertEqual(result.steps[0].attempts[0].verification.message, "result accepted")
        self.assertEqual(
            result.steps[1].attempts[0].verification.metadata["has_active_window"],
            True,
        )
        self.assertEqual(fake_client.typed_text, "Hello")
        self.assertTrue(fake_client.last_vision_fallback)

    def test_agent_runtime_executes_pointer_and_hotkey_workflow_steps(self) -> None:
        fake_client = FakeClient()
        runtime = AgentRuntime(client=fake_client)
        workflow = Workflow(
            name="input",
            steps=[
                WorkflowStep(action="move_mouse", x=10, y=20, verify=False),
                WorkflowStep(
                    action="drag",
                    from_x=10,
                    from_y=20,
                    to_x=30,
                    to_y=40,
                    button="right",
                    duration_ms=100,
                    verify=False,
                ),
                WorkflowStep(action="hotkey", value="ctrl+s", verify=False),
            ],
        )

        result = runtime.execute_workflow(workflow)

        self.assertTrue(result.ok)
        self.assertEqual(fake_client.moved_to, (10, 20))
        self.assertEqual(fake_client.dragged, (10, 20, 30, 40, "right", 100))
        self.assertEqual(fake_client.hotkeys[-1], ("ctrl", "s"))

    def test_agent_runtime_retries_failed_actions_and_records_attempts(self) -> None:
        fake_client = FlakyActionClient(failures_before_success=1)
        runtime = AgentRuntime(client=fake_client, retries=2)
        step = WorkflowStep(action="click", selector="role=push button,label=Submit")

        result = runtime.execute_step(step)

        self.assertTrue(result.ok)
        self.assertEqual(len(result.attempts), 2)
        self.assertFalse(result.attempts[0].ok)
        self.assertEqual(result.attempts[0].message, "target not ready")
        self.assertTrue(result.attempts[1].ok)
        self.assertEqual(fake_client.click_calls, 2)

    def test_agent_runtime_refreshes_graph_during_selector_replay_recovery(self) -> None:
        fake_client = SemanticClickMissClient()
        runtime = AgentRuntime(client=fake_client, retries=1)

        result = runtime.execute_step(
            WorkflowStep(action="click", selector="role=push button,label=Submit")
        )

        self.assertTrue(result.ok)
        self.assertEqual(len(result.attempts), 2)
        self.assertEqual(fake_client.semantic_click_calls, 1)
        self.assertEqual(fake_client.clicked_at, (55, 35))
        self.assertEqual(
            result.attempts[1].recovery["strategy"],
            "refresh_desktop_graph",
        )
        self.assertEqual(result.recovery["strategy"], "refresh_desktop_graph")

    def test_agent_runtime_enables_vision_fallback_during_selector_replay_recovery(
        self,
    ) -> None:
        fake_client = VisionFallbackFindClient()
        runtime = AgentRuntime(client=fake_client, retries=2)

        result = runtime.execute_step(
            WorkflowStep(action="find_element", selector="label=Submit")
        )

        self.assertTrue(result.ok)
        self.assertEqual(len(result.attempts), 3)
        self.assertEqual(result.attempts[1].recovery["strategy"], "refresh_desktop_graph")
        self.assertEqual(result.attempts[2].recovery["strategy"], "vision_fallback")
        self.assertEqual(result.recovery["strategy"], "vision_fallback")
        self.assertEqual(
            result.recovery["strategies"],
            ["refresh_desktop_graph", "vision_fallback"],
        )
        self.assertTrue(fake_client.last_vision_fallback)

    def test_agent_runtime_returns_recovery_metadata_after_exhausting_retries(self) -> None:
        runtime = AgentRuntime(client=FlakyActionClient(failures_before_success=4), retries=1)
        workflow = Workflow(
            name="submit",
            steps=[WorkflowStep(action="click", selector="role=push button,label=Submit")],
        )

        result = runtime.execute_workflow(workflow)

        self.assertFalse(result.ok)
        self.assertEqual(result.recovery["failed_step"], 0)
        self.assertEqual(result.recovery["action"], "click")
        self.assertEqual(result.recovery["attempts"], 2)
        self.assertEqual(result.recovery["next_action"], "inspect_state")

    def test_agent_runtime_uses_custom_verifier(self) -> None:
        runtime = AgentRuntime(client=FakeClient(), retries=1)
        calls = 0

        def verifier(step: WorkflowStep, result: object) -> VerificationResult:
            nonlocal calls
            calls += 1
            return VerificationResult(
                ok=calls == 2,
                message="eventually verified" if calls == 2 else "not settled",
                metadata={"action": step.action},
            )

        result = runtime.execute_step(
            WorkflowStep(action="click", selector="role=push button"),
            verifier=verifier,
        )

        self.assertTrue(result.ok)
        self.assertEqual(len(result.attempts), 2)
        self.assertEqual(result.attempts[0].verification.message, "not settled")
        self.assertEqual(result.attempts[1].verification.metadata["action"], "click")

    def test_agent_runtime_execute_goal_uses_planner_observe_step(self) -> None:
        runtime = AgentRuntime(client=FakeClient())

        result = runtime.execute_goal("Inspect desktop")

        self.assertTrue(result.ok)
        self.assertEqual(result.goal, "Inspect desktop")
        self.assertEqual(result.steps[0].step.action, "observe")
        self.assertEqual(result.steps[0].result.mime_type, "image/png")

    def test_agent_runtime_replans_failed_goal_with_provider(self) -> None:
        class FailingFirstPlanner(PlanningEngine):
            def plan_workflow(self, goal: str) -> Workflow:
                return Workflow(name=goal, steps=[WorkflowStep(action="click")])

        def replanner(request: WorkflowReplanningRequest) -> Workflow:
            self.assertEqual(request.failed_step_index, 0)
            self.assertIn("click step requires", request.reason)
            return Workflow(name=request.goal, steps=[WorkflowStep(action="observe")])

        runtime = AgentRuntime(
            client=FakeClient(),
            planner=FailingFirstPlanner(workflow_replanner=replanner),
        )

        result = runtime.execute_goal("Recover desktop", max_replans=1)

        self.assertTrue(result.ok)
        self.assertTrue(result.recovery["replanned"])
        self.assertEqual([step.step.action for step in result.steps], ["click", "observe"])

    def test_agent_runtime_generates_editable_workflow_from_goal_and_graph(self) -> None:
        runtime = AgentRuntime(client=FakeClient())
        runtime.ingest_desktop_snapshot(snapshot_id="snapshot:generate")

        workflow = runtime.generate_workflow("Click Submit and type 'Hello'")
        json_roundtrip = load_workflow_text(
            dump_workflow_text(workflow, format_name="json"),
            format_name="json",
        )
        yaml_roundtrip = load_workflow_text(
            dump_workflow_text(workflow, format_name="yaml"),
            format_name="yaml",
        )

        self.assertEqual(
            [step.action for step in workflow.steps],
            ["observe", "find_element", "click", "type_text"],
        )
        self.assertEqual(workflow.steps[1].selector, "role=push button,label=Submit")
        self.assertEqual(workflow.steps[2].selector, "role=push button,label=Submit")
        self.assertTrue(workflow.steps[2].vision_fallback)
        self.assertEqual(workflow.steps[3].value, "Hello")
        self.assertEqual(json_roundtrip.steps[2].selector, "role=push button,label=Submit")
        self.assertEqual(yaml_roundtrip.steps[3].value, "Hello")

    def test_agent_runtime_refines_workflow_with_structured_provider(self) -> None:
        def refiner(request: WorkflowRefinementRequest) -> dict[str, object]:
            self.assertEqual(request.draft.steps[1].selector, "role=push button,label=Submit")
            return {
                "name": request.goal,
                "steps": [
                    {"action": "observe", "value": request.goal},
                    {
                        "action": "find_element",
                        "selector": "role=push button,label=Submit",
                    },
                    {
                        "action": "click",
                        "selector": "role=push button,label=Submit",
                        "vision_fallback": True,
                    },
                    {"action": "type_text", "value": "Refined", "verify": False},
                ],
            }

        runtime = AgentRuntime(
            client=FakeClient(),
            planner=PlanningEngine(workflow_refiner=refiner),
        )
        runtime.ingest_desktop_snapshot(snapshot_id="snapshot:refine")

        workflow = runtime.refine_workflow("Click Submit")

        self.assertEqual(workflow.steps[3].value, "Refined")
        self.assertFalse(workflow.steps[3].verify)

        with TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "refined.yaml"
            saved = runtime.save_refined_workflow("Click Submit", path)
            loaded = load_workflow_file(saved)

        self.assertEqual(loaded.steps[2].selector, "role=push button,label=Submit")
        self.assertEqual(loaded.steps[3].value, "Refined")

    def test_agent_runtime_rejects_unstructured_provider_workflow(self) -> None:
        def refiner(_request: WorkflowRefinementRequest) -> dict[str, object]:
            return {
                "name": "unsafe",
                "steps": [{"action": "shell", "value": "rm -rf /tmp/nope"}],
            }

        runtime = AgentRuntime(planner=PlanningEngine(workflow_refiner=refiner))

        with self.assertRaisesRegex(ValueError, "unsupported"):
            runtime.refine_workflow("Run shell")

    def test_workflow_loader_reads_json_and_yaml_definitions(self) -> None:
        json_workflow = load_workflow_text(
            json.dumps(
                {
                    "name": "json-submit",
                    "steps": [
                        {"action": "find_element", "selector": "role=push button"},
                        {"action": "type_text", "value": "Hello", "verify": False},
                        {
                            "action": "drag",
                            "from_x": 1,
                            "from_y": 2,
                            "to_x": 3,
                            "to_y": 4,
                            "button": "middle",
                            "duration_ms": 150,
                        },
                    ],
                }
            )
        )
        yaml_workflow = load_workflow_text(
            dedent(
                """
            name: yaml-submit
            steps:
              - action: find_element
                selector: role=push button,label=Submit
              - action: click
                selector: role=push button,label=Submit
                vision_fallback: true
                verify: false
            """
            )
        )

        self.assertEqual(json_workflow.name, "json-submit")
        self.assertEqual(json_workflow.steps[1].value, "Hello")
        self.assertEqual(json_workflow.steps[2].from_x, 1)
        self.assertEqual(json_workflow.steps[2].button, "middle")
        self.assertEqual(json_workflow.steps[2].duration_ms, 150)
        self.assertEqual(yaml_workflow.name, "yaml-submit")
        self.assertTrue(yaml_workflow.steps[1].vision_fallback)
        self.assertFalse(yaml_workflow.steps[1].verify)

    def test_workflow_recorder_exports_json_and_yaml(self) -> None:
        recorder = WorkflowRecorder("recorded")
        recorder.record_step(
            WorkflowStep(action="find_element", selector="role=push button")
        )
        recorder.record_step(
            WorkflowStep(
                action="click",
                selector="role=push button,label=Submit",
                vision_fallback=True,
            )
        )
        recorder.record_step(WorkflowStep(action="type_text", value="true"))

        json_workflow = load_workflow_text(recorder.to_json(), format_name="json")
        yaml_workflow = load_workflow_text(recorder.to_yaml(), format_name="yaml")

        self.assertEqual(json_workflow.name, "recorded")
        self.assertTrue(json_workflow.steps[1].vision_fallback)
        self.assertEqual(yaml_workflow.steps[0].selector, "role=push button")
        self.assertEqual(yaml_workflow.steps[2].value, "true")

    def test_agent_runtime_executes_workflow_file(self) -> None:
        fake_client = FakeClient()
        runtime = AgentRuntime(client=fake_client)

        with TemporaryDirectory() as tmpdir:
            workflow_path = Path(tmpdir) / "workflow.yaml"
            workflow_path.write_text(
                dedent(
                    """
                name: file-submit
                steps:
                  - action: find_element
                    selector: role=push button,label=Submit
                  - action: click
                    selector: role=push button,label=Submit
                    vision_fallback: true
                  - action: type_text
                    value: Hello
                    verify: false
                """,
                ),
                encoding="utf-8",
            )

            result = runtime.execute_workflow_file(workflow_path)

        self.assertTrue(result.ok)
        self.assertEqual(result.goal, "file-submit")
        self.assertEqual(fake_client.typed_text, "Hello")
        self.assertTrue(fake_client.last_vision_fallback)

    def test_agent_runtime_saves_generated_workflow_file(self) -> None:
        runtime = AgentRuntime(client=FakeClient())
        runtime.ingest_desktop_snapshot(snapshot_id="snapshot:save-generated")

        with TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "generated.yaml"
            saved = runtime.save_generated_workflow(
                "Click Submit and type 'Hello'",
                path,
            )
            loaded = load_workflow_file(saved)

        self.assertEqual(loaded.name, "Click Submit and type 'Hello'")
        self.assertEqual(loaded.steps[2].selector, "role=push button,label=Submit")
        self.assertEqual(loaded.steps[3].value, "Hello")

    def test_agent_runtime_records_actions_and_saves_workflow(self) -> None:
        fake_client = FakeClient()
        runtime = AgentRuntime(client=fake_client)

        runtime.start_recording("manual")
        runtime.find_element("role=push button,label=Submit")
        runtime.click_selector("role=push button,label=Submit", vision_fallback=True)
        runtime.type_text("Hello")
        workflow = runtime.stop_recording()

        self.assertEqual(
            [step.action for step in workflow.steps],
            ["find_element", "click", "type_text"],
        )
        self.assertEqual(workflow.steps[1].selector, "role=push button,label=Submit")
        self.assertTrue(workflow.steps[1].vision_fallback)

        with TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "recording.yaml"
            saved = runtime.save_recording(path)
            loaded = load_workflow_file(saved)

        self.assertEqual(loaded.name, "manual")
        self.assertEqual(loaded.steps[2].value, "Hello")

    def test_agent_runtime_records_pointer_and_hotkey_actions(self) -> None:
        runtime = AgentRuntime(client=FakeClient())

        runtime.start_recording("input")
        runtime.move_mouse(10, 20)
        runtime.drag(10, 20, 30, 40, button="middle", duration_ms=125)
        runtime.hotkey("ctrl+s")
        workflow = runtime.stop_recording()

        self.assertEqual(
            [step.action for step in workflow.steps],
            ["move_mouse", "drag", "hotkey"],
        )
        self.assertEqual(workflow.steps[0].x, 10)
        self.assertEqual(workflow.steps[1].from_x, 10)
        self.assertEqual(workflow.steps[1].button, "middle")
        self.assertEqual(workflow.steps[1].duration_ms, 125)
        self.assertEqual(workflow.steps[2].value, "ctrl+s")

    def test_agent_runtime_requires_client_for_rpc_calls(self) -> None:
        runtime = AgentRuntime()

        with self.assertRaisesRegex(RuntimeError, "PeekabooXClient"):
            runtime.list_windows()

    def test_mcp_server_registers_runtime_handlers(self) -> None:
        runtime = AgentRuntime(client=FakeClient())
        server = McpServer(runtime=runtime)

        server.register_default_tools()

        self.assertTrue(callable(server.tools["list_windows"]))
        self.assertIn(
            "title_regex",
            server.tools["list_windows"].input_schema["properties"],
        )

    def test_mcp_server_rebinds_default_tools_after_runtime_is_attached(self) -> None:
        server = McpServer()
        server.register_default_tools()
        server.runtime = AgentRuntime(client=FakeClient())

        server.register_default_tools()
        windows = server.call_tool("list_windows", {})
        diagnosed = server.call_tool(
            "list_windows",
            {
                "app": "Terminal",
                "focused": True,
                "limit": 1,
                "sort": "focused",
                "backend": "at-spi",
                "diagnose": True,
            },
        )

        self.assertEqual(windows[0]["title"], "Terminal")
        self.assertEqual(diagnosed["backend_name"], "fake")
        self.assertEqual(diagnosed["windows"][0]["title"], "Terminal")
        self.assertTrue(diagnosed["backend_reports"][0]["selected"])

    def test_mcp_server_validates_window_query_arguments(self) -> None:
        server = McpServer(runtime=AgentRuntime(client=FakeClient()))
        server.register_default_tools()

        with self.assertRaisesRegex(ValueError, "limit"):
            server.call_tool("list_windows", {"limit": 0})

        with self.assertRaisesRegex(ValueError, "sort"):
            server.call_tool("list_windows", {"sort": "unknown"})

    def test_mcp_server_calls_doctor_tool(self) -> None:
        server = McpServer(runtime=AgentRuntime(client=FakeClient()))
        server.register_default_tools()
        expected = DoctorResult(
            status="fail",
            checks=(
                DoctorCheck(
                    name="display-server",
                    status="fail",
                    detail="neither WAYLAND_DISPLAY nor DISPLAY is set",
                ),
            ),
            ok_count=0,
            warn_count=0,
            fail_count=1,
            exit_code=1,
            strict=True,
        )

        with patch("peekaboox.agent.runtime.run_doctor", return_value=expected) as run:
            result = server.call_tool("doctor", {"strict": True, "timeout_seconds": 1.5})

        run.assert_called_once_with(strict=True, timeout_seconds=1.5)
        self.assertEqual(result["status"], "fail")
        self.assertEqual(result["fail_count"], 1)
        self.assertEqual(result["checks"][0]["name"], "display-server")
        self.assertEqual(result["checks"][0]["category"], "desktop")
        self.assertEqual(result["checks"][0]["severity"], "error")
        self.assertEqual(result["categories"][0]["name"], "desktop")
        self.assertEqual(result["categories"][0]["severity"], "error")

    def test_mcp_server_calls_preflight_tool(self) -> None:
        server = McpServer(runtime=AgentRuntime(client=FakeClient()))
        server.register_default_tools()
        doctor = DoctorResult(
            status="fail",
            checks=(
                DoctorCheck(
                    name="capture-file",
                    status="fail",
                    detail="no backend candidate detected",
                ),
            ),
            categories=(
                DoctorCategory(
                    name="capture",
                    status="fail",
                    severity="error",
                    ok_count=0,
                    warn_count=0,
                    fail_count=1,
                    total_count=1,
                ),
            ),
            ok_count=0,
            warn_count=0,
            fail_count=1,
            exit_code=0,
        )

        with patch("peekaboox.agent.runtime.run_doctor", return_value=doctor) as run:
            result = server.call_tool(
                "preflight",
                {"categories": ["capture"], "operation": "capture_screen"},
            )

        run.assert_called_once_with(strict=False, timeout_seconds=30.0)
        self.assertFalse(result["ok"])
        self.assertEqual(result["blocked_categories"], ["capture"])
        self.assertEqual(result["category_status"]["capture"], "fail")

    def test_mcp_server_validates_doctor_arguments(self) -> None:
        server = McpServer(runtime=AgentRuntime(client=FakeClient()))
        server.register_default_tools()

        with self.assertRaisesRegex(ValueError, "timeout_seconds"):
            server.call_tool("doctor", {"timeout_seconds": 0})

    def test_mcp_server_lists_tool_descriptors(self) -> None:
        server = McpServer(runtime=AgentRuntime(client=FakeClient()))

        server.register_default_tools()
        descriptors = server.list_tools()

        names = {descriptor["name"] for descriptor in descriptors}
        self.assertIn("capture_screen", names)
        self.assertIn("capture_backends", names)
        self.assertIn("doctor", names)
        self.assertIn("find_element", names)
        self.assertIn("list_plugins", names)
        self.assertTrue(all("inputSchema" in descriptor for descriptor in descriptors))

    def test_mcp_server_calls_observe_find_click_and_type_tools(self) -> None:
        fake_client = FakeClient()
        server = McpServer(runtime=AgentRuntime(client=fake_client))
        server.register_default_tools()

        capture = server.call_tool("capture_screen", {"include_semantic_tree": True})
        delta = server.call_tool(
            "capture_delta",
            {
                "stream_id": "agent-loop",
                "reset": True,
                "region": {"x": 1, "y": 2, "width": 3, "height": 4},
                "per_channel_threshold": 2,
                "low_bandwidth": True,
            },
        )
        backends = server.call_tool(
            "capture_backends",
            {
                "output": "target/test-capture.png",
                "region": {"x": 1, "y": 2, "width": 3, "height": 4},
                "diagnose": True,
                "probe": "file",
            },
        )
        dmabuf = server.call_tool("probe_dmabuf", {"import_target": "compute"})
        elements = server.call_tool(
            "find_element",
            {"selector": "role=push button,label=Submit", "vision_fallback": True},
        )
        click = server.call_tool("click", {"selector": "role=push button", "vision_fallback": True})
        moved = server.call_tool("move_mouse", {"x": 30, "y": 40})
        dragged = server.call_tool(
            "drag",
            {
                "from_x": 1,
                "from_y": 2,
                "to_x": 3,
                "to_y": 4,
                "button": "right",
                "duration_ms": 75,
            },
        )
        typed = server.call_tool("type_text", {"text": "Hello"})
        pasted = server.call_tool("paste_text", {"text": "World", "preserve_clipboard": True})
        hotkey = server.call_tool("hotkey", {"keys": ["ctrl", "s"]})
        state = server.call_tool("get_desktop_state", {})
        desktop_focus = server.call_tool("desktop_focus", {"app": "telegram"})
        desktop_locate = server.call_tool(
            "desktop_locate",
            {"app": "telegram", "target": "search-input"},
        )
        desktop_click = server.call_tool(
            "desktop_click",
            {"app": "telegram", "target": "search-input", "dry_run": True},
        )
        desktop_drag = server.call_tool(
            "desktop_drag",
            {
                "app": "paint",
                "target": "canvas",
                "from_ratio": [0.1, 0.2],
                "to_ratio": [0.9, 0.8],
                "dry_run": True,
            },
        )
        desktop_type = server.call_tool(
            "desktop_type_into",
            {
                "app": "telegram",
                "target": "message-input",
                "text": "PeekabooX",
                "dry_run": True,
            },
        )
        desktop_assert = server.call_tool(
            "desktop_assert",
            {"app": "telegram", "target": "saved-messages"},
        )

        self.assertEqual(capture["image_base64"], "cG5n")
        self.assertEqual(capture["semantic_tree"][0]["label"], "Submit")
        self.assertEqual(delta["stream_id"], "agent-loop")
        self.assertEqual(delta["patch_base64"], "cGF0Y2g=")
        self.assertEqual(delta["changed_bounds"]["width"], 3)
        self.assertEqual(backends["image_backends"][0]["name"], "portal")
        self.assertEqual(backends["probes"][0]["probe"], "file")
        self.assertEqual(backends["region"]["width"], 3)
        self.assertEqual(dmabuf["backend_name"], "fake-dmabuf")
        self.assertEqual(elements[0]["bounds"]["x"], 10)
        self.assertEqual(fake_client.last_find_selector, "role=push button,label=Submit")
        self.assertTrue(fake_client.last_vision_fallback)
        self.assertTrue(click["ok"])
        self.assertEqual(fake_client.clicked_at, None)
        self.assertTrue(moved["ok"])
        self.assertEqual(fake_client.moved_to, (30, 40))
        self.assertTrue(dragged["ok"])
        self.assertEqual(fake_client.dragged, (1, 2, 3, 4, "right", 75))
        self.assertEqual(fake_client.typed_text, "Hello")
        self.assertEqual(typed["message"], "typed 5 chars")
        self.assertEqual(fake_client.pasted_text, "World")
        self.assertTrue(fake_client.preserve_clipboard)
        self.assertEqual(pasted["message"], "pasted 5 chars")
        self.assertTrue(hotkey["ok"])
        self.assertEqual(fake_client.hotkeys[-1], ("ctrl", "s"))
        self.assertEqual(state["active_window"]["title"], "Terminal")
        self.assertEqual(desktop_focus["action"], "focus")
        self.assertEqual(desktop_locate["x"], 10)
        self.assertEqual(desktop_click["action"], "click")
        self.assertEqual(desktop_drag["action"], "drag")
        self.assertEqual(desktop_type["action"], "type-into")
        self.assertEqual(desktop_assert["action"], "assert")
        self.assertEqual(fake_client.desktop_calls[-1][0], "assert")

    def test_capture_screen_resolves_window_filters_and_relative_region(self) -> None:
        fake_client = FakeClient()
        runtime = AgentRuntime(client=fake_client)

        runtime.capture_screen(
            region=Rect(x=10, y=20, width=100, height=40),
            app="Terminal",
            title_regex="Term.*",
        )

        self.assertEqual(
            fake_client.last_window_result_query,
            {
                "id": None,
                "app": "Terminal",
                "title": None,
                "title_regex": "Term.*",
                "focused": False,
                "limit": 1,
                "sort": "focused",
                "backend": None,
                "diagnose": False,
            },
        )
        self.assertIsNotNone(fake_client.last_capture)
        self.assertEqual(
            fake_client.last_capture["region"],
            Rect(x=11, y=22, width=100, height=40),
        )
        self.assertIsNone(fake_client.last_capture["window_id"])

        runtime.capture_screen(window_title="Terminal")
        self.assertIsNotNone(fake_client.last_capture)
        self.assertIsNone(fake_client.last_capture["region"])
        self.assertEqual(fake_client.last_capture["window_id"], "window-1")

    def test_mcp_server_calls_list_plugins_tool(self) -> None:
        with TemporaryDirectory() as tmpdir:
            plugin_dir = Path(tmpdir) / "demo"
            plugin_dir.mkdir()
            (plugin_dir / PLUGIN_MANIFEST_FILE).write_text(
                json.dumps(
                    {
                        "schema_version": PLUGIN_SDK_VERSION,
                        "id": "mcp.demo",
                        "name": "MCP Demo",
                        "version": "1.0.0",
                    }
                ),
                encoding="utf-8",
            )
            runtime = AgentRuntime(client=FakeClient(), plugin_paths=(Path(tmpdir),))
            server = McpServer(runtime=runtime)
            server.register_default_tools()

            result = server.call_tool("list_plugins", {})

        self.assertEqual(result["sdk_version"], PLUGIN_SDK_VERSION)
        self.assertEqual(result["plugins"][0]["manifest"]["id"], "mcp.demo")
        self.assertTrue(result["plugins"][0]["manifest_path"].endswith(PLUGIN_MANIFEST_FILE))

    def test_mcp_server_calls_process_plugin_tool(self) -> None:
        with TemporaryDirectory() as tmpdir:
            plugin_dir = Path(tmpdir) / "demo"
            plugin_dir.mkdir()
            (plugin_dir / "plugin.py").write_text(
                "import json, sys\n"
                "request = json.load(sys.stdin)\n"
                "json.dump({'ok': True, 'result': {'echo': request['arguments']}}, sys.stdout)\n",
                encoding="utf-8",
            )
            (plugin_dir / PLUGIN_MANIFEST_FILE).write_text(
                json.dumps(
                    {
                        "schema_version": PLUGIN_SDK_VERSION,
                        "id": "mcp.exec",
                        "name": "MCP Exec",
                        "version": "1.0.0",
                        "entrypoint": {
                            "kind": "process",
                            "command": [sys.executable, "plugin.py"],
                        },
                        "tools": [
                            {
                                "name": "mcp.echo",
                                "description": "Echo arguments",
                                "input_schema": {
                                    "type": "object",
                                    "properties": {"value": {"type": "string"}},
                                    "additionalProperties": False,
                                },
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )
            runtime = AgentRuntime(client=FakeClient(), plugin_paths=(Path(tmpdir),))
            server = McpServer(runtime=runtime)
            server.register_default_tools()

            result = server.call_tool(
                "call_plugin_tool",
                {
                    "plugin_id": "mcp.exec",
                    "tool": "mcp.echo",
                    "arguments": {"value": "ok"},
                },
            )

        self.assertTrue(result["ok"])
        self.assertEqual(result["result"]["echo"]["value"], "ok")

    def test_mcp_server_calls_state_and_vision_file_tools(self) -> None:
        server = McpServer(runtime=AgentRuntime(client=FakeClient()))
        server.register_default_tools()

        ui_state = server.call_tool(
            "detect_ui_state",
            {"image_paths": ["first.png", "second.png"]},
        )
        ui_elements = server.call_tool(
            "detect_ui_elements",
            {"image_path": "screen.png", "region": {"x": 1, "y": 2, "width": 3, "height": 4}},
        )

        self.assertEqual(ui_state["state"], "stable")
        self.assertEqual(ui_elements["backend_kind"], "vision")
        self.assertEqual(ui_elements["elements"][0]["bounds"]["width"], 3)

    def test_mcp_server_ingests_and_returns_desktop_graph_snapshot(self) -> None:
        server = McpServer(runtime=AgentRuntime(client=FakeClient()))
        server.register_default_tools()

        snapshot = server.call_tool(
            "ingest_desktop_snapshot",
            {"snapshot_id": "snapshot:mcp"},
        )
        latest = server.call_tool("latest_desktop_snapshot", {})

        self.assertEqual(snapshot["id"], "snapshot:mcp")
        self.assertEqual(latest["active_window_id"], "window:window-1")
        self.assertEqual(latest["nodes"][1]["label"], "Terminal")

    def test_mcp_server_records_desktop_events_and_refreshes_stale_graph(self) -> None:
        server = McpServer(runtime=AgentRuntime(client=FakeClient()))
        server.register_default_tools()
        server.call_tool("ingest_desktop_snapshot", {"snapshot_id": "snapshot:mcp-event"})

        update = server.call_tool(
            "record_desktop_event",
            {
                "kind": "window.focused",
                "source": "accessibility",
                "target_id": "window-1",
            },
        )
        status = server.call_tool("desktop_graph_status", {})
        nodes = server.call_tool(
            "query_desktop_graph",
            {
                "kind": "element",
                "label_contains": "submit",
                "refresh_if_stale": True,
            },
        )
        refreshed_status = server.call_tool("desktop_graph_status", {})

        self.assertTrue(update["stale"])
        self.assertTrue(status["stale"])
        self.assertIn("element:button-1", update["invalidation"]["affected_node_ids"])
        self.assertEqual(nodes[0]["id"], "element:button-1")
        self.assertFalse(refreshed_status["stale"])

    def test_mcp_server_queries_desktop_graph_nodes(self) -> None:
        server = McpServer(runtime=AgentRuntime(client=FakeClient()))
        server.register_default_tools()
        server.call_tool("ingest_desktop_snapshot", {"snapshot_id": "snapshot:mcp-query"})

        result = server.call_tool(
            "query_desktop_graph",
            {
                "kind": "element",
                "label_contains": "submit",
                "contained_by": "window-1",
            },
        )

        self.assertEqual(result[0]["id"], "element:button-1")
        self.assertEqual(result[0]["attributes"]["element_id"], "button-1")

    def test_mcp_server_executes_goal_tool(self) -> None:
        server = McpServer(runtime=AgentRuntime(client=FakeClient()))
        server.register_default_tools()

        result = server.call_tool("execute_goal", {"goal": "Inspect desktop"})

        self.assertTrue(result["ok"])
        self.assertEqual(result["goal"], "Inspect desktop")
        self.assertEqual(result["steps"][0]["step"]["action"], "observe")
        self.assertEqual(result["steps"][0]["result"]["image"], "cG5n")

    def test_mcp_server_generates_and_saves_workflow_drafts(self) -> None:
        server = McpServer(runtime=AgentRuntime(client=FakeClient()))
        server.register_default_tools()
        server.call_tool("ingest_desktop_snapshot", {"snapshot_id": "snapshot:mcp-generate"})

        draft = server.call_tool(
            "generate_workflow",
            {"goal": "Click Submit and type 'Hello'", "format": "yaml"},
        )
        loaded_draft = load_workflow_text(draft["text"], format_name="yaml")

        self.assertEqual(draft["format"], "yaml")
        self.assertEqual(draft["workflow"]["steps"][2]["selector"], "role=push button,label=Submit")
        self.assertEqual(loaded_draft.steps[3].value, "Hello")

        with TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "generated.json"
            saved = server.call_tool(
                "save_generated_workflow",
                {"goal": "Click Submit", "path": str(path)},
            )
            loaded = load_workflow_file(saved["path"])

        self.assertEqual(loaded.steps[1].selector, "role=push button,label=Submit")

    def test_mcp_server_refines_and_saves_workflow_drafts(self) -> None:
        def refiner(request: WorkflowRefinementRequest) -> Workflow:
            steps = list(request.draft.steps)
            steps.append(WorkflowStep(action="type_text", value="Reviewed", verify=False))
            return Workflow(name=request.goal, steps=steps)

        runtime = AgentRuntime(
            client=FakeClient(),
            planner=PlanningEngine(workflow_refiner=refiner),
        )
        server = McpServer(runtime=runtime)
        server.register_default_tools()
        server.call_tool("ingest_desktop_snapshot", {"snapshot_id": "snapshot:mcp-refine"})

        refined = server.call_tool(
            "refine_workflow",
            {"goal": "Click Submit", "format": "yaml"},
        )
        loaded_refined = load_workflow_text(refined["text"], format_name="yaml")

        self.assertEqual(refined["workflow"]["steps"][3]["value"], "Reviewed")
        self.assertEqual(loaded_refined.steps[3].value, "Reviewed")

        with TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "refined.json"
            saved = server.call_tool(
                "save_refined_workflow",
                {"goal": "Click Submit", "path": str(path)},
            )
            loaded = load_workflow_file(saved["path"])

        self.assertEqual(loaded.steps[3].value, "Reviewed")

    def test_mcp_server_executes_explicit_workflow_tool(self) -> None:
        fake_client = FakeClient()
        server = McpServer(runtime=AgentRuntime(client=fake_client))
        server.register_default_tools()

        result = server.call_tool(
            "execute_workflow",
            {
                "name": "submit",
                "steps": [
                    {
                        "action": "find_element",
                        "selector": "role=push button,label=Submit",
                    },
                    {
                        "action": "click",
                        "selector": "role=push button,label=Submit",
                        "vision_fallback": True,
                    },
                    {"action": "type_text", "value": "Hello", "verify": False},
                ],
            },
        )

        self.assertTrue(result["ok"])
        self.assertEqual(len(result["steps"]), 3)
        self.assertEqual(result["steps"][1]["attempts"][0]["verification"]["ok"], True)
        self.assertEqual(
            result["steps"][2]["attempts"][0]["verification"]["message"],
            "verification skipped",
        )
        self.assertEqual(fake_client.typed_text, "Hello")
        self.assertTrue(fake_client.last_vision_fallback)

    def test_mcp_server_reports_workflow_recovery_metadata(self) -> None:
        server = McpServer(
            runtime=AgentRuntime(client=SemanticClickMissClient(), retries=1)
        )
        server.register_default_tools()

        result = server.call_tool(
            "execute_workflow",
            {
                "name": "healed-click",
                "steps": [
                    {
                        "action": "click",
                        "selector": "role=push button,label=Submit",
                    }
                ],
            },
        )

        self.assertTrue(result["ok"])
        self.assertEqual(result["steps"][0]["recovery"]["strategy"], "refresh_desktop_graph")
        self.assertEqual(
            result["steps"][0]["attempts"][1]["recovery"]["strategy"],
            "refresh_desktop_graph",
        )

    def test_mcp_server_executes_workflow_file_tool(self) -> None:
        fake_client = FakeClient()
        server = McpServer(runtime=AgentRuntime(client=fake_client))
        server.register_default_tools()

        with TemporaryDirectory() as tmpdir:
            workflow_path = Path(tmpdir) / "workflow.json"
            workflow_path.write_text(
                json.dumps(
                    {
                        "name": "mcp-file-workflow",
                        "steps": [
                            {
                                "action": "find_element",
                                "selector": "role=push button,label=Submit",
                            },
                            {
                                "action": "type_text",
                                "value": "Hello",
                                "verify": False,
                            },
                        ],
                    }
                ),
                encoding="utf-8",
            )

            result = server.call_tool("execute_workflow_file", {"path": str(workflow_path)})

        self.assertTrue(result["ok"])
        self.assertEqual(result["goal"], "mcp-file-workflow")
        self.assertEqual(fake_client.typed_text, "Hello")

    def test_mcp_server_records_and_saves_workflow(self) -> None:
        fake_client = FakeClient()
        server = McpServer(runtime=AgentRuntime(client=fake_client))
        server.register_default_tools()

        started = server.call_tool("start_workflow_recording", {"name": "mcp-recording"})
        server.call_tool("find_element", {"selector": "role=push button,label=Submit"})
        server.call_tool(
            "click",
            {"selector": "role=push button,label=Submit", "vision_fallback": True},
        )
        server.call_tool("type_text", {"text": "Hello"})
        active = server.call_tool("get_recorded_workflow", {})
        stopped = server.call_tool("stop_workflow_recording", {})

        self.assertEqual(started["name"], "mcp-recording")
        self.assertEqual(active["steps"][1]["action"], "click")
        self.assertEqual(stopped["name"], "mcp-recording")
        self.assertEqual(
            [step["action"] for step in stopped["steps"]],
            ["find_element", "click", "type_text"],
        )
        self.assertTrue(stopped["steps"][1]["vision_fallback"])

        with TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "recording.json"
            saved = server.call_tool("save_recorded_workflow", {"path": str(path)})
            loaded = load_workflow_file(saved["path"])

        self.assertEqual(loaded.name, "mcp-recording")
        self.assertEqual(loaded.steps[2].value, "Hello")

    def test_mcp_server_records_coordinate_click_with_semantic_selector(self) -> None:
        server = McpServer(runtime=AgentRuntime(client=FakeClient()))
        server.register_default_tools()

        server.call_tool("start_workflow_recording", {"name": "mcp-semantic-click"})
        server.call_tool("click", {"x": 55, "y": 35})
        workflow = server.call_tool("stop_workflow_recording", {})

        self.assertEqual(workflow["steps"][0]["selector"], "role=push button,label=Submit")
        self.assertIsNone(workflow["steps"][0]["x"])
        self.assertIsNone(workflow["steps"][0]["y"])

    def test_mcp_server_handles_jsonrpc_initialize_and_tools_list(self) -> None:
        server = McpServer(runtime=AgentRuntime(client=FakeClient()))
        server.register_default_tools()

        initialized = server.handle_jsonrpc(
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {"protocolVersion": "2025-11-25"},
            }
        )
        tools = server.handle_jsonrpc({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
        notification = server.handle_jsonrpc(
            {"jsonrpc": "2.0", "method": "notifications/initialized"}
        )

        self.assertEqual(initialized["result"]["protocolVersion"], "2025-11-25")
        self.assertIn("tools", initialized["result"]["capabilities"])
        tool_descriptors = tools["result"]["tools"]
        names = {tool["name"] for tool in tool_descriptors}
        self.assertIn("capture_screen", names)
        self.assertIn("capture_delta", names)
        self.assertIn("capture_backends", names)
        self.assertIn("doctor", names)
        self.assertIn("desktop_profiles", names)
        self.assertIn("find_elements", names)
        capture_screen = next(tool for tool in tool_descriptors if tool["name"] == "capture_screen")
        self.assertIn("inputSchema", capture_screen)
        self.assertIn("outputSchema", capture_screen)
        self.assertTrue(capture_screen["annotations"]["readOnlyHint"])
        self.assertIn("resources", initialized["result"]["capabilities"])
        self.assertIn("prompts", initialized["result"]["capabilities"])
        self.assertIn("logging", initialized["result"]["capabilities"])
        self.assertIn("completions", initialized["result"]["capabilities"])
        self.assertIsNone(notification)

    def test_mcp_server_handles_resources_read_templates(self) -> None:
        server = McpServer(runtime=AgentRuntime(client=FakeClient()))
        server.register_default_tools()

        listed = server.handle_jsonrpc({"jsonrpc": "2.0", "id": 1, "method": "resources/list"})
        uris = {resource["uri"] for resource in listed["result"]["resources"]}
        self.assertIn("peekaboox://server/info", uris)
        self.assertIn("peekaboox://tools", uris)
        self.assertIn("peekaboox://docs/runtime", uris)

        read = server.handle_jsonrpc(
            {
                "jsonrpc": "2.0",
                "id": 2,
                "method": "resources/read",
                "params": {"uri": "peekaboox://server/info"},
            }
        )
        info = json.loads(read["result"]["contents"][0]["text"])
        self.assertEqual(info["name"], "peekaboox-mcp")
        self.assertTrue(info["capabilities"]["resources"])
        self.assertEqual(info["runtime"]["preflight_mode"], "off")

        docs = server.handle_jsonrpc(
            {
                "jsonrpc": "2.0",
                "id": 3,
                "method": "resources/read",
                "params": {"uri": "peekaboox://docs/runtime"},
            }
        )
        self.assertIn("Python Runtime", docs["result"]["contents"][0]["text"])
        self.assertEqual(docs["result"]["contents"][0]["mimeType"], "text/markdown")

        templates = server.handle_jsonrpc(
            {"jsonrpc": "2.0", "id": 4, "method": "resources/templates/list"}
        )
        template_names = {
            template["name"]
            for template in templates["result"]["resourceTemplates"]
        }
        self.assertIn("docs", template_names)

    def test_mcp_server_handles_prompts_logging_and_completion(self) -> None:
        server = McpServer(runtime=AgentRuntime(client=FakeClient()))
        server.register_default_tools()

        prompts = server.handle_jsonrpc({"jsonrpc": "2.0", "id": 1, "method": "prompts/list"})
        prompt_names = {prompt["name"] for prompt in prompts["result"]["prompts"]}
        self.assertIn("build-workflow", prompt_names)
        self.assertIn("recover-from-tool-error", prompt_names)

        prompt = server.handle_jsonrpc(
            {
                "jsonrpc": "2.0",
                "id": 2,
                "method": "prompts/get",
                "params": {
                    "name": "build-workflow",
                    "arguments": {"goal": "Open Telegram Saved Messages"},
                },
            }
        )
        text = prompt["result"]["messages"][0]["content"]["text"]
        self.assertIn("Open Telegram Saved Messages", text)
        self.assertIn("editable workflow", text)

        missing = server.handle_jsonrpc(
            {
                "jsonrpc": "2.0",
                "id": 3,
                "method": "prompts/get",
                "params": {"name": "build-workflow", "arguments": {}},
            }
        )
        self.assertEqual(missing["error"]["code"], -32602)

        logged = server.handle_jsonrpc(
            {
                "jsonrpc": "2.0",
                "id": 4,
                "method": "logging/setLevel",
                "params": {"level": "warning"},
            }
        )
        self.assertEqual(logged["result"], {})
        self.assertEqual(server.log_level, "warning")

        completion = server.handle_jsonrpc(
            {
                "jsonrpc": "2.0",
                "id": 5,
                "method": "completion/complete",
                "params": {
                    "argument": {"name": "target", "value": "search"},
                    "context": {"app": "telegram"},
                },
            }
        )
        self.assertIn("search-input", completion["result"]["completion"]["values"])

    def test_mcp_server_tool_aliases_call_runtime_surface(self) -> None:
        fake_client = FakeClient()
        server = McpServer(runtime=AgentRuntime(client=fake_client))
        server.register_default_tools()

        profiles = server.call_tool("desktop_profiles", {"app": "telegram"})
        self.assertEqual(profiles["profiles"][0]["id"], "telegram")
        self.assertIn("message-input", profiles["profiles"][0]["targets"])

        elements = server.call_tool(
            "find_elements",
            {"selector": "role=push button", "limit": 1, "vision_fallback": True},
        )
        self.assertEqual(len(elements), 1)
        self.assertEqual(elements[0]["label"], "Submit")
        self.assertTrue(fake_client.last_vision_fallback)

        ocr = server.call_tool("ocr", {"image_path": "tests/fixtures/ocr/ocr_sample.png"})
        self.assertEqual(ocr["text"], "Submit")
        dmabuf = server.call_tool("capture_dmabuf", {"import_target": "egl_texture"})
        self.assertEqual(dmabuf["import_target"], "egl_texture")

    def test_mcp_server_exposes_planning_workflow_audit_and_graph_tools(self) -> None:
        runtime = AgentRuntime(client=FakeClient())
        server = McpServer(runtime=runtime)
        server.register_default_tools()

        plan = server.call_tool("plan", {"goal": "Open settings"})
        self.assertTrue(plan["steps"])
        workflow = server.call_tool(
            "plan_workflow",
            {"goal": "Observe desktop", "format": "yaml"},
        )
        self.assertEqual(workflow["format"], "yaml")
        self.assertIn("workflow", workflow)

        with TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "workflow.yaml"
            path.write_text(workflow["text"], encoding="utf-8")
            loaded = server.call_tool("load_workflow_file", {"path": str(path)})
        self.assertIn("steps", loaded["workflow"])

        replanned = server.call_tool(
            "replan_workflow",
            {
                "goal": "Observe desktop",
                "failed_workflow": loaded["workflow"],
                "failed_result": {
                    "recovery": {
                        "failed_step": 0,
                        "reason": "selector miss",
                        "attempts": 2,
                    }
                },
            },
        )
        self.assertIn("workflow", replanned)

        runtime.ingest_desktop_snapshot()
        edges = server.call_tool("query_desktop_edges", {"latest_only": True})
        self.assertIsInstance(edges, list)
        self.assertIn("events", server.call_tool("capability_audit", {}))
        self.assertIn("events", server.call_tool("confirmation_audit", {}))
        self.assertIn("events", server.call_tool("preflight_audit", {}))

    def test_mcp_server_returns_image_content_for_capture_screen(self) -> None:
        server = McpServer(runtime=AgentRuntime(client=FakeClient()))
        server.register_default_tools()

        response = server.handle_jsonrpc(
            {
                "jsonrpc": "2.0",
                "id": 12,
                "method": "tools/call",
                "params": {"name": "capture_screen", "arguments": {}},
            }
        )

        self.assertFalse(response["result"]["isError"])
        content = response["result"]["content"]
        self.assertEqual(content[0]["type"], "image")
        self.assertEqual(content[0]["mimeType"], "image/png")
        text_payload = json.loads(content[1]["text"])
        self.assertEqual(text_payload["image_base64"], "cG5n")

    def test_mcp_server_handles_jsonrpc_tool_call_with_structured_content(self) -> None:
        fake_client = FakeClient()
        server = McpServer(runtime=AgentRuntime(client=fake_client))
        server.register_default_tools()

        response = server.handle_jsonrpc(
            {
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {
                    "name": "find_element",
                    "arguments": {
                        "selector": "role=push button,label=Submit",
                        "vision_fallback": True,
                    },
                },
            }
        )

        self.assertFalse(response["result"]["isError"])
        self.assertEqual(response["result"]["structuredContent"][0]["label"], "Submit")
        text_payload = json.loads(response["result"]["content"][0]["text"])
        self.assertEqual(text_payload[0]["bounds"]["width"], 90)
        self.assertTrue(fake_client.last_vision_fallback)

    def test_mcp_server_handles_jsonrpc_execute_workflow_tool_call(self) -> None:
        server = McpServer(runtime=AgentRuntime(client=FakeClient()))
        server.register_default_tools()

        response = server.handle_jsonrpc(
            {
                "jsonrpc": "2.0",
                "id": 6,
                "method": "tools/call",
                "params": {
                    "name": "execute_workflow",
                    "arguments": {
                        "name": "observe",
                        "steps": [{"action": "observe"}],
                    },
                },
            }
        )

        self.assertFalse(response["result"]["isError"])
        self.assertTrue(response["result"]["structuredContent"]["ok"])
        self.assertEqual(
            response["result"]["structuredContent"]["steps"][0]["step"]["action"],
            "observe",
        )

    def test_mcp_server_reports_tool_execution_errors_as_tool_results(self) -> None:
        server = McpServer(runtime=AgentRuntime(client=FakeClient()))
        server.register_default_tools()

        response = server.handle_jsonrpc(
            {
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {"name": "click", "arguments": {}},
            }
        )

        self.assertTrue(response["result"]["isError"])
        self.assertEqual(response["result"]["structuredContent"]["tool"], "click")

    def test_mcp_server_reports_preflight_errors_as_structured_tool_results(self) -> None:
        runtime = AgentRuntime(client=FakeClient(), preflight_mode="strict")
        server = McpServer(runtime=runtime)
        server.register_default_tools()
        doctor = DoctorResult(
            status="fail",
            checks=(
                DoctorCheck(
                    name="input-click",
                    status="fail",
                    detail="no input backend candidate detected",
                ),
            ),
            categories=(
                DoctorCategory(
                    name="input",
                    status="fail",
                    severity="error",
                    ok_count=0,
                    warn_count=0,
                    fail_count=1,
                    total_count=1,
                ),
            ),
            ok_count=0,
            warn_count=0,
            fail_count=1,
            exit_code=0,
        )

        with patch("peekaboox.agent.runtime.run_doctor", return_value=doctor):
            response = server.handle_jsonrpc(
                {
                    "jsonrpc": "2.0",
                    "id": 11,
                    "method": "tools/call",
                    "params": {"name": "click", "arguments": {"x": 10, "y": 20}},
                }
            )

        content = response["result"]["structuredContent"]
        self.assertTrue(response["result"]["isError"])
        self.assertEqual(content["error"], "PreflightError")
        self.assertEqual(content["tool"], "click")
        self.assertEqual(content["next_action"], "run_doctor")
        self.assertEqual(content["blocked_categories"], ["input"])
        self.assertEqual(content["warning_categories"], [])
        self.assertEqual(content["category_status"]["input"], "fail")
        self.assertEqual(content["preflight"]["operation"], "click")
        self.assertEqual(content["preflight"]["blocked_categories"], ["input"])
        text_payload = json.loads(response["result"]["content"][0]["text"])
        self.assertEqual(text_payload["blocked_categories"], ["input"])

    def test_mcp_server_reports_capability_denials_as_tool_errors(self) -> None:
        runtime = AgentRuntime(
            client=FakeClient(),
            capability_policy=CapabilityPolicy.deny([Capability.CLICK]),
        )
        server = McpServer(runtime=runtime)
        server.register_default_tools()

        response = server.handle_jsonrpc(
            {
                "jsonrpc": "2.0",
                "id": 7,
                "method": "tools/call",
                "params": {"name": "click", "arguments": {"x": 10, "y": 20}},
            }
        )

        self.assertTrue(response["result"]["isError"])
        self.assertEqual(
            response["result"]["structuredContent"]["error"],
            "CapabilityDeniedError",
        )
        self.assertEqual(response["result"]["structuredContent"]["capability"], Capability.CLICK)
        self.assertEqual(response["result"]["structuredContent"]["operation"], "click")
        self.assertEqual(
            response["result"]["structuredContent"]["next_action"],
            "adjust_capability_profile",
        )
        self.assertEqual(runtime.capability_audit()[-1].capability, Capability.CLICK)

    @unittest.skipUnless(_protobuf_available(), "protobuf runtime dependencies are not installed")
    def test_mcp_create_server_applies_capability_profile(self) -> None:
        server = create_server(
            "127.0.0.1:47777",
            connect=True,
            capability_profile=CapabilityProfile.OBSERVE,
        )

        response = server.handle_jsonrpc(
            {
                "jsonrpc": "2.0",
                "id": 10,
                "method": "tools/call",
                "params": {"name": "click", "arguments": {"x": 10, "y": 20}},
            }
        )

        self.assertTrue(response["result"]["isError"])
        self.assertEqual(
            response["result"]["structuredContent"]["error"],
            "CapabilityDeniedError",
        )

    def test_mcp_create_server_applies_preflight_options(self) -> None:
        server = create_server(
            "127.0.0.1:47777",
            connect=False,
            preflight_mode="warn",
            preflight_timeout_seconds=4.5,
        )

        self.assertIsNotNone(server.runtime)
        self.assertEqual(server.runtime.preflight_mode, "warn")
        self.assertEqual(server.runtime.preflight_timeout_seconds, 4.5)

    def test_mcp_server_reports_confirmation_requirements_as_tool_errors(self) -> None:
        runtime = AgentRuntime(
            client=FakeClient(),
            confirmation_policy=ConfirmationPolicy.require_for([DangerousAction.TYPE_TEXT]),
        )
        server = McpServer(runtime=runtime)
        server.register_default_tools()

        response = server.handle_jsonrpc(
            {
                "jsonrpc": "2.0",
                "id": 8,
                "method": "tools/call",
                "params": {"name": "type_text", "arguments": {"text": "Hello"}},
            }
        )

        self.assertTrue(response["result"]["isError"])
        self.assertEqual(
            response["result"]["structuredContent"]["error"],
            "ConfirmationRequiredError",
        )
        self.assertEqual(
            response["result"]["structuredContent"]["action"],
            DangerousAction.TYPE_TEXT,
        )
        self.assertEqual(
            response["result"]["structuredContent"]["next_action"],
            "request_confirmation",
        )
        self.assertEqual(
            runtime.confirmation_audit()[-1].action,
            DangerousAction.TYPE_TEXT,
        )

    def test_mcp_server_reports_confirmation_denials_as_structured_tool_errors(self) -> None:
        runtime = AgentRuntime(
            client=FakeClient(),
            confirmation_policy=ConfirmationPolicy.require_for(
                [DangerousAction.WORKFLOW_EXECUTE],
                confirmer=lambda _request: False,
            ),
        )
        server = McpServer(runtime=runtime)
        server.register_default_tools()

        response = server.handle_jsonrpc(
            {
                "jsonrpc": "2.0",
                "id": 13,
                "method": "tools/call",
                "params": {
                    "name": "execute_workflow",
                    "arguments": {"steps": [{"action": "observe"}]},
                },
            }
        )

        content = response["result"]["structuredContent"]
        self.assertTrue(response["result"]["isError"])
        self.assertEqual(content["error"], "ConfirmationDeniedError")
        self.assertEqual(content["action"], DangerousAction.WORKFLOW_EXECUTE)
        self.assertEqual(content["next_action"], "stop")
        self.assertFalse(content["retryable"])

    def test_mcp_server_persists_runtime_audit_for_tool_calls(self) -> None:
        with TemporaryDirectory() as tmpdir:
            audit_path = Path(tmpdir) / "mcp-runtime-audit.jsonl"
            runtime = AgentRuntime(
                client=FakeClient(),
                audit_logger=JsonlAuditLogger(audit_path, source="mcp"),
            )
            server = McpServer(runtime=runtime)
            server.register_default_tools()

            response = server.handle_jsonrpc(
                {
                    "jsonrpc": "2.0",
                    "id": 9,
                    "method": "tools/call",
                    "params": {"name": "list_windows", "arguments": {}},
                }
            )

            records = [
                json.loads(line)
                for line in audit_path.read_text(encoding="utf-8").splitlines()
            ]

        self.assertFalse(response["result"]["isError"])
        self.assertEqual(records[0]["source"], "mcp")
        self.assertEqual(records[0]["event"], "capability")
        self.assertEqual(records[0]["details"]["operation"], "list_windows")

    def test_mcp_server_reports_unknown_tools_as_protocol_errors(self) -> None:
        server = McpServer(runtime=AgentRuntime(client=FakeClient()))
        server.register_default_tools()

        response = server.handle_jsonrpc(
            {
                "jsonrpc": "2.0",
                "id": 5,
                "method": "tools/call",
                "params": {"name": "missing", "arguments": {}},
            }
        )

        self.assertEqual(response["error"]["code"], -32602)
        self.assertIn("unknown MCP tool", response["error"]["message"])

    def test_mcp_server_serves_line_delimited_stdio_requests(self) -> None:
        server = McpServer(runtime=AgentRuntime(client=FakeClient()))
        server.register_default_tools()
        input_stream = StringIO(
            "\n".join(
                [
                    json.dumps({"jsonrpc": "2.0", "id": 1, "method": "initialize"}),
                    json.dumps({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
                    json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}),
                    "",
                ]
            )
        )
        output_stream = StringIO()

        server.serve_stdio(input_stream=input_stream, output_stream=output_stream)

        responses = [json.loads(line) for line in output_stream.getvalue().splitlines()]
        self.assertEqual([response["id"] for response in responses], [1, 2])
        self.assertEqual(responses[0]["result"]["serverInfo"]["name"], "peekaboox-mcp")
        self.assertIn("tools", responses[1]["result"])

    @unittest.skipUnless(_protobuf_available(), "protobuf runtime dependencies are not installed")
    def test_python_client_maps_generated_list_windows_response(self) -> None:
        from peekaboox.v1 import peekaboox_pb2

        class Stub:
            def __init__(self) -> None:
                self.request = None
                self.timeout = None

            def ListWindows(self, request, timeout):
                self.request = request
                self.timeout = timeout
                return peekaboox_pb2.ListWindowsResponse(
                    backend_name="test",
                    backend_kind="x11",
                    warnings=["fallback used"],
                    backend_reports=[
                        peekaboox_pb2.WindowBackendReport(
                            backend_name="test",
                            backend_kind="x11",
                            raw_window_count=1,
                            matched_window_count=1,
                            selected=True,
                        )
                    ],
                    windows=[
                        peekaboox_pb2.WindowInfo(
                            id="w1",
                            title="Editor",
                            app_id="org.example.Editor",
                            bounds=peekaboox_pb2.Rect(x=3, y=4, width=1024, height=768),
                            focused=True,
                            state="normal",
                        )
                    ]
                )

        stub = Stub()
        client = PeekabooXClient(stub=stub, messages=peekaboox_pb2, timeout_seconds=1.25)

        windows = client.list_windows(focused=True, limit=1, sort="focused", backend="xdotool")
        result = client.list_windows_result(diagnose=True)

        self.assertIsInstance(stub.request, peekaboox_pb2.ListWindowsRequest)
        self.assertEqual(stub.timeout, 1.25)
        self.assertTrue(stub.request.diagnose)
        self.assertEqual(windows[0].title, "Editor")
        self.assertEqual(windows[0].bounds.width, 1024)
        self.assertEqual(result.backend_name, "test")
        self.assertEqual(result.warnings, ("fallback used",))
        self.assertTrue(result.backend_reports[0].selected)

    @unittest.skipUnless(_protobuf_available(), "protobuf runtime dependencies are not installed")
    def test_python_client_maps_generated_capture_delta_response(self) -> None:
        from peekaboox.v1 import peekaboox_pb2

        class Stub:
            def __init__(self) -> None:
                self.request = None

            def CaptureDelta(self, request, timeout):
                self.request = request
                return peekaboox_pb2.CaptureDeltaResponse(
                    stream_id="agent-loop",
                    sequence=2,
                    full_frame=False,
                    frame_width=800,
                    frame_height=600,
                    pixel_format=peekaboox_pb2.PIXEL_FORMAT_RGBA8,
                    changed_bounds=peekaboox_pb2.Rect(x=10, y=20, width=30, height=40),
                    changed_pixels=1200,
                    changed_ratio=0.0025,
                    patch_stride=120,
                    patch=b"patch",
                    capture_region=peekaboox_pb2.Rect(x=1, y=2, width=3, height=4),
                    low_bandwidth=True,
                    metadata=peekaboox_pb2.CaptureMetadata(
                        width=800,
                        height=600,
                        backend="fake/portal",
                        captured_at_unix_ms=123,
                    ),
                )

        stub = Stub()
        client = PeekabooXClient(stub=stub, messages=peekaboox_pb2)

        result = client.capture_delta(
            stream_id="agent-loop",
            reset=True,
            region=Rect(x=1, y=2, width=3, height=4),
            per_channel_threshold=2,
            low_bandwidth=True,
        )

        self.assertIsInstance(stub.request, peekaboox_pb2.CaptureDeltaRequest)
        self.assertEqual(stub.request.stream_id, "agent-loop")
        self.assertTrue(stub.request.reset)
        self.assertEqual(stub.request.target.region.width, 3)
        self.assertEqual(stub.request.per_channel_threshold, 2)
        self.assertTrue(stub.request.low_bandwidth)
        self.assertEqual(result.pixel_format, "rgba8")
        self.assertTrue(result.low_bandwidth)
        self.assertEqual(result.capture_region, Rect(x=1, y=2, width=3, height=4))
        self.assertEqual(result.changed_bounds.width, 30)
        self.assertEqual(result.patch, b"patch")

    @unittest.skipUnless(_protobuf_available(), "protobuf runtime dependencies are not installed")
    def test_python_client_maps_generated_capture_backends_response(self) -> None:
        from peekaboox.v1 import peekaboox_pb2

        class Stub:
            def __init__(self) -> None:
                self.request = None

            def CaptureBackends(self, request, timeout):
                self.request = request
                return peekaboox_pb2.CaptureBackendsResponse(
                    session_type="wayland",
                    desktop="GNOME",
                    pipewire_session_available=True,
                    pipewire_backend_feature_enabled=True,
                    egl_backend_feature_enabled=False,
                    output_path=request.output,
                    region=peekaboox_pb2.Rect(x=1, y=2, width=3, height=4),
                    image_backends=[
                        peekaboox_pb2.CaptureBackend(
                            name="portal",
                            backend_kind="wayland",
                            available=True,
                            supports_output=True,
                            supports_file_capture=True,
                            supports_stdout_capture=True,
                            supports_stdout_region_capture=True,
                            selected=True,
                        )
                    ],
                    zero_copy_backends=[
                        peekaboox_pb2.ZeroCopyBackend(
                            name="pipewire",
                            backend_kind="wayland",
                            transport="dmabuf",
                            availability="available",
                            selected=True,
                            pipewire_backend_feature_enabled=True,
                            egl_backend_feature_enabled=False,
                        )
                    ],
                    probes=[
                        peekaboox_pb2.CaptureBackendProbeResult(
                            probe="region",
                            ok=True,
                            backend_name="portal",
                            backend_kind="wayland",
                            detail="captured 3x4",
                            width=3,
                            height=4,
                        )
                    ],
                )

        stub = Stub()
        client = PeekabooXClient(stub=stub, messages=peekaboox_pb2)

        result = client.capture_backends(
            output="target/capture.png",
            region=Rect(x=1, y=2, width=3, height=4),
            diagnose=True,
            probe="region",
        )

        self.assertIsInstance(stub.request, peekaboox_pb2.CaptureBackendsRequest)
        self.assertEqual(stub.request.output, "target/capture.png")
        self.assertEqual(stub.request.region.width, 3)
        self.assertTrue(stub.request.diagnose)
        self.assertEqual(stub.request.probe, peekaboox_pb2.CAPTURE_BACKEND_PROBE_REGION)
        self.assertEqual(result.session_type, "wayland")
        self.assertEqual(result.desktop, "GNOME")
        self.assertEqual(result.region, Rect(x=1, y=2, width=3, height=4))
        self.assertEqual(result.image_backends[0].name, "portal")
        self.assertTrue(result.zero_copy_backends[0].selected)
        self.assertEqual(result.probes[0].probe, "region")
        self.assertEqual(result.probes[0].width, 3)

    @unittest.skipUnless(_protobuf_available(), "protobuf runtime dependencies are not installed")
    def test_python_client_builds_generated_capture_screen_window_target(self) -> None:
        from peekaboox.v1 import peekaboox_pb2

        class Stub:
            def __init__(self) -> None:
                self.request = None

            def CaptureScreen(self, request, timeout):
                self.request = request
                return peekaboox_pb2.CaptureScreenResponse(
                    image=b"png",
                    mime_type="image/png",
                    metadata=peekaboox_pb2.CaptureMetadata(
                        width=800,
                        height=600,
                        backend="fake",
                        captured_at_unix_ms=123,
                    ),
                )

        stub = Stub()
        client = PeekabooXClient(stub=stub, messages=peekaboox_pb2)

        result = client.capture_screen(window_id="window-1")

        self.assertEqual(stub.request.target.window_id, "window-1")
        self.assertEqual(result.image, b"png")
        self.assertEqual(result.metadata.width, 800)

    @unittest.skipUnless(_protobuf_available(), "protobuf runtime dependencies are not installed")
    def test_python_client_builds_generated_desktop_requests(self) -> None:
        from peekaboox.v1 import peekaboox_pb2

        class Stub:
            def __init__(self) -> None:
                self.requests = []

            def DesktopFocus(self, request, timeout):
                self.requests.append(("focus", request))
                return peekaboox_pb2.DesktopActionResponse(
                    app=request.app,
                    action="focus",
                    detail="focused",
                    backend_name="fake-desktop",
                )

            def DesktopLocate(self, request, timeout):
                self.requests.append(("locate", request))
                return peekaboox_pb2.DesktopLocateResponse(
                    app=request.app,
                    target=request.target,
                    point=peekaboox_pb2.Point(x=10, y=20),
                    rect=peekaboox_pb2.Rect(x=1, y=2, width=30, height=40),
                    source="fake",
                )

            def DesktopClick(self, request, timeout):
                self.requests.append(("click", request))
                return peekaboox_pb2.DesktopActionResponse(
                    app=request.app,
                    action="click",
                    detail="clicked",
                    backend_name="fake-desktop",
                )

            def DesktopDrag(self, request, timeout):
                self.requests.append(("drag", request))
                return peekaboox_pb2.DesktopActionResponse(
                    app=request.app,
                    action="drag",
                    detail="dragged",
                    backend_name="fake-desktop",
                )

            def DesktopTypeInto(self, request, timeout):
                self.requests.append(("type", request))
                return peekaboox_pb2.DesktopActionResponse(
                    app=request.app,
                    action="type-into",
                    detail="typed",
                    backend_name="fake-desktop",
                )

            def DesktopAssert(self, request, timeout):
                self.requests.append(("assert", request))
                return peekaboox_pb2.DesktopActionResponse(
                    app=request.app,
                    action="assert",
                    detail="asserted",
                    backend_name="fake-desktop",
                )

        stub = Stub()
        client = PeekabooXClient(stub=stub, messages=peekaboox_pb2)

        self.assertEqual(
            client.desktop_focus("telegram", window_id="window-1", verify=True).action,
            "focus",
        )
        locate = client.desktop_locate("telegram", "search-input", window_id="window-1")
        self.assertEqual(locate.rect.width, 30)
        self.assertEqual(
            client.desktop_click(
                "telegram",
                "search-input",
                button="right",
                dry_run=True,
                window_id="window-1",
                verify=True,
            ).action,
            "click",
        )
        self.assertEqual(
            client.desktop_drag(
                "paint",
                "canvas",
                from_ratio=(0.1, 0.2),
                to_ratio=(0.9, 0.8),
                dry_run=True,
                window_id="window-2",
                verify=True,
            ).action,
            "drag",
        )
        self.assertEqual(
            client.desktop_type_into(
                "telegram",
                "message-input",
                "PeekabooX",
                window_id="window-1",
                verify=True,
            ).action,
            "type-into",
        )
        self.assertEqual(
            client.desktop_assert(
                "telegram",
                "message-list",
                assertion="contains",
                expected_text="PeekabooX",
                window_id="window-1",
            ).action,
            "assert",
        )

        self.assertEqual(stub.requests[0][1].window_id, "window-1")
        self.assertTrue(stub.requests[0][1].verify)
        self.assertEqual(stub.requests[1][1].window_id, "window-1")
        self.assertEqual(stub.requests[2][1].button, peekaboox_pb2.MOUSE_BUTTON_RIGHT)
        self.assertEqual(stub.requests[2][1].window_id, "window-1")
        self.assertTrue(stub.requests[2][1].verify)
        self.assertAlmostEqual(stub.requests[3][1].from_ratio_x, 0.1)
        self.assertEqual(
            stub.requests[5][1].assertion,
            peekaboox_pb2.DESKTOP_ASSERTION_KIND_CONTAINS,
        )
        self.assertEqual(stub.requests[5][1].expected_text, "PeekabooX")

    @unittest.skipUnless(_protobuf_available(), "protobuf runtime dependencies are not installed")
    def test_python_client_builds_generated_click_request(self) -> None:
        from peekaboox.v1 import peekaboox_pb2

        class Stub:
            def __init__(self) -> None:
                self.request = None

            def Click(self, request, timeout):
                self.request = request
                return peekaboox_pb2.ActionResponse(ok=True, message="ok")

        stub = Stub()
        client = PeekabooXClient(stub=stub, messages=peekaboox_pb2)

        result = client.click(7, 9)

        self.assertTrue(result.ok)
        self.assertEqual(stub.request.coordinates.x, 7)
        self.assertEqual(stub.request.coordinates.y, 9)
        self.assertFalse(stub.request.vision_fallback)

    @unittest.skipUnless(_protobuf_available(), "protobuf runtime dependencies are not installed")
    def test_python_client_builds_generated_semantic_click_request(self) -> None:
        from peekaboox.v1 import peekaboox_pb2

        class Stub:
            def __init__(self) -> None:
                self.request = None

            def Click(self, request, timeout):
                self.request = request
                return peekaboox_pb2.ActionResponse(ok=True, message="ok")

        stub = Stub()
        client = PeekabooXClient(stub=stub, messages=peekaboox_pb2)

        result = client.click_selector("role=push button,label=Submit", vision_fallback=True)

        self.assertTrue(result.ok)
        self.assertEqual(stub.request.semantic_selector, "role=push button,label=Submit")
        self.assertTrue(stub.request.vision_fallback)

    @unittest.skipUnless(_protobuf_available(), "protobuf runtime dependencies are not installed")
    def test_python_client_builds_generated_pointer_and_hotkey_requests(self) -> None:
        from peekaboox.v1 import peekaboox_pb2

        class Stub:
            def __init__(self) -> None:
                self.requests = []

            def MoveMouse(self, request, timeout):
                self.requests.append(("move", request))
                return peekaboox_pb2.ActionResponse(ok=True, message="ok")

            def Drag(self, request, timeout):
                self.requests.append(("drag", request))
                return peekaboox_pb2.ActionResponse(ok=True, message="ok")

            def Hotkey(self, request, timeout):
                self.requests.append(("hotkey", request))
                return peekaboox_pb2.ActionResponse(ok=True, message="ok")

        stub = Stub()
        client = PeekabooXClient(stub=stub, messages=peekaboox_pb2)

        self.assertTrue(client.move_mouse(7, 9).ok)
        self.assertTrue(client.drag(1, 2, 3, 4, button="right", duration_ms=500).ok)
        self.assertTrue(client.hotkey(["ctrl", "s"]).ok)

        self.assertEqual(stub.requests[0][1].coordinates.x, 7)
        self.assertEqual(getattr(stub.requests[1][1], "from").x, 1)
        self.assertEqual(stub.requests[1][1].to.y, 4)
        self.assertEqual(stub.requests[1][1].button, peekaboox_pb2.MOUSE_BUTTON_RIGHT)
        self.assertEqual(stub.requests[1][1].duration_ms, 500)
        self.assertEqual(list(stub.requests[2][1].keys), ["ctrl", "s"])

    @unittest.skipUnless(_protobuf_available(), "protobuf runtime dependencies are not installed")
    def test_python_client_builds_generated_paste_probe_and_plugin_requests(self) -> None:
        from peekaboox.v1 import peekaboox_pb2

        class Stub:
            def __init__(self) -> None:
                self.requests = []

            def PasteText(self, request, timeout):
                self.requests.append(("paste", request))
                return peekaboox_pb2.ActionResponse(
                    ok=True,
                    message="ok",
                    backend_name="clipboard",
                    backend_kind="wayland",
                )

            def ProbeDmaBuf(self, request, timeout):
                self.requests.append(("dmabuf", request))
                return peekaboox_pb2.DmaBufProbeResponse(
                    import_target=peekaboox_pb2.DMA_BUF_IMPORT_TARGET_EGL_TEXTURE,
                    backend_name="dmabuf",
                    stream_node_id=7,
                    width=800,
                    height=600,
                    pixel_format="rgba8",
                    fourcc=875713112,
                    planes=1,
                    memory_layout="single-plane",
                    synchronization="implicit",
                )

            def ListPlugins(self, request, timeout):
                self.requests.append(("list_plugins", request))
                return peekaboox_pb2.PluginListResponse(
                    sdk_version=PLUGIN_SDK_VERSION,
                    plugins=[
                        peekaboox_pb2.Plugin(
                            id="demo",
                            name="Demo",
                            version="1.0.0",
                            root_dir="/tmp/demo",
                            manifest_path="/tmp/demo/peekaboox.plugin.json",
                            tools=[
                                peekaboox_pb2.PluginTool(
                                    name="demo.echo",
                                    description="Echo",
                                    input_schema_json='{"type":"object"}',
                                )
                            ],
                        )
                    ],
                )

            def CallPluginTool(self, request, timeout):
                self.requests.append(("call_plugin", request))
                return peekaboox_pb2.PluginToolExecutionResponse(
                    ok=True,
                    plugin_id=request.plugin_id,
                    tool=request.tool,
                    exit_code=0,
                    stdout='{"result":{"ok":true}}',
                    result_json='{"ok":true}',
                )

        stub = Stub()
        client = PeekabooXClient(stub=stub, messages=peekaboox_pb2)

        paste = client.paste_text("Hello", preserve_clipboard=True)
        dmabuf = client.probe_dmabuf("egl_texture")
        plugins = client.list_plugins(paths=["examples/plugins"])
        executed = client.call_plugin_tool(
            "demo",
            "demo.echo",
            {"value": "ok"},
            paths=["examples/plugins"],
            timeout_seconds=1.5,
        )

        self.assertEqual(paste.backend_name, "clipboard")
        self.assertTrue(stub.requests[0][1].preserve_clipboard)
        self.assertEqual(
            stub.requests[1][1].import_target,
            peekaboox_pb2.DMA_BUF_IMPORT_TARGET_EGL_TEXTURE,
        )
        self.assertEqual(dmabuf.import_target, "egl_texture")
        self.assertEqual(stub.requests[2][1].paths[0], "examples/plugins")
        self.assertEqual(plugins.plugins[0].tools[0].name, "demo.echo")
        self.assertEqual(json.loads(stub.requests[3][1].arguments_json)["value"], "ok")
        self.assertEqual(stub.requests[3][1].timeout_ms, 1500)
        self.assertEqual(executed.result["ok"], True)

    @unittest.skipUnless(_protobuf_available(), "protobuf runtime dependencies are not installed")
    def test_python_client_maps_generated_ui_elements(self) -> None:
        from peekaboox.v1 import peekaboox_pb2

        class Stub:
            def __init__(self) -> None:
                self.request = None

            def FindElement(self, request, timeout):
                self.request = request
                return peekaboox_pb2.FindElementResponse(
                    elements=[
                        peekaboox_pb2.UiElement(
                            id="element-1",
                            role="push button",
                            label="Submit",
                            bounds=peekaboox_pb2.Rect(x=10, y=20, width=90, height=30),
                            confidence=1.0,
                            states=["enabled", "visible"],
                        )
                    ]
                )

        stub = Stub()
        client = PeekabooXClient(stub=stub, messages=peekaboox_pb2)

        elements = client.find_element("role=push button,label=Submit", vision_fallback=True)

        self.assertTrue(stub.request.vision_fallback)
        self.assertEqual(elements[0].role, "push button")
        self.assertEqual(elements[0].label, "Submit")
        self.assertEqual(elements[0].bounds.x, 10)
        self.assertEqual(elements[0].states, ("enabled", "visible"))

    @unittest.skipUnless(_protobuf_available(), "protobuf runtime dependencies are not installed")
    def test_python_client_maps_generated_ocr_response(self) -> None:
        from peekaboox.v1 import peekaboox_pb2

        class Stub:
            def __init__(self) -> None:
                self.request = None

            def OcrScreen(self, request, timeout):
                self.request = request
                return peekaboox_pb2.OcrResponse(
                    backend_name="tesseract",
                    text="Submit",
                    blocks=[
                        peekaboox_pb2.OcrBlock(
                            text="Submit",
                            element=peekaboox_pb2.UiElement(
                                id="ocr:10:20:90:30",
                                role="text",
                                label="Submit",
                                bounds=peekaboox_pb2.Rect(x=10, y=20, width=90, height=30),
                                confidence=0.95,
                            ),
                        )
                    ],
                    words=[
                        peekaboox_pb2.OcrBlock(
                            text="Submit",
                            element=peekaboox_pb2.UiElement(
                                id="ocr-word:10:20:90:30",
                                role="word",
                                label="Submit",
                                bounds=peekaboox_pb2.Rect(x=10, y=20, width=90, height=30),
                                confidence=0.95,
                            ),
                        )
                    ],
                    warnings=["low contrast"],
                )

        stub = Stub()
        client = PeekabooXClient(stub=stub, messages=peekaboox_pb2)

        result = client.ocr_region(
            Rect(x=10, y=20, width=90, height=30),
            language="eng",
            image_path="sample.png",
            page_segmentation_mode=6,
            engine_mode=1,
            dpi=300,
            min_confidence=0.5,
            whitelist="Submit",
            config=("preserve_interword_spaces=1",),
            scale=2.0,
            grayscale=True,
            threshold=180,
            invert=True,
            contrast=10.0,
            deskew=True,
        )

        self.assertEqual(stub.request.region.x, 10)
        self.assertEqual(stub.request.language, "eng")
        self.assertEqual(stub.request.image_path, "sample.png")
        self.assertEqual(stub.request.page_segmentation_mode, 6)
        self.assertEqual(stub.request.engine_mode, 1)
        self.assertEqual(stub.request.dpi, 300)
        self.assertAlmostEqual(stub.request.min_confidence, 0.5)
        self.assertEqual(stub.request.whitelist, "Submit")
        self.assertEqual(tuple(stub.request.config), ("preserve_interword_spaces=1",))
        self.assertAlmostEqual(stub.request.scale, 2.0)
        self.assertTrue(stub.request.grayscale)
        self.assertEqual(stub.request.threshold, 180)
        self.assertTrue(stub.request.invert)
        self.assertAlmostEqual(stub.request.contrast, 10.0)
        self.assertTrue(stub.request.deskew)
        self.assertEqual(result.backend_name, "tesseract")
        self.assertEqual(result.blocks[0].element.label, "Submit")
        self.assertEqual(result.words[0].element.role, "word")
        self.assertEqual(result.warnings, ("low contrast",))

    @unittest.skipUnless(_protobuf_available(), "protobuf runtime dependencies are not installed")
    def test_python_client_maps_generated_visual_diff_response(self) -> None:
        from peekaboox.v1 import peekaboox_pb2

        class Stub:
            def __init__(self) -> None:
                self.request = None

            def CompareImages(self, request, timeout):
                self.request = request
                return peekaboox_pb2.VisualDiffResponse(
                    compared_region=peekaboox_pb2.Rect(x=0, y=0, width=4, height=3),
                    compared_pixels=12,
                    changed_pixels=2,
                    changed_ratio=2 / 12,
                    mean_absolute_error=12.5,
                    max_channel_delta=255,
                    changed_bounds=peekaboox_pb2.Rect(x=1, y=1, width=2, height=1),
                    matches=False,
                )

        stub = Stub()
        client = PeekabooXClient(stub=stub, messages=peekaboox_pb2)

        result = client.compare_images(
            b"expected",
            b"actual",
            region=Rect(x=0, y=0, width=4, height=3),
            per_channel_threshold=3,
            max_changed_ratio=0.01,
        )

        self.assertEqual(stub.request.expected_image, b"expected")
        self.assertEqual(stub.request.actual_image, b"actual")
        self.assertEqual(stub.request.region.width, 4)
        self.assertEqual(stub.request.per_channel_threshold, 3)
        self.assertAlmostEqual(stub.request.max_changed_ratio, 0.01, places=6)
        self.assertEqual(result.compared_pixels, 12)
        self.assertEqual(result.changed_bounds, Rect(x=1, y=1, width=2, height=1))
        self.assertFalse(result.matches)

    @unittest.skipUnless(_protobuf_available(), "protobuf runtime dependencies are not installed")
    def test_python_client_maps_generated_ui_state_response(self) -> None:
        from peekaboox.v1 import peekaboox_pb2

        class Stub:
            def __init__(self) -> None:
                self.request = None

            def DetectUiState(self, request, timeout):
                self.request = request
                return peekaboox_pb2.UiStateResponse(
                    state=peekaboox_pb2.UI_STATE_KIND_LOADING,
                    compared_transitions=2,
                    stable_transitions=1,
                    loading_transitions=1,
                    trailing_stable_transitions=0,
                    latest_diff=peekaboox_pb2.VisualDiffResponse(
                        compared_region=peekaboox_pb2.Rect(x=0, y=0, width=4, height=3),
                        compared_pixels=12,
                        changed_pixels=2,
                        changed_ratio=2 / 12,
                        mean_absolute_error=12.5,
                        max_channel_delta=255,
                        changed_bounds=peekaboox_pb2.Rect(x=1, y=1, width=2, height=1),
                        matches=False,
                    ),
                    max_changed_ratio=2 / 12,
                    mean_changed_ratio=1 / 12,
                    changed_bounds=peekaboox_pb2.Rect(x=1, y=1, width=2, height=1),
                )

        stub = Stub()
        client = PeekabooXClient(stub=stub, messages=peekaboox_pb2)

        result = client.detect_ui_state(
            [b"first", b"second"],
            region=Rect(x=0, y=0, width=4, height=3),
            per_channel_threshold=3,
            stable_max_changed_ratio=0.001,
            loading_min_changed_ratio=0.02,
            required_stable_transitions=2,
        )

        self.assertEqual(list(stub.request.images), [b"first", b"second"])
        self.assertEqual(stub.request.region.width, 4)
        self.assertEqual(stub.request.per_channel_threshold, 3)
        self.assertAlmostEqual(stub.request.stable_max_changed_ratio, 0.001, places=6)
        self.assertAlmostEqual(stub.request.loading_min_changed_ratio, 0.02, places=6)
        self.assertEqual(stub.request.required_stable_transitions, 2)
        self.assertEqual(result.state, "loading")
        self.assertEqual(result.compared_transitions, 2)
        self.assertEqual(result.latest_diff.changed_pixels, 2)
        self.assertEqual(result.changed_bounds, Rect(x=1, y=1, width=2, height=1))

    @unittest.skipUnless(_protobuf_available(), "protobuf runtime dependencies are not installed")
    def test_python_client_maps_generated_detect_ui_elements_response(self) -> None:
        from peekaboox.v1 import peekaboox_pb2

        class Stub:
            def __init__(self) -> None:
                self.request = None

            def DetectUiElements(self, request, timeout):
                self.request = request
                return peekaboox_pb2.DetectUiElementsResponse(
                    backend_name="heuristic_vision",
                    backend_kind="vision",
                    warnings=["low contrast"],
                    elements=[
                        peekaboox_pb2.UiElement(
                            id="vision:0:10:20:100:40",
                            role="visual-region",
                            bounds=peekaboox_pb2.Rect(x=10, y=20, width=100, height=40),
                            confidence=0.86,
                            states=["visible"],
                        )
                    ],
                )

        stub = Stub()
        client = PeekabooXClient(stub=stub, messages=peekaboox_pb2)

        result = client.detect_ui_elements(
            b"image",
            region=Rect(x=10, y=20, width=100, height=40),
            edge_threshold=24,
            min_width=8,
            min_height=8,
            min_component_pixels=12,
            max_elements=25,
            merge_distance=2,
        )

        self.assertEqual(stub.request.image, b"image")
        self.assertEqual(stub.request.region.x, 10)
        self.assertEqual(stub.request.edge_threshold, 24)
        self.assertEqual(stub.request.min_width, 8)
        self.assertEqual(stub.request.min_height, 8)
        self.assertEqual(stub.request.min_component_pixels, 12)
        self.assertEqual(stub.request.max_elements, 25)
        self.assertEqual(stub.request.merge_distance, 2)
        self.assertEqual(result.backend_name, "heuristic_vision")
        self.assertEqual(result.backend_kind, "vision")
        self.assertEqual(result.warnings, ("low contrast",))
        self.assertEqual(result.elements[0].bounds.width, 100)
        self.assertEqual(result.elements[0].states, ("visible",))


if __name__ == "__main__":
    unittest.main()
