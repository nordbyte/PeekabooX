from __future__ import annotations

import http.server
import json
from typing import Any, Protocol, TextIO

from peekaboox.mcp.helpers import (
    _jsonrpc_error,
    _mcp_http_content_length,
    _mcp_http_host_requires_auth,
    _mcp_http_request_authorized,
    _normalize_mcp_auth_token,
)

PARSE_ERROR = -32700


class JsonRpcServer(Protocol):
    log_level: str

    def handle_jsonrpc_line(self, line: str) -> dict[str, Any] | list[dict[str, Any]] | None: ...

    def handle_jsonrpc(self, message: Any) -> dict[str, Any] | list[dict[str, Any]] | None: ...

    def list_tools(self) -> list[dict[str, Any]]: ...


def serve_stdio(
    server: JsonRpcServer,
    input_stream: TextIO,
    output_stream: TextIO,
) -> None:
    for line in input_stream:
        if not line.strip():
            continue
        response = server.handle_jsonrpc_line(line)
        if response is None:
            continue
        output_stream.write(json.dumps(response, separators=(",", ":")) + "\n")
        output_stream.flush()


def serve_http(
    server: JsonRpcServer,
    host: str,
    port: int,
    *,
    sse: bool,
    auth_token: str | None,
    max_request_bytes: int,
    server_name: str,
    server_version: str,
) -> None:
    auth_token = _normalize_mcp_auth_token(auth_token)
    if max_request_bytes <= 0:
        raise ValueError("max_request_bytes must be greater than zero")
    if auth_token is None and _mcp_http_host_requires_auth(host):
        raise ValueError(
            "refusing to expose unauthenticated MCP HTTP/SSE on a non-loopback host; "
            "set --auth-token or PEEKABOOX_MCP_TOKEN"
        )

    class Handler(http.server.BaseHTTPRequestHandler):
        server_version = f"{server_name}/{server_version}"

        def do_GET(self) -> None:  # noqa: N802 - stdlib callback name
            if self.path == "/health":
                self._write_json(
                    {
                        "name": server_name,
                        "version": server_version,
                        "status": "ok",
                        "auth": "required" if auth_token is not None else "none",
                    }
                )
                return
            if self.path in ("/", "/mcp"):
                if not self._require_auth():
                    return
                self._write_json(
                    {
                        "name": server_name,
                        "version": server_version,
                        "transport": "sse" if sse else "http",
                        "jsonrpc_endpoint": "/mcp",
                        "sse_endpoint": "/sse",
                        "auth": "required" if auth_token is not None else "none",
                        "max_request_bytes": max_request_bytes,
                    }
                )
                return
            if self.path.startswith("/sse"):
                if not self._require_auth():
                    return
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.send_header("Cache-Control", "no-cache")
                self.send_header("Connection", "keep-alive")
                self.end_headers()
                self.wfile.write(b"event: endpoint\n")
                self.wfile.write(b"data: /mcp\n\n")
                self.wfile.write(b"event: tools\n")
                self.wfile.write(
                    b"data: "
                    + json.dumps({"tools": server.list_tools()}, separators=(",", ":")).encode()
                    + b"\n\n"
                )
                self.wfile.flush()
                return
            self.send_error(404, "not found")

        def do_POST(self) -> None:  # noqa: N802 - stdlib callback name
            if self.path not in ("/", "/mcp"):
                self.send_error(404, "not found")
                return
            if not self._require_auth():
                return
            try:
                length = _mcp_http_content_length(
                    self.headers.get("Content-Length"),
                    max_request_bytes,
                )
            except ValueError:
                self.send_error(411, "invalid content length")
                return
            except OverflowError:
                self.send_error(413, "request body too large")
                return
            payload = self.rfile.read(length).decode("utf-8")
            try:
                message = json.loads(payload)
            except json.JSONDecodeError as error:
                response: dict[str, Any] | list[dict[str, Any]] | None = _jsonrpc_error(
                    None, PARSE_ERROR, f"invalid JSON: {error.msg}"
                )
            else:
                response = server.handle_jsonrpc(message)
            if response is None:
                self.send_response(202)
                self.end_headers()
                return
            self._write_json(response)

        def log_message(self, format: str, *args: Any) -> None:
            if server.log_level == "debug":
                super().log_message(format, *args)

        def _write_json(self, payload: Any) -> None:
            body = json.dumps(payload, separators=(",", ":")).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def _require_auth(self) -> bool:
            if _mcp_http_request_authorized(self.headers, auth_token):
                return True
            self.send_response(401)
            self.send_header("Content-Type", "application/json")
            self.send_header("WWW-Authenticate", 'Bearer realm="PeekabooX MCP"')
            self.end_headers()
            self.wfile.write(
                json.dumps(
                    {
                        "error": "unauthorized",
                        "message": "missing or invalid PeekabooX MCP token",
                    },
                    separators=(",", ":"),
                ).encode("utf-8")
            )
            return False

    httpd = http.server.ThreadingHTTPServer((host, port), Handler)
    httpd.serve_forever()
