use super::super::types::{LayoutPreset, WorkbenchView};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaletteCommand {
    View(WorkbenchView),
    Filter(&'static str),
    ConsoleFilter(&'static str),
    WebSocketFilter(&'static str),
    ClearFilter,
    SortNext,
    SortDirection,
    ToggleLayout,
    ToggleDensity,
    LayoutPreset(LayoutPreset),
    ToggleHelp,
    ToggleThemePreview,
    TogglePerf,
    OpenSessions,
    OpenBrowser,
    RefreshPage,
    CopyCurl,
    CopyShareBundle,
    SaveExchange,
    Replay,
    EditReplay,
    DiffReplay,
    OpenEditor,
    CopyBody,
    BodySearch,
    EditConsole,
    SqlQuery,
    CreateScript,
    EditScript,
    RunScript,
    DuplicateScript,
    RenameScript,
    DeleteScript,
    ResetScriptTemplates,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PaletteEntry {
    pub(crate) title: &'static str,
    pub(crate) hint: &'static str,
    pub(crate) command: PaletteCommand,
}

pub(crate) const PALETTE_ENTRIES: &[PaletteEntry] = &[
    PaletteEntry {
        title: "View: Network",
        hint: "requests traffic http",
        command: PaletteCommand::View(WorkbenchView::Network),
    },
    PaletteEntry {
        title: "View: Console",
        hint: "logs javascript errors",
        command: PaletteCommand::View(WorkbenchView::Console),
    },
    PaletteEntry {
        title: "View: WebSockets",
        hint: "ws frames streaming realtime",
        command: PaletteCommand::View(WorkbenchView::WebSockets),
    },
    PaletteEntry {
        title: "View: Scripts",
        hint: "script workflows rhai javascript",
        command: PaletteCommand::View(WorkbenchView::Scripts),
    },
    PaletteEntry {
        title: "View: Storage",
        hint: "localStorage sessionStorage",
        command: PaletteCommand::View(WorkbenchView::Storage),
    },
    PaletteEntry {
        title: "View: Cookies",
        hint: "cookie jar events",
        command: PaletteCommand::View(WorkbenchView::Cookies),
    },
    PaletteEntry {
        title: "Filter: All",
        hint: "preset clear",
        command: PaletteCommand::Filter(""),
    },
    PaletteEntry {
        title: "Filter: Errors",
        hint: "preset 4xx 5xx",
        command: PaletteCommand::Filter("has:error"),
    },
    PaletteEntry {
        title: "Filter: JSON",
        hint: "preset mime json",
        command: PaletteCommand::Filter("mime:json"),
    },
    PaletteEntry {
        title: "Filter: Fetch",
        hint: "preset fetch",
        command: PaletteCommand::Filter("type:fetch"),
    },
    PaletteEntry {
        title: "Filter: XHR",
        hint: "preset xhr ajax",
        command: PaletteCommand::Filter("type:xhr"),
    },
    PaletteEntry {
        title: "Filter: SSE",
        hint: "preset server sent events event-stream",
        command: PaletteCommand::Filter("mime:event-stream"),
    },
    PaletteEntry {
        title: "Filter: Images",
        hint: "preset png jpg svg webp",
        command: PaletteCommand::Filter("type:image"),
    },
    PaletteEntry {
        title: "Filter: Scripts",
        hint: "preset js javascript",
        command: PaletteCommand::Filter("type:script"),
    },
    PaletteEntry {
        title: "Filter: Styles",
        hint: "preset css stylesheet",
        command: PaletteCommand::Filter("type:stylesheet"),
    },
    PaletteEntry {
        title: "Filter: Documents",
        hint: "preset html document",
        command: PaletteCommand::Filter("type:document"),
    },
    PaletteEntry {
        title: "Filter: With Body",
        hint: "preset payload response",
        command: PaletteCommand::Filter("has:body"),
    },
    PaletteEntry {
        title: "Filter: Slow",
        hint: "preset latency duration",
        command: PaletteCommand::Filter("duration:>500"),
    },
    PaletteEntry {
        title: "Filter: Large",
        hint: "preset bytes size",
        command: PaletteCommand::Filter("size:>100kb"),
    },
    PaletteEntry {
        title: "Filter: Replayed",
        hint: "preset replay history",
        command: PaletteCommand::Filter("has:replay"),
    },
    PaletteEntry {
        title: "Console Filter: All",
        hint: "console levels clear",
        command: PaletteCommand::ConsoleFilter(""),
    },
    PaletteEntry {
        title: "Console Filter: Errors",
        hint: "console error fatal",
        command: PaletteCommand::ConsoleFilter("level:error"),
    },
    PaletteEntry {
        title: "Console Filter: Warnings",
        hint: "console warn warnings",
        command: PaletteCommand::ConsoleFilter("level:warn"),
    },
    PaletteEntry {
        title: "Console Filter: Info",
        hint: "console info logs",
        command: PaletteCommand::ConsoleFilter("level:info"),
    },
    PaletteEntry {
        title: "Console Filter: Eval",
        hint: "console faro eval",
        command: PaletteCommand::ConsoleFilter("kind:eval"),
    },
    PaletteEntry {
        title: "WebSocket Filter: All",
        hint: "websocket frames clear",
        command: PaletteCommand::WebSocketFilter(""),
    },
    PaletteEntry {
        title: "WebSocket Filter: Sent",
        hint: "websocket sent outbound",
        command: PaletteCommand::WebSocketFilter("sent"),
    },
    PaletteEntry {
        title: "WebSocket Filter: Received",
        hint: "websocket received inbound",
        command: PaletteCommand::WebSocketFilter("received"),
    },
    PaletteEntry {
        title: "WebSocket Filter: Text",
        hint: "websocket text opcode",
        command: PaletteCommand::WebSocketFilter("text"),
    },
    PaletteEntry {
        title: "Clear Filter",
        hint: "reset search",
        command: PaletteCommand::ClearFilter,
    },
    PaletteEntry {
        title: "Sort: Next Mode",
        hint: "status duration size method",
        command: PaletteCommand::SortNext,
    },
    PaletteEntry {
        title: "Sort: Toggle Direction",
        hint: "ascending descending",
        command: PaletteCommand::SortDirection,
    },
    PaletteEntry {
        title: "Layout: Toggle Focus",
        hint: "maximize pane",
        command: PaletteCommand::ToggleLayout,
    },
    PaletteEntry {
        title: "Layout: Toggle Density",
        hint: "compact comfortable chrome",
        command: PaletteCommand::ToggleDensity,
    },
    PaletteEntry {
        title: "Layout: Compact Network",
        hint: "preset dense request table",
        command: PaletteCommand::LayoutPreset(LayoutPreset::CompactNetwork),
    },
    PaletteEntry {
        title: "Layout: Body Heavy",
        hint: "preset response viewer",
        command: PaletteCommand::LayoutPreset(LayoutPreset::BodyHeavy),
    },
    PaletteEntry {
        title: "Layout: Console Heavy",
        hint: "preset console focus",
        command: PaletteCommand::LayoutPreset(LayoutPreset::ConsoleHeavy),
    },
    PaletteEntry {
        title: "Layout: WebSocket Heavy",
        hint: "preset websocket focus",
        command: PaletteCommand::LayoutPreset(LayoutPreset::WebSocketHeavy),
    },
    PaletteEntry {
        title: "Debug: Toggle Perf",
        hint: "render timing overlay",
        command: PaletteCommand::TogglePerf,
    },
    PaletteEntry {
        title: "Sessions: Browse",
        hint: "switch delete captured sessions",
        command: PaletteCommand::OpenSessions,
    },
    PaletteEntry {
        title: "Theme: Preview",
        hint: "colors swatches gruvbox",
        command: PaletteCommand::ToggleThemePreview,
    },
    PaletteEntry {
        title: "Open Browser",
        hint: "start capture cdp",
        command: PaletteCommand::OpenBrowser,
    },
    PaletteEntry {
        title: "Refresh Page",
        hint: "reload browser cdp f5",
        command: PaletteCommand::RefreshPage,
    },
    PaletteEntry {
        title: "Copy Curl",
        hint: "selected request",
        command: PaletteCommand::CopyCurl,
    },
    PaletteEntry {
        title: "Copy Share Bundle",
        hint: "redacted markdown request replay",
        command: PaletteCommand::CopyShareBundle,
    },
    PaletteEntry {
        title: "Save Exchange",
        hint: "selected request response",
        command: PaletteCommand::SaveExchange,
    },
    PaletteEntry {
        title: "Replay Request",
        hint: "curl selected",
        command: PaletteCommand::Replay,
    },
    PaletteEntry {
        title: "Edit Replay Request",
        hint: "modify curl",
        command: PaletteCommand::EditReplay,
    },
    PaletteEntry {
        title: "Diff Latest Replay",
        hint: "compare response",
        command: PaletteCommand::DiffReplay,
    },
    PaletteEntry {
        title: "Open Body in Editor",
        hint: "response request",
        command: PaletteCommand::OpenEditor,
    },
    PaletteEntry {
        title: "Copy Body Selection",
        hint: "response body json path value",
        command: PaletteCommand::CopyBody,
    },
    PaletteEntry {
        title: "Body Search",
        hint: "find response body text",
        command: PaletteCommand::BodySearch,
    },
    PaletteEntry {
        title: "Console: Evaluate JS",
        hint: "scratch expression",
        command: PaletteCommand::EditConsole,
    },
    PaletteEntry {
        title: "SQL Query",
        hint: "read-only sqlite workbench database",
        command: PaletteCommand::SqlQuery,
    },
    PaletteEntry {
        title: "Scripts: New",
        hint: "create editor workflow",
        command: PaletteCommand::CreateScript,
    },
    PaletteEntry {
        title: "Scripts: Edit",
        hint: "open selected script in editor",
        command: PaletteCommand::EditScript,
    },
    PaletteEntry {
        title: "Scripts: Run",
        hint: "execute selected script",
        command: PaletteCommand::RunScript,
    },
    PaletteEntry {
        title: "Scripts: Duplicate",
        hint: "copy selected script",
        command: PaletteCommand::DuplicateScript,
    },
    PaletteEntry {
        title: "Scripts: Rename",
        hint: "rename selected script",
        command: PaletteCommand::RenameScript,
    },
    PaletteEntry {
        title: "Scripts: Delete",
        hint: "remove selected script",
        command: PaletteCommand::DeleteScript,
    },
    PaletteEntry {
        title: "Scripts: Reset Templates",
        hint: "preload useful automation examples",
        command: PaletteCommand::ResetScriptTemplates,
    },
    PaletteEntry {
        title: "Show Keys",
        hint: "help modal shortcuts",
        command: PaletteCommand::ToggleHelp,
    },
];
