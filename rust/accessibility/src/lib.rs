use std::collections::HashSet;
use std::time::Duration;

use dbus::Path;
use dbus::arg::{RefArg, Variant};
use dbus::blocking::Connection;
use peekaboox_core::{BackendKind, PeekabooXError, Point, Rect, Result, UiElement};

const ATSPI_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_TREE_DEPTH: usize = 12;
const MAX_TREE_NODES: usize = 5_000;
const MAX_TREE_ELEMENTS: usize = 2_000;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ElementQuery {
    pub role: Option<String>,
    pub label: Option<String>,
    pub bounds: Option<Rect>,
    pub contains_point: Option<Point>,
    pub state: Option<String>,
    pub min_confidence: Option<f32>,
}

impl ElementQuery {
    pub fn from_selector(selector: &str) -> Self {
        let selector = selector.trim();
        if selector.is_empty() {
            return Self::default();
        }

        let mut query = Self::default();
        for part in split_selector_parts(selector) {
            if let Some((key, value)) = selector_key_value(&part) {
                let value = value.trim();
                match key.trim().to_ascii_lowercase().as_str() {
                    "role" => query.role = non_empty_string(value),
                    "label" | "name" | "text" => query.label = non_empty_string(value),
                    "bounds" | "rect" => query.bounds = parse_rect(value),
                    "contains" | "point" | "at" => query.contains_point = parse_point(value),
                    "state" | "states" => query.state = non_empty_string(value),
                    "confidence" | "confidence>" | "min_confidence" | "min-confidence" => {
                        query.min_confidence = value.parse::<f32>().ok()
                    }
                    _ => {
                        if query.label.is_none() {
                            query.label = non_empty_string(&part);
                        }
                    }
                }
            } else if query.label.is_none() {
                query.label = non_empty_string(&part);
            }
        }

        query
    }

    pub fn matches(&self, element: &UiElement) -> bool {
        let role_matches = self
            .role
            .as_deref()
            .is_none_or(|role| contains_case_insensitive(&element.role, role));
        let label_matches = self.label.as_deref().is_none_or(|label| {
            element
                .label
                .as_deref()
                .is_some_and(|element_label| contains_case_insensitive(element_label, label))
        });
        let bounds_match = self.bounds.is_none_or(|bounds| element.bounds == bounds);
        let contains_point_match = self
            .contains_point
            .is_none_or(|point| rect_contains_point(element.bounds, point));
        let state_matches = self.state.as_deref().is_none_or(|state| {
            element
                .states
                .iter()
                .any(|element_state| contains_case_insensitive(element_state, state))
        });
        let confidence_matches = self
            .min_confidence
            .is_none_or(|min_confidence| element.confidence >= min_confidence);

        role_matches
            && label_matches
            && bounds_match
            && contains_point_match
            && state_matches
            && confidence_matches
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
            collect_atspi_elements(
                &connection,
                &application,
                0,
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
    find_elements(&ElementQuery::from_selector(selector))
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

    let object_id = format!("{}{}", object_ref.0, object_ref.1);
    if !visited.insert(object_id) {
        return;
    }

    match atspi_ui_element(connection, object_ref) {
        Ok(Some(element)) => elements.push(element),
        Ok(None) => {}
        Err(error) => warnings.push(format!("{}{}: {}", object_ref.0, object_ref.1, error)),
    }

    let children = match atspi_children(connection, object_ref) {
        Ok(children) => children,
        Err(error) => {
            warnings.push(format!(
                "{}{} children: {}",
                object_ref.0, object_ref.1, error
            ));
            return;
        }
    };

    for child in children {
        collect_atspi_elements(connection, &child, depth + 1, elements, visited, warnings);
        if elements.len() >= MAX_TREE_ELEMENTS || visited.len() >= MAX_TREE_NODES {
            return;
        }
    }
}

fn atspi_ui_element(connection: &Connection, object_ref: &AtSpiRef) -> Result<Option<UiElement>> {
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
        id: format!("{}{}", object_ref.0, object_ref.1),
        role,
        label,
        bounds,
        confidence: if bounds.width > 0 && bounds.height > 0 {
            1.0
        } else {
            0.7
        },
        states,
    }))
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

    let query = ElementQuery::from_selector(selector);
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

    if let Some(query_label) = query.label.as_deref()
        && let Some(label) = element.label.as_deref()
    {
        if label.eq_ignore_ascii_case(query_label) {
            score += 1_000;
        } else if contains_case_insensitive(label, query_label) {
            score += 500;
        }
    }

    if let Some(query_role) = query.role.as_deref() {
        if element.role.eq_ignore_ascii_case(query_role) {
            score += 200;
        } else if contains_case_insensitive(&element.role, query_role) {
            score += 100;
        }
    }

    if element_center(element).is_some() {
        score += 10;
    }

    score
}

fn element_center(element: &UiElement) -> Option<Point> {
    if element.bounds.width == 0 || element.bounds.height == 0 {
        return None;
    }

    let x = i64::from(element.bounds.x) + i64::from(element.bounds.width / 2);
    let y = i64::from(element.bounds.y) + i64::from(element.bounds.height / 2);

    Some(Point::new(i32::try_from(x).ok()?, i32::try_from(y).ok()?))
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

fn parse_i32_list(value: &str, expected_len: usize) -> Option<Vec<i32>> {
    let numbers = numeric_parts(value)
        .into_iter()
        .map(str::parse::<i32>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .ok()?;
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

fn is_structural_role(role: &str) -> bool {
    matches!(
        role,
        "application" | "panel" | "filler" | "root pane" | "layered pane" | "unknown"
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
        "bounds" | "rect" => Some(4),
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

    #[test]
    fn selector_defaults_to_label_query() {
        let query = ElementQuery::from_selector("Submit");

        assert_eq!(query.role, None);
        assert_eq!(query.label.as_deref(), Some("Submit"));
    }

    #[test]
    fn selector_accepts_role_and_label_parts() {
        let query = ElementQuery::from_selector("role=push button,label=Save");

        assert_eq!(query.role.as_deref(), Some("push button"));
        assert_eq!(query.label.as_deref(), Some("Save"));
    }

    #[test]
    fn query_matches_case_insensitive_role_and_label() {
        let query = ElementQuery::from_selector("role:button,text:submit");
        let element = UiElement {
            id: "element-1".to_owned(),
            role: "Push Button".to_owned(),
            label: Some("Submit order".to_owned()),
            bounds: Rect::new(0, 0, 100, 40),
            confidence: 1.0,
            states: vec!["enabled".to_owned()],
        };

        assert!(query.matches(&element));
    }

    #[test]
    fn selector_accepts_bounds_state_and_confidence_parts() {
        let query =
            ElementQuery::from_selector("role=button,state=enabled,contains=25,30,confidence>=0.9");
        let element = UiElement {
            id: "element-1".to_owned(),
            role: "push button".to_owned(),
            label: Some("Submit order".to_owned()),
            bounds: Rect::new(10, 20, 100, 40),
            confidence: 0.95,
            states: vec!["enabled".to_owned(), "visible".to_owned()],
        };

        assert_eq!(query.contains_point, Some(Point::new(25, 30)));
        assert_eq!(query.min_confidence, Some(0.9));
        assert!(query.matches(&element));
    }

    #[test]
    fn selector_accepts_exact_bounds() {
        let query = ElementQuery::from_selector("bounds=10,20,90,30");
        let element = UiElement {
            id: "element-1".to_owned(),
            role: "push button".to_owned(),
            label: Some("Submit".to_owned()),
            bounds: Rect::new(10, 20, 90, 30),
            confidence: 1.0,
            states: Vec::new(),
        };

        assert_eq!(query.bounds, Some(Rect::new(10, 20, 90, 30)));
        assert!(query.matches(&element));
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
        let element = UiElement {
            id: "button-1".to_owned(),
            role: "push button".to_owned(),
            label: Some("Submit".to_owned()),
            bounds: Rect::new(10, 20, 90, 30),
            confidence: 1.0,
            states: Vec::new(),
        };

        assert_eq!(
            element_center(&element),
            Some(peekaboox_core::Point::new(55, 35))
        );
    }

    #[test]
    fn resolves_click_target_by_best_label_match() {
        let weak_match = UiElement {
            id: "button-2".to_owned(),
            role: "push button".to_owned(),
            label: Some("Submit later".to_owned()),
            bounds: Rect::new(0, 0, 80, 20),
            confidence: 1.0,
            states: Vec::new(),
        };
        let exact_match = UiElement {
            id: "button-1".to_owned(),
            role: "push button".to_owned(),
            label: Some("Submit".to_owned()),
            bounds: Rect::new(10, 20, 90, 30),
            confidence: 1.0,
            states: Vec::new(),
        };

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
