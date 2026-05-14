use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::{
        PLUGIN_MANIFEST_FILE, PLUGIN_SDK_VERSION, PluginEntrypoint, PluginEntrypointKind,
        PluginManifest, PluginTool, discover_plugins, load_plugin, validate_manifest,
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

    fn unique_temp_dir(name: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("{name}-{}", std::process::id()));
        if Path::new(&path).exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        path
    }
}
