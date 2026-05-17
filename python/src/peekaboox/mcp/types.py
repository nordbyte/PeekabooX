from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass
from typing import Any


class JsonRpcProtocolError(ValueError):
    def __init__(self, code: int, message: str) -> None:
        super().__init__(message)
        self.code = code
        self.message = message


@dataclass(slots=True)
class McpTool:
    """Local MCP-compatible tool definition and callable handler."""

    name: str
    description: str
    input_schema: dict[str, Any]
    handler: Callable[[dict[str, Any]], Any] | None = None
    title: str | None = None
    output_schema: dict[str, Any] | None = None
    annotations: dict[str, Any] | None = None

    def descriptor(self) -> dict[str, Any]:
        descriptor = {
            "name": self.name,
            "description": self.description,
            "inputSchema": self.input_schema,
        }
        if self.title:
            descriptor["title"] = self.title
        if self.output_schema is not None:
            descriptor["outputSchema"] = self.output_schema
        if self.annotations is not None:
            descriptor["annotations"] = self.annotations
        return descriptor

    def call(self, arguments: dict[str, Any] | None = None) -> Any:
        if self.handler is None:
            raise RuntimeError(f"MCP tool {self.name!r} is not bound to an AgentRuntime")
        return self.handler(arguments or {})

    def __call__(self, **arguments: Any) -> Any:
        return self.call(arguments)


@dataclass(frozen=True, slots=True)
class McpResource:
    uri: str
    name: str
    title: str
    description: str
    mime_type: str = "application/json"

    def descriptor(self) -> dict[str, Any]:
        return {
            "uri": self.uri,
            "name": self.name,
            "title": self.title,
            "description": self.description,
            "mimeType": self.mime_type,
        }


@dataclass(frozen=True, slots=True)
class McpResourceTemplate:
    uri_template: str
    name: str
    title: str
    description: str
    mime_type: str = "text/markdown"

    def descriptor(self) -> dict[str, Any]:
        return {
            "uriTemplate": self.uri_template,
            "name": self.name,
            "title": self.title,
            "description": self.description,
            "mimeType": self.mime_type,
        }


@dataclass(frozen=True, slots=True)
class McpPromptArgument:
    name: str
    description: str
    required: bool = False

    def descriptor(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "description": self.description,
            "required": self.required,
        }


@dataclass(frozen=True, slots=True)
class McpPrompt:
    name: str
    description: str
    arguments: tuple[McpPromptArgument, ...] = ()

    def descriptor(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "description": self.description,
            "arguments": [argument.descriptor() for argument in self.arguments],
        }
