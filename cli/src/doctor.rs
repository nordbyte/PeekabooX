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
        match self.status {
            CheckStatus::Ok => "ok",
            CheckStatus::Warn => "warn",
            CheckStatus::Fail => "fail",
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "status": self.status_label(),
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
        let status = if has_failures { "fail" } else { "ok" };
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "status": status,
                "checks": checks.iter().map(DoctorCheck::to_json).collect::<Vec<_>>(),
            }))
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
            .map(|profile| format!("{}:{}", profile.id, profile.targets.join("|")))
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
