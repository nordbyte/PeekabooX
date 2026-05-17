use super::*;

pub(super) fn spawn_accessibility_event_listener(
    config: &ServerConfig,
    accessibility_cache: SharedAccessibilityCache,
    audit: SharedAudit,
    shutdown: Arc<AtomicBool>,
) -> Option<std::thread::JoinHandle<()>> {
    if !config.accessibility_events {
        audit_write(
            &audit,
            "accessibility_events_disabled",
            Some(API_VERSION),
            "ok",
            None,
            json!({}),
        );
        return None;
    }

    Some(std::thread::spawn(
        move || match run_accessibility_event_listener(
            Arc::clone(&accessibility_cache),
            Arc::clone(&audit),
            shutdown,
        ) {
            Ok(()) => audit_write(
                &audit,
                "accessibility_events_stopped",
                Some(API_VERSION),
                "ok",
                None,
                json!({}),
            ),
            Err(error) => audit_write(
                &audit,
                "accessibility_events",
                Some(API_VERSION),
                "error",
                Some(&error),
                json!({}),
            ),
        },
    ))
}

pub(super) fn run_accessibility_event_listener(
    accessibility_cache: SharedAccessibilityCache,
    audit: SharedAudit,
    shutdown: Arc<AtomicBool>,
) -> Result<(), String> {
    let address =
        peekaboox_accessibility::atspi_bus_address().map_err(|error| error.to_string())?;
    let connection = Connection::new_address(&address)
        .map_err(|error| format!("failed to connect to AT-SPI event bus: {error}"))?;
    register_atspi_events(&connection, &audit);
    subscribe_atspi_event_interfaces(&connection, accessibility_cache, Arc::clone(&audit))?;

    audit_write(
        &audit,
        "accessibility_events_started",
        Some(API_VERSION),
        "ok",
        None,
        json!({
            "interfaces": ATSPI_EVENT_INTERFACES,
            "registrations": ATSPI_EVENT_REGISTRATIONS
        }),
    );

    while !shutdown.load(Ordering::Relaxed) {
        connection
            .process(Duration::from_millis(250))
            .map_err(|error| format!("AT-SPI event processing failed: {error}"))?;
    }

    Ok(())
}

pub(super) fn register_atspi_events(connection: &Connection, audit: &SharedAudit) {
    let proxy = connection.with_proxy(
        ATSPI_EVENT_REGISTRY_DESTINATION,
        ATSPI_EVENT_REGISTRY_PATH,
        Duration::from_secs(2),
    );

    for event_name in ATSPI_EVENT_REGISTRATIONS {
        let result: Result<(), dbus::Error> = proxy.method_call(
            ATSPI_EVENT_REGISTRY_INTERFACE,
            "RegisterEvent",
            (*event_name, Vec::<String>::new(), ""),
        );
        if let Err(error) = result {
            audit_write(
                audit,
                "accessibility_event_registration",
                Some(API_VERSION),
                "error",
                Some(&error.to_string()),
                json!({ "event": event_name }),
            );
        }
    }
}

pub(super) fn subscribe_atspi_event_interfaces(
    connection: &Connection,
    accessibility_cache: SharedAccessibilityCache,
    audit: SharedAudit,
) -> Result<(), String> {
    connection.set_signal_match_mode(true);
    for interface in ATSPI_EVENT_INTERFACES {
        let rule = atspi_event_match_rule(interface);
        let cache = Arc::clone(&accessibility_cache);
        let audit = Arc::clone(&audit);
        connection
            .add_match(rule, move |_: (), _, message| {
                let reason = atspi_event_reason(message);
                if invalidate_accessibility_cache(&cache) {
                    audit_write(
                        &audit,
                        "accessibility_cache_invalidated",
                        Some(API_VERSION),
                        "ok",
                        None,
                        json!({ "reason": reason }),
                    );
                }
                true
            })
            .map_err(|error| format!("failed to subscribe to AT-SPI events: {error}"))?;
    }

    Ok(())
}

pub(super) fn atspi_event_match_rule(interface: &'static str) -> MatchRule<'static> {
    let mut rule = MatchRule::new();
    rule.msg_type = Some(MessageType::Signal);
    rule.interface = Some(interface.into());
    rule
}

pub(super) fn atspi_event_reason(message: &Message) -> String {
    let interface = message
        .interface()
        .map(|interface| interface.to_string())
        .unwrap_or_else(|| "unknown-interface".to_owned());
    let member = message
        .member()
        .map(|member| member.to_string())
        .unwrap_or_else(|| "unknown-member".to_owned());

    format!("{interface}.{member}")
}

#[derive(Debug)]
pub(super) struct EmergencyHotkeyDevice {
    pub(super) path: PathBuf,
    pub(super) file: File,
}

pub(super) fn spawn_emergency_hotkey_listener(
    config: &ServerConfig,
    audit: SharedAudit,
    shutdown: Arc<AtomicBool>,
) -> Option<std::thread::JoinHandle<()>> {
    if !config.emergency_hotkey {
        audit_write(
            &audit,
            "emergency_hotkey_disabled",
            Some(API_VERSION),
            "ok",
            None,
            emergency_hotkey_details(),
        );
        return None;
    }

    Some(std::thread::spawn(
        move || match run_emergency_hotkey_listener(Arc::clone(&audit), shutdown) {
            Ok(()) => audit_write(
                &audit,
                "emergency_hotkey_stopped",
                Some(API_VERSION),
                "ok",
                None,
                emergency_hotkey_details(),
            ),
            Err(error) => audit_write(
                &audit,
                "emergency_hotkey",
                Some(API_VERSION),
                "error",
                Some(&error),
                emergency_hotkey_details(),
            ),
        },
    ))
}

pub(super) fn run_emergency_hotkey_listener(
    audit: SharedAudit,
    shutdown: Arc<AtomicBool>,
) -> Result<(), String> {
    let mut devices = open_emergency_hotkey_devices(INPUT_EVENT_DIR)?;
    if devices.is_empty() {
        return Err(format!(
            "no readable {INPUT_EVENT_DIR}/event* devices; {EMERGENCY_STOP_HOTKEY_LABEL} requires Linux input device read access"
        ));
    }

    audit_write(
        &audit,
        "emergency_hotkey_started",
        Some(API_VERSION),
        "ok",
        None,
        json!({
            "hotkey": EMERGENCY_STOP_HOTKEY_LABEL,
            "devices": devices.iter().map(|device| device.path.display().to_string()).collect::<Vec<_>>()
        }),
    );

    let mut state = EmergencyHotkeyState::default();
    while !shutdown.load(Ordering::Relaxed) {
        let mut index = 0;
        while index < devices.len() {
            match read_emergency_hotkey_device(&mut devices[index], &mut state) {
                Ok(true) => {
                    shutdown.store(true, Ordering::Relaxed);
                    perform_emergency_stop(&audit, "emergency_hotkey_triggered");
                    return Ok(());
                }
                Ok(false) => index += 1,
                Err(error) => {
                    let device = devices.remove(index);
                    audit_write(
                        &audit,
                        "emergency_hotkey_device",
                        Some(API_VERSION),
                        "error",
                        Some(&error),
                        json!({
                            "hotkey": EMERGENCY_STOP_HOTKEY_LABEL,
                            "device": device.path.display().to_string()
                        }),
                    );
                }
            }
        }

        if devices.is_empty() {
            return Err("all emergency hotkey input devices became unavailable".to_owned());
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    Ok(())
}

pub(super) fn open_emergency_hotkey_devices(
    input_dir: impl AsRef<Path>,
) -> Result<Vec<EmergencyHotkeyDevice>, String> {
    let entries = fs::read_dir(input_dir.as_ref()).map_err(|error| {
        format!(
            "failed to read input device directory {}: {error}",
            input_dir.as_ref().display()
        )
    })?;
    let mut devices = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with("event") {
            continue;
        }
        match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&path)
        {
            Ok(file) => devices.push(EmergencyHotkeyDevice { path, file }),
            Err(error) => {
                eprintln!(
                    "failed to open emergency hotkey device {}: {error}",
                    path.display()
                );
            }
        }
    }

    Ok(devices)
}

pub(super) fn read_emergency_hotkey_device(
    device: &mut EmergencyHotkeyDevice,
    state: &mut EmergencyHotkeyState,
) -> Result<bool, String> {
    let event_size = linux_input_event_size();
    let mut buffer = vec![0_u8; event_size * 32];

    loop {
        match device.file.read(&mut buffer) {
            Ok(0) => return Ok(false),
            Ok(bytes_read) => {
                for chunk in buffer[..bytes_read].chunks_exact(event_size) {
                    let Some((event_type, key_code, value)) = parse_linux_input_event(chunk) else {
                        continue;
                    };
                    if state.update_linux_key_event(event_type, key_code, value) {
                        return Ok(true);
                    }
                }
                if bytes_read < buffer.len() {
                    return Ok(false);
                }
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(false),
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => {
                return Err(format!("failed to read {}: {error}", device.path.display()));
            }
        }
    }
}

pub(super) fn linux_input_event_size() -> usize {
    std::mem::size_of::<libc::timeval>() + 8
}

pub(super) fn parse_linux_input_event(bytes: &[u8]) -> Option<(u16, u16, i32)> {
    let time_size = std::mem::size_of::<libc::timeval>();
    if bytes.len() < time_size + 8 {
        return None;
    }

    let event_type = u16::from_ne_bytes(bytes[time_size..time_size + 2].try_into().ok()?);
    let key_code = u16::from_ne_bytes(bytes[time_size + 2..time_size + 4].try_into().ok()?);
    let value = i32::from_ne_bytes(bytes[time_size + 4..time_size + 8].try_into().ok()?);
    Some((event_type, key_code, value))
}

pub(super) fn perform_emergency_stop(audit: &SharedAudit, event: &str) {
    match peekaboox_input::emergency_stop() {
        Ok(()) => audit_write(
            audit,
            event,
            Some(API_VERSION),
            "ok",
            None,
            emergency_hotkey_details(),
        ),
        Err(error) => audit_write(
            audit,
            event,
            Some(API_VERSION),
            "error",
            Some(&error.to_string()),
            emergency_hotkey_details(),
        ),
    }
}

pub(super) fn emergency_hotkey_details() -> serde_json::Value {
    json!({ "hotkey": EMERGENCY_STOP_HOTKEY_LABEL })
}
