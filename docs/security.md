# Security

PeekabooX is local-first, but desktop automation can still perform sensitive
actions. The daemon therefore starts with a conservative default policy.

## Permission Gates

`peekabooxd` allows passive operations by default:

- `ping`
- `capture`
- `list_windows`
- input `--dry-run` checks

Real input injection through the daemon is denied unless explicitly enabled:

```bash
peekabooxd run --allow-input
```

Trusted local operator sessions can use the daemon profile preset instead:

```bash
peekabooxd run --profile operator
```

or:

```bash
PEEKABOOX_ALLOW_INPUT=1 peekabooxd run
```

This applies to daemon-routed clicks, pointer movement, drags, hotkeys,
`type_text`, `paste_text`, desktop helper actions that can move focus or inject
input (`desktop_focus`, `desktop_click`, `desktop_drag`, `desktop_type_into`),
and gRPC input requests including semantic selectors. Direct local CLI
execution remains available for development, but agent-facing integrations
should route through the daemon policy.

## Audit Logs

The daemon writes newline-delimited JSON audit records.

Default path:

```bash
$XDG_STATE_HOME/peekaboox/audit.jsonl
```

Fallback path:

```bash
~/.local/state/peekaboox/audit.jsonl
```

Override:

```bash
peekabooxd run --audit-log /path/to/audit.jsonl
```

For privacy, text-input audit records store only text length, not the typed
contents.

Accessibility cache invalidations are audited as event metadata only. The daemon
does not write UI labels or full semantic trees to audit records during event
handling.

## Runtime Capability Policy

`AgentRuntime` has an in-process capability policy for Python and MCP callers.
The default policy allows all runtime capabilities, while the daemon's real
input gate still applies to daemon-routed input requests.

Available runtime capabilities:

- `observe`
- `click`
- `type_text`
- `workflow_execute`
- `workflow_record`
- `workflow_generate`
- `vision`
- `memory_read`
- `memory_write`
- `plugin_read`
- `plugin_execute`

Reusable runtime profiles are available for Python and MCP:

- `observe`: `observe`, `vision`, `memory_read`, and `plugin_read`
- `plan`: `observe`, `vision`, `memory_read`, `memory_write`, and
  `workflow_generate`, plus `plugin_read`
- `assist`: `observe`, `click`, `workflow_execute`, `workflow_generate`,
  `vision`, `memory_read`, `memory_write`, and `plugin_read`
- `operator`: all runtime capabilities

Example:

```python
from peekaboox.agent import AgentRuntime
from peekaboox.security import CapabilityProfile

runtime = AgentRuntime.connect(
    capability_profile=CapabilityProfile.OBSERVE,
)
```

Custom allowlists can still be built with `CapabilityPolicy.allow_only(...)`.

Denied runtime calls raise `CapabilityDeniedError`. MCP JSON-RPC tool calls
surface the same denial as a tool result with `isError: true`, so clients can
distinguish policy denial from protocol errors. Every capability check appends
an in-memory audit event available through `runtime.capability_audit()`.
Preflight denials are also surfaced as tool results with `isError: true`, but
include structured `blocked_categories`, `warning_categories`, `next_action`,
and `preflight` fields so clients do not need to parse the message text.

Pass `audit_log_path` to persist capability, confirmation, and preflight checks
as JSONL:

```python
runtime = AgentRuntime.connect(audit_log_path="peekaboox-runtime-audit.jsonl")
```

Directly constructed runtimes can use `JsonlAuditLogger`:

```python
from peekaboox.security import JsonlAuditLogger

runtime = AgentRuntime(audit_logger=JsonlAuditLogger("peekaboox-runtime-audit.jsonl"))
```

Preflight checks append `PreflightAuditEvent` entries in memory and write JSONL
records with `event: "preflight"` when an audit logger is configured. A clean
preflight records status `ok`; usable categories with Doctor warnings record
status `warning`; blocked or missing categories record status `blocked` with the
blocking reason in the JSONL `error` field.

## Confirmation Mode

`AgentRuntime` also supports an optional confirmation policy for dangerous
agent-facing operations. The default policy is disabled. When enabled, `click`,
`type_text`, `paste_text`, and `execute_workflow` can require an
application-provided confirmer before any daemon call or workflow step is
executed.

Example:

```python
from peekaboox.agent import AgentRuntime
from peekaboox.security import ConfirmationPolicy, DangerousAction

runtime = AgentRuntime.connect(
    confirmation_policy=ConfirmationPolicy.require_for(
        [DangerousAction.CLICK, DangerousAction.TYPE_TEXT],
        confirmer=lambda request: request.operation == "click",
    )
)
```

If confirmation is required but no confirmer is configured, the runtime raises
`ConfirmationRequiredError`. If the confirmer rejects the request, it raises
`ConfirmationDeniedError`. MCP JSON-RPC tool calls surface both as tool results
with `isError: true`. Confirmation checks append in-memory audit events
available through `runtime.confirmation_audit()`.

Run the MCP server with persistent runtime audit enabled:

```bash
PYTHONPATH=python/src python3 -m peekaboox.mcp.server --audit-log /path/to/runtime-audit.jsonl
```

Constrain MCP tool calls with the same profile names:

```bash
PYTHONPATH=python/src python3 -m peekaboox.mcp.server --capability-profile observe
```

Enable Doctor-backed preflight gates for MCP tool calls at startup:

```bash
PYTHONPATH=python/src python3 -m peekaboox.mcp.server --preflight-mode strict --preflight-timeout 5
```

or:

```bash
PEEKABOOX_RUNTIME_AUDIT_LOG=/path/to/runtime-audit.jsonl \
PEEKABOOX_MCP_CAPABILITY_PROFILE=observe \
  PYTHONPATH=python/src python3 -m peekaboox.mcp.server
```

The daemon has separate startup profiles for its own gates:

- `observe`: real input disabled, daemon-wide vision fallback disabled
- `assist`: real input disabled, daemon-wide vision fallback enabled
- `operator`: real input enabled, daemon-wide vision fallback enabled

Use `PEEKABOOX_DAEMON_PROFILE=operator` or `peekabooxd run --profile operator`
only for trusted local automation sessions.

## Daemon Sandbox Profiles

`peekabooxd run --sandbox <profile>` applies an optional Linux process sandbox
before the daemon starts accepting requests:

- `off`: no additional in-process sandboxing
- `basic`: `no_new_privileges`, non-dumpable process state, and private file
  creation permissions
- `strict`: `basic` plus user, mount, and IPC namespace setup with private mount
  propagation

`strict` depends on Linux user namespaces being enabled for the current user. If
the kernel or service policy denies namespace creation, daemon startup fails and
the reason is written to the audit log as `sandbox_applied`.

Environment equivalent:

```bash
PEEKABOOX_DAEMON_SANDBOX=basic peekabooxd run
```

The systemd unit at `integrations/systemd/peekabooxd.service` enables the
`basic` sandbox. A stricter observe-only unit is available at
`integrations/systemd/peekabooxd-hardened.service`; it combines the daemon's
own sandbox with systemd mount/tmp/home/address-family restrictions.

## Emergency Stop

`peekabooxd` starts a best-effort emergency hotkey listener by default. Pressing
`CTRL + ALT + ESC` sets the daemon shutdown flag and calls the input emergency
stop path, which releases common modifiers through the available backend.

The listener reads Linux `/dev/input/event*` devices directly, so the daemon
process needs permission to read input devices, for example through the local
`input` group or a service-level device policy. If those devices are not
readable, the daemon keeps running and writes an audit error for the hotkey
listener.

Disable the hotkey listener for constrained service environments:

```bash
peekabooxd run --no-emergency-hotkey
```

or:

```bash
PEEKABOOX_EMERGENCY_HOTKEY=0 peekabooxd run
```

The daemon also calls the same emergency stop path while shutting down. Input
backend failures attempt a modifier release before reporting the error, reducing
the risk of stuck modifier keys after interrupted automation.

## systemd User Service

A starter user unit is available at:

```bash
integrations/systemd/peekabooxd.service
```

For observe-only sessions, use:

```bash
integrations/systemd/peekabooxd-hardened.service
```

Install for the current user:

```bash
mkdir -p ~/.config/systemd/user
cp integrations/systemd/peekabooxd.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now peekabooxd.service
```

The unit does not enable real input injection by default. Add
`--profile operator` to `ExecStart` only for trusted local automation sessions.
