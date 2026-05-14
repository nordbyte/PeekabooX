__all__ = ["McpServer", "McpTool"]


def __getattr__(name: str):
    if name in __all__:
        from .server import McpServer, McpTool

        return {"McpServer": McpServer, "McpTool": McpTool}[name]
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
