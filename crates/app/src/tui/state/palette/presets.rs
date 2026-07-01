pub(crate) struct FilterPreset {
    pub(crate) label: &'static str,
    pub(crate) query: &'static str,
}

pub(crate) const FILTER_PRESETS: &[FilterPreset] = &[
    FilterPreset {
        label: "all",
        query: "",
    },
    FilterPreset {
        label: "errors",
        query: "has:error",
    },
    FilterPreset {
        label: "json",
        query: "mime:json",
    },
    FilterPreset {
        label: "fetch",
        query: "type:fetch",
    },
    FilterPreset {
        label: "xhr",
        query: "type:xhr",
    },
    FilterPreset {
        label: "sse",
        query: "mime:event-stream",
    },
    FilterPreset {
        label: "images",
        query: "type:image",
    },
    FilterPreset {
        label: "scripts",
        query: "type:script",
    },
    FilterPreset {
        label: "styles",
        query: "type:stylesheet",
    },
    FilterPreset {
        label: "docs",
        query: "type:document",
    },
    FilterPreset {
        label: "with body",
        query: "has:body",
    },
    FilterPreset {
        label: "slow",
        query: "duration:>500",
    },
    FilterPreset {
        label: "large",
        query: "size:>100kb",
    },
    FilterPreset {
        label: "replayed",
        query: "has:replay",
    },
];

pub(crate) const CONSOLE_FILTER_PRESETS: &[FilterPreset] = &[
    FilterPreset {
        label: "all",
        query: "",
    },
    FilterPreset {
        label: "errors",
        query: "level:error",
    },
    FilterPreset {
        label: "warnings",
        query: "level:warn",
    },
    FilterPreset {
        label: "info",
        query: "level:info",
    },
    FilterPreset {
        label: "debug",
        query: "level:debug",
    },
    FilterPreset {
        label: "eval",
        query: "kind:eval",
    },
    FilterPreset {
        label: "page",
        query: "kind:page",
    },
];

pub(crate) const WEBSOCKET_FILTER_PRESETS: &[FilterPreset] = &[
    FilterPreset {
        label: "all",
        query: "",
    },
    FilterPreset {
        label: "sent",
        query: "sent",
    },
    FilterPreset {
        label: "received",
        query: "received",
    },
    FilterPreset {
        label: "text",
        query: "text",
    },
    FilterPreset {
        label: "binary",
        query: "binary",
    },
];

pub(crate) fn next_filter_preset<'a>(
    presets: &'a [FilterPreset],
    current_query: &str,
) -> &'a FilterPreset {
    let current = presets
        .iter()
        .position(|preset| preset.query == current_query);
    let next = current.map(|index| index + 1).unwrap_or(1) % presets.len();
    &presets[next]
}

pub(crate) fn filter_preset_status(scope: &str, preset: &FilterPreset) -> String {
    if preset.query.is_empty() {
        format!("{scope} filter preset all")
    } else {
        format!("{scope} filter preset {}", preset.label)
    }
}

pub(crate) fn filter_query_for_preset_label(label: &str) -> Option<&'static str> {
    FILTER_PRESETS
        .iter()
        .find(|preset| preset.label == label)
        .map(|preset| preset.query)
}
