use regex::{Regex, RegexBuilder};

pub(super) struct FilterPattern {
    lower: String,
    regex: Option<Regex>,
}

impl FilterPattern {
    pub(super) fn parse(value: &str) -> Self {
        let (raw, explicit_regex) = slash_pattern(value).unwrap_or((value, false));
        let mut regex = None;
        if explicit_regex || looks_like_regex(raw) {
            let compiled = RegexBuilder::new(raw).case_insensitive(true).build();
            if let Ok(compiled_regex) = compiled {
                regex = Some(compiled_regex);
            }
        }
        Self {
            lower: raw.to_lowercase(),
            regex,
        }
    }

    pub(super) fn matches(&self, value: &str) -> bool {
        self.regex
            .as_ref()
            .map(|regex| regex.is_match(value))
            .unwrap_or_else(|| contains_lower(value, &self.lower))
    }
}

fn slash_pattern(value: &str) -> Option<(&str, bool)> {
    (value.len() >= 2 && value.starts_with('/') && value.ends_with('/'))
        .then(|| (&value[1..value.len() - 1], true))
}

fn looks_like_regex(value: &str) -> bool {
    value.chars().any(|ch| {
        matches!(
            ch,
            '^' | '$' | '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\'
        )
    })
}

fn contains_lower(value: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    if value.is_ascii() && needle.is_ascii() {
        let value = value.as_bytes();
        let needle = needle.as_bytes();
        if needle.len() > value.len() {
            return false;
        }
        return value.windows(needle.len()).any(|window| {
            window
                .iter()
                .zip(needle)
                .all(|(left, right)| left.eq_ignore_ascii_case(right))
        });
    }
    value.to_lowercase().contains(needle)
}
