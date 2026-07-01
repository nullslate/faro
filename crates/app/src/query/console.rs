use crate::query::pattern::FilterPattern;
use faro_core::{ConsoleLevel, ConsoleLog};

pub(super) fn filter_console_indices(logs: &[ConsoleLog], filter: &str) -> Vec<usize> {
    let filter = ConsoleFilter::parse(filter);
    logs.iter()
        .enumerate()
        .filter_map(|(index, log)| filter.matches(log).then_some(index))
        .collect()
}

struct ConsoleFilter {
    raw_terms: Vec<FilterPattern>,
    level: Option<FilterPattern>,
    source: Option<FilterPattern>,
    kind: Option<String>,
}

impl ConsoleFilter {
    fn parse(input: &str) -> Self {
        let mut filter = Self {
            raw_terms: Vec::new(),
            level: None,
            source: None,
            kind: None,
        };

        for token in input.split_whitespace() {
            let Some((key, value)) = token.split_once(':') else {
                if token.eq_ignore_ascii_case("eval") {
                    filter.kind = Some("eval".to_string());
                } else {
                    filter.raw_terms.push(FilterPattern::parse(token));
                }
                continue;
            };
            let value = value.trim().to_lowercase();
            if value.is_empty() {
                continue;
            }
            match key.to_lowercase().as_str() {
                "level" => filter.level = Some(FilterPattern::parse(&value)),
                "source" => filter.source = Some(FilterPattern::parse(&value)),
                "kind" | "type" => filter.kind = Some(value),
                _ => filter.raw_terms.push(FilterPattern::parse(token)),
            }
        }

        filter
    }

    fn matches(&self, log: &ConsoleLog) -> bool {
        if let Some(level) = &self.level {
            let log_level = console_level_name(&log.level);
            let level_matches = level.matches(log_level)
                || (level.matches("error") && matches!(log.level, ConsoleLevel::Fatal));
            if !level_matches {
                return false;
            }
        }

        if let Some(source) = &self.source {
            let log_source = log.source.as_deref().unwrap_or("-");
            if !source.matches(log_source) {
                return false;
            }
        }

        if let Some(kind) = &self.kind {
            let is_eval = log.source.as_deref() == Some("faro-console");
            match kind.as_str() {
                "eval" if !is_eval => return false,
                "page" if is_eval => return false,
                _ => {}
            }
        }

        if self.raw_terms.is_empty() {
            return true;
        }

        let haystack = [
            log.message.as_str(),
            log.source.as_deref().unwrap_or_default(),
            console_level_name(&log.level),
        ]
        .join(" ");
        self.raw_terms.iter().all(|term| term.matches(&haystack))
    }
}

fn console_level_name(level: &ConsoleLevel) -> &'static str {
    match level {
        ConsoleLevel::Trace => "trace",
        ConsoleLevel::Debug => "debug",
        ConsoleLevel::Info => "info",
        ConsoleLevel::Warning => "warn",
        ConsoleLevel::Error => "error",
        ConsoleLevel::Fatal => "fatal",
    }
}
