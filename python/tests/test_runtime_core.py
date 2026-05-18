from runtime_support import *  # noqa: F401,F403
from runtime_support import _protobuf_available


class RuntimeCoreTests(unittest.TestCase):
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
        self.assertTrue(
            runtime.hotkey(
                ["control+s"],
                dry_run=True,
                backend="auto",
                delay_ms=25,
                key_delay_ms=30,
                repeat=2,
                interval_ms=40,
                release_before=True,
                release_after=True,
            ).ok
        )
        self.assertEqual(fake_client.hotkeys[-1], ("ctrl", "s"))
        self.assertTrue(fake_client.last_hotkey_options["dry_run"])
        self.assertEqual(fake_client.last_hotkey_options["backend"], "auto")
        self.assertEqual(fake_client.last_hotkey_options["delay_ms"], 25)
        self.assertEqual(fake_client.last_hotkey_options["key_delay_ms"], 30)
        self.assertEqual(fake_client.last_hotkey_options["repeat"], 2)
        self.assertEqual(fake_client.last_hotkey_options["interval_ms"], 40)
        self.assertTrue(fake_client.last_hotkey_options["release_before"])
        self.assertTrue(fake_client.last_hotkey_options["release_after"])
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

    def test_process_plugin_tool_limits_output_while_draining_pipe(self) -> None:
        with TemporaryDirectory() as tmpdir:
            plugin_dir = Path(tmpdir) / "demo"
            plugin_dir.mkdir()
            (plugin_dir / "plugin.py").write_text(
                "import sys\nsys.stdin.read()\nsys.stdout.write('x' * 2048)\n",
                encoding="utf-8",
            )
            (plugin_dir / PLUGIN_MANIFEST_FILE).write_text(
                json.dumps(
                    {
                        "schema_version": PLUGIN_SDK_VERSION,
                        "id": "limit.demo",
                        "name": "Limit Demo",
                        "version": "1.0.0",
                        "entrypoint": {
                            "kind": "process",
                            "command": [sys.executable, "plugin.py"],
                        },
                        "tools": [
                            {
                                "name": "limit.out",
                                "description": "Emit output",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )
            plugin = discover_plugins([Path(tmpdir)]).plugins[0]

            result = execute_plugin_tool(plugin, "limit.out", {}, max_output_bytes=16)

        self.assertFalse(result.ok)
        self.assertEqual(len(result.stdout), 16)
        self.assertEqual(result.error, "plugin output exceeded max_output_bytes=16")

    def test_process_plugin_tool_timeout_covers_blocked_stdin_write(self) -> None:
        with TemporaryDirectory() as tmpdir:
            plugin_dir = Path(tmpdir) / "demo"
            plugin_dir.mkdir()
            (plugin_dir / "plugin.py").write_text(
                "import time\ntime.sleep(2)\n",
                encoding="utf-8",
            )
            (plugin_dir / PLUGIN_MANIFEST_FILE).write_text(
                json.dumps(
                    {
                        "schema_version": PLUGIN_SDK_VERSION,
                        "id": "timeout.demo",
                        "name": "Timeout Demo",
                        "version": "1.0.0",
                        "entrypoint": {
                            "kind": "process",
                            "command": [sys.executable, "plugin.py"],
                        },
                        "tools": [
                            {
                                "name": "timeout.sleep",
                                "description": "Sleep",
                                "input_schema": {
                                    "type": "object",
                                    "properties": {"payload": {"type": "string"}},
                                    "additionalProperties": False,
                                },
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )
            plugin = discover_plugins([Path(tmpdir)]).plugins[0]

            result = execute_plugin_tool(
                plugin,
                "timeout.sleep",
                {"payload": "x" * 2_000_000},
                timeout_seconds=0.05,
            )

        self.assertFalse(result.ok)
        self.assertEqual(result.exit_code, -1)
        self.assertIn("timed out", result.error or "")

    def test_plugin_trust_policy_gates_execution(self) -> None:
        with TemporaryDirectory() as tmpdir:
            plugin_dir = Path(tmpdir) / "demo"
            plugin_dir.mkdir()
            (plugin_dir / "plugin.py").write_text(
                "import json, sys\nsys.stdin.read()\njson.dump({'ok': True, 'result': 7}, sys.stdout)\n",
                encoding="utf-8",
            )
            (plugin_dir / PLUGIN_MANIFEST_FILE).write_text(
                json.dumps(
                    {
                        "schema_version": PLUGIN_SDK_VERSION,
                        "id": "trusted.demo",
                        "name": "Trusted Demo",
                        "version": "1.0.0",
                        "entrypoint": {
                            "kind": "process",
                            "command": [sys.executable, "plugin.py"],
                        },
                        "tools": [{"name": "trusted.run", "description": "Run"}],
                    }
                ),
                encoding="utf-8",
            )
            plugin = discover_plugins([Path(tmpdir)]).plugins[0]
            trust_policy = Path(tmpdir) / "trusted_plugins.json"

            with self.assertRaisesRegex(ValueError, "not trusted"):
                execute_plugin_tool(
                    plugin,
                    "trusted.run",
                    {},
                    require_trusted=True,
                    trust_policy_path=trust_policy,
                )

            trusted = trust_plugin(plugin, trust_policy)
            verified = verify_plugin_trust(plugin, trust_policy_path=trust_policy)
            result = execute_plugin_tool(
                plugin,
                "trusted.run",
                {},
                require_trusted=True,
                trust_policy_path=trust_policy,
            )

        self.assertTrue(trusted.trusted)
        self.assertTrue(verified.trusted)
        self.assertTrue(result.ok)
        self.assertEqual(result.result, 7)

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

    def test_agent_cli_passes_grpc_token_to_runtime(self) -> None:
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
                    "--grpc-token",
                    "secret-token",
                    "windows",
                ]
            )

        self.assertEqual(exit_code, 0)
        self.assertEqual(connect.call_args.kwargs["grpc_token"], "secret-token")

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

    def test_memory_store_compacts_desktop_graph_snapshots(self) -> None:
        store = MemoryStore()
        for index in range(3):
            store.ingest_desktop_state(
                FakeClient().get_desktop_state(),
                snapshot_id=f"snapshot:{index}",
                captured_at_unix_ms=100 + index,
            )

        removed = store.compact_desktop_graph(max_snapshots=1)
        status = store.desktop_graph_status()

        self.assertEqual(removed, 2)
        self.assertEqual(status.latest_snapshot_id, "snapshot:2")
        self.assertEqual(status.snapshot_count, 1)
        self.assertGreater(status.node_count, 0)

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
            restored.compact_desktop_graph(max_snapshots=1)
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
                WorkflowStep(
                    action="type_text",
                    value="Hello",
                    typing_speed_chars_per_second=20,
                    delay_ms=10,
                    backend="wtype",
                ),
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
        self.assertEqual(fake_client.last_type_options["typing_speed_chars_per_second"], 20)
        self.assertEqual(fake_client.last_type_options["delay_ms"], 10)
        self.assertEqual(fake_client.last_type_options["backend"], "wtype")
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
                WorkflowStep(
                    action="hotkey",
                    value="ctrl+s",
                    dry_run=True,
                    backend="xdotool",
                    delay_ms=25,
                    key_delay_ms=30,
                    repeat=2,
                    interval_ms=40,
                    release_before=True,
                    release_after=True,
                    verify=False,
                ),
            ],
        )

        result = runtime.execute_workflow(workflow)

        self.assertTrue(result.ok)
        self.assertEqual(fake_client.moved_to, (10, 20))
        self.assertEqual(fake_client.dragged, (10, 20, 30, 40, "right", 100))
        self.assertEqual(fake_client.hotkeys[-1], ("ctrl", "s"))
        self.assertTrue(fake_client.last_hotkey_options["dry_run"])
        self.assertEqual(fake_client.last_hotkey_options["backend"], "xdotool")
        self.assertEqual(fake_client.last_hotkey_options["delay_ms"], 25)
        self.assertEqual(fake_client.last_hotkey_options["key_delay_ms"], 30)
        self.assertEqual(fake_client.last_hotkey_options["repeat"], 2)
        self.assertEqual(fake_client.last_hotkey_options["interval_ms"], 40)
        self.assertTrue(fake_client.last_hotkey_options["release_before"])
        self.assertTrue(fake_client.last_hotkey_options["release_after"])

    def test_agent_runtime_executes_extended_workflow_actions(self) -> None:
        fake_client = FakeClient()
        runtime = AgentRuntime(client=fake_client)
        workflow = Workflow(
            name="extended-actions",
            steps=[
                WorkflowStep(action="ocr_screen", expected_text="Submit"),
                WorkflowStep(action="assert_text", expected_text="Submit"),
                WorkflowStep(
                    action="compare_images",
                    expected_path="expected.png",
                    actual_path="actual.png",
                    max_changed_ratio=0.01,
                ),
                WorkflowStep(
                    action="detect_ui_state",
                    image_paths=("one.png", "two.png"),
                    required_stable_transitions=1,
                ),
                WorkflowStep(action="detect_ui_elements", image_path="screen.png"),
                WorkflowStep(action="desktop_focus", app="telegram", verify=False),
                WorkflowStep(
                    action="desktop_type_into",
                    app="telegram",
                    target="search-input",
                    value="Saved Messages",
                    dry_run=True,
                    verify=False,
                ),
                WorkflowStep(action="wait", sleep_ms=0),
            ],
        )

        result = runtime.execute_workflow(workflow)

        self.assertTrue(result.ok)
        self.assertEqual(len(result.steps), 8)
        self.assertEqual(fake_client.desktop_calls[0][0], "focus")
        self.assertEqual(fake_client.desktop_calls[1][0], "type_into")

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

    def test_agent_runtime_execute_goal_rejects_observe_only_action_goal(self) -> None:
        class ObserveOnlyPlanner(PlanningEngine):
            def plan_workflow(self, goal: str) -> Workflow:
                return Workflow(name=goal, steps=[WorkflowStep(action="observe")])

        runtime = AgentRuntime(client=FakeClient(), planner=ObserveOnlyPlanner())

        result = runtime.execute_goal("Click Submit")

        self.assertFalse(result.ok)
        self.assertEqual(result.steps[0].step.action, "observe")
        self.assertIn("observation", result.recovery["reason"])

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
        self.assertEqual(json_roundtrip.schema_version, WORKFLOW_SCHEMA_VERSION)
        self.assertEqual(yaml_roundtrip.schema_version, WORKFLOW_SCHEMA_VERSION)
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

        with self.assertRaisesRegex(ValueError, "not supported"):
            runtime.refine_workflow("Run shell")

    def test_workflow_loader_reads_json_and_yaml_definitions(self) -> None:
        json_workflow = load_workflow_text(
            json.dumps(
                {
                    "schema_version": WORKFLOW_SCHEMA_VERSION,
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
            schema_version: peekaboox.workflow.v1
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
        self.assertEqual(json_workflow.schema_version, WORKFLOW_SCHEMA_VERSION)
        self.assertEqual(json_workflow.steps[1].value, "Hello")
        self.assertEqual(json_workflow.steps[2].from_x, 1)
        self.assertEqual(json_workflow.steps[2].button, "middle")
        self.assertEqual(json_workflow.steps[2].duration_ms, 150)
        self.assertEqual(yaml_workflow.name, "yaml-submit")
        self.assertEqual(yaml_workflow.schema_version, WORKFLOW_SCHEMA_VERSION)
        self.assertTrue(yaml_workflow.steps[1].vision_fallback)
        self.assertFalse(yaml_workflow.steps[1].verify)

    def test_workflow_loader_migrates_legacy_workflows_and_rejects_invalid_contracts(self) -> None:
        legacy_workflow = load_workflow_text(
            json.dumps(
                {
                    "name": "legacy",
                    "steps": [{"action": "observe"}],
                }
            ),
            format_name="json",
        )

        self.assertEqual(legacy_workflow.schema_version, WORKFLOW_SCHEMA_VERSION)
        self.assertEqual(
            json.loads(dump_workflow_text(legacy_workflow, format_name="json"))[
                "schema_version"
            ],
            WORKFLOW_SCHEMA_VERSION,
        )

        with self.assertRaisesRegex(ValueError, "unsupported top-level keys"):
            load_workflow_text(
                json.dumps(
                    {
                        "schema_version": WORKFLOW_SCHEMA_VERSION,
                        "name": "bad",
                        "steps": [{"action": "observe"}],
                        "unknown": True,
                    }
                ),
                format_name="json",
            )
        with self.assertRaisesRegex(ValueError, "unsupported keys"):
            load_workflow_text(
                json.dumps(
                    {
                        "schema_version": WORKFLOW_SCHEMA_VERSION,
                        "name": "bad",
                        "steps": [{"action": "observe", "unknown": True}],
                    }
                ),
                format_name="json",
            )
        with self.assertRaisesRegex(ValueError, "click requires"):
            load_workflow_text(
                json.dumps(
                    {
                        "schema_version": WORKFLOW_SCHEMA_VERSION,
                        "name": "bad",
                        "steps": [{"action": "click"}],
                    }
                ),
                format_name="json",
            )

    def test_workflow_json_schema_exposes_current_contract(self) -> None:
        schema = workflow_json_schema()

        self.assertEqual(schema["properties"]["schema_version"]["const"], WORKFLOW_SCHEMA_VERSION)
        self.assertFalse(schema["additionalProperties"])
        self.assertIn("click", schema["properties"]["steps"]["items"]["properties"]["action"]["enum"])
        self.assertIn(
            "desktop_type_into",
            schema["properties"]["steps"]["items"]["properties"]["action"]["enum"],
        )
        self.assertIn(
            "plugin_call",
            schema["properties"]["steps"]["items"]["properties"]["action"]["enum"],
        )

    def test_agent_runtime_lists_and_prints_workflow_templates(self) -> None:
        runtime = AgentRuntime(client=FakeClient())

        templates = runtime.list_workflow_templates(category="vision")
        plugin_template = runtime.workflow_template_info("plugin-tool-call")

        self.assertTrue(any(template["id"] == "ocr-visible-text" for template in templates))
        self.assertEqual(plugin_template["workflow"]["steps"][0]["action"], "plugin_call")

        output = StringIO()
        with patch("sys.stdout", output):
            exit_code = agent_runtime_module.main(["workflow", "templates"])

        self.assertEqual(exit_code, 0)
        self.assertIn("observe-desktop category=observe", output.getvalue())

    def test_agent_cli_prints_workflow_template_yaml(self) -> None:
        output = StringIO()
        with patch("sys.stdout", output):
            exit_code = agent_runtime_module.main(
                ["workflow", "template", "semantic-click", "--format", "yaml"]
            )

        self.assertEqual(exit_code, 0)
        self.assertIn("name: 'semantic-click'", output.getvalue())

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
        recorder.record_step(
            WorkflowStep(
                action="type_text",
                value="true",
                typing_speed_chars_per_second=25,
                delay_ms=5,
                backend="ydotool",
            )
        )
        recorder.record_step(
            WorkflowStep(
                action="paste_text",
                value="pasted",
                preserve_clipboard=True,
                dry_run=True,
                clipboard_backend="xclip",
                hotkey_backend="xdotool",
                delay_ms=30,
                restore_delay_ms=70,
                restore_policy="best-effort",
            )
        )
        recorder.record_step(
            WorkflowStep(
                action="hotkey",
                value="ctrl+s",
                dry_run=True,
                backend="ydotool",
                delay_ms=25,
                key_delay_ms=30,
                repeat=2,
                interval_ms=40,
                release_before=True,
                release_after=True,
            )
        )

        json_workflow = load_workflow_text(recorder.to_json(), format_name="json")
        yaml_workflow = load_workflow_text(recorder.to_yaml(), format_name="yaml")

        self.assertEqual(json_workflow.name, "recorded")
        self.assertTrue(json_workflow.steps[1].vision_fallback)
        self.assertEqual(yaml_workflow.steps[0].selector, "role=push button")
        self.assertEqual(yaml_workflow.steps[2].value, "true")
        self.assertEqual(yaml_workflow.steps[2].typing_speed_chars_per_second, 25)
        self.assertEqual(yaml_workflow.steps[2].delay_ms, 5)
        self.assertEqual(yaml_workflow.steps[2].backend, "ydotool")
        self.assertTrue(yaml_workflow.steps[3].preserve_clipboard)
        self.assertTrue(yaml_workflow.steps[3].dry_run)
        self.assertEqual(yaml_workflow.steps[3].clipboard_backend, "xclip")
        self.assertEqual(yaml_workflow.steps[3].hotkey_backend, "xdotool")
        self.assertEqual(yaml_workflow.steps[3].delay_ms, 30)
        self.assertEqual(yaml_workflow.steps[3].restore_delay_ms, 70)
        self.assertEqual(yaml_workflow.steps[3].restore_policy, "best-effort")
        self.assertEqual(yaml_workflow.steps[4].value, "ctrl+s")
        self.assertTrue(yaml_workflow.steps[4].dry_run)
        self.assertEqual(yaml_workflow.steps[4].backend, "ydotool")
        self.assertEqual(yaml_workflow.steps[4].delay_ms, 25)
        self.assertEqual(yaml_workflow.steps[4].key_delay_ms, 30)
        self.assertEqual(yaml_workflow.steps[4].repeat, 2)
        self.assertEqual(yaml_workflow.steps[4].interval_ms, 40)
        self.assertTrue(yaml_workflow.steps[4].release_before)
        self.assertTrue(yaml_workflow.steps[4].release_after)

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

    def test_workflow_bundle_writes_replay_artifacts(self) -> None:
        workflow = Workflow(
            name="bundle-submit",
            steps=[
                WorkflowStep(action="find_element", selector="role=push button"),
                WorkflowStep(action="click", selector="role=push button,label=Submit"),
            ],
        )
        doctor = DoctorResult(
            status="ok",
            checks=(),
            categories=(),
            ok_count=0,
            warn_count=0,
            fail_count=0,
            exit_code=0,
        )

        with TemporaryDirectory() as tmpdir:
            bundle = create_workflow_bundle(
                workflow,
                Path(tmpdir) / "bundle",
                source_path="workflow.yaml",
                doctor_result=doctor,
            )
            metadata = json.loads((bundle / "metadata.json").read_text(encoding="utf-8"))
            normalized = json.loads(
                (bundle / "workflow.normalized.json").read_text(encoding="utf-8")
            )

            self.assertTrue((bundle / "workflow.json").is_file())
            self.assertTrue((bundle / "workflow.yaml").is_file())
            self.assertTrue((bundle / "doctor.json").is_file())
            self.assertEqual(metadata["workflow"], "bundle-submit")
            self.assertEqual(metadata["source_path"], "workflow.yaml")
            self.assertEqual(normalized["steps"][1]["selector"], "role=push button,label=Submit")

    def test_agent_cli_validates_workflow_file(self) -> None:
        with TemporaryDirectory() as tmpdir:
            workflow_path = Path(tmpdir) / "workflow.yaml"
            workflow_path.write_text(
                "name: cli-workflow\nsteps:\n  - action: observe\n",
                encoding="utf-8",
            )
            output = StringIO()
            with patch("sys.stdout", output):
                exit_code = agent_runtime_module.main(["workflow", "validate", str(workflow_path)])

        self.assertEqual(exit_code, 0)
        payload = json.loads(output.getvalue())
        self.assertTrue(payload["ok"])
        self.assertEqual(payload["workflow"]["name"], "cli-workflow")
        self.assertEqual(payload["workflow"]["schema_version"], WORKFLOW_SCHEMA_VERSION)

    def test_agent_cli_prints_workflow_schema(self) -> None:
        output = StringIO()
        with patch("sys.stdout", output):
            exit_code = agent_runtime_module.main(["workflow", "schema"])

        self.assertEqual(exit_code, 0)
        payload = json.loads(output.getvalue())
        self.assertEqual(payload["properties"]["schema_version"]["const"], WORKFLOW_SCHEMA_VERSION)

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
        runtime.type_text(
            "Hello",
            typing_speed_chars_per_second=30,
            dry_run=True,
            backend="xdotool",
            delay_ms=5,
        )
        workflow = runtime.stop_recording()

        self.assertEqual(
            [step.action for step in workflow.steps],
            ["find_element", "click", "type_text"],
        )
        self.assertEqual(workflow.steps[1].selector, "role=push button,label=Submit")
        self.assertTrue(workflow.steps[1].vision_fallback)
        self.assertEqual(workflow.steps[2].typing_speed_chars_per_second, 30)
        self.assertTrue(workflow.steps[2].dry_run)
        self.assertEqual(workflow.steps[2].backend, "xdotool")
        self.assertEqual(workflow.steps[2].delay_ms, 5)

        with TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "recording.yaml"
            saved = runtime.save_recording(path)
            loaded = load_workflow_file(saved)

        self.assertEqual(loaded.name, "manual")
        self.assertEqual(loaded.steps[2].value, "Hello")
        self.assertEqual(loaded.steps[2].typing_speed_chars_per_second, 30)
        self.assertTrue(loaded.steps[2].dry_run)

    def test_agent_runtime_records_pointer_and_hotkey_actions(self) -> None:
        runtime = AgentRuntime(client=FakeClient())

        runtime.start_recording("input")
        runtime.move_mouse(10, 20)
        runtime.drag(10, 20, 30, 40, button="middle", duration_ms=125)
        runtime.hotkey(
            "control+s",
            dry_run=True,
            backend="ydotool",
            delay_ms=25,
            key_delay_ms=30,
            repeat=2,
            interval_ms=40,
            release_before=True,
            release_after=True,
        )
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
        self.assertTrue(workflow.steps[2].dry_run)
        self.assertEqual(workflow.steps[2].backend, "ydotool")
        self.assertEqual(workflow.steps[2].delay_ms, 25)
        self.assertEqual(workflow.steps[2].key_delay_ms, 30)
        self.assertEqual(workflow.steps[2].repeat, 2)
        self.assertEqual(workflow.steps[2].interval_ms, 40)
        self.assertTrue(workflow.steps[2].release_before)
        self.assertTrue(workflow.steps[2].release_after)

    def test_agent_runtime_requires_client_for_rpc_calls(self) -> None:
        runtime = AgentRuntime()

        with self.assertRaisesRegex(RuntimeError, "PeekabooXClient"):
            runtime.list_windows()
