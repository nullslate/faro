use super::*;

impl WorkbenchState {
    pub(super) fn next_script(&mut self) {
        if self.scripts.is_empty() {
            return;
        }
        let next = match self.script_state.selected() {
            Some(index) if index + 1 < self.scripts.len() => index + 1,
            _ => 0,
        };
        self.script_state.select(Some(next));
    }

    pub(super) fn previous_script(&mut self) {
        if self.scripts.is_empty() {
            return;
        }
        let previous = match self.script_state.selected() {
            Some(0) | None => self.scripts.len() - 1,
            Some(index) => index - 1,
        };
        self.script_state.select(Some(previous));
    }

    pub(crate) fn selected_script(&self) -> Option<&ScriptRecord> {
        self.script_state
            .selected()
            .and_then(|index| self.scripts.get(index))
    }

    pub(crate) fn select_script_by_id(&mut self, script_id: &str) {
        self.script_state.select(
            self.scripts
                .iter()
                .position(|script| script.id == script_id),
        );
    }
}
