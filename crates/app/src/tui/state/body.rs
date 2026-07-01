use super::{
    BodyTreeCache, BodyTreeItem, WorkbenchState, formatted_response_body, looks_like_json,
};
use std::collections::HashSet;

impl WorkbenchState {
    pub(crate) fn toggle_selected_body_tree_node(&mut self) {
        let items = self.body_tree_items();
        let Some(item) = items.get(self.body_tree_selected) else {
            return;
        };
        if !item.expandable {
            self.status = "body node is not expandable".to_string();
            return;
        }
        if self.collapsed_body_nodes.remove(&item.key) {
            self.status = format!("expanded {}", item.label);
        } else {
            self.collapsed_body_nodes.insert(item.key.clone());
            self.status = format!("collapsed {}", item.label);
        }
        let selected_key = item.key.clone();
        let len = self.body_tree_items().len();
        if len > 0 {
            self.body_tree_selected = self.body_tree_selected.min(len - 1);
        }
        self.select_body_tree_key(&selected_key);
        self.note_status_changed();
    }

    pub(super) fn has_body_tree(&self) -> bool {
        !self.body_tree_items().is_empty()
    }

    pub(super) fn next_body_tree_node(&mut self) {
        let len = self.body_tree_items().len();
        if len == 0 {
            return;
        }
        self.body_tree_selected = (self.body_tree_selected + 1).min(len - 1);
        self.sync_body_tree_selected_key();
        self.body_scroll = self
            .body_tree_selected
            .saturating_sub(3)
            .min(u16::MAX as usize) as u16;
    }

    pub(super) fn previous_body_tree_node(&mut self) {
        if self.body_tree_items().is_empty() {
            return;
        }
        self.body_tree_selected = self.body_tree_selected.saturating_sub(1);
        self.sync_body_tree_selected_key();
        self.body_scroll = self
            .body_tree_selected
            .saturating_sub(3)
            .min(u16::MAX as usize) as u16;
    }

    pub(super) fn select_body_tree_key(&mut self, key: &str) {
        let items = self.body_tree_items();
        if let Some(index) = items.iter().position(|item| item.key == key) {
            self.body_tree_selected = index;
            self.body_tree_selected_key = Some(key.to_string());
            self.body_scroll = self
                .body_tree_selected
                .saturating_sub(3)
                .min(u16::MAX as usize) as u16;
        } else {
            self.sync_body_tree_selected_key();
        }
    }

    pub(super) fn sync_body_tree_selected_key(&mut self) {
        let items = self.body_tree_items();
        if let Some(item) = items.get(self.body_tree_selected) {
            self.body_tree_selected_key = Some(item.key.clone());
        } else {
            self.body_tree_selected = 0;
            self.body_tree_selected_key = items.first().map(|item| item.key.clone());
        }
    }

    pub(super) fn body_line_count(&self) -> u16 {
        let tree_count = self.body_tree_items().len();
        if tree_count > 0 {
            return (tree_count + 2).min(u16::MAX as usize) as u16;
        }
        self.selected_request()
            .map(formatted_response_body)
            .unwrap_or_else(|| "No response body captured for this request.".to_string())
            .lines()
            .count()
            .max(1)
            .min(u16::MAX as usize) as u16
    }

    pub(crate) fn body_tree_items(&self) -> Vec<BodyTreeItem> {
        let Some(request) = self.selected_request() else {
            return Vec::new();
        };
        let Some(body) = request.response_body.as_deref() else {
            return Vec::new();
        };
        let response_body_ref = request
            .response
            .as_ref()
            .and_then(|response| response.body_ref.clone());
        let collapsed_keys = sorted_collapsed_body_keys(&self.collapsed_body_nodes);
        if let Some(cache) = self.body_tree_cache.borrow().as_ref()
            && cache.request_id == request.request.id
            && cache.response_body_ref == response_body_ref
            && cache.response_body_len == body.len()
            && cache.max_items == self.config.ui.max_body_tree_items
            && cache.collapsed_keys == collapsed_keys
        {
            return cache.items.clone();
        }
        let mime = request
            .response
            .as_ref()
            .and_then(|response| response.mime_type.as_deref());
        let items = build_body_tree_items(
            mime,
            body,
            &self.collapsed_body_nodes,
            self.config.ui.max_body_tree_items,
        );
        self.body_tree_cache.replace(Some(BodyTreeCache {
            request_id: request.request.id.clone(),
            response_body_ref,
            response_body_len: body.len(),
            max_items: self.config.ui.max_body_tree_items,
            collapsed_keys,
            items: items.clone(),
        }));
        items
    }
}

fn build_body_tree_items(
    mime: Option<&str>,
    body: &str,
    collapsed_body_nodes: &HashSet<String>,
    max_items: usize,
) -> Vec<BodyTreeItem> {
    if looks_like_json(mime, body)
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(body)
    {
        let mut items = Vec::new();
        let mut context = BodyTreeBuildContext {
            collapsed: collapsed_body_nodes,
            truncated: false,
            max_items,
        };
        push_json_tree_item(
            &mut items,
            &mut context,
            "$".to_string(),
            "$".to_string(),
            &value,
            0,
        );
        if context.truncated {
            items.push(BodyTreeItem {
                key: "$.__faro_truncated".to_string(),
                depth: 0,
                label: "truncated".to_string(),
                value: Some(format!("showing first {max_items} nodes")),
                expandable: false,
                collapsed: false,
            });
        }
        return items;
    }
    if mime.map(|mime| mime.contains("html")).unwrap_or(false) {
        return html_body_tree_items(body, collapsed_body_nodes, max_items);
    }
    Vec::new()
}

struct BodyTreeBuildContext<'a> {
    collapsed: &'a HashSet<String>,
    truncated: bool,
    max_items: usize,
}

fn sorted_collapsed_body_keys(collapsed: &HashSet<String>) -> Vec<String> {
    let mut keys = collapsed.iter().cloned().collect::<Vec<_>>();
    keys.sort();
    keys
}

fn push_json_tree_item(
    items: &mut Vec<BodyTreeItem>,
    context: &mut BodyTreeBuildContext<'_>,
    key: String,
    label: String,
    value: &serde_json::Value,
    depth: usize,
) {
    if items.len() >= context.max_items {
        context.truncated = true;
        return;
    }
    let expandable = matches!(
        value,
        serde_json::Value::Object(_) | serde_json::Value::Array(_)
    );
    let collapsed_here = context.collapsed.contains(&key);
    let value_label = json_tree_value_label(value);

    items.push(BodyTreeItem {
        key: key.clone(),
        depth,
        label,
        value: value_label,
        expandable,
        collapsed: collapsed_here,
    });

    if collapsed_here {
        return;
    }

    match value {
        serde_json::Value::Object(map) => {
            for (child_key, child_value) in map {
                if context.truncated {
                    break;
                }
                push_json_tree_item(
                    items,
                    context,
                    format!("{key}.{child_key}"),
                    child_key.clone(),
                    child_value,
                    depth + 1,
                );
            }
        }
        serde_json::Value::Array(values) => {
            for (index, child_value) in values.iter().enumerate() {
                if context.truncated {
                    break;
                }
                push_json_tree_item(
                    items,
                    context,
                    format!("{key}[{index}]"),
                    format!("[{index}]"),
                    child_value,
                    depth + 1,
                );
            }
        }
        _ => {}
    }
}

fn json_tree_value_label(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => Some(format!("{{{}}}", map.len())),
        serde_json::Value::Array(values) => Some(format!("[{}]", values.len())),
        serde_json::Value::String(value) => Some(format!("\"{}\"", compact_string(value, 90))),
        serde_json::Value::Number(value) => Some(value.to_string()),
        serde_json::Value::Bool(value) => Some(value.to_string()),
        serde_json::Value::Null => Some("null".to_string()),
    }
}

fn compact_string(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    value
        .chars()
        .take(max.saturating_sub(1))
        .collect::<String>()
        + "…"
}

fn html_body_tree_items(
    body: &str,
    collapsed: &HashSet<String>,
    max_items: usize,
) -> Vec<BodyTreeItem> {
    let mut items = Vec::new();
    let mut depth = 0_usize;
    let mut cursor = 0_usize;
    let mut sequence = Vec::<usize>::new();
    let mut hidden_depth = None::<usize>;
    while cursor < body.len() {
        if items.len() >= max_items {
            items.push(BodyTreeItem {
                key: "html.__faro_truncated".to_string(),
                depth: 0,
                label: "truncated".to_string(),
                value: Some(format!("showing first {max_items} nodes")),
                expandable: false,
                collapsed: false,
            });
            break;
        }
        let remaining = &body[cursor..];
        let Some(tag_start_offset) = remaining.find('<') else {
            push_html_tree_text(&mut items, hidden_depth, depth, &mut sequence, remaining);
            break;
        };
        if tag_start_offset > 0 {
            push_html_tree_text(
                &mut items,
                hidden_depth,
                depth,
                &mut sequence,
                &remaining[..tag_start_offset],
            );
        }
        let tag_start = cursor + tag_start_offset;
        let Some(tag_end_offset) = body[tag_start..].find('>') else {
            push_html_tree_text(
                &mut items,
                hidden_depth,
                depth,
                &mut sequence,
                &body[tag_start..],
            );
            break;
        };
        let tag_end = tag_start + tag_end_offset + 1;
        let tag = body[tag_start..tag_end].trim();
        if tag.starts_with("</") {
            depth = depth.saturating_sub(1);
            if hidden_depth == Some(depth) {
                hidden_depth = None;
            }
            cursor = tag_end;
            continue;
        }
        let key = next_html_tree_key(&mut sequence, depth);
        let (name, attrs) = html_tree_tag_name_and_attrs(tag);
        let expandable = !html_tree_tag_is_self_closing(tag);
        if hidden_depth.is_none() {
            items.push(BodyTreeItem {
                key: key.clone(),
                depth,
                label: name,
                value: (!attrs.is_empty()).then_some(attrs),
                expandable,
                collapsed: expandable && collapsed.contains(&key),
            });
            if expandable && collapsed.contains(&key) {
                hidden_depth = Some(depth);
            }
        }
        if expandable {
            depth = depth.saturating_add(1).min(24);
        }
        cursor = tag_end;
    }
    if cursor < body.len() {
        push_html_tree_text(
            &mut items,
            hidden_depth,
            depth,
            &mut sequence,
            &body[cursor..],
        );
    }
    items
}

fn push_html_tree_text(
    items: &mut Vec<BodyTreeItem>,
    hidden_depth: Option<usize>,
    depth: usize,
    sequence: &mut Vec<usize>,
    text: &str,
) {
    if hidden_depth.is_some() {
        return;
    }
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return;
    }
    items.push(BodyTreeItem {
        key: next_html_tree_key(sequence, depth),
        depth,
        label: "text".to_string(),
        value: Some(compact_string(&text, 120)),
        expandable: false,
        collapsed: false,
    });
}

fn next_html_tree_key(sequence: &mut Vec<usize>, depth: usize) -> String {
    if sequence.len() <= depth {
        sequence.resize(depth + 1, 0);
    }
    sequence[depth] += 1;
    sequence.truncate(depth + 1);
    format!(
        "html:{}",
        sequence
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(".")
    )
}

fn html_tree_tag_name_and_attrs(tag: &str) -> (String, String) {
    let inner = tag
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim_end_matches('/')
        .trim_start_matches('/')
        .trim();
    let mut parts = inner.splitn(2, char::is_whitespace);
    let name = parts.next().unwrap_or("").to_string();
    let attrs = parts
        .next()
        .map(|attrs| compact_string(attrs.trim(), 90))
        .unwrap_or_default();
    (name, attrs)
}

fn html_tree_tag_is_self_closing(tag: &str) -> bool {
    let (name, _) = html_tree_tag_name_and_attrs(tag);
    tag.ends_with("/>")
        || matches!(
            name.as_str(),
            "!doctype"
                | "area"
                | "base"
                | "br"
                | "col"
                | "embed"
                | "hr"
                | "img"
                | "input"
                | "link"
                | "meta"
                | "param"
                | "source"
                | "track"
                | "wbr"
        )
}
