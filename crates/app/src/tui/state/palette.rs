mod catalog;
mod presets;

pub(super) use catalog::PALETTE_ENTRIES;
pub(crate) use catalog::{PaletteCommand, PaletteEntry};
pub(super) use presets::{
    CONSOLE_FILTER_PRESETS, FILTER_PRESETS, WEBSOCKET_FILTER_PRESETS, filter_preset_status,
    filter_query_for_preset_label, next_filter_preset,
};

pub(super) fn palette_matches(entry: &PaletteEntry, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    let haystack = format!("{} {}", entry.title, entry.hint).to_lowercase();
    query
        .split_whitespace()
        .all(|part| fuzzy_contains(&haystack, part))
}

fn fuzzy_contains(haystack: &str, needle: &str) -> bool {
    let mut chars = haystack.chars();
    needle
        .chars()
        .all(|needle_char| chars.any(|haystack_char| haystack_char == needle_char))
}
