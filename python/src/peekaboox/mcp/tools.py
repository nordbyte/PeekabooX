from __future__ import annotations

from collections.abc import Callable
from typing import Any

from peekaboox.mcp.schemas import tool_annotations, tool_output_schema, tool_title
from peekaboox.mcp.types import McpTool


def build_tool(
    name: str,
    description: str,
    input_schema: dict[str, Any],
    handler: Callable[[dict[str, Any]], Any] | None,
) -> McpTool:
    return McpTool(
        name=name,
        description=description,
        input_schema=input_schema,
        handler=handler,
        title=tool_title(name),
        output_schema=tool_output_schema(name),
        annotations=tool_annotations(name),
    )
