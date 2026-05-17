from __future__ import annotations

from peekaboox.mcp.types import McpPrompt, McpPromptArgument


def default_prompts() -> tuple[McpPrompt, ...]:
    return (
        McpPrompt(
            "diagnose-desktop",
            "Diagnose the current desktop/capture/input/OCR environment.",
            (
                McpPromptArgument("problem", "Observed failure or symptom.", False),
                McpPromptArgument("strict", "Whether the caller wants release-blocking checks.", False),
            ),
        ),
        McpPrompt(
            "safe-desktop-action",
            "Plan a gated desktop action using preflight, capability, and confirmation checks.",
            (
                McpPromptArgument("goal", "The user-visible desktop goal.", True),
                McpPromptArgument("app", "Optional desktop app profile.", False),
                McpPromptArgument("target", "Optional app target name.", False),
            ),
        ),
        McpPrompt(
            "inspect-window",
            "Inspect a visible window before choosing capture, OCR, elements, or input tools.",
            (
                McpPromptArgument("app", "Optional app id/name filter.", False),
                McpPromptArgument("title", "Optional title substring or regex.", False),
                McpPromptArgument("window_id", "Optional exact window id.", False),
            ),
        ),
        McpPrompt(
            "build-workflow",
            "Create an editable PeekabooX workflow plan from a goal.",
            (
                McpPromptArgument("goal", "The workflow goal.", True),
                McpPromptArgument("format", "json or yaml.", False),
            ),
        ),
        McpPrompt(
            "recover-from-tool-error",
            "Recover from a structured MCP tool error without parsing prose.",
            (
                McpPromptArgument("error_json", "The MCP structuredContent error object.", True),
            ),
        ),
        McpPrompt(
            "plugin-development",
            "Design or validate a PeekabooX plugin SDK package.",
            (
                McpPromptArgument("plugin_id", "Optional plugin id.", False),
                McpPromptArgument("tool", "Optional plugin tool name.", False),
            ),
        ),
        McpPrompt(
            "ocr-visible-text",
            "Extract and verify visible text from a screen, window, or image.",
            (
                McpPromptArgument("text_goal", "Text to find or verify.", False),
                McpPromptArgument("app", "Optional desktop app scope.", False),
            ),
        ),
        McpPrompt(
            "semantic-click-plan",
            "Plan a semantic click using accessibility, graph cache, and vision fallback.",
            (
                McpPromptArgument("selector", "Optional semantic selector.", False),
                McpPromptArgument("app", "Optional app scope.", False),
                McpPromptArgument("target", "Optional desktop helper target.", False),
            ),
        ),
    )
