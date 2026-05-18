from __future__ import annotations

from dataclasses import dataclass, field

from peekaboox.workflows.io import dump_workflow_text, workflow_to_dict
from peekaboox.workflows.model import Workflow, WorkflowStep


@dataclass(frozen=True, slots=True)
class WorkflowTemplate:
    id: str
    name: str
    description: str
    category: str
    capabilities: tuple[str, ...]
    workflow: Workflow
    tags: tuple[str, ...] = field(default_factory=tuple)

    @property
    def step_count(self) -> int:
        return len(self.workflow.steps)

    def as_dict(self, *, include_workflow: bool = False) -> dict[str, object]:
        value: dict[str, object] = {
            "id": self.id,
            "name": self.name,
            "description": self.description,
            "category": self.category,
            "capabilities": list(self.capabilities),
            "tags": list(self.tags),
            "steps": self.step_count,
        }
        if include_workflow:
            value["workflow"] = workflow_to_dict(self.workflow)
            value["json"] = dump_workflow_text(self.workflow, format_name="json")
            value["yaml"] = dump_workflow_text(self.workflow, format_name="yaml")
        return value


def list_workflow_templates(
    *,
    category: str | None = None,
    capability: str | None = None,
    tag: str | None = None,
) -> tuple[WorkflowTemplate, ...]:
    templates = BUILTIN_WORKFLOW_TEMPLATES
    if category:
        normalized = category.casefold()
        templates = tuple(template for template in templates if template.category.casefold() == normalized)
    if capability:
        normalized = capability.casefold()
        templates = tuple(
            template
            for template in templates
            if any(item.casefold() == normalized for item in template.capabilities)
        )
    if tag:
        normalized = tag.casefold()
        templates = tuple(
            template
            for template in templates
            if any(item.casefold() == normalized for item in template.tags)
        )
    return templates


def get_workflow_template(template_id: str) -> WorkflowTemplate:
    normalized = template_id.strip().casefold()
    if not normalized:
        raise ValueError("template_id must not be empty")
    for template in BUILTIN_WORKFLOW_TEMPLATES:
        if template.id.casefold() == normalized:
            return template
    known = ", ".join(template.id for template in BUILTIN_WORKFLOW_TEMPLATES)
    raise ValueError(f"unknown workflow template {template_id!r}; expected one of: {known}")


def workflow_template_dicts(
    *,
    include_workflow: bool = False,
    category: str | None = None,
    capability: str | None = None,
    tag: str | None = None,
) -> list[dict[str, object]]:
    return [
        template.as_dict(include_workflow=include_workflow)
        for template in list_workflow_templates(
            category=category,
            capability=capability,
            tag=tag,
        )
    ]


BUILTIN_WORKFLOW_TEMPLATES: tuple[WorkflowTemplate, ...] = (
    WorkflowTemplate(
        id="observe-desktop",
        name="Observe Desktop",
        description="Capture the desktop with semantic tree and refresh desktop graph state.",
        category="observe",
        capabilities=("observe", "vision", "memory_write"),
        tags=("capture", "graph"),
        workflow=Workflow(
            name="observe-desktop",
            steps=[
                WorkflowStep(action="observe"),
                WorkflowStep(action="get_desktop_state"),
            ],
        ),
    ),
    WorkflowTemplate(
        id="semantic-click",
        name="Semantic Click",
        description="Find a semantic element, click it, and verify the desktop state afterwards.",
        category="input",
        capabilities=("observe", "click", "vision"),
        tags=("selector", "click"),
        workflow=Workflow(
            name="semantic-click",
            steps=[
                WorkflowStep(action="find_element", selector="label=Replace Me"),
                WorkflowStep(action="click", selector="label=Replace Me", vision_fallback=True),
                WorkflowStep(action="get_desktop_state"),
            ],
        ),
    ),
    WorkflowTemplate(
        id="type-or-paste-text",
        name="Type Or Paste Text",
        description="Focus a target and paste text with clipboard preservation.",
        category="input",
        capabilities=("observe", "click", "type_text"),
        tags=("text", "paste"),
        workflow=Workflow(
            name="type-or-paste-text",
            steps=[
                WorkflowStep(action="click", selector="role=text,label=Replace Me", vision_fallback=True),
                WorkflowStep(
                    action="paste_text",
                    value="PeekabooX",
                    preserve_clipboard=True,
                    restore_policy="best-effort",
                ),
            ],
        ),
    ),
    WorkflowTemplate(
        id="desktop-app-action",
        name="Desktop App Action",
        description="Focus an app profile, interact with a named target, and assert it is present.",
        category="desktop",
        capabilities=("observe", "click", "vision"),
        tags=("profiles", "desktop"),
        workflow=Workflow(
            name="desktop-app-action",
            steps=[
                WorkflowStep(action="desktop_focus", app="text-editor", verify=True),
                WorkflowStep(action="desktop_click", app="text-editor", target="document", verify=True),
                WorkflowStep(action="desktop_assert", app="text-editor", target="document"),
            ],
        ),
    ),
    WorkflowTemplate(
        id="ocr-visible-text",
        name="OCR Visible Text",
        description="Run OCR on the visible desktop or scoped window and assert expected text.",
        category="vision",
        capabilities=("observe", "vision"),
        tags=("ocr", "assert"),
        workflow=Workflow(
            name="ocr-visible-text",
            steps=[
                WorkflowStep(action="ocr_screen", language="eng"),
                WorkflowStep(action="assert_text", expected_text="Replace Me"),
            ],
        ),
    ),
    WorkflowTemplate(
        id="visual-regression",
        name="Visual Regression",
        description="Compare expected and actual screenshots with thresholded diff settings.",
        category="vision",
        capabilities=("vision",),
        tags=("compare", "diff"),
        workflow=Workflow(
            name="visual-regression",
            steps=[
                WorkflowStep(
                    action="compare_images",
                    expected_path="expected.png",
                    actual_path="actual.png",
                    max_changed_ratio=0.01,
                    size_policy="common-region",
                    alpha="ignore",
                ),
            ],
        ),
    ),
    WorkflowTemplate(
        id="ui-state-wait",
        name="UI State Wait",
        description="Classify a sequence of captures as stable/loading/changing before continuing.",
        category="vision",
        capabilities=("vision",),
        tags=("state", "wait"),
        workflow=Workflow(
            name="ui-state-wait",
            steps=[
                WorkflowStep(
                    action="detect_ui_state",
                    image_paths=("frame-1.png", "frame-2.png"),
                    required_stable_transitions=1,
                ),
                WorkflowStep(action="wait", sleep_ms=250),
            ],
        ),
    ),
    WorkflowTemplate(
        id="plugin-tool-call",
        name="Plugin Tool Call",
        description="Call a declared PeekabooX plugin tool with explicit arguments.",
        category="plugin",
        capabilities=("plugin_read", "plugin_execute"),
        tags=("plugin",),
        workflow=Workflow(
            name="plugin-tool-call",
            steps=[
                WorkflowStep(
                    action="plugin_call",
                    plugin_id="system-info",
                    tool="system.info",
                    arguments={},
                    timeout_seconds=10.0,
                ),
            ],
        ),
    ),
)
