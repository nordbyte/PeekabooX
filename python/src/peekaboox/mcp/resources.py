from __future__ import annotations

from peekaboox.mcp.types import McpResource, McpResourceTemplate

DOC_RESOURCES = {
    "api": "docs/api.md",
    "runtime": "docs/runtime.md",
    "security": "docs/security.md",
    "plugins": "docs/plugins.md",
    "cli": "docs/cli.md",
    "examples": "examples/README.md",
}


def default_resources() -> tuple[McpResource, ...]:
    resources = [
        McpResource(
            uri="peekaboox://server/info",
            name="server-info",
            title="PeekabooX MCP Server Info",
            description="Server version, protocol capabilities, runtime status, and counts.",
        ),
        McpResource(
            uri="peekaboox://tools",
            name="tools",
            title="PeekabooX MCP Tools",
            description="Current MCP tool descriptors, schemas, annotations, and output schemas.",
        ),
        McpResource(
            uri="peekaboox://workflows/templates",
            name="workflow-templates",
            title="Workflow Templates",
            description="Built-in workflow templates with categories, capabilities, and tags.",
        ),
        McpResource(
            uri="peekaboox://desktop/profiles",
            name="desktop-profiles",
            title="Desktop App Profiles",
            description="Supported desktop helper app profiles and target names.",
        ),
        McpResource(
            uri="peekaboox://doctor/latest",
            name="doctor-latest",
            title="Latest Doctor Result",
            description="Most recent Doctor result cached by runtime preflight or doctor calls.",
        ),
        McpResource(
            uri="peekaboox://preflight/latest",
            name="preflight-latest",
            title="Latest Preflight Decision",
            description="Most recent runtime preflight audit event.",
        ),
        McpResource(
            uri="peekaboox://desktop/latest-snapshot",
            name="desktop-latest-snapshot",
            title="Latest Desktop Graph Snapshot",
            description="Latest semantic desktop graph snapshot from the runtime memory store.",
        ),
        McpResource(
            uri="peekaboox://desktop/graph/status",
            name="desktop-graph-status",
            title="Desktop Graph Status",
            description="Semantic desktop graph freshness and invalidation status.",
        ),
        McpResource(
            uri="peekaboox://plugins",
            name="plugins",
            title="PeekabooX Plugins",
            description="Discovered local plugins and declared tools.",
        ),
        McpResource(
            uri="peekaboox://audit/capabilities",
            name="capability-audit",
            title="Capability Audit",
            description="In-memory capability audit events for this MCP runtime.",
        ),
        McpResource(
            uri="peekaboox://audit/confirmations",
            name="confirmation-audit",
            title="Confirmation Audit",
            description="In-memory confirmation audit events for this MCP runtime.",
        ),
        McpResource(
            uri="peekaboox://audit/preflight",
            name="preflight-audit",
            title="Preflight Audit",
            description="In-memory preflight audit events for this MCP runtime.",
        ),
    ]
    resources.extend(
        McpResource(
            uri=f"peekaboox://docs/{name}",
            name=f"docs-{name}",
            title=f"PeekabooX {name.title()} Docs",
            description=f"Repository documentation from {path}.",
            mime_type="text/markdown",
        )
        for name, path in DOC_RESOURCES.items()
    )
    return tuple(resources)


def default_resource_templates() -> tuple[McpResourceTemplate, ...]:
    return (
        McpResourceTemplate(
            uri_template="peekaboox://docs/{document}",
            name="docs",
            title="PeekabooX Documentation",
            description=(
                "Read repository documentation; document is one of "
                + ", ".join(sorted(DOC_RESOURCES))
                + "."
            ),
        ),
        McpResourceTemplate(
            uri_template="peekaboox://audit/{kind}",
            name="audit",
            title="PeekabooX Runtime Audit",
            description="Read runtime audit events; kind is capabilities, confirmations, or preflight.",
            mime_type="application/json",
        ),
        McpResourceTemplate(
            uri_template="peekaboox://workflows/templates/{template_id}",
            name="workflow-template",
            title="Workflow Template",
            description="Read a built-in workflow template by template_id.",
            mime_type="application/json",
        ),
    )
