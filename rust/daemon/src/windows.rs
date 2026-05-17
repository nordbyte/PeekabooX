use super::*;

pub(super) fn grpc_list_windows_audit_details(
    request: &proto::ListWindowsRequest,
) -> serde_json::Value {
    json!({
        "id": request.id.as_deref(),
        "app": request.app.as_deref(),
        "title": request.title.as_deref(),
        "title_regex": request.title_regex.as_deref(),
        "focused": request.focused,
        "limit": request.limit,
        "sort": request.sort.as_deref(),
        "backend": request.backend.as_deref(),
        "diagnose": request.diagnose,
    })
}

pub(super) fn window_query_from_proto(
    request: proto::ListWindowsRequest,
) -> Result<peekaboox_windows::WindowQuery, Status> {
    window_query_from_fields(WindowQueryFields {
        id: request.id,
        app: request.app,
        title: request.title,
        title_regex: request.title_regex,
        focused: request.focused,
        limit: request.limit.map(|value| value as usize),
        sort: request.sort,
        backend: request.backend,
        diagnose: request.diagnose,
    })
    .map_err(Status::invalid_argument)
}

pub(super) struct WindowQueryFields {
    pub(super) id: Option<String>,
    pub(super) app: Option<String>,
    pub(super) title: Option<String>,
    pub(super) title_regex: Option<String>,
    pub(super) focused: bool,
    pub(super) limit: Option<usize>,
    pub(super) sort: Option<String>,
    pub(super) backend: Option<String>,
    pub(super) diagnose: bool,
}

pub(super) fn window_query_from_fields(
    fields: WindowQueryFields,
) -> Result<peekaboox_windows::WindowQuery, String> {
    let sort = match clean_optional_string(fields.sort) {
        Some(value) => peekaboox_windows::WindowSort::from_name(&value)
            .ok_or_else(|| format!("invalid windows sort: {value}"))?,
        None => peekaboox_windows::WindowSort::Backend,
    };
    let backend = match clean_optional_string(fields.backend) {
        Some(value) => peekaboox_windows::WindowBackendSelection::from_name(&value)
            .ok_or_else(|| format!("invalid windows backend: {value}"))?,
        None => peekaboox_windows::WindowBackendSelection::Auto,
    };

    if fields.limit == Some(0) {
        return Err("windows limit must be greater than zero".to_owned());
    }

    Ok(peekaboox_windows::WindowQuery {
        id: clean_optional_string(fields.id),
        app: clean_optional_string(fields.app),
        title: clean_optional_string(fields.title),
        title_regex: clean_optional_string(fields.title_regex),
        focused_only: fields.focused,
        limit: fields.limit,
        sort,
        backend,
        diagnose: fields.diagnose,
    })
}

pub(super) fn clean_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

pub(super) fn window_list_result_dto(
    metadata: peekaboox_windows::WindowListMetadata,
) -> WindowListResultDto {
    WindowListResultDto {
        backend_name: metadata.backend_name,
        backend_kind: backend_kind_name(metadata.backend_kind),
        warnings: metadata.warnings,
        backend_reports: metadata
            .backend_reports
            .into_iter()
            .map(|report| WindowBackendReportDto {
                backend_name: report.backend_name,
                backend_kind: backend_kind_name(report.backend_kind),
                raw_window_count: report.raw_window_count,
                matched_window_count: report.matched_window_count,
                selected: report.selected,
                error: report.error,
            })
            .collect(),
        windows: metadata.windows.iter().map(WindowDto::from).collect(),
    }
}

pub(super) fn grpc_list_windows(
    list_windows: WindowListProvider,
    request: proto::ListWindowsRequest,
) -> Result<proto::ListWindowsResponse, Status> {
    let query = window_query_from_proto(request)?;
    let metadata = list_windows(query).map_err(|error| Status::internal(error.to_string()))?;
    Ok(proto::ListWindowsResponse {
        windows: metadata.windows.iter().map(proto_window_info).collect(),
        backend_name: metadata.backend_name,
        backend_kind: backend_kind_name(metadata.backend_kind),
        warnings: metadata.warnings,
        backend_reports: metadata
            .backend_reports
            .iter()
            .map(proto_window_backend_report)
            .collect(),
    })
}

pub(super) struct GrpcFindElementResult {
    pub(super) response: proto::FindElementResponse,
    pub(super) cache_hit: bool,
    pub(super) cache_age_ms: u128,
    pub(super) vision_fallback_used: bool,
}

pub(super) fn grpc_find_element(
    selector: &str,
    use_vision_fallback: bool,
    options: &ElementLookupOptions,
    accessibility_cache: &SharedAccessibilityCache,
) -> Result<GrpcFindElementResult, Status> {
    let result = find_elements_with_optional_vision_fallback(
        selector,
        use_vision_fallback,
        options,
        accessibility_cache,
    )
    .map_err(|error| Status::internal(error.to_string()))?;
    let elements = result.elements.iter().map(proto_ui_element).collect();

    Ok(GrpcFindElementResult {
        response: proto::FindElementResponse {
            elements,
            backend_name: result.backend_name,
            backend_kind: result.backend_kind,
            warnings: result.warnings,
            cache_hit: result.cache_hit,
            cache_age_ms: u64::try_from(result.cache_age_ms).unwrap_or(u64::MAX),
            vision_fallback_used: result.vision_fallback_used,
        },
        cache_hit: result.cache_hit,
        cache_age_ms: result.cache_age_ms,
        vision_fallback_used: result.vision_fallback_used,
    })
}

pub(super) fn grpc_desktop_state(
    accessibility_cache: &SharedAccessibilityCache,
    list_windows: WindowListProvider,
) -> Result<proto::DesktopState, Status> {
    let metadata = list_windows(peekaboox_windows::WindowQuery::default())
        .map_err(|error| Status::internal(error.to_string()))?;
    let active_window = metadata
        .windows
        .iter()
        .find(|window| window.focused)
        .map(proto_window_info);
    let windows = metadata.windows.iter().map(proto_window_info).collect();
    let elements = cached_accessibility_tree(accessibility_cache)
        .map(|tree| {
            tree.metadata
                .elements
                .iter()
                .map(proto_ui_element)
                .collect()
        })
        .unwrap_or_default();

    Ok(proto::DesktopState {
        active_window,
        windows,
        elements,
    })
}
