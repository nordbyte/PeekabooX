use super::*;

#[derive(Debug, Default)]
pub(super) struct IncrementalCaptureState {
    pub(super) streams: HashMap<String, IncrementalCaptureStream>,
    pub(super) order: Vec<String>,
}

impl IncrementalCaptureState {
    pub(super) fn insert(&mut self, stream_id: String, stream: IncrementalCaptureStream) {
        if !self.streams.contains_key(&stream_id) {
            self.order.push(stream_id.clone());
        }
        self.streams.insert(stream_id.clone(), stream);
        self.order.retain(|id| self.streams.contains_key(id));
        while self.streams.len() > MAX_INCREMENTAL_CAPTURE_STREAMS {
            let Some(evicted) = self.order.first().cloned() else {
                break;
            };
            self.order.remove(0);
            if evicted != stream_id {
                self.streams.remove(&evicted);
            }
        }
    }
}

#[derive(Debug)]
pub(super) struct IncrementalCaptureStream {
    pub(super) sequence: u64,
    pub(super) frame: CaptureFrame,
}

#[derive(Debug)]
pub(super) struct CapturedFrame {
    pub(super) frame: CaptureFrame,
    pub(super) backend_name: String,
    pub(super) backend_kind: BackendKind,
    pub(super) captured_at_unix_ms: u64,
}

#[derive(Debug)]
pub(super) struct CaptureDeltaData {
    pub(super) stream_id: String,
    pub(super) delta: IncrementalCaptureDelta,
    pub(super) low_bandwidth: bool,
    pub(super) capture_region: Option<Rect>,
    pub(super) backend_name: String,
    pub(super) backend_kind: BackendKind,
    pub(super) captured_at_unix_ms: u64,
}

#[derive(Debug)]
pub(super) struct AccessibilityCache {
    pub(super) ttl: Duration,
    pub(super) snapshot: Option<AccessibilityCacheSnapshot>,
}

#[derive(Debug, Clone)]
pub(super) struct AccessibilityCacheSnapshot {
    pub(super) loaded_at: Instant,
    pub(super) metadata: AccessibilityTreeMetadata,
}

#[derive(Debug, Clone)]
pub(super) struct CachedAccessibilityTree {
    pub(super) metadata: AccessibilityTreeMetadata,
    pub(super) cache_hit: bool,
    pub(super) age_ms: u128,
}

#[derive(Debug, Clone)]
pub(super) struct ElementLookupResult {
    pub(super) backend_name: String,
    pub(super) backend_kind: String,
    pub(super) warnings: Vec<String>,
    pub(super) elements: Vec<UiElement>,
    pub(super) cache_hit: bool,
    pub(super) cache_age_ms: u128,
    pub(super) vision_fallback_used: bool,
}

#[derive(Debug, Clone, Default)]
pub(super) struct ElementLookupScope {
    pub(super) app: Option<String>,
    pub(super) window_title: Option<String>,
    pub(super) window_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct ElementVisionFallbackConfig {
    pub(super) region: Option<Rect>,
    pub(super) options: UiElementDetectionOptions,
}

#[derive(Debug, Clone, Default)]
pub(super) struct ElementLookupOptions {
    pub(super) scope: ElementLookupScope,
    pub(super) vision: ElementVisionFallbackConfig,
}

impl AccessibilityCache {
    pub(super) fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            snapshot: None,
        }
    }

    pub(super) fn fresh(&self) -> Option<CachedAccessibilityTree> {
        let snapshot = self.snapshot.as_ref()?;
        let age = snapshot.loaded_at.elapsed();
        if age > self.ttl {
            return None;
        }

        Some(CachedAccessibilityTree {
            metadata: snapshot.metadata.clone(),
            cache_hit: true,
            age_ms: age.as_millis(),
        })
    }

    pub(super) fn store(&mut self, metadata: AccessibilityTreeMetadata) -> CachedAccessibilityTree {
        self.snapshot = Some(AccessibilityCacheSnapshot {
            loaded_at: Instant::now(),
            metadata: metadata.clone(),
        });

        CachedAccessibilityTree {
            metadata,
            cache_hit: false,
            age_ms: 0,
        }
    }

    pub(super) fn invalidate(&mut self) -> bool {
        self.snapshot.take().is_some()
    }
}

pub(super) fn cached_accessibility_tree(
    cache: &SharedAccessibilityCache,
) -> Result<CachedAccessibilityTree, String> {
    {
        let cache = cache
            .lock()
            .map_err(|_| "failed to lock accessibility cache".to_owned())?;
        if let Some(snapshot) = cache.fresh() {
            return Ok(snapshot);
        }
    }

    let metadata = peekaboox_accessibility::semantic_tree().map_err(|error| error.to_string())?;
    let mut cache = cache
        .lock()
        .map_err(|_| "failed to lock accessibility cache".to_owned())?;
    Ok(cache.store(metadata))
}

pub(super) fn find_elements_with_optional_vision_fallback(
    selector: &str,
    use_vision_fallback: bool,
    options: &ElementLookupOptions,
    accessibility_cache: &SharedAccessibilityCache,
) -> Result<ElementLookupResult, String> {
    element_lookup_with_optional_vision_fallback(
        selector,
        use_vision_fallback,
        options,
        cached_accessibility_tree(accessibility_cache),
        vision_fallback_elements,
    )
}

pub(super) fn element_lookup_with_optional_vision_fallback(
    selector: &str,
    use_vision_fallback: bool,
    options: &ElementLookupOptions,
    accessibility_result: Result<CachedAccessibilityTree, String>,
    fallback_elements: impl FnOnce(
        &ElementQuery,
        &ElementLookupOptions,
    ) -> Result<ElementLookupResult, String>,
) -> Result<ElementLookupResult, String> {
    let query = ElementQuery::parse(selector).map_err(|error| error.to_string())?;
    match accessibility_result {
        Ok(tree) => {
            let mut metadata = tree.metadata;
            metadata.elements.retain(|element| {
                query.matches(element) && element_matches_scope(element, &options.scope)
            });
            if !metadata.elements.is_empty() || !use_vision_fallback {
                return Ok(ElementLookupResult {
                    backend_name: metadata.backend_name,
                    backend_kind: backend_kind_name(metadata.backend_kind),
                    warnings: metadata.warnings,
                    elements: metadata.elements,
                    cache_hit: tree.cache_hit,
                    cache_age_ms: tree.age_ms,
                    vision_fallback_used: false,
                });
            }

            let mut fallback = fallback_elements(&query, options)?;
            fallback
                .warnings
                .push("no accessibility elements matched; used vision fallback".to_owned());
            Ok(fallback)
        }
        Err(error) if use_vision_fallback => {
            let mut fallback = fallback_elements(&query, options)?;
            fallback.warnings.push(format!(
                "accessibility lookup failed: {error}; used vision fallback"
            ));
            Ok(fallback)
        }
        Err(error) => Err(error),
    }
}

pub(super) fn vision_fallback_elements(
    query: &ElementQuery,
    options: &ElementLookupOptions,
) -> Result<ElementLookupResult, String> {
    let screenshot = vision_fallback_temp_path().map_err(|error| error.to_string())?;
    let capture_region = element_vision_capture_region(options)?;
    capture_to_file(&screenshot, capture_region).map_err(|error| error.to_string())?;
    let result =
        peekaboox_vision::detect_ui_elements_from_image_file(&screenshot, &options.vision.options)
            .map_err(|error| error.to_string());
    remove_best_effort(&screenshot, "vision fallback screenshot");

    let mut elements = result?;
    apply_element_scope_metadata(&mut elements, &options.scope, capture_region);
    elements.retain(|element| query.matches(element));
    Ok(ElementLookupResult {
        backend_name: VISION_UI_BACKEND_NAME.to_owned(),
        backend_kind: VISION_UI_BACKEND_KIND.to_owned(),
        warnings: Vec::new(),
        elements,
        cache_hit: false,
        cache_age_ms: 0,
        vision_fallback_used: true,
    })
}

pub(super) fn element_matches_scope(element: &UiElement, scope: &ElementLookupScope) -> bool {
    scope
        .window_id
        .as_deref()
        .is_none_or(|window_id| element.window_id.as_deref() == Some(window_id))
        && scope.window_title.as_deref().is_none_or(|window_title| {
            element
                .window_title
                .as_deref()
                .is_some_and(|value| contains_case_insensitive(value, window_title))
        })
        && scope.app.as_deref().is_none_or(|app| {
            element
                .app_id
                .as_deref()
                .is_some_and(|value| contains_case_insensitive(value, app))
        })
}

pub(super) fn element_vision_capture_region(
    options: &ElementLookupOptions,
) -> Result<Option<Rect>, String> {
    if options.vision.region.is_some() {
        return Ok(options.vision.region);
    }
    if options.scope.window_id.is_none()
        && options.scope.window_title.is_none()
        && options.scope.app.is_none()
    {
        return Ok(None);
    }
    resolve_ocr_window_region(
        None,
        options.scope.window_id.as_deref(),
        options.scope.window_title.as_deref(),
        options.scope.app.as_deref(),
    )
    .map(Some)
    .map_err(|error| error.to_string())
}

pub(super) fn apply_element_scope_metadata(
    elements: &mut [UiElement],
    scope: &ElementLookupScope,
    capture_region: Option<Rect>,
) {
    for element in elements {
        if let Some(region) = capture_region {
            element.bounds.x += region.x;
            element.bounds.y += region.y;
            element.center = element.bounds.center();
        }
        element.window_id.clone_from(&scope.window_id);
        element.window_title.clone_from(&scope.window_title);
        element.app_id.clone_from(&scope.app);
    }
}

pub(super) fn contains_case_insensitive(value: &str, needle: &str) -> bool {
    value
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

pub(super) fn vision_fallback_temp_path() -> std::io::Result<PathBuf> {
    reserve_unique_temp_path_in(&std::env::temp_dir(), "peekaboox-vision-fallback", "png")
}

pub(super) fn unique_temp_path_in(dir: &Path, prefix: &str, extension: &str) -> PathBuf {
    let counter = TEMP_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    dir.join(format!(
        "{prefix}-{}-{nanos}-{counter}.{extension}",
        std::process::id(),
    ))
}

pub(super) fn reserve_unique_temp_path_in(
    dir: &Path,
    prefix: &str,
    extension: &str,
) -> std::io::Result<PathBuf> {
    for _ in 0..32 {
        let path = unique_temp_path_in(dir, prefix, extension);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(_) => return Ok(path),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }

    Err(std::io::Error::new(
        ErrorKind::AlreadyExists,
        format!("failed to allocate unique temporary {prefix} file"),
    ))
}

pub(super) fn remove_best_effort(path: &PathBuf, description: &str) {
    if let Err(error) = fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        eprintln!("failed to remove {description} {}: {error}", path.display());
    }
}

pub(super) fn invalidate_accessibility_cache(cache: &SharedAccessibilityCache) -> bool {
    match cache.lock() {
        Ok(mut cache) => cache.invalidate(),
        Err(_) => false,
    }
}
