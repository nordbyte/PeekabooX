from runtime_support import *  # noqa: F401,F403
from runtime_support import _protobuf_available


class ClientTests(unittest.TestCase):
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
    def test_python_client_sends_grpc_token_metadata(self) -> None:
        from peekaboox.v1 import peekaboox_pb2

        class Stub:
            def __init__(self) -> None:
                self.metadata = None

            def ListWindows(self, request, timeout, metadata=None):
                self.metadata = metadata
                return peekaboox_pb2.ListWindowsResponse(
                    backend_name="test",
                    backend_kind="x11",
                )

        stub = Stub()
        client = PeekabooXClient(
            stub=stub,
            messages=peekaboox_pb2,
            grpc_token=" secret-token ",
        )

        client.list_windows()

        self.assertEqual(stub.metadata, (("x-peekaboox-token", "secret-token"),))

    @unittest.skipUnless(_protobuf_available(), "protobuf runtime dependencies are not installed")
    def test_python_client_loads_grpc_token_from_env(self) -> None:
        from peekaboox.v1 import peekaboox_pb2

        class Stub:
            def __init__(self) -> None:
                self.metadata = None

            def ListWindows(self, request, timeout, metadata=None):
                self.metadata = metadata
                return peekaboox_pb2.ListWindowsResponse(
                    backend_name="test",
                    backend_kind="x11",
                )

        stub = Stub()
        with patch.dict("os.environ", {"PEEKABOOX_GRPC_TOKEN": "env-token"}):
            client = PeekabooXClient(stub=stub, messages=peekaboox_pb2)

        client.list_windows()

        self.assertEqual(stub.metadata, (("x-peekaboox-token", "env-token"),))

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
                    focus_diagnostics=["windows: selected fake", "verify: focused"],
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

            def DesktopProfiles(self, request, timeout):
                self.requests.append(("profiles", request))
                return peekaboox_pb2.DesktopProfilesResponse(
                    schema_version="desktop-profiles.v1",
                    count=1,
                    profiles=[
                        peekaboox_pb2.DesktopProfile(
                            id="telegram",
                            aliases=["telegram"],
                            search_name="Telegram",
                            desktop_ids=["org.telegram.desktop"],
                            commands=[
                                peekaboox_pb2.DesktopProfileCommand(
                                    program="flatpak",
                                    args=["run", "org.telegram.desktop"],
                                    display="flatpak run org.telegram.desktop",
                                    available=True,
                                )
                            ],
                            targets=[
                                peekaboox_pb2.DesktopProfileTarget(
                                    name="message-input",
                                    supports=["locate", "click", "type-into"],
                                    sources=["visual-layout"],
                                    can_locate=True,
                                    can_click=True,
                                    can_type=True,
                                    can_assert_present=True,
                                    visual_layout=True,
                                    visual_rect=True,
                                )
                            ],
                            availability=peekaboox_pb2.DesktopProfileAvailability(
                                checked=True,
                                installed=True,
                                command_available=True,
                                desktop_entry_available=False,
                                available_commands=["flatpak run org.telegram.desktop"],
                            ),
                        )
                    ],
                )

        stub = Stub()
        client = PeekabooXClient(stub=stub, messages=peekaboox_pb2)

        focus = client.desktop_focus("telegram", window_id="window-1", verify=True)
        self.assertEqual(focus.action, "focus")
        self.assertEqual(
            focus.focus_diagnostics,
            ["windows: selected fake", "verify: focused"],
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
        profiles = client.desktop_profiles(
            "telegram",
            supports="type-into",
            check=True,
            installed=True,
        )
        self.assertEqual(profiles.profiles[0].commands[0].display, "flatpak run org.telegram.desktop")
        self.assertTrue(profiles.profiles[0].availability.installed)

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
        self.assertEqual(stub.requests[6][1].supports, "type-into")
        self.assertTrue(stub.requests[6][1].check)
        self.assertTrue(stub.requests[6][1].installed)

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

        result = client.click(
            7,
            9,
            button="right",
            dry_run=True,
            bounds_policy="clamp",
            backend="xdotool",
            restore=True,
        )

        self.assertTrue(result.ok)
        self.assertEqual(stub.request.coordinates.x, 7)
        self.assertEqual(stub.request.coordinates.y, 9)
        self.assertFalse(stub.request.vision_fallback)
        self.assertEqual(stub.request.button, peekaboox_pb2.MOUSE_BUTTON_RIGHT)
        self.assertTrue(stub.request.dry_run)
        self.assertEqual(stub.request.bounds_policy, "clamp")
        self.assertEqual(stub.request.backend, "xdotool")
        self.assertTrue(stub.request.restore)

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

        result = client.click_selector(
            "role=push button,label=Submit",
            vision_fallback=True,
            button="middle",
            dry_run=True,
        )

        self.assertTrue(result.ok)
        self.assertEqual(stub.request.semantic_selector, "role=push button,label=Submit")
        self.assertTrue(stub.request.vision_fallback)
        self.assertEqual(stub.request.button, peekaboox_pb2.MOUSE_BUTTON_MIDDLE)
        self.assertTrue(stub.request.dry_run)

    @unittest.skipUnless(_protobuf_available(), "protobuf runtime dependencies are not installed")
    def test_python_client_builds_generated_scoped_click_request(self) -> None:
        from peekaboox.v1 import peekaboox_pb2

        class Stub:
            def __init__(self) -> None:
                self.request = None

            def Click(self, request, timeout):
                self.request = request
                return peekaboox_pb2.ActionResponse(ok=True, message="ok")

        stub = Stub()
        client = PeekabooXClient(stub=stub, messages=peekaboox_pb2)

        result = client.click(
            region=Rect(x=10, y=20, width=300, height=200),
            ratio_x=0.25,
            ratio_y=0.75,
            window_title="Calculator",
            dry_run=True,
        )

        self.assertTrue(result.ok)
        self.assertEqual(stub.request.region.x, 10)
        self.assertEqual(stub.request.ratio_x, 0.25)
        self.assertEqual(stub.request.ratio_y, 0.75)
        self.assertEqual(stub.request.window_title, "Calculator")
        self.assertTrue(stub.request.dry_run)

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

            def TypeText(self, request, timeout):
                self.requests.append(("type", request))
                return peekaboox_pb2.ActionResponse(ok=True, message="ok")

            def Hotkey(self, request, timeout):
                self.requests.append(("hotkey", request))
                return peekaboox_pb2.ActionResponse(ok=True, message="ok")

        stub = Stub()
        client = PeekabooXClient(stub=stub, messages=peekaboox_pb2)

        self.assertTrue(client.move_mouse(7, 9).ok)
        self.assertTrue(
            client.move_mouse(
                relative_x=3,
                relative_y=-2,
                dry_run=True,
                duration_ms=120,
                steps=4,
                bounds_policy="clamp",
                backend="xdotool",
                restore=True,
            ).ok
        )
        self.assertTrue(client.drag(1, 2, 3, 4, button="right", duration_ms=500).ok)
        self.assertTrue(
            client.drag(
                from_current=True,
                to_ratio=(0.8, 0.5),
                region=Rect(x=0, y=0, width=400, height=240),
                dry_run=True,
                steps=6,
                bounds_policy="clamp",
                backend="xdotool",
                restore=True,
            ).ok
        )
        self.assertTrue(
            client.type_text(
                "Hello",
                typing_speed_chars_per_second=20,
                dry_run=True,
                backend="wtype",
                delay_ms=10,
            ).ok
        )
        self.assertTrue(
            client.hotkey(
                ["control+s"],
                dry_run=True,
                backend="ydotool",
                delay_ms=25,
                key_delay_ms=30,
                repeat=2,
                interval_ms=40,
                release_before=True,
                release_after=True,
            ).ok
        )

        self.assertEqual(stub.requests[0][1].coordinates.x, 7)
        self.assertEqual(stub.requests[1][1].relative.x, 3)
        self.assertEqual(stub.requests[1][1].relative.y, -2)
        self.assertTrue(stub.requests[1][1].dry_run)
        self.assertEqual(stub.requests[1][1].duration_ms, 120)
        self.assertEqual(stub.requests[1][1].steps, 4)
        self.assertEqual(stub.requests[1][1].bounds_policy, "clamp")
        self.assertEqual(stub.requests[1][1].backend, "xdotool")
        self.assertTrue(stub.requests[1][1].restore)
        self.assertEqual(getattr(stub.requests[2][1], "from").x, 1)
        self.assertEqual(stub.requests[2][1].to.y, 4)
        self.assertEqual(stub.requests[2][1].button, peekaboox_pb2.MOUSE_BUTTON_RIGHT)
        self.assertEqual(stub.requests[2][1].duration_ms, 500)
        self.assertTrue(stub.requests[3][1].from_current)
        self.assertAlmostEqual(stub.requests[3][1].to_ratio_x, 0.8)
        self.assertAlmostEqual(stub.requests[3][1].to_ratio_y, 0.5)
        self.assertEqual(stub.requests[3][1].region.width, 400)
        self.assertTrue(stub.requests[3][1].dry_run)
        self.assertEqual(stub.requests[3][1].steps, 6)
        self.assertEqual(stub.requests[3][1].bounds_policy, "clamp")
        self.assertEqual(stub.requests[3][1].backend, "xdotool")
        self.assertTrue(stub.requests[3][1].restore)
        self.assertEqual(stub.requests[4][1].text, "Hello")
        self.assertEqual(stub.requests[4][1].typing_speed_chars_per_second, 20)
        self.assertTrue(stub.requests[4][1].dry_run)
        self.assertEqual(stub.requests[4][1].backend, "wtype")
        self.assertEqual(stub.requests[4][1].delay_ms, 10)
        self.assertEqual(list(stub.requests[5][1].keys), ["ctrl", "s"])
        self.assertTrue(stub.requests[5][1].dry_run)
        self.assertEqual(stub.requests[5][1].backend, "ydotool")
        self.assertEqual(stub.requests[5][1].delay_ms, 25)
        self.assertEqual(stub.requests[5][1].key_delay_ms, 30)
        self.assertEqual(stub.requests[5][1].repeat, 2)
        self.assertEqual(stub.requests[5][1].interval_ms, 40)
        self.assertTrue(stub.requests[5][1].release_before)
        self.assertTrue(stub.requests[5][1].release_after)

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

        paste = client.paste_text(
            "Hello",
            preserve_clipboard=True,
            dry_run=True,
            clipboard_backend="xclip",
            hotkey_backend="xdotool",
            delay_ms=30,
            restore_delay_ms=70,
            restore_policy="best-effort",
        )
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
        self.assertTrue(stub.requests[0][1].dry_run)
        self.assertEqual(stub.requests[0][1].clipboard_backend, "xclip")
        self.assertEqual(stub.requests[0][1].hotkey_backend, "xdotool")
        self.assertEqual(stub.requests[0][1].delay_ms, 30)
        self.assertEqual(stub.requests[0][1].restore_delay_ms, 70)
        self.assertEqual(stub.requests[0][1].restore_policy, "best-effort")
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
            ignore_regions=(Rect(x=1, y=1, width=2, height=1),),
            per_channel_threshold=3,
            max_changed_ratio=0.01,
            max_changed_pixels=4,
            max_mean_absolute_error=16.0,
            max_channel_delta=200,
            size_policy="common-region",
            alpha="compare",
        )

        self.assertEqual(stub.request.expected_image, b"expected")
        self.assertEqual(stub.request.actual_image, b"actual")
        self.assertEqual(stub.request.region.width, 4)
        self.assertEqual(stub.request.ignore_regions[0].width, 2)
        self.assertEqual(stub.request.per_channel_threshold, 3)
        self.assertAlmostEqual(stub.request.max_changed_ratio, 0.01, places=6)
        self.assertEqual(stub.request.max_changed_pixels, 4)
        self.assertAlmostEqual(stub.request.max_mean_absolute_error, 16.0, places=6)
        self.assertEqual(stub.request.max_channel_delta, 200)
        self.assertEqual(stub.request.size_policy, "common-region")
        self.assertEqual(stub.request.alpha, "compare")
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
            ignore_regions=(Rect(x=1, y=1, width=2, height=1),),
            per_channel_threshold=3,
            stable_max_changed_ratio=0.001,
            stable_max_changed_pixels=2,
            stable_max_mean_absolute_error=4.5,
            stable_max_channel_delta=9,
            loading_min_changed_ratio=0.02,
            loading_min_changed_pixels=3,
            required_stable_transitions=2,
            size_policy="common-region",
            alpha="compare",
        )

        self.assertEqual(list(stub.request.images), [b"first", b"second"])
        self.assertEqual(stub.request.region.width, 4)
        self.assertEqual(stub.request.ignore_regions[0].width, 2)
        self.assertEqual(stub.request.per_channel_threshold, 3)
        self.assertAlmostEqual(stub.request.stable_max_changed_ratio, 0.001, places=6)
        self.assertEqual(stub.request.stable_max_changed_pixels, 2)
        self.assertAlmostEqual(
            stub.request.stable_max_mean_absolute_error, 4.5, places=6
        )
        self.assertEqual(stub.request.stable_max_channel_delta, 9)
        self.assertAlmostEqual(stub.request.loading_min_changed_ratio, 0.02, places=6)
        self.assertEqual(stub.request.loading_min_changed_pixels, 3)
        self.assertEqual(stub.request.required_stable_transitions, 2)
        self.assertEqual(stub.request.size_policy, "common-region")
        self.assertEqual(stub.request.alpha, "compare")
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
            ignore_regions=[Rect(x=0, y=0, width=5, height=5)],
            edge_threshold=24,
            min_width=8,
            min_height=8,
            min_component_pixels=12,
            min_confidence=0.75,
            max_width=300,
            max_height=200,
            min_area=64,
            max_area=20_000,
            max_elements=25,
            merge_distance=2,
            padding=3,
            sort="area",
            mask_output_path="target/mask.png",
            overlay_output_path="target/overlay.png",
        )

        self.assertEqual(stub.request.image, b"image")
        self.assertEqual(stub.request.region.x, 10)
        self.assertEqual(stub.request.ignore_regions[0].width, 5)
        self.assertEqual(stub.request.edge_threshold, 24)
        self.assertEqual(stub.request.min_width, 8)
        self.assertEqual(stub.request.min_height, 8)
        self.assertEqual(stub.request.min_component_pixels, 12)
        self.assertAlmostEqual(stub.request.min_confidence, 0.75)
        self.assertEqual(stub.request.max_width, 300)
        self.assertEqual(stub.request.max_height, 200)
        self.assertEqual(stub.request.min_area, 64)
        self.assertEqual(stub.request.max_area, 20_000)
        self.assertEqual(stub.request.max_elements, 25)
        self.assertEqual(stub.request.merge_distance, 2)
        self.assertEqual(stub.request.padding, 3)
        self.assertEqual(stub.request.sort, "area")
        self.assertEqual(stub.request.mask_output_path, "target/mask.png")
        self.assertEqual(stub.request.overlay_output_path, "target/overlay.png")
        self.assertEqual(result.backend_name, "heuristic_vision")
        self.assertEqual(result.backend_kind, "vision")
        self.assertEqual(result.warnings, ("low contrast",))
        self.assertEqual(result.elements[0].bounds.width, 100)
        self.assertEqual(result.elements[0].states, ("visible",))
