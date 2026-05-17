from runtime_support import *  # noqa: F401,F403
from runtime_support import _protobuf_available


class McpTests(unittest.TestCase):
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
        hotkey_schema = server.tools["hotkey"].input_schema["properties"]
        self.assertEqual(hotkey_schema["backend"]["enum"], ["auto", "ydotool", "xdotool"])
        self.assertEqual(hotkey_schema["repeat"]["minimum"], 1)
        self.assertIn("release_before", hotkey_schema)
        self.assertIn("release_after", hotkey_schema)
        self.assertIn("outputSchema", server.tools["capture_screen"].descriptor())
        for tool_name in (
            "desktop_focus",
            "desktop_click",
            "desktop_drag",
            "desktop_type_into",
        ):
            desktop_action_output = server.tools[tool_name].descriptor()["outputSchema"]
            self.assertIn("focus_diagnostics", desktop_action_output["properties"])
            self.assertIn("focus_diagnostics", desktop_action_output["required"])
        self.assertIn("annotations", server.tools["click"].descriptor())
        self.assertTrue(server.tools["capture_screen"].descriptor()["annotations"]["readOnlyHint"])
        self.assertTrue(server.tools["click"].descriptor()["annotations"]["destructiveHint"])
        click_schema = server.tools["click"].input_schema["properties"]
        self.assertIn("region", click_schema)
        self.assertIn("ratio_x", click_schema)
        self.assertIn("backend", click_schema)
        self.assertIn("bounds_policy", click_schema)
        self.assertIn("restore", click_schema)
        capture_schema = server.tools["capture_screen"].input_schema["properties"]
        self.assertIn("app", capture_schema)
        self.assertIn("window_title", capture_schema)
        self.assertIn("title_regex", capture_schema)
        compare_schema = server.tools["compare_images"].input_schema["properties"]
        self.assertIn("ignore_regions", compare_schema)
        self.assertIn("max_changed_pixels", compare_schema)
        self.assertIn("max_mean_absolute_error", compare_schema)
        self.assertIn("max_channel_delta", compare_schema)
        self.assertIn("size_policy", compare_schema)
        self.assertIn("alpha", compare_schema)
        state_schema = server.tools["detect_ui_state"].input_schema["properties"]
        self.assertIn("ignore_regions", state_schema)
        self.assertIn("stable_max_changed_pixels", state_schema)
        self.assertIn("stable_max_mean_absolute_error", state_schema)
        self.assertIn("stable_max_channel_delta", state_schema)
        self.assertIn("loading_min_changed_pixels", state_schema)
        self.assertIn("size_policy", state_schema)
        self.assertIn("alpha", state_schema)
        vision_schema = server.tools["detect_ui_elements"].input_schema["properties"]
        self.assertIn("ignore_regions", vision_schema)
        self.assertIn("min_confidence", vision_schema)
        self.assertIn("max_width", vision_schema)
        self.assertIn("max_height", vision_schema)
        self.assertIn("min_area", vision_schema)
        self.assertIn("max_area", vision_schema)
        self.assertIn("padding", vision_schema)
        self.assertIn("sort", vision_schema)
        self.assertIn("mask_output_path", vision_schema)
        self.assertIn("overlay_output_path", vision_schema)
        window_schema = server.tools["list_windows"].input_schema["properties"]
        self.assertIn("title_regex", window_schema)
        self.assertIn("diagnose", window_schema)
        self.assertEqual(window_schema["limit"]["minimum"], 1)

    def test_mcp_input_schemas_are_codex_function_compatible(self) -> None:
        server = McpServer()
        server.register_default_tools()

        unsupported_top_level_keywords = {"allOf", "anyOf", "enum", "not", "oneOf"}
        for tool in server.list_tools():
            schema = tool["inputSchema"]
            found = unsupported_top_level_keywords.intersection(schema)
            self.assertFalse(found, f"{tool['name']} has unsupported top-level keys: {found}")

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
        typed = server.call_tool(
            "type_text",
            {
                "text": "Hello",
                "typing_speed_chars_per_second": 20,
                "dry_run": True,
                "backend": "wtype",
                "delay_ms": 10,
            },
        )
        pasted = server.call_tool(
            "paste_text",
            {
                "text": "World",
                "preserve_clipboard": True,
                "dry_run": True,
                "clipboard_backend": "xclip",
                "hotkey_backend": "xdotool",
                "delay_ms": 30,
                "restore_delay_ms": 70,
                "restore_policy": "best-effort",
            },
        )
        hotkey = server.call_tool(
            "hotkey",
            {
                "keys": ["control+s"],
                "dry_run": True,
                "backend": "auto",
                "delay_ms": 25,
                "key_delay_ms": 30,
                "repeat": 2,
                "interval_ms": 40,
                "release_before": True,
                "release_after": True,
            },
        )
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
        self.assertEqual(fake_client.last_type_options["typing_speed_chars_per_second"], 20)
        self.assertTrue(fake_client.last_type_options["dry_run"])
        self.assertEqual(fake_client.last_type_options["backend"], "wtype")
        self.assertEqual(fake_client.last_type_options["delay_ms"], 10)
        self.assertEqual(typed["message"], "typed 5 chars")
        self.assertEqual(fake_client.pasted_text, "World")
        self.assertTrue(fake_client.preserve_clipboard)
        self.assertTrue(fake_client.last_paste_options["dry_run"])
        self.assertEqual(fake_client.last_paste_options["clipboard_backend"], "xclip")
        self.assertEqual(fake_client.last_paste_options["hotkey_backend"], "xdotool")
        self.assertEqual(fake_client.last_paste_options["delay_ms"], 30)
        self.assertEqual(fake_client.last_paste_options["restore_delay_ms"], 70)
        self.assertEqual(fake_client.last_paste_options["restore_policy"], "best-effort")
        self.assertEqual(pasted["message"], "pasted 5 chars")
        self.assertTrue(hotkey["ok"])
        self.assertEqual(fake_client.hotkeys[-1], ("ctrl", "s"))
        self.assertTrue(fake_client.last_hotkey_options["dry_run"])
        self.assertEqual(fake_client.last_hotkey_options["backend"], "auto")
        self.assertEqual(fake_client.last_hotkey_options["delay_ms"], 25)
        self.assertEqual(fake_client.last_hotkey_options["key_delay_ms"], 30)
        self.assertEqual(fake_client.last_hotkey_options["repeat"], 2)
        self.assertEqual(fake_client.last_hotkey_options["interval_ms"], 40)
        self.assertTrue(fake_client.last_hotkey_options["release_before"])
        self.assertTrue(fake_client.last_hotkey_options["release_after"])
        self.assertEqual(state["active_window"]["title"], "Terminal")
        self.assertEqual(desktop_focus["action"], "focus")
        self.assertEqual(
            desktop_focus["focus_diagnostics"],
            ["windows: selected fake-window", "verify: fake-window focused"],
        )
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
            {
                "image_paths": ["first.png", "second.png"],
                "ignore_regions": [{"x": 1, "y": 2, "width": 3, "height": 4}],
                "stable_max_changed_pixels": 3,
                "stable_max_mean_absolute_error": 1.5,
                "stable_max_channel_delta": 8,
                "loading_min_changed_pixels": 4,
                "size_policy": "common-region",
                "alpha": "compare",
            },
        )
        ui_elements = server.call_tool(
            "detect_ui_elements",
            {
                "image_path": "screen.png",
                "region": {"x": 1, "y": 2, "width": 3, "height": 4},
                "ignore_regions": [{"x": 9, "y": 9, "width": 2, "height": 2}],
                "min_confidence": 0.5,
                "max_width": 20,
                "max_height": 20,
                "min_area": 4,
                "max_area": 400,
                "padding": 2,
                "sort": "confidence",
                "mask_output_path": "target/mask.png",
                "overlay_output_path": "target/overlay.png",
            },
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
        self.assertIn(
            "message-input",
            [target["name"] for target in profiles["profiles"][0]["targets"]],
        )

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

    def test_mcp_server_returns_desktop_action_diagnostics_in_structured_content(self) -> None:
        server = McpServer(runtime=AgentRuntime(client=FakeClient()))
        server.register_default_tools()

        calls = (
            (
                "desktop_focus",
                {"app": "telegram", "verify": True},
                "focus",
            ),
            (
                "desktop_click",
                {"app": "telegram", "target": "search-input", "verify": True},
                "click",
            ),
            (
                "desktop_drag",
                {
                    "app": "paint",
                    "target": "canvas",
                    "from_ratio": [0.2, 0.5],
                    "to_ratio": [0.8, 0.5],
                    "verify": True,
                },
                "drag",
            ),
            (
                "desktop_type_into",
                {
                    "app": "telegram",
                    "target": "message-input",
                    "text": "PeekabooX",
                    "verify": True,
                },
                "type-into",
            ),
        )

        for index, (tool_name, arguments, action) in enumerate(calls, start=4):
            with self.subTest(tool_name=tool_name):
                response = server.handle_jsonrpc(
                    {
                        "jsonrpc": "2.0",
                        "id": index,
                        "method": "tools/call",
                        "params": {
                            "name": tool_name,
                            "arguments": arguments,
                        },
                    }
                )

                self.assertFalse(response["result"]["isError"])
                structured = response["result"]["structuredContent"]
                self.assertEqual(structured["action"], action)
                self.assertEqual(
                    structured["focus_diagnostics"],
                    ["windows: selected fake-window", "verify: fake-window focused"],
                )
                text_payload = json.loads(response["result"]["content"][0]["text"])
                self.assertEqual(
                    text_payload["focus_diagnostics"],
                    structured["focus_diagnostics"],
                )

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

    def test_mcp_create_server_passes_client_timeout_to_runtime(self) -> None:
        runtime = AgentRuntime(client=FakeClient())
        with patch(
            "peekaboox.mcp.server.AgentRuntime.connect",
            return_value=runtime,
        ) as connect:
            server = create_server(
                "127.0.0.1:47777",
                client_timeout_seconds=12.5,
            )

        self.assertIs(server.runtime, runtime)
        self.assertEqual(connect.call_args.kwargs["client_timeout_seconds"], 12.5)

    def test_mcp_create_server_passes_grpc_token_to_runtime(self) -> None:
        runtime = AgentRuntime(client=FakeClient())
        with patch(
            "peekaboox.mcp.server.AgentRuntime.connect",
            return_value=runtime,
        ) as connect:
            server = create_server(
                "127.0.0.1:47777",
                grpc_token="secret-token",
            )

        self.assertIs(server.runtime, runtime)
        self.assertEqual(connect.call_args.kwargs["grpc_token"], "secret-token")

    def test_mcp_http_auth_helpers_accept_bearer_and_custom_headers(self) -> None:
        self.assertTrue(
            mcp_server_module._mcp_http_request_authorized(
                {"Authorization": "Bearer secret-token"},
                "secret-token",
            )
        )
        self.assertTrue(
            mcp_server_module._mcp_http_request_authorized(
                {"X-PeekabooX-MCP-Token": "secret-token"},
                "secret-token",
            )
        )
        self.assertFalse(
            mcp_server_module._mcp_http_request_authorized(
                {"Authorization": "Bearer wrong-token"},
                "secret-token",
            )
        )

    def test_mcp_http_host_policy_requires_auth_for_non_loopback(self) -> None:
        self.assertFalse(mcp_server_module._mcp_http_host_requires_auth("127.0.0.1"))
        self.assertFalse(mcp_server_module._mcp_http_host_requires_auth("::1"))
        self.assertFalse(mcp_server_module._mcp_http_host_requires_auth("localhost"))
        self.assertTrue(mcp_server_module._mcp_http_host_requires_auth("0.0.0.0"))
        self.assertTrue(mcp_server_module._mcp_http_host_requires_auth("192.168.1.5"))

    def test_mcp_http_content_length_enforces_limit(self) -> None:
        self.assertEqual(mcp_server_module._mcp_http_content_length("12", 12), 12)
        with self.assertRaises(OverflowError):
            mcp_server_module._mcp_http_content_length("13", 12)
        with self.assertRaises(ValueError):
            mcp_server_module._mcp_http_content_length(None, 12)
        with self.assertRaises(ValueError):
            mcp_server_module._mcp_http_content_length("-1", 12)

    def test_mcp_http_server_rejects_non_loopback_without_auth(self) -> None:
        server = McpServer(runtime=AgentRuntime(client=FakeClient()))

        with self.assertRaisesRegex(ValueError, "unauthenticated MCP HTTP/SSE"):
            server.serve_http("0.0.0.0", 0)

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
