use super::{CurrentCookieEntry, CurrentStorageEntry, WorkbenchState};
use std::collections::BTreeMap;

impl WorkbenchState {
    pub(crate) fn select_storage_position(&mut self, position: usize) {
        if position < self.current_storage_entries().len() {
            self.storage_selected = position;
            self.storage_scroll = self
                .storage_selected
                .saturating_sub(3)
                .min(u16::MAX as usize) as u16;
        }
    }

    pub(crate) fn select_cookie_position(&mut self, position: usize) {
        if position < self.current_cookie_entries().len() {
            self.cookie_selected = position;
            self.cookie_scroll = self
                .cookie_selected
                .saturating_sub(3)
                .min(u16::MAX as usize) as u16;
        }
    }

    pub(super) fn next_storage_entry(&mut self) {
        let len = self.current_storage_entries().len();
        if len == 0 {
            return;
        }
        self.storage_selected = (self.storage_selected + 1).min(len - 1);
        self.storage_scroll = self
            .storage_selected
            .saturating_sub(3)
            .min(u16::MAX as usize) as u16;
    }

    pub(super) fn previous_storage_entry(&mut self) {
        if self.current_storage_entries().is_empty() {
            return;
        }
        self.storage_selected = self.storage_selected.saturating_sub(1);
        self.storage_scroll = self
            .storage_selected
            .saturating_sub(3)
            .min(u16::MAX as usize) as u16;
    }

    pub(super) fn next_cookie_entry(&mut self) {
        let len = self.current_cookie_entries().len();
        if len == 0 {
            return;
        }
        self.cookie_selected = (self.cookie_selected + 1).min(len - 1);
        self.cookie_scroll = self
            .cookie_selected
            .saturating_sub(3)
            .min(u16::MAX as usize) as u16;
    }

    pub(super) fn previous_cookie_entry(&mut self) {
        if self.current_cookie_entries().is_empty() {
            return;
        }
        self.cookie_selected = self.cookie_selected.saturating_sub(1);
        self.cookie_scroll = self
            .cookie_selected
            .saturating_sub(3)
            .min(u16::MAX as usize) as u16;
    }

    pub(crate) fn current_storage_entries(&self) -> Vec<CurrentStorageEntry> {
        let mut entries: BTreeMap<(String, String, String), String> = BTreeMap::new();

        for snapshot in &self.storage_snapshots {
            for entry in &snapshot.entries {
                entries.insert(
                    (
                        snapshot.storage_type.clone(),
                        snapshot.origin.clone(),
                        entry.key.clone(),
                    ),
                    entry.value.clone(),
                );
            }
        }

        for event in &self.storage_events {
            match event.operation.as_str() {
                "clear" => {
                    let storage_type = &event.storage_type;
                    let origin = &event.origin;
                    entries.retain(|(entry_type, entry_origin, _), _| {
                        entry_type != storage_type || entry_origin != origin
                    });
                }
                "remove" => {
                    if let Some(key) = &event.key {
                        entries.remove(&(
                            event.storage_type.clone(),
                            event.origin.clone(),
                            key.clone(),
                        ));
                    }
                }
                _ => {
                    if let (Some(key), Some(value)) = (&event.key, &event.new_value) {
                        entries.insert(
                            (
                                event.storage_type.clone(),
                                event.origin.clone(),
                                key.clone(),
                            ),
                            value.clone(),
                        );
                    }
                }
            }
        }

        entries
            .into_iter()
            .map(|((storage_type, origin, key), value)| CurrentStorageEntry {
                storage_type,
                origin,
                key,
                value,
            })
            .collect()
    }

    pub(crate) fn current_cookie_entries(&self) -> Vec<CurrentCookieEntry> {
        let mut entries: BTreeMap<(String, String, String), CurrentCookieEntry> = BTreeMap::new();

        for snapshot in &self.cookie_snapshots {
            for cookie in &snapshot.cookies {
                entries.insert(
                    (
                        cookie.domain.clone(),
                        cookie.path.clone(),
                        cookie.name.clone(),
                    ),
                    CurrentCookieEntry {
                        name: cookie.name.clone(),
                        value: cookie.value.clone(),
                        domain: cookie.domain.clone(),
                        path: cookie.path.clone(),
                        expires: cookie.expires,
                        http_only: cookie.http_only,
                        secure: cookie.secure,
                        same_site: cookie.same_site.clone(),
                        flags: cookie_flags(
                            cookie.http_only,
                            cookie.secure,
                            cookie.same_site.as_deref(),
                        ),
                    },
                );
            }
        }

        for event in &self.cookie_events {
            let Some(name) = event.name.as_ref() else {
                continue;
            };
            if event.operation == "delete" || event.operation == "expire" {
                let domain = event.domain.clone().unwrap_or_default();
                let path = event.path.clone().unwrap_or_else(|| "/".to_string());
                entries.remove(&(domain, path, name.clone()));
                continue;
            }

            let domain = event.domain.clone().unwrap_or_default();
            let path = event.path.clone().unwrap_or_else(|| "/".to_string());
            let value = event.value.clone().unwrap_or_default();
            let flags = cookie_event_flags(event.attributes_json.as_ref());

            entries.insert(
                (domain.clone(), path.clone(), name.clone()),
                CurrentCookieEntry {
                    name: name.clone(),
                    value,
                    domain,
                    path,
                    expires: None,
                    http_only: flags.contains("httpOnly"),
                    secure: flags.contains("secure"),
                    same_site: None,
                    flags,
                },
            );
        }

        entries.into_values().collect()
    }

    pub(crate) fn selected_storage_entry(&self) -> Option<CurrentStorageEntry> {
        self.current_storage_entries()
            .get(self.storage_selected)
            .cloned()
    }

    pub(crate) fn selected_cookie_entry(&self) -> Option<CurrentCookieEntry> {
        self.current_cookie_entries()
            .get(self.cookie_selected)
            .cloned()
    }
}

fn cookie_flags(http_only: bool, secure: bool, same_site: Option<&str>) -> String {
    let mut flags = Vec::new();
    if http_only {
        flags.push("httpOnly".to_string());
    }
    if secure {
        flags.push("secure".to_string());
    }
    if let Some(same_site) = same_site {
        flags.push(format!("sameSite={same_site}"));
    }
    flags.join(",")
}

fn cookie_event_flags(attributes_json: Option<&serde_json::Value>) -> String {
    let Some(attributes) = attributes_json.and_then(|value| value.as_object()) else {
        return String::new();
    };

    attributes
        .keys()
        .filter(|key| !matches!(key.as_str(), "name" | "value" | "domain" | "path"))
        .cloned()
        .collect::<Vec<_>>()
        .join(",")
}
