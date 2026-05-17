use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use peekaboox_core::{BackendKind, Point};
use peekaboox_input::{InputAction, MouseButton};
use serde_json::{Value, json};

use crate::CliError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CheckStatus {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DoctorCheck {
    name: String,
    status: CheckStatus,
    detail: String,
}

impl DoctorCheck {
    fn ok(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Ok,
            detail: detail.into(),
        }
    }

    fn warn(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Warn,
            detail: detail.into(),
        }
    }

    fn fail(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Fail,
            detail: detail.into(),
        }
    }

    fn status_label(&self) -> &'static str {
        status_label(self.status)
    }

    fn severity_label(&self) -> &'static str {
        severity_label(self.status)
    }

    fn category_label(&self) -> &'static str {
        check_category(self.name.as_str())
    }

    fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "category": self.category_label(),
            "status": self.status_label(),
            "severity": self.severity_label(),
            "detail": self.detail,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DoctorArgs {
    json: bool,
    strict: bool,
}

pub(crate) fn run(args: Vec<String>) -> Result<(), CliError> {
    let args = parse_args(args)?;
    let checks = collect_checks();
    let has_failures = checks.iter().any(|check| check.status == CheckStatus::Fail);

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&doctor_json(&checks))
                .map_err(|error| CliError::Failure(error.to_string()))?
        );
    } else {
        for check in &checks {
            println!(
                "{:<5} {:<32} {}",
                check.status_label(),
                check.name,
                check.detail
            );
        }
    }

    if args.strict && has_failures {
        Err(CliError::Failure(
            "doctor found required environment checks that failed".to_owned(),
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn print_usage() {
    println!("Usage: peekaboox doctor [--json] [--strict]");
}

pub(crate) fn diagnose(args: Vec<String>) -> Result<(), CliError> {
    let DiagnoseCommand::Bundle(args) = parse_diagnose_args(args)? else {
        print_diagnose_usage();
        return Err(CliError::HelpRequested);
    };
    let bundle = write_diagnose_bundle(args.output.as_deref())?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "ok": true,
                "bundle": bundle.display().to_string(),
            }))
            .map_err(|error| CliError::Failure(error.to_string()))?
        );
    } else {
        println!("diagnose bundle={}", bundle.display());
    }
    Ok(())
}

pub(crate) fn print_diagnose_usage() {
    println!("Usage: peekaboox diagnose bundle [--output <dir>] [--json]");
}

fn parse_args(args: Vec<String>) -> Result<DoctorArgs, CliError> {
    let mut json = false;
    let mut strict = false;

    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            "--strict" => strict = true,
            "--help" | "-h" => {
                print_usage();
                return Err(CliError::HelpRequested);
            }
            unknown => {
                return Err(CliError::Failure(format!(
                    "unknown doctor argument: {unknown}"
                )));
            }
        }
    }

    Ok(DoctorArgs { json, strict })
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DiagnoseCommand {
    Bundle(DiagnoseBundleArgs),
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiagnoseBundleArgs {
    output: Option<PathBuf>,
    json: bool,
}

fn parse_diagnose_args(args: Vec<String>) -> Result<DiagnoseCommand, CliError> {
    let Some(command) = args.first() else {
        return Ok(DiagnoseCommand::Help);
    };
    if command == "--help" || command == "-h" {
        return Ok(DiagnoseCommand::Help);
    }
    if command != "bundle" {
        return Err(CliError::Failure(format!(
            "unknown diagnose command: {command}"
        )));
    }
    let mut output = None;
    let mut json = false;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--output" | "-o" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(CliError::Failure("missing value for --output".to_owned()));
                };
                output = Some(PathBuf::from(value));
            }
            "--json" => json = true,
            "--help" | "-h" => return Ok(DiagnoseCommand::Help),
            unknown => {
                return Err(CliError::Failure(format!(
                    "unknown diagnose bundle argument: {unknown}"
                )));
            }
        }
        index += 1;
    }
    Ok(DiagnoseCommand::Bundle(DiagnoseBundleArgs { output, json }))
}

fn write_diagnose_bundle(output: Option<&Path>) -> Result<PathBuf, CliError> {
    let bundle = output
        .map(Path::to_path_buf)
        .unwrap_or_else(default_diagnose_bundle_path);
    std::fs::create_dir_all(&bundle).map_err(|error| {
        CliError::Failure(format!("failed to create {}: {error}", bundle.display()))
    })?;
    write_json_file(&bundle.join("doctor.json"), &doctor_json(&collect_checks()))?;
    write_json_file(&bundle.join("versions.json"), &versions_json())?;
    write_json_file(&bundle.join("environment.json"), &environment_json())?;
    write_json_file(
        &bundle.join("capture-backends.json"),
        &capture_backends_json(),
    )?;
    write_json_file(
        &bundle.join("manifest.json"),
        &diagnose_manifest_json(&bundle),
    )?;
    Ok(bundle)
}

fn write_json_file(path: &Path, value: &Value) -> Result<(), CliError> {
    std::fs::write(
        path,
        serde_json::to_string_pretty(value)
            .map_err(|error| CliError::Failure(error.to_string()))?
            + "\n",
    )
    .map_err(|error| CliError::Failure(format!("failed to write {}: {error}", path.display())))
}

fn default_diagnose_bundle_path() -> PathBuf {
    PathBuf::from("target")
        .join("diagnose-bundles")
        .join(format!("peekaboox-diagnose-{}", unix_time_ms()))
}

fn unix_time_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

fn collect_checks() -> Vec<DoctorCheck> {
    let mut checks = Vec::new();

    checks.extend(environment_checks());
    checks.extend(command_checks());
    checks.extend(capture_checks());
    checks.extend(window_checks());
    checks.extend(input_checks());
    checks.extend(ocr_checks());
    checks.extend(python_grpc_checks());
    checks.extend(desktop_profile_checks());

    checks
}

fn doctor_json(checks: &[DoctorCheck]) -> Value {
    let has_failures = checks.iter().any(|check| check.status == CheckStatus::Fail);
    json!({
        "status": if has_failures { "fail" } else { "ok" },
        "categories": category_summaries(checks),
        "checks": checks.iter().map(DoctorCheck::to_json).collect::<Vec<_>>(),
    })
}

fn versions_json() -> Value {
    json!({
        "peekaboox": env!("CARGO_PKG_VERSION"),
        "rust_target": std::env::consts::ARCH,
        "os": std::env::consts::OS,
    })
}

fn environment_json() -> Value {
    let keys = [
        "XDG_SESSION_TYPE",
        "XDG_CURRENT_DESKTOP",
        "WAYLAND_DISPLAY",
        "DISPLAY",
        "XDG_RUNTIME_DIR",
        "PIPEWIRE_RUNTIME_DIR",
        "PIPEWIRE_REMOTE",
        "PATH",
    ];
    json!({
        "variables": keys
            .into_iter()
            .map(|key| {
                let value = std::env::var(key).unwrap_or_else(|_| "<unset>".to_owned());
                (key.to_owned(), Value::String(value))
            })
            .collect::<serde_json::Map<_, _>>(),
        "redaction": "only non-secret desktop/session diagnostics are included",
    })
}

fn capture_backends_json() -> Value {
    let environment = peekaboox_capture::CaptureEnvironment::detect();
    let output = Path::new("screenshot.png");
    let capabilities = peekaboox_capture::capture_backend_capabilities(&environment, output)
        .into_iter()
        .map(|capability| {
            json!({
                "name": capability.name,
                "backend_kind": backend_kind_label(capability.backend_kind),
                "command": capability.command,
                "available": capability.available,
                "supports_output": capability.supports_output,
                "supports_file_capture": capability.supports_file_capture,
                "supports_stdout_capture": capability.supports_stdout_capture,
                "supports_stdout_region_capture": capability.supports_stdout_region_capture,
                "selected": capability.selected,
                "reason": capability.reason,
            })
        })
        .collect::<Vec<_>>();
    let zero_copy = peekaboox_capture::zero_copy_capture_capabilities(&environment)
        .into_iter()
        .map(|capability| {
            json!({
                "backend_name": capability.backend_name,
                "backend_kind": backend_kind_label(capability.backend_kind),
                "transport": capability.transport.name(),
                "availability": format!("{:?}", capability.availability),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "session_type": environment.session_type.name(),
        "desktop": environment.current_desktop,
        "pipewire_session_available": environment.pipewire_session_available,
        "image_backends": capabilities,
        "zero_copy_backends": zero_copy,
    })
}

fn diagnose_manifest_json(bundle: &Path) -> Value {
    json!({
        "schema_version": 1,
        "created_at_unix_ms": unix_time_ms(),
        "bundle": bundle.display().to_string(),
        "files": [
            "doctor.json",
            "versions.json",
            "environment.json",
            "capture-backends.json",
            "manifest.json"
        ],
    })
}

fn environment_checks() -> Vec<DoctorCheck> {
    let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unset".to_owned());
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "unset".to_owned());
    let wayland = std::env::var("WAYLAND_DISPLAY").ok();
    let display = std::env::var("DISPLAY").ok();
    let mut checks = vec![DoctorCheck::ok(
        "desktop-session",
        format!("XDG_SESSION_TYPE={session} XDG_CURRENT_DESKTOP={desktop}"),
    )];
    if wayland.is_some() || display.is_some() {
        checks.push(DoctorCheck::ok(
            "display-server",
            format!(
                "WAYLAND_DISPLAY={} DISPLAY={}",
                wayland.as_deref().unwrap_or("-"),
                display.as_deref().unwrap_or("-")
            ),
        ));
    } else {
        checks.push(DoctorCheck::fail(
            "display-server",
            "neither WAYLAND_DISPLAY nor DISPLAY is set",
        ));
    }
    checks
}

fn command_checks() -> Vec<DoctorCheck> {
    [
        ("gdbus", "GNOME Shell and portal helpers"),
        ("gtk-launch", "desktop app launch fallback"),
        ("tesseract", "OCR backend"),
        ("xdotool", "X11 window/input backend"),
        ("ydotool", "uinput input backend"),
        ("wtype", "Wayland text input backend"),
        ("wl-copy", "Wayland clipboard backend"),
        ("xclip", "X11 clipboard backend"),
        ("xsel", "X11 clipboard backend"),
        ("python3", "Python client/runtime checks"),
    ]
    .into_iter()
    .map(|(command, purpose)| {
        if command_exists(command) {
            DoctorCheck::ok(format!("command:{command}"), purpose)
        } else {
            DoctorCheck::warn(
                format!("command:{command}"),
                format!("missing; {purpose} disabled"),
            )
        }
    })
    .collect()
}

fn capture_checks() -> Vec<DoctorCheck> {
    let environment = peekaboox_capture::CaptureEnvironment::detect();
    let file_backend =
        peekaboox_capture::select_backend(&environment, std::path::Path::new("screenshot.png"));
    let frame_backend = peekaboox_capture::select_frame_backend(&environment);
    let region_backend = peekaboox_capture::select_region_frame_backend(&environment);
    let zero_copy = peekaboox_capture::zero_copy_capture_capabilities(&environment);
    let mut checks = vec![backend_check(
        "capture-file",
        file_backend.map(|backend| {
            format!(
                "{} ({})",
                backend.name(),
                backend_kind_label(backend.backend_kind())
            )
        }),
    )];
    checks.push(optional_backend_check(
        "capture-frame",
        frame_backend.map(|backend| {
            format!(
                "{} ({})",
                backend.name(),
                backend_kind_label(backend.backend_kind())
            )
        }),
    ));
    checks.push(optional_backend_check(
        "capture-region",
        region_backend.map(|backend| {
            format!(
                "{} ({})",
                backend.name(),
                backend_kind_label(backend.backend_kind())
            )
        }),
    ));
    checks.extend(zero_copy.into_iter().map(|capability| {
        if capability.availability.is_available() {
            DoctorCheck::ok(
                "capture-dmabuf",
                format!("{} available", capability.transport.name()),
            )
        } else {
            DoctorCheck::warn(
                "capture-dmabuf",
                format!(
                    "{}: {:?}",
                    capability.transport.name(),
                    capability.availability
                ),
            )
        }
    }));
    checks
}

fn window_checks() -> Vec<DoctorCheck> {
    let environment = peekaboox_windows::WindowEnvironment::detect();
    let candidates = peekaboox_windows::candidate_backends(&environment);
    if candidates.is_empty() {
        vec![DoctorCheck::fail(
            "windows",
            "no window enumeration backend available",
        )]
    } else {
        vec![DoctorCheck::ok(
            "windows",
            candidates
                .iter()
                .map(|backend| {
                    format!(
                        "{} ({})",
                        backend.name(),
                        backend_kind_label(backend.backend_kind())
                    )
                })
                .collect::<Vec<_>>()
                .join(", "),
        )]
    }
}

fn input_checks() -> Vec<DoctorCheck> {
    let environment = peekaboox_input::InputEnvironment::detect();
    [
        (
            "input-click",
            InputAction::Click {
                position: Point::new(1, 1),
                button: MouseButton::Left,
            },
        ),
        ("input-type", InputAction::TypeText("PeekabooX".to_owned())),
        (
            "input-paste",
            InputAction::PasteText {
                text: "PeekabooX".to_owned(),
                preserve_clipboard: true,
            },
        ),
        (
            "input-hotkey",
            InputAction::Hotkey(vec!["ctrl+s".to_owned()]),
        ),
    ]
    .into_iter()
    .map(|(name, action)| {
        let candidates = peekaboox_input::candidate_backends(&environment, &action);
        if candidates.is_empty() {
            DoctorCheck::warn(name, "no non-mutating backend candidate detected")
        } else {
            DoctorCheck::ok(
                name,
                candidates
                    .iter()
                    .map(|backend| {
                        format!(
                            "{} ({})",
                            backend.name(),
                            backend_kind_label(backend.backend_kind())
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        }
    })
    .collect()
}

fn ocr_checks() -> Vec<DoctorCheck> {
    let backend = peekaboox_vision::TesseractOcrBackend::new(
        "tesseract",
        peekaboox_vision::OcrOptions::default(),
    );
    if backend.is_available() {
        vec![DoctorCheck::ok("ocr", "tesseract available")]
    } else {
        vec![DoctorCheck::warn("ocr", "tesseract not available")]
    }
}

fn python_grpc_checks() -> Vec<DoctorCheck> {
    if !command_exists("python3") {
        return vec![DoctorCheck::warn("python-grpc", "python3 not found")];
    }
    let status = Command::new("python3")
        .args(["-c", "import grpc, google.protobuf; print('grpc-ok')"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if status.is_ok_and(|status| status.success()) {
        vec![DoctorCheck::ok(
            "python-grpc",
            "grpc and protobuf import successfully",
        )]
    } else {
        vec![DoctorCheck::warn(
            "python-grpc",
            "grpc/protobuf imports failed; install python[dev] or package dependencies",
        )]
    }
}

fn desktop_profile_checks() -> Vec<DoctorCheck> {
    let profiles = peekaboox_desktop::desktop_profiles();
    vec![DoctorCheck::ok(
        "desktop-profiles",
        profiles
            .iter()
            .map(|profile| {
                format!(
                    "{}:{}",
                    profile.id,
                    profile
                        .targets
                        .iter()
                        .map(|target| target.name.as_str())
                        .collect::<Vec<_>>()
                        .join("|")
                )
            })
            .collect::<Vec<_>>()
            .join(", "),
    )]
}

fn backend_check(name: &str, detail: Option<String>) -> DoctorCheck {
    match detail {
        Some(detail) => DoctorCheck::ok(name, detail),
        None => DoctorCheck::fail(name, "no backend candidate detected"),
    }
}

fn optional_backend_check(name: &str, detail: Option<String>) -> DoctorCheck {
    match detail {
        Some(detail) => DoctorCheck::ok(name, detail),
        None => DoctorCheck::warn(name, "no direct backend candidate detected"),
    }
}

fn backend_kind_label(kind: BackendKind) -> String {
    format!("{kind:?}").to_ascii_lowercase()
}

fn severity_label(status: CheckStatus) -> &'static str {
    match status {
        CheckStatus::Ok => "info",
        CheckStatus::Warn => "warning",
        CheckStatus::Fail => "error",
    }
}

fn status_label(status: CheckStatus) -> &'static str {
    match status {
        CheckStatus::Ok => "ok",
        CheckStatus::Warn => "warn",
        CheckStatus::Fail => "fail",
    }
}

fn check_category(name: &str) -> &'static str {
    if name.starts_with("capture-") {
        return "capture";
    }
    if name.starts_with("input-") {
        return "input";
    }
    match name {
        "desktop-session" | "display-server" | "windows" | "desktop-profiles" | "command:gdbus"
        | "command:gtk-launch" => "desktop",
        "ocr" | "command:tesseract" => "ocr",
        "python-grpc" | "command:python3" => "python",
        "command:xdotool" | "command:ydotool" | "command:wtype" | "command:wl-copy"
        | "command:xclip" | "command:xsel" => "input",
        _ => "general",
    }
}

fn category_summaries(checks: &[DoctorCheck]) -> Vec<Value> {
    let mut categories = checks
        .iter()
        .map(DoctorCheck::category_label)
        .collect::<Vec<_>>();
    categories.sort_unstable();
    categories.dedup();
    categories
        .into_iter()
        .map(|category| {
            let mut ok_count = 0;
            let mut warn_count = 0;
            let mut fail_count = 0;
            for check in checks
                .iter()
                .filter(|check| check.category_label() == category)
            {
                match check.status {
                    CheckStatus::Ok => ok_count += 1,
                    CheckStatus::Warn => warn_count += 1,
                    CheckStatus::Fail => fail_count += 1,
                }
            }
            let status = if fail_count > 0 {
                CheckStatus::Fail
            } else if warn_count > 0 {
                CheckStatus::Warn
            } else {
                CheckStatus::Ok
            };
            json!({
                "name": category,
                "status": status_label(status),
                "severity": severity_label(status),
                "ok_count": ok_count,
                "warn_count": warn_count,
                "fail_count": fail_count,
                "total_count": ok_count + warn_count + fail_count,
            })
        })
        .collect()
}

fn command_exists(command: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {}", shell_escape(command)))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn shell_escape(value: &str) -> String {
    let escaped = value.replace('\'', "'\\''");
    format!("'{escaped}'")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        DiagnoseBundleArgs, DiagnoseCommand, DoctorCheck, category_summaries, parse_diagnose_args,
    };

    #[test]
    fn doctor_check_json_includes_category_and_severity() {
        let check = DoctorCheck::warn("capture-frame", "no direct backend candidate detected");
        let payload = check.to_json();

        assert_eq!(payload["category"], "capture");
        assert_eq!(payload["status"], "warn");
        assert_eq!(payload["severity"], "warning");
    }

    #[test]
    fn category_summaries_roll_up_worst_status() {
        let summaries = category_summaries(&[
            DoctorCheck::ok("input-click", "ok"),
            DoctorCheck::warn("command:wtype", "missing"),
            DoctorCheck::fail("display-server", "missing display"),
        ]);

        let input = summaries
            .iter()
            .find(|summary| summary["name"] == "input")
            .unwrap();
        let desktop = summaries
            .iter()
            .find(|summary| summary["name"] == "desktop")
            .unwrap();

        assert_eq!(input["status"], "warn");
        assert_eq!(input["severity"], "warning");
        assert_eq!(input["total_count"], 2);
        assert_eq!(desktop["status"], "fail");
        assert_eq!(desktop["severity"], "error");
    }

    #[test]
    fn diagnose_bundle_accepts_output_and_json() {
        let command = parse_diagnose_args(vec![
            "bundle".to_owned(),
            "--output".to_owned(),
            "target/diagnose-test".to_owned(),
            "--json".to_owned(),
        ])
        .unwrap();

        assert_eq!(
            command,
            DiagnoseCommand::Bundle(DiagnoseBundleArgs {
                output: Some(PathBuf::from("target/diagnose-test")),
                json: true,
            })
        );
    }
}
