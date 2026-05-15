use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PLUGIN_SDK_VERSION: &str = "peekaboox.plugin.v1";
pub const PLUGIN_MANIFEST_FILE: &str = "peekaboox.plugin.json";
pub const PLUGIN_PATH_ENV: &str = "PEEKABOOX_PLUGIN_PATH";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub schema_version: String,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub entrypoint: Option<PluginEntrypoint>,
    #[serde(default)]
    pub tools: Vec<PluginTool>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginEntrypoint {
    pub kind: PluginEntrypointKind,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginEntrypointKind {
    Process,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginTool {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default = "default_input_schema")]
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginDescriptor {
    pub manifest: PluginManifest,
    pub root_dir: PathBuf,
    pub manifest_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginDiscoveryResult {
    pub plugins: Vec<PluginDescriptor>,
    pub errors: Vec<PluginDiscoveryError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginDiscoveryError {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginExecutionPolicy {
    pub timeout: Duration,
    pub max_output_bytes: usize,
    pub environment: BTreeMap<String, String>,
}

impl Default for PluginExecutionPolicy {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(10),
            max_output_bytes: 1_048_576,
            environment: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PluginToolExecutionResult {
    pub ok: bool,
    pub plugin_id: String,
    pub tool: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub result: Option<Value>,
    pub error: Option<String>,
}

pub fn default_plugin_search_paths() -> Vec<PathBuf> {
    let mut paths = plugin_paths_from_env();
    paths.push(PathBuf::from("plugins"));
    paths
}

pub fn plugin_paths_from_env() -> Vec<PathBuf> {
    env::var_os(PLUGIN_PATH_ENV)
        .map(|value| env::split_paths(&value).collect())
        .unwrap_or_default()
}

pub fn discover_plugins(paths: &[PathBuf]) -> PluginDiscoveryResult {
    let search_paths = if paths.is_empty() {
        default_plugin_search_paths()
    } else {
        paths.to_vec()
    };
    let mut plugins = Vec::new();
    let mut errors = Vec::new();

    for path in search_paths {
        discover_path(&path, &mut plugins, &mut errors);
    }

    plugins.sort_by(|left, right| left.manifest.id.cmp(&right.manifest.id));
    errors.sort_by(|left, right| left.path.cmp(&right.path));
    PluginDiscoveryResult { plugins, errors }
}

pub fn load_plugin(path: impl AsRef<Path>) -> Result<PluginDescriptor, String> {
    let manifest_path = manifest_path(path.as_ref());
    let payload = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("failed to read {}: {error}", manifest_path.display()))?;
    let manifest: PluginManifest = serde_json::from_str(&payload).map_err(|error| {
        format!(
            "invalid plugin manifest {}: {error}",
            manifest_path.display()
        )
    })?;
    validate_manifest(&manifest)?;
    let root_dir = manifest_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    Ok(PluginDescriptor {
        manifest,
        root_dir,
        manifest_path,
    })
}

pub fn validate_manifest(manifest: &PluginManifest) -> Result<(), String> {
    if manifest.schema_version != PLUGIN_SDK_VERSION {
        return Err(format!(
            "unsupported plugin schema_version {:?}; expected {PLUGIN_SDK_VERSION:?}",
            manifest.schema_version
        ));
    }
    validate_identifier("plugin id", &manifest.id)?;
    validate_non_empty("plugin name", &manifest.name)?;
    validate_non_empty("plugin version", &manifest.version)?;

    for capability in &manifest.capabilities {
        validate_capability(capability)?;
    }
    if let Some(entrypoint) = &manifest.entrypoint
        && (entrypoint.command.is_empty() || entrypoint.command.iter().any(|part| part.is_empty()))
    {
        return Err("plugin entrypoint.command must contain non-empty command parts".to_owned());
    }
    for tool in &manifest.tools {
        validate_identifier("plugin tool name", &tool.name)?;
        validate_non_empty("plugin tool description", &tool.description)?;
        if !tool.input_schema.is_object() {
            return Err(format!(
                "plugin tool {:?} input_schema must be a JSON object",
                tool.name
            ));
        }
        for capability in &tool.capabilities {
            validate_capability(capability)?;
        }
    }
    Ok(())
}

pub fn execute_plugin_tool(
    plugin: &PluginDescriptor,
    tool_name: &str,
    arguments: Value,
    policy: &PluginExecutionPolicy,
) -> Result<PluginToolExecutionResult, String> {
    let entrypoint = plugin.manifest.entrypoint.as_ref().ok_or_else(|| {
        format!(
            "plugin {:?} does not declare an entrypoint",
            plugin.manifest.id
        )
    })?;
    let tool = plugin
        .manifest
        .tools
        .iter()
        .find(|tool| tool.name == tool_name)
        .ok_or_else(|| {
            format!(
                "plugin {:?} does not declare tool {:?}",
                plugin.manifest.id, tool_name
            )
        })?;

    validate_json_schema(&tool.input_schema, &arguments)
        .map_err(|error| format!("arguments for tool {tool_name:?} are invalid: {error}"))?;

    let Some(program) = entrypoint.command.first() else {
        return Err(format!(
            "plugin {:?} entrypoint command is empty",
            plugin.manifest.id
        ));
    };

    let request = serde_json::json!({
        "schema_version": PLUGIN_SDK_VERSION,
        "plugin_id": plugin.manifest.id,
        "tool": tool_name,
        "arguments": arguments,
    });
    let mut command = Command::new(program);
    command
        .args(entrypoint.command.iter().skip(1))
        .current_dir(&plugin.root_dir)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for key in SAFE_PLUGIN_ENV {
        if let Ok(value) = env::var(key) {
            command.env(key, value);
        }
    }
    command.env("PEEKABOOX_PLUGIN_ID", &plugin.manifest.id);
    command.env("PEEKABOOX_PLUGIN_TOOL", tool_name);
    command.env("PEEKABOOX_PLUGIN_ROOT", &plugin.root_dir);
    for (key, value) in &policy.environment {
        command.env(key, value);
    }

    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to start plugin {:?}: {error}", plugin.manifest.id))?;
    let mut child_stdin = child
        .stdin
        .take()
        .ok_or_else(|| format!("failed to open stdin for plugin {:?}", plugin.manifest.id))?;
    child_stdin
        .write_all(request.to_string().as_bytes())
        .map_err(|error| format!("failed to write plugin request: {error}"))?;
    drop(child_stdin);

    let started = Instant::now();
    loop {
        if child
            .try_wait()
            .map_err(|error| format!("failed to poll plugin process: {error}"))?
            .is_some()
        {
            break;
        }
        if started.elapsed() >= policy.timeout {
            let _ = child.kill();
            let output = child
                .wait_with_output()
                .map_err(|error| format!("failed to collect timed-out plugin output: {error}"))?;
            return Ok(PluginToolExecutionResult {
                ok: false,
                plugin_id: plugin.manifest.id.clone(),
                tool: tool_name.to_owned(),
                exit_code: -1,
                stdout: limited_output(&output.stdout, policy.max_output_bytes),
                stderr: limited_output(&output.stderr, policy.max_output_bytes),
                result: None,
                error: Some(format!(
                    "plugin timed out after {} ms",
                    policy.timeout.as_millis()
                )),
            });
        }
        sleep(Duration::from_millis(10));
    }

    let output = child
        .wait_with_output()
        .map_err(|error| format!("failed to collect plugin output: {error}"))?;
    let stdout_too_large = output.stdout.len() > policy.max_output_bytes;
    let stderr_too_large = output.stderr.len() > policy.max_output_bytes;
    let stdout = limited_output(&output.stdout, policy.max_output_bytes);
    let stderr = limited_output(&output.stderr, policy.max_output_bytes);
    let payload = parse_stdout_json(&stdout);
    let mut ok = output.status.success()
        && !matches!(&payload, Some(Value::Object(object)) if object.get("ok") == Some(&Value::Bool(false)));
    let mut error = None;

    if stdout_too_large || stderr_too_large {
        ok = false;
        error = Some(format!(
            "plugin output exceeded max_output_bytes={}",
            policy.max_output_bytes
        ));
    } else if !ok {
        error = payload
            .as_ref()
            .and_then(|payload| payload.get("error"))
            .map(value_to_error_string)
            .or_else(|| {
                if stderr.trim().is_empty() {
                    Some(format!("plugin exited with status {}", output.status))
                } else {
                    Some(stderr.trim().to_owned())
                }
            });
    }

    let result = match payload {
        Some(Value::Object(mut object)) => object.remove("result"),
        other => other,
    };

    Ok(PluginToolExecutionResult {
        ok,
        plugin_id: plugin.manifest.id.clone(),
        tool: tool_name.to_owned(),
        exit_code: output.status.code().unwrap_or(-1),
        stdout,
        stderr,
        result,
        error,
    })
}

pub fn validate_json_schema(schema: &Value, value: &Value) -> Result<(), String> {
    validate_json_schema_at(schema, value, "$")
}

fn discover_path(
    path: &Path,
    plugins: &mut Vec<PluginDescriptor>,
    errors: &mut Vec<PluginDiscoveryError>,
) {
    if !path.exists() {
        return;
    }
    if path.is_file() {
        push_plugin(path, plugins, errors);
        return;
    }
    if path.join(PLUGIN_MANIFEST_FILE).is_file() {
        push_plugin(path, plugins, errors);
        return;
    }

    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) => {
            errors.push(PluginDiscoveryError {
                path: path.to_path_buf(),
                message: error.to_string(),
            });
            return;
        }
    };
    for entry in entries.flatten() {
        let entry_path = entry.path();
        let is_plugin_dir = entry_path.is_dir() && entry_path.join(PLUGIN_MANIFEST_FILE).is_file();
        let is_manifest_file = entry_path.is_file()
            && entry_path
                .file_name()
                .is_some_and(|name| name == PLUGIN_MANIFEST_FILE);
        if is_plugin_dir || is_manifest_file {
            push_plugin(&entry_path, plugins, errors);
        }
    }
}

fn push_plugin(
    path: &Path,
    plugins: &mut Vec<PluginDescriptor>,
    errors: &mut Vec<PluginDiscoveryError>,
) {
    match load_plugin(path) {
        Ok(plugin) => plugins.push(plugin),
        Err(message) => errors.push(PluginDiscoveryError {
            path: path.to_path_buf(),
            message,
        }),
    }
}

fn manifest_path(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.join(PLUGIN_MANIFEST_FILE)
    } else {
        path.to_path_buf()
    }
}

fn default_input_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

fn validate_non_empty(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{label} must not be empty"))
    } else {
        Ok(())
    }
}

fn validate_identifier(label: &str, value: &str) -> Result<(), String> {
    validate_non_empty(label, value)?;
    if value.len() > 128 {
        return Err(format!("{label} must be 128 characters or shorter"));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(format!(
            "{label} {:?} must use only ASCII letters, digits, dots, underscores, or dashes",
            value
        ));
    }
    Ok(())
}

fn validate_capability(capability: &str) -> Result<(), String> {
    validate_identifier("plugin capability", capability)
}

const SAFE_PLUGIN_ENV: &[&str] = &[
    "PATH",
    "HOME",
    "LANG",
    "LC_ALL",
    "PYTHONPATH",
    "PYTHONHOME",
    "VIRTUAL_ENV",
    "XDG_RUNTIME_DIR",
];

fn validate_json_schema_at(schema: &Value, value: &Value, path: &str) -> Result<(), String> {
    let Some(schema_object) = schema.as_object() else {
        return Err(format!("{path}: schema must be an object"));
    };

    if let Some(enum_values) = schema_object.get("enum") {
        let Some(enum_values) = enum_values.as_array() else {
            return Err(format!("{path}: enum must be an array"));
        };
        if !enum_values.iter().any(|candidate| candidate == value) {
            return Err(format!(
                "{path}: value is not one of the allowed enum values"
            ));
        }
    }

    if let Some(type_value) = schema_object.get("type") {
        validate_schema_type(type_value, value, path)?;
    }

    if value.is_object() {
        validate_object_schema(schema_object, value, path)?;
    } else if let Some(required) = schema_object.get("required")
        && !required.as_array().is_some_and(Vec::is_empty)
    {
        return Err(format!("{path}: required fields need an object value"));
    }

    if let Some(items_schema) = schema_object.get("items")
        && let Some(items) = value.as_array()
    {
        for (index, item) in items.iter().enumerate() {
            validate_json_schema_at(items_schema, item, &format!("{path}[{index}]"))?;
        }
    }

    validate_numeric_bounds(schema_object, value, path)?;
    validate_string_bounds(schema_object, value, path)?;
    validate_array_bounds(schema_object, value, path)?;
    Ok(())
}

fn validate_schema_type(type_value: &Value, value: &Value, path: &str) -> Result<(), String> {
    let matches = match type_value {
        Value::String(expected) => value_matches_schema_type(value, expected),
        Value::Array(expected_values) => expected_values.iter().any(|expected| {
            expected
                .as_str()
                .is_some_and(|expected| value_matches_schema_type(value, expected))
        }),
        _ => return Err(format!("{path}: type must be a string or string array")),
    };

    if matches {
        Ok(())
    } else {
        Err(format!(
            "{path}: value does not match schema type {type_value}"
        ))
    }
}

fn value_matches_schema_type(value: &Value, expected: &str) -> bool {
    match expected {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => false,
    }
}

fn validate_object_schema(
    schema_object: &serde_json::Map<String, Value>,
    value: &Value,
    path: &str,
) -> Result<(), String> {
    let object = value.as_object().expect("checked object");
    if let Some(required) = schema_object.get("required") {
        let Some(required) = required.as_array() else {
            return Err(format!("{path}: required must be an array"));
        };
        for field in required {
            let Some(field) = field.as_str() else {
                return Err(format!("{path}: required entries must be strings"));
            };
            if !object.contains_key(field) {
                return Err(format!("{path}.{field}: required field is missing"));
            }
        }
    }

    let properties = schema_object.get("properties").and_then(Value::as_object);
    if let Some(properties) = properties {
        for (field, field_schema) in properties {
            if let Some(field_value) = object.get(field) {
                validate_json_schema_at(field_schema, field_value, &format!("{path}.{field}"))?;
            }
        }
    }

    if schema_object.get("additionalProperties") == Some(&Value::Bool(false))
        && let Some(properties) = properties
    {
        for field in object.keys() {
            if !properties.contains_key(field) {
                return Err(format!(
                    "{path}.{field}: additional property is not allowed"
                ));
            }
        }
    }

    Ok(())
}

fn validate_numeric_bounds(
    schema_object: &serde_json::Map<String, Value>,
    value: &Value,
    path: &str,
) -> Result<(), String> {
    let Some(number) = value.as_f64() else {
        return Ok(());
    };
    if let Some(minimum) = schema_object.get("minimum").and_then(Value::as_f64)
        && number < minimum
    {
        return Err(format!("{path}: value is smaller than minimum {minimum}"));
    }
    if let Some(maximum) = schema_object.get("maximum").and_then(Value::as_f64)
        && number > maximum
    {
        return Err(format!("{path}: value is greater than maximum {maximum}"));
    }
    Ok(())
}

fn validate_string_bounds(
    schema_object: &serde_json::Map<String, Value>,
    value: &Value,
    path: &str,
) -> Result<(), String> {
    let Some(text) = value.as_str() else {
        return Ok(());
    };
    if let Some(min_length) = schema_object.get("minLength").and_then(Value::as_u64)
        && text.chars().count() < min_length as usize
    {
        return Err(format!(
            "{path}: string is shorter than minLength {min_length}"
        ));
    }
    if let Some(max_length) = schema_object.get("maxLength").and_then(Value::as_u64)
        && text.chars().count() > max_length as usize
    {
        return Err(format!(
            "{path}: string is longer than maxLength {max_length}"
        ));
    }
    Ok(())
}

fn validate_array_bounds(
    schema_object: &serde_json::Map<String, Value>,
    value: &Value,
    path: &str,
) -> Result<(), String> {
    let Some(items) = value.as_array() else {
        return Ok(());
    };
    if let Some(min_items) = schema_object.get("minItems").and_then(Value::as_u64)
        && items.len() < min_items as usize
    {
        return Err(format!("{path}: array has fewer than minItems {min_items}"));
    }
    if let Some(max_items) = schema_object.get("maxItems").and_then(Value::as_u64)
        && items.len() > max_items as usize
    {
        return Err(format!("{path}: array has more than maxItems {max_items}"));
    }
    Ok(())
}

fn limited_output(bytes: &[u8], max_output_bytes: usize) -> String {
    let limit = bytes.len().min(max_output_bytes);
    String::from_utf8_lossy(&bytes[..limit]).into_owned()
}

fn parse_stdout_json(stdout: &str) -> Option<Value> {
    let text = stdout.trim();
    if text.is_empty() {
        return None;
    }
    serde_json::from_str(text)
        .ok()
        .or_else(|| Some(Value::String(text.to_owned())))
}

fn value_to_error_string(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::{
        PLUGIN_MANIFEST_FILE, PLUGIN_SDK_VERSION, PluginEntrypoint, PluginEntrypointKind,
        PluginExecutionPolicy, PluginManifest, PluginTool, discover_plugins, execute_plugin_tool,
        load_plugin, validate_json_schema, validate_manifest,
    };

    #[test]
    fn validates_minimal_manifest() {
        let manifest = PluginManifest {
            schema_version: PLUGIN_SDK_VERSION.to_owned(),
            id: "demo.plugin".to_owned(),
            name: "Demo Plugin".to_owned(),
            version: "1.0.0".to_owned(),
            description: None,
            capabilities: vec!["observe".to_owned()],
            entrypoint: Some(PluginEntrypoint {
                kind: PluginEntrypointKind::Process,
                command: vec!["python3".to_owned(), "plugin.py".to_owned()],
            }),
            tools: vec![PluginTool {
                name: "demo.inspect".to_owned(),
                description: "Inspect demo state".to_owned(),
                capabilities: vec!["observe".to_owned()],
                input_schema: serde_json::json!({"type": "object"}),
            }],
            metadata: Default::default(),
        };

        validate_manifest(&manifest).unwrap();
    }

    #[test]
    fn rejects_unknown_schema_version() {
        let manifest = PluginManifest {
            schema_version: "peekaboox.plugin.v0".to_owned(),
            id: "demo".to_owned(),
            name: "Demo".to_owned(),
            version: "1.0.0".to_owned(),
            description: None,
            capabilities: Vec::new(),
            entrypoint: None,
            tools: Vec::new(),
            metadata: Default::default(),
        };

        assert!(
            validate_manifest(&manifest)
                .unwrap_err()
                .contains("schema_version")
        );
    }

    #[test]
    fn discovers_plugins_under_search_directory() {
        let root = unique_temp_dir("peekaboox-plugin-discovery");
        let plugin_dir = root.join("plugins").join("demo");
        fs::create_dir_all(&plugin_dir).unwrap();
        fs::write(
            plugin_dir.join(PLUGIN_MANIFEST_FILE),
            serde_json::json!({
                "schema_version": PLUGIN_SDK_VERSION,
                "id": "demo",
                "name": "Demo",
                "version": "1.0.0",
                "tools": [{"name": "demo.inspect", "description": "Inspect demo state"}]
            })
            .to_string(),
        )
        .unwrap();

        let result = discover_plugins(&[root.join("plugins")]);

        assert_eq!(result.errors, Vec::new());
        assert_eq!(result.plugins.len(), 1);
        assert_eq!(result.plugins[0].manifest.id, "demo");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn load_plugin_accepts_manifest_file_path() {
        let root = unique_temp_dir("peekaboox-plugin-file");
        fs::create_dir_all(&root).unwrap();
        let manifest_path = root.join(PLUGIN_MANIFEST_FILE);
        fs::write(
            &manifest_path,
            serde_json::json!({
                "schema_version": PLUGIN_SDK_VERSION,
                "id": "file.demo",
                "name": "File Demo",
                "version": "1.0.0"
            })
            .to_string(),
        )
        .unwrap();

        let plugin = load_plugin(&manifest_path).unwrap();

        assert_eq!(plugin.manifest.id, "file.demo");
        assert_eq!(plugin.root_dir, root);
        fs::remove_dir_all(plugin.root_dir).ok();
    }

    #[test]
    fn validates_tool_arguments_against_schema_subset() {
        let schema = serde_json::json!({
            "type": "object",
            "required": ["text"],
            "properties": {
                "text": {"type": "string", "minLength": 1},
                "count": {"type": "integer", "minimum": 1}
            },
            "additionalProperties": false
        });

        validate_json_schema(&schema, &serde_json::json!({"text": "hello", "count": 2})).unwrap();
        assert!(
            validate_json_schema(&schema, &serde_json::json!({"count": 2}))
                .unwrap_err()
                .contains("required")
        );
        assert!(
            validate_json_schema(
                &schema,
                &serde_json::json!({"text": "hello", "extra": true})
            )
            .unwrap_err()
            .contains("additional property")
        );
    }

    #[test]
    fn executes_process_plugin_tool_with_schema_validation() {
        let root = unique_temp_dir("peekaboox-plugin-exec");
        let plugin_dir = root.join("demo");
        fs::create_dir_all(&plugin_dir).unwrap();
        let script = plugin_dir.join("plugin.py");
        fs::write(
            &script,
            "import json, sys\nrequest = json.load(sys.stdin)\njson.dump({'ok': True, 'result': {'tool': request['tool'], 'value': request['arguments']['value']}}, sys.stdout)\n",
        )
        .unwrap();
        fs::write(
            plugin_dir.join(PLUGIN_MANIFEST_FILE),
            serde_json::json!({
                "schema_version": PLUGIN_SDK_VERSION,
                "id": "exec.demo",
                "name": "Exec Demo",
                "version": "1.0.0",
                "entrypoint": {
                    "kind": "process",
                    "command": ["python3", "plugin.py"]
                },
                "tools": [{
                    "name": "exec.echo",
                    "description": "Echo a value",
                    "input_schema": {
                        "type": "object",
                        "required": ["value"],
                        "properties": {"value": {"type": "string"}},
                        "additionalProperties": false
                    }
                }]
            })
            .to_string(),
        )
        .unwrap();
        let plugin = load_plugin(&plugin_dir).unwrap();

        let result = execute_plugin_tool(
            &plugin,
            "exec.echo",
            serde_json::json!({"value": "ok"}),
            &PluginExecutionPolicy::default(),
        )
        .unwrap();

        assert!(result.ok);
        assert_eq!(result.result.unwrap()["value"], "ok");
        fs::remove_dir_all(root).ok();
    }

    fn unique_temp_dir(name: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("{name}-{}", std::process::id()));
        if Path::new(&path).exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        path
    }
}
