pub(super) const DEFAULT_CONFIG: &str = r##"# Faro configuration.

[app]
# Default database path used when --db is omitted. Relative paths resolve
# from this config file's directory.
db_path = "faro.db"
# Start capture immediately for URL launches instead of waiting for "o".
launch_on_start = false

[ui]
# Number of rows affected by the dark bottom overlay in the request tree.
bottom_fade_rows = 3

[redaction]
# Case-insensitive header names redacted from share/curl output.
header_names = ["authorization", "proxy-authorization", "cookie", "set-cookie", "x-api-key", "x-auth-token", "x-csrf-token", "x-xsrf-token"]
# Case-insensitive JSON key substrings redacted from share body previews.
json_key_patterns = ["authorization", "auth", "cookie", "email", "jwt", "key", "password", "secret", "session", "token"]
# Case-insensitive text prefixes redacted from plain-text share body previews.
text_patterns = ["bearer ", "token=", "password=", "secret="]
# Maximum captured body text returned by MCP body tools.
mcp_body_limit_bytes = 262144

[theme]
text = "#d4be98"
muted = "#928374"
accent = "#89b482"
panel_title = "#d8a657"
panel_border = "#3c3836"
active_border = "#89b482"
tree_edge = "#928374"
ok = "#a9b665"
redirect = "#7daea3"
client_error = "#d8a657"
server_error = "#ea6962"
method_get = "#7daea3"
method_post = "#a9b665"
method_write = "#d8a657"
method_delete = "#ea6962"
resource_xhr = "#d3869b"
resource_image = "#7daea3"
resource_script = "#d8a657"
resource_style = "#89b482"
resource_sse = "#a9b665"
"##;

pub(super) const LEGACY_NEON_DEFAULT_CONFIG: &str = r##"# Faro configuration.

[app]
# Default database path used when --db is omitted.
db_path = "faro.db"
# Start capture immediately for URL launches instead of waiting for "o".
launch_on_start = false

[ui]
# Number of rows affected by the dark bottom overlay in the request tree.
bottom_fade_rows = 3

[theme]
text = "#e5e5e5"
muted = "#969696"
accent = "#23d18b"
panel_title = "#29b8db"
panel_border = "#545454"
active_border = "#23d18b"
tree_edge = "#969696"
ok = "#23d18b"
redirect = "#11a8cd"
client_error = "#e5e510"
server_error = "#cd3131"
method_get = "#29b8db"
method_post = "#23d18b"
method_write = "#f5f543"
method_delete = "#f14c4c"
resource_xhr = "#d670d6"
resource_image = "#3b8eea"
resource_script = "#f5f543"
resource_style = "#29b8db"
resource_sse = "#23d18b"
"##;

pub(super) const LEGACY_GRUVBOX_DEFAULT_CONFIG: &str = r##"# Faro configuration.

[app]
# Default database path used when --db is omitted.
db_path = "faro.db"
# Start capture immediately for URL launches instead of waiting for "o".
launch_on_start = false

[ui]
# Number of rows affected by the dark bottom overlay in the request tree.
bottom_fade_rows = 3

[theme]
text = "#ebdbb2"
muted = "#928374"
accent = "#b8bb26"
panel_title = "#fabd2f"
panel_border = "#504945"
active_border = "#b8bb26"
tree_edge = "#928374"
ok = "#b8bb26"
redirect = "#83a598"
client_error = "#fabd2f"
server_error = "#fb4934"
method_get = "#83a598"
method_post = "#b8bb26"
method_write = "#fabd2f"
method_delete = "#fb4934"
resource_xhr = "#d3869b"
resource_image = "#83a598"
resource_script = "#fabd2f"
resource_style = "#8ec07c"
resource_sse = "#b8bb26"
"##;

pub(super) const LEGACY_TERMINAL_DEFAULT_CONFIG: &str = r##"# Faro configuration.

[app]
# Default database path used when --db is omitted.
db_path = "faro.db"
# Start capture immediately for URL launches instead of waiting for "o".
launch_on_start = false

[ui]
# Number of rows affected by the dark bottom overlay in the request tree.
bottom_fade_rows = 3

[theme]
text = "default"
muted = "dark_gray"
accent = "green"
panel_title = "yellow"
panel_border = "dark_gray"
active_border = "green"
tree_edge = "dark_gray"
ok = "green"
redirect = "blue"
client_error = "yellow"
server_error = "red"
method_get = "blue"
method_post = "green"
method_write = "yellow"
method_delete = "red"
resource_xhr = "magenta"
resource_image = "blue"
resource_script = "yellow"
resource_style = "cyan"
resource_sse = "green"
"##;

pub(super) const LEGACY_NEUTRAL_DEFAULT_CONFIG: &str = r##"# Faro configuration.

[app]
# Default database path used when --db is omitted.
db_path = "faro.db"
# Start capture immediately for URL launches instead of waiting for "o".
launch_on_start = false

[ui]
# Number of rows affected by the dark bottom overlay in the request tree.
bottom_fade_rows = 3

[theme]
text = "white"
muted = "gray"
accent = "yellow"
panel_title = "yellow"
panel_border = "gray"
active_border = "yellow"
tree_edge = "gray"
ok = "yellow"
redirect = "white"
client_error = "yellow"
server_error = "red"
method_get = "white"
method_post = "yellow"
method_write = "yellow"
method_delete = "red"
resource_xhr = "white"
resource_image = "white"
resource_script = "yellow"
resource_style = "gray"
resource_sse = "yellow"
"##;
