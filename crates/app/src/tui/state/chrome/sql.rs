use super::super::*;

impl WorkbenchState {
    pub(crate) fn apply_sql_request_filter(&mut self, query: String, ids: HashSet<String>) {
        let count = ids.len();
        self.last_sql_query = query;
        self.sql_request_filter_ids = Some(ids);
        self.sql_request_filter_query = Some(self.last_sql_query.clone());
        self.sql_result = None;
        self.set_view(WorkbenchView::Network);
        self.apply_filter();
        self.status = format!("SQL filtered requests to {count} ids");
    }

    pub(crate) fn show_sql_error(&mut self, query: String, error: String) {
        self.sql_result = Some(SqlResultsView {
            query,
            columns: Vec::new(),
            rows: Vec::new(),
            duration_ms: 0,
            error: Some(error.clone()),
        });
        self.sql_row_scroll = 0;
        self.sql_col_scroll = 0;
        self.status = format!("SQL failed: {error}");
    }

    pub(crate) fn close_sql_result(&mut self) {
        self.sql_result = None;
        self.sql_row_scroll = 0;
        self.sql_col_scroll = 0;
    }

    pub(crate) fn scroll_sql_rows_down(&mut self) {
        if let Some(result) = &self.sql_result {
            self.sql_row_scroll = self
                .sql_row_scroll
                .saturating_add(1)
                .min(result.rows.len().saturating_sub(1));
        }
    }

    pub(crate) fn scroll_sql_rows_up(&mut self) {
        self.sql_row_scroll = self.sql_row_scroll.saturating_sub(1);
    }

    pub(crate) fn scroll_sql_columns_right(&mut self) {
        if let Some(result) = &self.sql_result {
            self.sql_col_scroll = self
                .sql_col_scroll
                .saturating_add(1)
                .min(result.columns.len().saturating_sub(1));
        }
    }

    pub(crate) fn scroll_sql_columns_left(&mut self) {
        self.sql_col_scroll = self.sql_col_scroll.saturating_sub(1);
    }

    pub(crate) fn scroll_sql_top(&mut self) {
        self.sql_row_scroll = 0;
    }

    pub(crate) fn scroll_sql_bottom(&mut self) {
        if let Some(result) = &self.sql_result {
            self.sql_row_scroll = result.rows.len().saturating_sub(1);
        }
    }
}
