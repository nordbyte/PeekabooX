use std::collections::HashSet;
use std::time::Duration;

use dbus::Path;
use dbus::arg::{RefArg, Variant};
use dbus::blocking::Connection;
use peekaboox_core::{BackendKind, PeekabooXError, Point, Rect, Result, UiElement};
use regex::Regex;

const ATSPI_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_TREE_DEPTH: usize = 12;
const MAX_TREE_NODES: usize = 5_000;
const MAX_TREE_ELEMENTS: usize = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextMatchMode {
    Contains,
    Exact,
    Regex,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextMatcher {
    pub value: String,
    pub mode: TextMatchMode,
}

impl TextMatcher {
    fn contains(value: &str) -> Option<Self> {
        non_empty_string(value).map(|value| Self {
            value,
            mode: TextMatchMode::Contains,
        })
    }

    fn exact(value: &str) -> Option<Self> {
        non_empty_string(value).map(|value| Self {
            value,
            mode: TextMatchMode::Exact,
        })
    }

    fn regex(value: &str) -> Result<Option<Self>> {
        let Some(value) = non_empty_string(value) else {
            return Ok(None);
        };
        Regex::new(&value).map_err(|error| {
            PeekabooXError::new(format!("invalid selector regex {value:?}: {error}"))
        })?;
        Ok(Some(Self {
            value,
            mode: TextMatchMode::Regex,
        }))
    }

    fn matches(&self, value: &str) -> bool {
        match self.mode {
            TextMatchMode::Contains => contains_case_insensitive(value, &self.value),
            TextMatchMode::Exact => value.eq_ignore_ascii_case(&self.value),
            TextMatchMode::Regex => Regex::new(&self.value)
                .map(|regex| regex.is_match(value))
                .unwrap_or(false),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ElementQuery {
    pub id: Option<TextMatcher>,
    pub role: Option<TextMatcher>,
    pub label: Option<TextMatcher>,
    pub bounds: Option<Rect>,
    pub contains_point: Option<Point>,
    pub state: Option<TextMatcher>,
    pub not_state: Option<TextMatcher>,
    pub min_confidence: Option<f32>,
    pub within: Option<Rect>,
    pub intersects: Option<Rect>,
    pub min_width: Option<u32>,
    pub min_height: Option<u32>,
    pub window_id: Option<TextMatcher>,
    pub window_title: Option<TextMatcher>,
    pub app: Option<TextMatcher>,
}

impl ElementQuery {
    pub fn parse(selector: &str) -> Result<Self> {
        let selector = selector.trim();
        if selector.is_empty() {
            return Ok(Self::default());
        }

        let mut query = Self::default();
        for part in split_selector_parts(selector) {
            if let Some((key, value)) = selector_key_value(&part) {
                let value = value.trim();
                match key.trim().to_ascii_lowercase().as_str() {
                    "id" | "element-id" => query.id = TextMatcher::contains(value),
                    "id-exact" | "element-id-exact" => query.id = TextMatcher::exact(value),
                    "id-regex" | "element-id-regex" => query.id = TextMatcher::regex(value)?,
                    "id-contains" | "element-id-contains" => {
                        query.id = TextMatcher::contains(value)
                    }
                    "role" | "role-contains" => query.role = TextMatcher::contains(value),
                    "role-exact" => query.role = TextMatcher::exact(value),
                    "role-regex" => query.role = TextMatcher::regex(value)?,
                    "label" | "name" | "text" | "label-contains" | "name-contains"
                    | "text-contains" => query.label = TextMatcher::contains(value),
                    "label-exact" | "name-exact" | "text-exact" => {
                        query.label = TextMatcher::exact(value)
                    }
                    "label-regex" | "name-regex" | "text-regex" => {
                        query.label = TextMatcher::regex(value)?
                    }
                    "bounds" | "rect" => query.bounds = Some(parse_rect_strict(value, key)?),
                    "contains" | "point" | "at" => {
                        query.contains_point = Some(parse_point_strict(value, key)?)
                    }
                    "state" | "states" | "state-contains" => {
                        query.state = TextMatcher::contains(value)
                    }
                    "state-exact" => query.state = TextMatcher::exact(value),
                    "state-regex" => query.state = TextMatcher::regex(value)?,
                    "not-state" | "not-states" => query.not_state = TextMatcher::contains(value),
                    "not-state-exact" => query.not_state = TextMatcher::exact(value),
                    "not-state-regex" => query.not_state = TextMatcher::regex(value)?,
                    "confidence" | "confidence>" | "min_confidence" | "min-confidence" => {
                        query.min_confidence = Some(parse_f32(value, key)?)
                    }
                    "within" => query.within = Some(parse_rect_strict(value, key)?),
                    "intersects" | "overlaps" => {
                        query.intersects = Some(parse_rect_strict(value, key)?)
                    }
                    "min-width" | "min_width" => query.min_width = Some(parse_u32(value, key)?),
                    "min-height" | "min_height" => query.min_height = Some(parse_u32(value, key)?),
                    "window-id" | "window_id" => query.window_id = TextMatcher::contains(value),
                    "window-id-exact" | "window_id_exact" => {
                        query.window_id = TextMatcher::exact(value)
                    }
                    "window-id-regex" | "window_id_regex" => {
                        query.window_id = TextMatcher::regex(value)?
                    }
                    "window-title" | "window_title" => {
                        query.window_title = TextMatcher::contains(value)
                    }
                    "window-title-exact" | "window_title_exact" => {
                        query.window_title = TextMatcher::exact(value)
                    }
                    "window-title-regex" | "window_title_regex" => {
                        query.window_title = TextMatcher::regex(value)?
                    }
                    "app" | "app-id" | "app_id" => query.app = TextMatcher::contains(value),
                    "app-exact" | "app-id-exact" | "app_id_exact" => {
                        query.app = TextMatcher::exact(value)
                    }
                    "app-regex" | "app-id-regex" | "app_id_regex" => {
                        query.app = TextMatcher::regex(value)?
                    }
                    _ => {
                        return Err(PeekabooXError::new(format!(
                            "unknown element selector key {key:?}"
                        )));
                    }
                }
            } else if query.label.is_none() {
                query.label = TextMatcher::contains(&part);
            } else {
                return Err(PeekabooXError::new(format!(
                    "unexpected unqualified selector part {part:?}"
                )));
            }
        }

        Ok(query)
    }

    pub fn from_selector(selector: &str) -> Self {
        Self::parse(selector).unwrap_or_default()
    }

    pub fn matches(&self, element: &UiElement) -> bool {
        let id_matches = self
            .id
            .as_ref()
            .is_none_or(|matcher| matcher.matches(&element.id));
        let role_matches = self
            .role
            .as_ref()
            .is_none_or(|matcher| matcher.matches(&element.role));
        let label_matches = self.label.as_ref().is_none_or(|matcher| {
            element
                .label
                .as_deref()
                .is_some_and(|element_label| matcher.matches(element_label))
        });
        let bounds_match = self.bounds.is_none_or(|bounds| element.bounds == bounds);
        let contains_point_match = self
            .contains_point
            .is_none_or(|point| rect_contains_point(element.bounds, point));
        let state_matches = self.state.as_ref().is_none_or(|matcher| {
            element
                .states
                .iter()
                .any(|element_state| matcher.matches(element_state))
        });
        let not_state_matches = self.not_state.as_ref().is_none_or(|matcher| {
            element
                .states
                .iter()
                .all(|element_state| !matcher.matches(element_state))
        });
        let confidence_matches = self
            .min_confidence
            .is_none_or(|min_confidence| element.confidence >= min_confidence);
        let within_matches = self
            .within
            .is_none_or(|bounds| rect_contains_rect(bounds, element.bounds));
        let intersects_matches = self
            .intersects
            .is_none_or(|bounds| rects_intersect(bounds, element.bounds));
        let min_width_matches = self
            .min_width
            .is_none_or(|min_width| element.bounds.width >= min_width);
        let min_height_matches = self
            .min_height
            .is_none_or(|min_height| element.bounds.height >= min_height);
        let window_id_matches = self.window_id.as_ref().is_none_or(|matcher| {
            element
                .window_id
                .as_deref()
                .is_some_and(|window_id| matcher.matches(window_id))
        });
        let window_title_matches = self.window_title.as_ref().is_none_or(|matcher| {
            element
                .window_title
                .as_deref()
                .is_some_and(|window_title| matcher.matches(window_title))
        });
        let app_matches = self.app.as_ref().is_none_or(|matcher| {
            element
                .app_id
                .as_deref()
                .is_some_and(|app_id| matcher.matches(app_id))
                || element
                    .window_title
                    .as_deref()
                    .is_some_and(|window_title| matcher.matches(window_title))
        });

        id_matches
            && role_matches
            && label_matches
            && bounds_match
            && contains_point_match
            && state_matches
            && not_state_matches
            && confidence_matches
            && within_matches
            && intersects_matches
            && min_width_matches
            && min_height_matches
            && window_id_matches
            && window_title_matches
            && app_matches
    }
}

pub trait AccessibilityBackend {
    fn semantic_tree(&self) -> Result<Vec<UiElement>>;
    fn find_elements(&self, query: &ElementQuery) -> Result<Vec<UiElement>>;
}

#[derive(Debug, Default)]
pub struct UnimplementedAccessibilityBackend;

impl AccessibilityBackend for UnimplementedAccessibilityBackend {
    fn semantic_tree(&self) -> Result<Vec<UiElement>> {
        Err(PeekabooXError::new(
            "accessibility backend is unavailable in this environment",
        ))
    }

    fn find_elements(&self, _query: &ElementQuery) -> Result<Vec<UiElement>> {
        Err(PeekabooXError::new(
            "accessibility element query is unavailable in this environment",
        ))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AccessibilityTreeMetadata {
    pub backend_name: String,
    pub backend_kind: BackendKind,
    pub elements: Vec<UiElement>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedClickTarget {
    pub element: UiElement,
    pub position: Point,
}

#[derive(Debug, Default)]
pub struct AtSpiAccessibilityBackend;

impl AtSpiAccessibilityBackend {
    pub fn semantic_tree_with_metadata(&self) -> Result<AccessibilityTreeMetadata> {
        let mut warnings = Vec::new();
        let connection = atspi_connection()?;
        let applications = atspi_root_applications(&connection)?;
        let mut elements = Vec::new();
        let mut visited = HashSet::new();

        for application in applications {
            let application_id = atspi_object_id(&application);
            let application_name = atspi_accessible_name(&connection, &application)
                .ok()
                .map(|label| label.trim().to_owned())
                .filter(|label| !label.is_empty());
            collect_atspi_elements(
                &connection,
                &application,
                0,
                AtSpiElementContext {
                    app_id: Some(application_name.unwrap_or(application_id)),
                    window_id: None,
                    window_title: None,
                    parent_id: None,
                },
                &mut elements,
                &mut visited,
                &mut warnings,
            );
            if elements.len() >= MAX_TREE_ELEMENTS {
                warnings.push(format!(
                    "stopped AT-SPI traversal after {MAX_TREE_ELEMENTS} elements"
                ));
                break;
            }
            if visited.len() >= MAX_TREE_NODES {
                warnings.push(format!(
                    "stopped AT-SPI traversal after {MAX_TREE_NODES} nodes"
                ));
                break;
            }
        }

        elements.sort_by(|left, right| {
            left.role
                .cmp(&right.role)
                .then_with(|| left.label.cmp(&right.label))
                .then_with(|| left.id.cmp(&right.id))
        });
        elements.dedup_by(|left, right| left.id == right.id);

        Ok(AccessibilityTreeMetadata {
            backend_name: "at-spi".to_owned(),
            backend_kind: BackendKind::AtSpi,
            elements,
            warnings,
        })
    }
}

impl AccessibilityBackend for AtSpiAccessibilityBackend {
    fn semantic_tree(&self) -> Result<Vec<UiElement>> {
        self.semantic_tree_with_metadata()
            .map(|metadata| metadata.elements)
    }

    fn find_elements(&self, query: &ElementQuery) -> Result<Vec<UiElement>> {
        Ok(self
            .semantic_tree()?
            .into_iter()
            .filter(|element| query.matches(element))
            .collect())
    }
}

pub fn semantic_tree() -> Result<AccessibilityTreeMetadata> {
    AtSpiAccessibilityBackend.semantic_tree_with_metadata()
}

pub fn find_elements(query: &ElementQuery) -> Result<AccessibilityTreeMetadata> {
    let mut metadata = AtSpiAccessibilityBackend.semantic_tree_with_metadata()?;
    metadata.elements.retain(|element| query.matches(element));
    Ok(metadata)
}

pub fn find_elements_by_selector(selector: &str) -> Result<AccessibilityTreeMetadata> {
    find_elements(&ElementQuery::parse(selector)?)
}

pub fn resolve_click_target(selector: &str) -> Result<ResolvedClickTarget> {
    let metadata = semantic_tree()?;
    resolve_click_target_from_elements(selector, metadata.elements)
}

pub fn resolve_click_target_from_tree(
    selector: &str,
    elements: &[UiElement],
) -> Result<ResolvedClickTarget> {
    resolve_click_target_from_elements(selector, elements.to_vec())
}

type AtSpiRef = (String, Path<'static>);

#[derive(Debug, Clone, Default)]
struct AtSpiElementContext {
    app_id: Option<String>,
    window_id: Option<String>,
    window_title: Option<String>,
    parent_id: Option<String>,
}

fn atspi_connection() -> Result<Connection> {
    let address = atspi_bus_address()?;

    Connection::new_address(&address)
        .map_err(|error| PeekabooXError::new(format!("failed to connect to AT-SPI bus: {error}")))
}

pub fn atspi_bus_address() -> Result<String> {
    let session = Connection::new_session().map_err(|error| {
        PeekabooXError::new(format!("failed to connect to session bus: {error}"))
    })?;
    let bus_proxy = session.with_proxy("org.a11y.Bus", "/org/a11y/bus", ATSPI_TIMEOUT);
    let (address,): (String,) = bus_proxy
        .method_call("org.a11y.Bus", "GetAddress", ())
        .map_err(|error| PeekabooXError::new(format!("AT-SPI bus lookup failed: {error}")))?;

    Ok(address)
}

fn atspi_root_applications(connection: &Connection) -> Result<Vec<AtSpiRef>> {
    let root_proxy = connection.with_proxy(
        "org.a11y.atspi.Registry",
        "/org/a11y/atspi/accessible/root",
        ATSPI_TIMEOUT,
    );
    let (applications,): (Vec<AtSpiRef>,) = root_proxy
        .method_call("org.a11y.atspi.Accessible", "GetChildren", ())
        .map_err(|error| PeekabooXError::new(format!("AT-SPI root enumeration failed: {error}")))?;

    Ok(applications)
}

fn collect_atspi_elements(
    connection: &Connection,
    object_ref: &AtSpiRef,
    depth: usize,
    context: AtSpiElementContext,
    elements: &mut Vec<UiElement>,
    visited: &mut HashSet<String>,
    warnings: &mut Vec<String>,
) {
    if depth > MAX_TREE_DEPTH
        || elements.len() >= MAX_TREE_ELEMENTS
        || visited.len() >= MAX_TREE_NODES
    {
        return;
    }

    let object_id = atspi_object_id(object_ref);
    if !visited.insert(object_id.clone()) {
        return;
    }

    let children = match atspi_children(connection, object_ref) {
        Ok(children) => children,
        Err(error) => {
            warnings.push(format!(
                "{}{} children: {}",
                object_ref.0, object_ref.1, error
            ));
            Vec::new()
        }
    };
    let child_ids = children.iter().map(atspi_object_id).collect::<Vec<_>>();
    let next_context = match atspi_ui_element(connection, object_ref, &context, child_ids) {
        Ok(Some(element)) => {
            let next_context = context_for_children(&context, &element);
            elements.push(element);
            next_context
        }
        Ok(None) => context.clone(),
        Err(error) => {
            warnings.push(format!("{}{}: {}", object_ref.0, object_ref.1, error));
            context.clone()
        }
    };

    for child in children {
        collect_atspi_elements(
            connection,
            &child,
            depth + 1,
            AtSpiElementContext {
                parent_id: Some(object_id.clone()),
                ..next_context.clone()
            },
            elements,
            visited,
            warnings,
        );
        if elements.len() >= MAX_TREE_ELEMENTS || visited.len() >= MAX_TREE_NODES {
            return;
        }
    }
}

fn atspi_ui_element(
    connection: &Connection,
    object_ref: &AtSpiRef,
    context: &AtSpiElementContext,
    child_ids: Vec<String>,
) -> Result<Option<UiElement>> {
    let role = atspi_role_name(connection, object_ref)?.trim().to_owned();
    if role.is_empty() {
        return Ok(None);
    }

    let label = atspi_accessible_name(connection, object_ref)
        .ok()
        .map(|label| label.trim().to_owned())
        .filter(|label| !label.is_empty());
    let bounds = atspi_extents(connection, object_ref)
        .ok()
        .and_then(rect_from_extents)
        .unwrap_or_else(|| Rect::new(0, 0, 0, 0));
    let states = atspi_state_set(connection, object_ref)
        .map(|state_set| atspi_state_names(&state_set))
        .unwrap_or_default();

    if label.is_none() && bounds.width == 0 && bounds.height == 0 && is_structural_role(&role) {
        return Ok(None);
    }

    Ok(Some(UiElement {
        id: atspi_object_id(object_ref),
        role,
        label,
        bounds,
        center: bounds.center(),
        confidence: if bounds.width > 0 && bounds.height > 0 {
            1.0
        } else {
            0.7
        },
        states,
        window_id: context.window_id.clone(),
        window_title: context.window_title.clone(),
        app_id: context.app_id.clone(),
        parent_id: context.parent_id.clone(),
        child_ids,
    }))
}

fn atspi_object_id(object_ref: &AtSpiRef) -> String {
    format!("{}{}", object_ref.0, object_ref.1)
}

fn context_for_children(
    parent_context: &AtSpiElementContext,
    element: &UiElement,
) -> AtSpiElementContext {
    let mut context = parent_context.clone();
    if is_window_role(&element.role) {
        context.window_id = Some(element.id.clone());
        context.window_title = element.label.clone();
    }
    context.parent_id = Some(element.id.clone());
    context
}

fn atspi_children(connection: &Connection, object_ref: &AtSpiRef) -> Result<Vec<AtSpiRef>> {
    let proxy = connection.with_proxy(object_ref.0.as_str(), object_ref.1.clone(), ATSPI_TIMEOUT);
    let (children,): (Vec<AtSpiRef>,) = proxy
        .method_call("org.a11y.atspi.Accessible", "GetChildren", ())
        .map_err(|error| PeekabooXError::new(format!("AT-SPI GetChildren failed: {error}")))?;

    Ok(children)
}

fn atspi_accessible_name(connection: &Connection, object_ref: &AtSpiRef) -> Result<String> {
    let proxy = connection.with_proxy(object_ref.0.as_str(), object_ref.1.clone(), ATSPI_TIMEOUT);
    let (value,): (Variant<Box<dyn RefArg>>,) = proxy
        .method_call(
            "org.freedesktop.DBus.Properties",
            "Get",
            ("org.a11y.atspi.Accessible", "Name"),
        )
        .map_err(|error| PeekabooXError::new(format!("AT-SPI Name lookup failed: {error}")))?;

    value
        .0
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| PeekabooXError::new("AT-SPI Name property was not a string"))
}

fn atspi_role_name(connection: &Connection, object_ref: &AtSpiRef) -> Result<String> {
    let proxy = connection.with_proxy(object_ref.0.as_str(), object_ref.1.clone(), ATSPI_TIMEOUT);
    let (role,): (String,) = proxy
        .method_call("org.a11y.atspi.Accessible", "GetRoleName", ())
        .map_err(|error| PeekabooXError::new(format!("AT-SPI role lookup failed: {error}")))?;

    Ok(role)
}

fn atspi_extents(connection: &Connection, object_ref: &AtSpiRef) -> Result<(i32, i32, i32, i32)> {
    let proxy = connection.with_proxy(object_ref.0.as_str(), object_ref.1.clone(), ATSPI_TIMEOUT);
    let ((x, y, width, height),): ((i32, i32, i32, i32),) = proxy
        .method_call("org.a11y.atspi.Component", "GetExtents", (0_u32,))
        .map_err(|error| PeekabooXError::new(format!("AT-SPI extents lookup failed: {error}")))?;

    Ok((x, y, width, height))
}

fn atspi_state_set(connection: &Connection, object_ref: &AtSpiRef) -> Result<Vec<u32>> {
    let proxy = connection.with_proxy(object_ref.0.as_str(), object_ref.1.clone(), ATSPI_TIMEOUT);
    let (states,): (Vec<u32>,) = proxy
        .method_call("org.a11y.atspi.Accessible", "GetState", ())
        .map_err(|error| PeekabooXError::new(format!("AT-SPI state lookup failed: {error}")))?;

    Ok(states)
}

fn atspi_state_names(states: &[u32]) -> Vec<String> {
    const STATE_NAMES: &[(usize, &str)] = &[
        (0, "invalid"),
        (1, "active"),
        (2, "armed"),
        (3, "busy"),
        (4, "checked"),
        (5, "collapsed"),
        (6, "defunct"),
        (7, "editable"),
        (8, "enabled"),
        (9, "expandable"),
        (10, "expanded"),
        (11, "focusable"),
        (12, "focused"),
        (13, "has-tooltip"),
        (14, "horizontal"),
        (15, "iconified"),
        (16, "modal"),
        (17, "multi-line"),
        (18, "multi-selectable"),
        (19, "opaque"),
        (20, "pressed"),
        (21, "resizable"),
        (22, "selectable"),
        (23, "selected"),
        (24, "sensitive"),
        (25, "showing"),
        (26, "single-line"),
        (27, "stale"),
        (28, "transient"),
        (29, "vertical"),
        (30, "visible"),
        (31, "manages-descendants"),
        (32, "indeterminate"),
        (33, "required"),
        (34, "truncated"),
        (35, "animated"),
        (36, "invalid-entry"),
        (37, "supports-autocompletion"),
        (38, "selectable-text"),
        (39, "is-default"),
        (40, "visited"),
        (41, "checkable"),
        (42, "has-popup"),
        (43, "read-only"),
    ];

    STATE_NAMES
        .iter()
        .filter(|(index, _)| atspi_state_bit_is_set(states, *index))
        .map(|(_, name)| (*name).to_owned())
        .collect()
}

fn atspi_state_bit_is_set(states: &[u32], index: usize) -> bool {
    let Some(word) = states.get(index / 32) else {
        return false;
    };
    word & (1_u32 << (index % 32)) != 0
}

fn resolve_click_target_from_elements(
    selector: &str,
    elements: Vec<UiElement>,
) -> Result<ResolvedClickTarget> {
    if selector.trim().is_empty() {
        return Err(PeekabooXError::new("click selector must not be empty"));
    }

    let query = ElementQuery::parse(selector)?;
    let mut matches = elements
        .into_iter()
        .filter(|element| query.matches(element))
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        click_target_score(&query, right)
            .cmp(&click_target_score(&query, left))
            .then_with(|| left.id.cmp(&right.id))
    });

    for element in matches {
        if let Some(position) = element_center(&element) {
            return Ok(ResolvedClickTarget { element, position });
        }
    }

    Err(PeekabooXError::new(format!(
        "no clickable accessibility element matched selector {selector:?}"
    )))
}

fn click_target_score(query: &ElementQuery, element: &UiElement) -> i32 {
    let mut score = 0;

    if let Some(query_label) = query.label.as_ref()
        && let Some(label) = element.label.as_deref()
    {
        if label.eq_ignore_ascii_case(&query_label.value) {
            score += 1_000;
        } else if query_label.matches(label) {
            score += 500;
        }
    }

    if let Some(query_role) = query.role.as_ref() {
        if element.role.eq_ignore_ascii_case(&query_role.value) {
            score += 200;
        } else if query_role.matches(&element.role) {
            score += 100;
        }
    }

    if element_center(element).is_some() {
        score += 10;
    }

    score
}

fn element_center(element: &UiElement) -> Option<Point> {
    element.center.or_else(|| element.bounds.center())
}

fn rect_from_extents((x, y, width, height): (i32, i32, i32, i32)) -> Option<Rect> {
    if width < 0 || height < 0 {
        return None;
    }

    Some(Rect::new(
        x,
        y,
        u32::try_from(width).ok()?,
        u32::try_from(height).ok()?,
    ))
}

fn parse_rect(value: &str) -> Option<Rect> {
    let numbers = parse_i32_list(value, 4)?;
    Some(Rect::new(
        numbers[0],
        numbers[1],
        u32::try_from(numbers[2]).ok()?,
        u32::try_from(numbers[3]).ok()?,
    ))
}

fn parse_point(value: &str) -> Option<Point> {
    let numbers = parse_i32_list(value, 2)?;
    Some(Point::new(numbers[0], numbers[1]))
}

fn parse_rect_strict(value: &str, key: &str) -> Result<Rect> {
    parse_rect(value).ok_or_else(|| {
        PeekabooXError::new(format!(
            "selector {key} must be x,y,width,height with non-negative size, got {value:?}"
        ))
    })
}

fn parse_point_strict(value: &str, key: &str) -> Result<Point> {
    parse_point(value)
        .ok_or_else(|| PeekabooXError::new(format!("selector {key} must be x,y, got {value:?}")))
}

fn parse_f32(value: &str, key: &str) -> Result<f32> {
    value.parse::<f32>().map_err(|error| {
        PeekabooXError::new(format!(
            "selector {key} must be a float, got {value:?}: {error}"
        ))
    })
}

fn parse_u32(value: &str, key: &str) -> Result<u32> {
    value.parse::<u32>().map_err(|error| {
        PeekabooXError::new(format!(
            "selector {key} must be an unsigned integer, got {value:?}: {error}"
        ))
    })
}

fn parse_i32_list(value: &str, expected_len: usize) -> Option<Vec<i32>> {
    let numbers = numeric_parts(value)
        .into_iter()
        .map(str::parse::<i32>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .ok()?;
    if expected_len == 4 && numbers.get(2).is_some_and(|value| *value < 0) {
        return None;
    }
    if expected_len == 4 && numbers.get(3).is_some_and(|value| *value < 0) {
        return None;
    }
    (numbers.len() == expected_len).then_some(numbers)
}

fn numeric_part_count(value: &str) -> usize {
    numeric_parts(value).len()
}

fn numeric_parts(value: &str) -> Vec<&str> {
    value
        .split([':', 'x', ';', '/'])
        .flat_map(|part| part.split_whitespace())
        .flat_map(|part| part.split(','))
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect()
}

fn rect_contains_point(rect: Rect, point: Point) -> bool {
    if rect.width == 0 || rect.height == 0 {
        return false;
    }

    let left = i64::from(rect.x);
    let top = i64::from(rect.y);
    let right = left + i64::from(rect.width);
    let bottom = top + i64::from(rect.height);
    let x = i64::from(point.x);
    let y = i64::from(point.y);

    x >= left && x < right && y >= top && y < bottom
}

fn rect_contains_rect(container: Rect, rect: Rect) -> bool {
    if container.width == 0 || container.height == 0 || rect.width == 0 || rect.height == 0 {
        return false;
    }

    let container_left = i64::from(container.x);
    let container_top = i64::from(container.y);
    let container_right = container_left + i64::from(container.width);
    let container_bottom = container_top + i64::from(container.height);
    let rect_left = i64::from(rect.x);
    let rect_top = i64::from(rect.y);
    let rect_right = rect_left + i64::from(rect.width);
    let rect_bottom = rect_top + i64::from(rect.height);

    rect_left >= container_left
        && rect_top >= container_top
        && rect_right <= container_right
        && rect_bottom <= container_bottom
}

fn rects_intersect(left: Rect, right: Rect) -> bool {
    if left.width == 0 || left.height == 0 || right.width == 0 || right.height == 0 {
        return false;
    }

    let left_x1 = i64::from(left.x);
    let left_y1 = i64::from(left.y);
    let left_x2 = left_x1 + i64::from(left.width);
    let left_y2 = left_y1 + i64::from(left.height);
    let right_x1 = i64::from(right.x);
    let right_y1 = i64::from(right.y);
    let right_x2 = right_x1 + i64::from(right.width);
    let right_y2 = right_y1 + i64::from(right.height);

    left_x1 < right_x2 && left_x2 > right_x1 && left_y1 < right_y2 && left_y2 > right_y1
}

fn is_structural_role(role: &str) -> bool {
    matches!(
        role,
        "application" | "panel" | "filler" | "root pane" | "layered pane" | "unknown"
    )
}

fn is_window_role(role: &str) -> bool {
    let role = role.to_ascii_lowercase();
    matches!(
        role.as_str(),
        "frame" | "window" | "dialog" | "alert" | "file chooser" | "page tab list"
    )
}

fn non_empty_string(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

fn split_selector_parts(selector: &str) -> Vec<String> {
    let parts = selector
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let mut normalized = Vec::new();
    let mut index = 0;

    while index < parts.len() {
        let mut part = parts[index].to_owned();
        if let Some((key, value)) = selector_key_value(&part)
            && let Some(expected_len) = numeric_selector_len(key)
        {
            let mut value_len = numeric_part_count(value);
            while value_len < expected_len && index + 1 < parts.len() {
                let next = parts[index + 1];
                if selector_key_value(next).is_some() {
                    break;
                }

                part.push(',');
                part.push_str(next);
                index += 1;
                value_len = selector_key_value(&part)
                    .map(|(_, value)| numeric_part_count(value))
                    .unwrap_or(value_len);
            }
        }

        normalized.push(part);
        index += 1;
    }

    normalized
}

fn selector_key_value(part: &str) -> Option<(&str, &str)> {
    part.split_once(">=")
        .or_else(|| part.split_once('='))
        .or_else(|| part.split_once(':'))
}

fn numeric_selector_len(key: &str) -> Option<usize> {
    match key.trim().to_ascii_lowercase().as_str() {
        "bounds" | "rect" | "within" | "intersects" | "overlaps" => Some(4),
        "contains" | "point" | "at" => Some(2),
        _ => None,
    }
}

fn contains_case_insensitive(value: &str, needle: &str) -> bool {
    value
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::{
        ElementQuery, atspi_state_names, contains_case_insensitive, element_center,
        rect_from_extents, resolve_click_target_from_elements,
    };
    use peekaboox_core::{Point, Rect, UiElement};

    fn test_element(id: &str, role: &str, label: Option<&str>, bounds: Rect) -> UiElement {
        UiElement {
            id: id.to_owned(),
            role: role.to_owned(),
            label: label.map(str::to_owned),
            bounds,
            center: bounds.center(),
            confidence: 1.0,
            states: Vec::new(),
            window_id: None,
            window_title: None,
            app_id: None,
            parent_id: None,
            child_ids: Vec::new(),
        }
    }

    #[test]
    fn selector_defaults_to_label_query() {
        let query = ElementQuery::parse("Submit").unwrap();

        assert_eq!(query.role, None);
        assert_eq!(
            query.label.as_ref().map(|matcher| matcher.value.as_str()),
            Some("Submit")
        );
    }

    #[test]
    fn selector_accepts_role_and_label_parts() {
        let query = ElementQuery::parse("role=push button,label=Save").unwrap();

        assert_eq!(
            query.role.as_ref().map(|matcher| matcher.value.as_str()),
            Some("push button")
        );
        assert_eq!(
            query.label.as_ref().map(|matcher| matcher.value.as_str()),
            Some("Save")
        );
    }

    #[test]
    fn query_matches_case_insensitive_role_and_label() {
        let query = ElementQuery::parse("role:button,text:submit").unwrap();
        let mut element = test_element(
            "element-1",
            "Push Button",
            Some("Submit order"),
            Rect::new(0, 0, 100, 40),
        );
        element.states = vec!["enabled".to_owned()];

        assert!(query.matches(&element));
    }

    #[test]
    fn selector_accepts_bounds_state_and_confidence_parts() {
        let query = ElementQuery::parse("role=button,state=enabled,contains=25,30,confidence>=0.9")
            .unwrap();
        let mut element = test_element(
            "element-1",
            "push button",
            Some("Submit order"),
            Rect::new(10, 20, 100, 40),
        );
        element.confidence = 0.95;
        element.states = vec!["enabled".to_owned(), "visible".to_owned()];

        assert_eq!(query.contains_point, Some(Point::new(25, 30)));
        assert_eq!(query.min_confidence, Some(0.9));
        assert!(query.matches(&element));
    }

    #[test]
    fn selector_accepts_exact_bounds() {
        let query = ElementQuery::parse("bounds=10,20,90,30").unwrap();
        let element = test_element(
            "element-1",
            "push button",
            Some("Submit"),
            Rect::new(10, 20, 90, 30),
        );

        assert_eq!(query.bounds, Some(Rect::new(10, 20, 90, 30)));
        assert!(query.matches(&element));
    }

    #[test]
    fn selector_supports_exact_regex_geometry_and_metadata() {
        let query = ElementQuery::parse(
            "id-exact=button-1,role-exact=push button,label-regex=^Sub.*,not-state=disabled,within=0,0,200,120,intersects=40,40,40,40,min-width=80,min-height=20,window-title=Demo,app=org.demo",
        )
        .unwrap();
        let mut element = test_element(
            "button-1",
            "push button",
            Some("Submit"),
            Rect::new(10, 20, 90, 30),
        );
        element.states = vec!["enabled".to_owned(), "visible".to_owned()];
        element.window_title = Some("Demo Window".to_owned());
        element.app_id = Some("org.demo.App".to_owned());

        assert!(query.matches(&element));
    }

    #[test]
    fn selector_rejects_invalid_numeric_and_unknown_keys() {
        assert!(ElementQuery::parse("bounds=not-a-rect").is_err());
        assert!(ElementQuery::parse("contains=10").is_err());
        assert!(ElementQuery::parse("unknown=value").is_err());
    }

    #[test]
    fn extents_reject_negative_sizes() {
        assert_eq!(rect_from_extents((1, 2, -1, 10)), None);
        assert_eq!(
            rect_from_extents((1, 2, 30, 40)),
            Some(Rect::new(1, 2, 30, 40))
        );
    }

    #[test]
    fn case_insensitive_contains_handles_simple_ascii() {
        assert!(contains_case_insensitive("Save Document", "save"));
    }

    #[test]
    fn element_center_uses_bounds_midpoint() {
        let element = test_element(
            "button-1",
            "push button",
            Some("Submit"),
            Rect::new(10, 20, 90, 30),
        );

        assert_eq!(
            element_center(&element),
            Some(peekaboox_core::Point::new(55, 35))
        );
    }

    #[test]
    fn resolves_click_target_by_best_label_match() {
        let weak_match = test_element(
            "button-2",
            "push button",
            Some("Submit later"),
            Rect::new(0, 0, 80, 20),
        );
        let exact_match = test_element(
            "button-1",
            "push button",
            Some("Submit"),
            Rect::new(10, 20, 90, 30),
        );

        let target =
            resolve_click_target_from_elements("Submit", vec![weak_match, exact_match]).unwrap();

        assert_eq!(target.element.id, "button-1");
        assert_eq!(target.position, peekaboox_core::Point::new(55, 35));
    }

    #[test]
    fn atspi_state_names_decode_bitset_words() {
        let states = atspi_state_names(&[(1 << 8) | (1 << 30), 1 << 11]);

        assert_eq!(
            states,
            vec![
                "enabled".to_owned(),
                "visible".to_owned(),
                "read-only".to_owned()
            ]
        );
    }
}
