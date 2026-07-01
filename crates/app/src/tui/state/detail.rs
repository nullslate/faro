use super::*;

impl WorkbenchState {
    pub(crate) fn next_sort_mode(&mut self) {
        self.sort_mode = self.sort_mode.next();
        self.apply_filter();
    }

    pub(crate) fn toggle_sort_direction(&mut self) {
        self.sort_descending = !self.sort_descending;
        self.apply_filter();
    }

    pub(crate) fn hydrate_selected_request(&mut self) {
        let Some(request_index) = self.selected_request_index() else {
            return;
        };
        let Some(request) = self.requests.get(request_index) else {
            return;
        };
        if request.details_loaded {
            return;
        }
        let request_id = request.request.id.clone();
        if let Err(error) = self.load_request_details(request_index, &request_id) {
            self.status = format!("request detail load failed: {error}");
            self.note_status_changed();
        }
    }

    pub(crate) fn hydrate_selected_request_for_active_detail(&mut self) {
        // Detail hydration is handled asynchronously by `detail_loader`.
        // Keeping navigation/filtering non-blocking matters when moving
        // through large request lists with response bodies or replay history.
    }

    pub(crate) fn refresh_replays_for_request(&mut self, request_id: &str) -> anyhow::Result<()> {
        let Some(request_index) = self.request_indices_by_id.get(request_id).copied() else {
            return Ok(());
        };
        let store = Store::open(&self.db_path)
            .with_context(|| format!("open database {}", self.db_path.display()))?;
        let replays = store
            .replays_for_request(request_id)?
            .into_iter()
            .map(|record| replay_view_for_record(&store, record))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let Some(request) = self.requests.get_mut(request_index) else {
            return Ok(());
        };
        let had_replays = !request.replays.is_empty();
        request.replays = replays;
        let has_replays = !request.replays.is_empty();
        request.details_loaded = true;
        self.update_replayed_request_stats(had_replays, has_replays);
        self.sync_selected_replay_index();
        Ok(())
    }

    pub(crate) fn apply_request_details(
        &mut self,
        request_id: &str,
        request_body: Option<String>,
        response_body: Option<String>,
        replays: Vec<ReplayView>,
    ) {
        let Some(request_index) = self.request_indices_by_id.get(request_id).copied() else {
            return;
        };
        let Some(request) = self.requests.get_mut(request_index) else {
            return;
        };
        let had_replays = !request.replays.is_empty();
        request.request_body = request_body;
        request.response_body = response_body;
        request.replays = replays;
        let has_replays = !request.replays.is_empty();
        request.details_loaded = true;
        self.update_replayed_request_stats(had_replays, has_replays);
        self.sync_selected_replay_index();
    }

    fn load_request_details(
        &mut self,
        request_index: usize,
        request_id: &str,
    ) -> anyhow::Result<()> {
        let store = Store::open(&self.db_path)
            .with_context(|| format!("open database {}", self.db_path.display()))?;
        let Some(request) = self.requests.get(request_index) else {
            return Ok(());
        };

        let request_body_ref = request.request.request_body_ref.clone();
        let response_body_ref = request
            .response
            .as_ref()
            .and_then(|response| response.body_ref.clone());
        let request_record_id = request.request.id.clone();

        let request_body = body_text_for_ref(&store, request_body_ref.as_deref())
            .with_context(|| format!("load request body for {request_record_id}"))?;
        let response_body = response_body_ref
            .as_deref()
            .map(|body_id| body_text_for_ref(&store, Some(body_id)))
            .transpose()
            .with_context(|| format!("load response body for {request_record_id}"))?
            .flatten();
        let replays = store
            .replays_for_request(request_id)?
            .into_iter()
            .map(|record| replay_view_for_record(&store, record))
            .collect::<anyhow::Result<Vec<_>>>()?;

        let Some(request) = self.requests.get_mut(request_index) else {
            return Ok(());
        };
        let had_replays = !request.replays.is_empty();
        request.request_body = request_body;
        request.response_body = response_body;
        request.replays = replays;
        let has_replays = !request.replays.is_empty();
        request.details_loaded = true;
        self.update_replayed_request_stats(had_replays, has_replays);
        self.sync_selected_replay_index();
        Ok(())
    }

    fn update_replayed_request_stats(&mut self, had_replays: bool, has_replays: bool) {
        match (had_replays, has_replays) {
            (false, true) => self.request_stats.replayed += 1,
            (true, false) => {
                self.request_stats.replayed = self.request_stats.replayed.saturating_sub(1);
            }
            _ => {}
        }
    }

    pub(crate) fn sync_selected_replay_index(&mut self) {
        let Some(replay_count) = self.selected_request().map(|request| request.replays.len())
        else {
            self.selected_replay_index = 0;
            return;
        };
        if replay_count == 0 {
            self.selected_replay_index = 0;
            return;
        }
        self.selected_replay_index = self
            .selected_replay_index
            .min(replay_count.saturating_sub(1));
    }

    pub(crate) fn detail_line_count(&self) -> u16 {
        let count = match (self.detail_tab, self.selected_request()) {
            (_, None) => 1,
            (DetailTab::Overview, Some(request)) => {
                9 + request
                    .response
                    .as_ref()
                    .map(|response| usize::from(response.body_truncated))
                    .unwrap_or(0)
            }
            (DetailTab::RequestHeaders, Some(request)) => request.request.request_headers.len() + 2,
            (DetailTab::RequestBody, Some(_)) => self
                .selected_request()
                .map(formatted_request_body)
                .unwrap_or_default()
                .lines()
                .count()
                .max(1),
            (DetailTab::ResponseHeaders, Some(request)) => request
                .response
                .as_ref()
                .map(|response| response.response_headers.len() + 2)
                .unwrap_or(1),
            (DetailTab::ResponseBody, Some(_)) => self
                .selected_request()
                .map(formatted_response_body)
                .unwrap_or_default()
                .lines()
                .count()
                .max(1),
            (DetailTab::Timing, Some(_)) => 6,
            (DetailTab::Replay, Some(request)) => {
                if request.replays.is_empty() {
                    3
                } else {
                    8 + request.replays.len().min(12)
                }
            }
        };
        count.min(u16::MAX as usize) as u16
    }

    pub(crate) fn copy_curl_text(&self) -> Option<String> {
        self.selected_request().map(build_curl)
    }

    pub(crate) fn copy_body_text(&self) -> Option<String> {
        if self.focus == FocusPane::Detail
            && self.detail_tab == DetailTab::Replay
            && let Some(replay) = self.selected_replay()
        {
            return replay
                .body
                .clone()
                .or_else(|| Some(format_replay_record(replay)));
        }
        if self.focus == FocusPane::Body {
            let items = self.body_tree_items();
            if let Some(item) = items.get(self.body_tree_selected) {
                return Some(match &item.value {
                    Some(value) => format!("{} = {value}", item.key),
                    None => item.key.clone(),
                });
            }
        }
        self.selected_request().map(formatted_response_body)
    }

    pub(crate) fn replay_curl_args(&self) -> Option<Vec<String>> {
        self.selected_request().map(build_curl_args)
    }

    pub(crate) fn selected_replay_context(&self) -> Option<ReplayContext> {
        let request = self.selected_request()?;
        Some((
            request.request.session_id.clone(),
            request.request.tab_id.clone(),
            request.request.run_id.clone(),
            request.request.id.clone(),
            self.copy_curl_text()?,
        ))
    }

    pub(crate) fn selected_editable_request(&self) -> Option<String> {
        let request = self.selected_request()?;
        let mut text = format!("{} {}\n", request.request.method, request.request.url);
        for header in &request.request.request_headers {
            text.push_str(&format!("{}: {}\n", header.name, header.value));
        }
        text.push('\n');
        if let Some(body) = request.request_body.as_deref() {
            text.push_str(body);
        }
        Some(text)
    }

    pub(crate) fn selected_replay_diff_bodies(&self) -> Option<(String, String)> {
        let request = self.selected_request()?;
        let original = formatted_response_body(request);
        let replay = self.selected_replay()?.body.clone().unwrap_or_default();
        Some((original, replay))
    }

    pub(crate) fn selected_replay_export_text(&self) -> Option<String> {
        let replay = self.selected_replay()?;
        let mut text = format_replay_record(replay);
        if let Some(body) = replay.body.as_deref() {
            text.push_str("\n--- response body ---\n");
            text.push_str(body);
        }
        Some(text)
    }

    pub(crate) fn selected_response_body_for_editor(&self) -> Option<(String, String)> {
        let request = self.selected_request()?;
        let extension = request
            .response
            .as_ref()
            .and_then(|response| response.mime_type.as_deref())
            .map(extension_for_mime)
            .unwrap_or("txt")
            .to_string();
        Some((formatted_response_body(request), extension))
    }

    pub(crate) fn selected_request_body_for_editor(&self) -> Option<(String, String)> {
        let request = self.selected_request()?;
        let extension = request
            .request
            .request_headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case("content-type"))
            .map(|header| extension_for_mime(&header.value))
            .unwrap_or("txt")
            .to_string();
        Some((formatted_request_body(request), extension))
    }
}
pub(crate) fn formatted_response_body(request: &RequestView) -> String {
    let Some(body) = request.response_body.as_deref() else {
        return "No response body captured for this request.".to_string();
    };

    if looks_like_json(
        request
            .response
            .as_ref()
            .and_then(|response| response.mime_type.as_deref()),
        body,
    ) {
        serde_json::from_str::<serde_json::Value>(body)
            .and_then(|value| serde_json::to_string_pretty(&value))
            .unwrap_or_else(|_| body.to_string())
    } else {
        body.to_string()
    }
}

pub(crate) fn formatted_request_body(request: &RequestView) -> String {
    let Some(body) = request.request_body.as_deref() else {
        return "No request body captured for this request.".to_string();
    };

    if looks_like_json(
        request
            .request
            .request_headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case("content-type"))
            .map(|header| header.value.as_str()),
        body,
    ) {
        serde_json::from_str::<serde_json::Value>(body)
            .and_then(|value| serde_json::to_string_pretty(&value))
            .unwrap_or_else(|_| body.to_string())
    } else {
        body.to_string()
    }
}

fn format_replay_record(replay: &ReplayView) -> String {
    let mut text = String::new();
    text.push_str(&format!("replay_id: {}\n", replay.record.id));
    text.push_str(&format!(
        "source_request_id: {}\n",
        replay.record.source_request_id
    ));
    text.push_str(&format!("timestamp: {}\n", replay.record.ts));
    text.push_str(&format!(
        "status: {}\n",
        replay
            .record
            .status_code
            .map(|status| status.to_string())
            .unwrap_or_else(|| "-".to_string())
    ));
    text.push_str(&format!(
        "exit: {}\n",
        replay
            .record
            .exit_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "-".to_string())
    ));
    if let Some(error) = replay.record.error.as_deref() {
        text.push_str(&format!("error: {error}\n"));
    }
    text.push_str(&format!("command: {}\n", replay.record.command));
    if let Some(output_path) = replay.record.output_path.as_deref() {
        text.push_str(&format!("output_path: {output_path}\n"));
    }
    text
}

pub(crate) fn looks_like_json(mime_type: Option<&str>, body: &str) -> bool {
    mime_type.map(|mime| mime.contains("json")).unwrap_or(false)
        || body.trim_start().starts_with('{')
        || body.trim_start().starts_with('[')
}

fn build_curl(request: &RequestView) -> String {
    build_curl_command(&build_curl_args(request))
}

pub(crate) fn build_curl_args(request: &RequestView) -> Vec<String> {
    service_build_curl_args(&request.request, request.request_body.as_deref())
}

fn extension_for_mime(mime: &str) -> &'static str {
    if mime.contains("json") {
        "json"
    } else if mime.contains("html") {
        "html"
    } else if mime.contains("css") {
        "css"
    } else if mime.contains("javascript") || mime.contains("ecmascript") {
        "js"
    } else if mime.contains("xml") {
        "xml"
    } else {
        "txt"
    }
}
