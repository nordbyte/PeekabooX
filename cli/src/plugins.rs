use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PluginsArgs {
    pub(super) paths: Vec<PathBuf>,
    pub(super) json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PluginsCommand {
    Run(PluginsArgs),
    Help,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct PluginCallArgs {
    pub(super) plugin_id: String,
    pub(super) tool: String,
    pub(super) arguments: serde_json::Value,
    pub(super) paths: Vec<PathBuf>,
    pub(super) timeout_ms: u64,
    pub(super) max_output_bytes: usize,
    pub(super) require_trusted: bool,
    pub(super) trust_policy: Option<PathBuf>,
    pub(super) json: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum PluginCallCommand {
    Run(PluginCallArgs),
    Help,
}

pub(super) fn plugins(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let PluginsCommand::Run(args) = parse_plugins_args(args)? else {
        print_plugins_usage();
        return Err(CliError::HelpRequested);
    };

    let result = if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::ListPlugins {
                paths: args
                    .paths
                    .iter()
                    .map(path_to_daemon_string)
                    .collect::<Result<Vec<_>, _>>()?,
            },
        )?;
        let ApiResult::Plugins(plugins) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected plugin list response".to_owned(),
            ));
        };
        plugins
    } else {
        plugin_list_dto(peekaboox_plugins::discover_plugins(&args.paths))
    };

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&result)
                .map_err(|error| CliError::Failure(error.to_string()))?
        );
    } else {
        print_plugin_list_dto(&result);
    }
    Ok(())
}

pub(super) fn parse_plugins_args(args: Vec<String>) -> Result<PluginsCommand, CliError> {
    let mut paths = Vec::new();
    let mut json = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--path" | "-p" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --path".to_owned()));
                };
                paths.push(PathBuf::from(value));
            }
            "--json" => json = true,
            "--help" | "-h" => return Ok(PluginsCommand::Help),
            unknown => {
                return Err(CliError::Failure(format!(
                    "unknown plugins argument: {unknown}"
                )));
            }
        }

        index += 1;
    }

    Ok(PluginsCommand::Run(PluginsArgs { paths, json }))
}

pub(super) fn plugin_call(args: Vec<String>, context: &CliContext) -> Result<(), CliError> {
    let PluginCallCommand::Run(args) = parse_plugin_call_args(args)? else {
        print_plugin_call_usage();
        return Err(CliError::HelpRequested);
    };

    if context.use_daemon && args.require_trusted {
        return Err(CliError::Failure(
            "--require-trusted is enforced by the local plugin runner; run without --daemon"
                .to_owned(),
        ));
    }

    let result = if context.use_daemon {
        let result = daemon_request(
            context,
            ApiRequest::CallPluginTool {
                plugin_id: args.plugin_id.clone(),
                tool: args.tool.clone(),
                arguments: args.arguments.clone(),
                paths: args
                    .paths
                    .iter()
                    .map(path_to_daemon_string)
                    .collect::<Result<Vec<_>, _>>()?,
                timeout_ms: args.timeout_ms,
                max_output_bytes: args.max_output_bytes,
            },
        )?;
        let ApiResult::PluginToolExecution(result) = result else {
            return Err(CliError::Failure(
                "daemon returned unexpected plugin execution response".to_owned(),
            ));
        };
        result
    } else {
        let discovery = peekaboox_plugins::discover_plugins(&args.paths);
        if !discovery.errors.is_empty() {
            return Err(CliError::Failure(format!(
                "plugin discovery failed: {}",
                discovery
                    .errors
                    .iter()
                    .map(|error| format!("{}: {}", error.path.display(), error.message))
                    .collect::<Vec<_>>()
                    .join("; ")
            )));
        }
        let plugin = discovery
            .plugins
            .iter()
            .find(|plugin| plugin.manifest.id == args.plugin_id)
            .ok_or_else(|| CliError::Failure(format!("unknown plugin: {}", args.plugin_id)))?;
        let policy = peekaboox_plugins::PluginExecutionPolicy {
            timeout: std::time::Duration::from_millis(args.timeout_ms),
            max_output_bytes: args.max_output_bytes,
            ..Default::default()
        };
        if args.require_trusted {
            peekaboox_plugins::require_plugin_trust(plugin, args.trust_policy.as_deref())
                .map_err(CliError::Failure)?;
        }
        plugin_execution_dto(
            peekaboox_plugins::execute_plugin_tool(
                plugin,
                &args.tool,
                args.arguments.clone(),
                &policy,
            )
            .map_err(CliError::Failure)?,
        )
    };

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&result)
                .map_err(|error| CliError::Failure(error.to_string()))?
        );
    } else {
        print_plugin_execution_result(&result);
    }
    Ok(())
}

pub(super) fn parse_plugin_call_args(args: Vec<String>) -> Result<PluginCallCommand, CliError> {
    let mut paths = Vec::new();
    let mut arguments = serde_json::json!({});
    let mut timeout_ms = 10_000;
    let mut max_output_bytes = 1_048_576;
    let mut require_trusted = false;
    let mut trust_policy = None;
    let mut json = false;
    let mut positional = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--path" | "-p" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --path".to_owned()));
                };
                paths.push(PathBuf::from(value));
            }
            "--arguments-json" | "--args-json" | "--args" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --arguments-json".to_owned(),
                    ));
                };
                arguments = serde_json::from_str(value).map_err(|error| {
                    CliError::Failure(format!("invalid arguments JSON: {error}"))
                })?;
            }
            "--timeout-ms" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --timeout-ms".to_owned(),
                    ));
                };
                timeout_ms = parse_u64("--timeout-ms", value)?;
            }
            "--max-output-bytes" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --max-output-bytes".to_owned(),
                    ));
                };
                max_output_bytes = parse_usize("--max-output-bytes", value)?;
            }
            "--require-trusted" => require_trusted = true,
            "--trust-policy" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure(
                        "missing value for --trust-policy".to_owned(),
                    ));
                };
                trust_policy = Some(PathBuf::from(value));
            }
            "--json" => json = true,
            "--help" | "-h" => return Ok(PluginCallCommand::Help),
            value if value.starts_with('-') => {
                return Err(CliError::Failure(format!(
                    "unknown plugin-call argument: {value}"
                )));
            }
            value => positional.push(value.to_owned()),
        }
        index += 1;
    }

    if positional.len() != 2 {
        return Err(CliError::Failure(
            "plugin-call requires <plugin-id> and <tool>".to_owned(),
        ));
    }
    Ok(PluginCallCommand::Run(PluginCallArgs {
        plugin_id: positional.remove(0),
        tool: positional.remove(0),
        arguments,
        paths,
        timeout_ms,
        max_output_bytes,
        require_trusted,
        trust_policy,
        json,
    }))
}

pub(super) fn plugin_execution_dto(
    result: peekaboox_plugins::PluginToolExecutionResult,
) -> PluginToolExecutionResultDto {
    PluginToolExecutionResultDto {
        ok: result.ok,
        plugin_id: result.plugin_id,
        tool: result.tool,
        exit_code: result.exit_code,
        stdout: result.stdout,
        stderr: result.stderr,
        result: result.result,
        error: result.error,
    }
}

pub(super) fn plugin_list_dto(
    result: peekaboox_plugins::PluginDiscoveryResult,
) -> PluginListResultDto {
    PluginListResultDto {
        sdk_version: peekaboox_plugins::PLUGIN_SDK_VERSION.to_owned(),
        plugins: result.plugins.iter().map(plugin_dto).collect(),
        errors: result
            .errors
            .iter()
            .map(|error| PluginDiscoveryErrorDto {
                path: error.path.display().to_string(),
                message: error.message.clone(),
            })
            .collect(),
    }
}

pub(super) fn plugin_dto(plugin: &peekaboox_plugins::PluginDescriptor) -> PluginDto {
    let entrypoint = plugin.manifest.entrypoint.as_ref();
    PluginDto {
        id: plugin.manifest.id.clone(),
        name: plugin.manifest.name.clone(),
        version: plugin.manifest.version.clone(),
        description: plugin.manifest.description.clone(),
        root_dir: plugin.root_dir.display().to_string(),
        manifest_path: plugin.manifest_path.display().to_string(),
        capabilities: plugin.manifest.capabilities.clone(),
        entrypoint_kind: entrypoint.map(|entrypoint| match entrypoint.kind {
            peekaboox_plugins::PluginEntrypointKind::Process => "process".to_owned(),
        }),
        entrypoint_command: entrypoint
            .map(|entrypoint| entrypoint.command.clone())
            .unwrap_or_default(),
        tools: plugin
            .manifest
            .tools
            .iter()
            .map(|tool| PluginToolDto {
                name: tool.name.clone(),
                description: tool.description.clone(),
                capabilities: tool.capabilities.clone(),
                input_schema_json: serde_json::to_string(&tool.input_schema)
                    .unwrap_or_else(|_| "{}".to_owned()),
            })
            .collect(),
        metadata: {
            let mut metadata = plugin.manifest.metadata.clone();
            if let Ok(fingerprint) = peekaboox_plugins::plugin_manifest_sha256(plugin) {
                metadata.insert("peekaboox.manifest_sha256".to_owned(), fingerprint);
            }
            metadata
        },
    }
}

pub(super) fn print_plugin_list_dto(result: &PluginListResultDto) {
    println!(
        "plugins sdk_version={} count={} errors={}",
        result.sdk_version,
        result.plugins.len(),
        result.errors.len()
    );
    for plugin in &result.plugins {
        let tool_names = plugin
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "plugin id={} name={} version={} capabilities={} tools={} path={}",
            plugin.id,
            plugin.name,
            plugin.version,
            join_or_dash(&plugin.capabilities),
            string_or_dash(&tool_names),
            plugin.manifest_path
        );
    }
    for error in &result.errors {
        println!(
            "plugin_error path={} message={}",
            error.path,
            error.message.replace('\n', " ")
        );
    }
}

pub(super) fn print_plugin_execution_result(result: &PluginToolExecutionResultDto) {
    println!(
        "plugin_tool plugin_id={} tool={} ok={} exit_code={}",
        result.plugin_id, result.tool, result.ok, result.exit_code
    );
    if let Some(value) = &result.result {
        println!("result={value}");
    }
    if let Some(error) = &result.error {
        println!("error={}", error.replace('\n', " "));
    }
    if !result.stdout.trim().is_empty() {
        println!("stdout={}", result.stdout.trim().replace('\n', "\\n"));
    }
    if !result.stderr.trim().is_empty() {
        println!("stderr={}", result.stderr.trim().replace('\n', "\\n"));
    }
}

pub(super) fn join_or_dash(values: &[String]) -> String {
    if values.is_empty() {
        "-".to_owned()
    } else {
        values.join(",")
    }
}

pub(super) fn string_or_dash(value: &str) -> &str {
    if value.is_empty() { "-" } else { value }
}
