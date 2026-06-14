# mcp-fs

Rust-based MCP server for filesystem access.

## Build

```bash
cargo build
```

## Build the browser UI

```bash
cd frontend
npm install
npm run build
```

## Run

```bash
cargo run -- --root "/path/to/project"
```

## Startup options

- `--root <path>`: configure the default root (default: CWD)
- `--allow-root <path>`: allow access to additional roots (repeatable). Relative paths are resolved from the working directory
- `--allow-escape`: allow paths outside the configured roots
- `--config <path>`: JSON config file to override env/defaults
- `--print-config-schema`: print the configuration schema and exit
- `--find-limit <n>`: default limit for `find_files` (default: 200). Use `0` for unlimited
- `--search-max-bytes <n>`: maximum output size for `search_files` responses (default: 51200). Use `0` for unlimited
- `--search-summary-top <n>`: when summary output is still too large, return top N files by match count (default: 20). Use `0` for unlimited
- `--read-max-bytes <n>`: maximum total output bytes for `read_file` (default: 51200). Use `0` for unlimited
- `--read-max-line-bytes <n>`: maximum bytes per line for `read_file` before truncation (default: 25600). Use `0` for unlimited
- `--enable-binary-read-tools <bool>`: expose `read_file_binary` and `read_multiple_files_binary` in `tools/list` (default: false)
- `--review-on-apply <bool>`: generate review artifacts for non-preview edit/write calls (default: true)
- `--preview-uris-in-content <bool>`: include preview URIs in tool `content` instead of response `_meta` (default: false)
- `--otel-enabled <bool>`: enable OpenTelemetry tracing (default: true)
- `--otel-endpoint <url>`: OTLP endpoint (default: http://127.0.0.1:4317)
- `--otel-service-name <name>`: OTEL service.name (default: mcp-fs)

Environment variables:

- `MCP_ROOT`: same as `--root`
- `MCP_ALLOWED_ROOTS`: comma-separated list of additional allowed roots
- `MCP_ALLOW_ESCAPE`: `1|true|yes` to allow escape
- `MCP_CONFIG`: JSON config file to override env/defaults
- `MCP_FIND_LIMIT`: same as `--find-limit`
- `MCP_SEARCH_MAX_BYTES`: same as `--search-max-bytes`
- `MCP_SEARCH_SUMMARY_TOP`: same as `--search-summary-top`
- `MCP_READ_MAX_BYTES`: same as `--read-max-bytes`
- `MCP_READ_MAX_LINE_BYTES`: same as `--read-max-line-bytes`
- `MCP_ENABLE_BINARY_READ_TOOLS`: same as `--enable-binary-read-tools`
- `MCP_REVIEW_ON_APPLY`: same as `--review-on-apply`
- `MCP_PREVIEW_URIS_IN_CONTENT`: same as `--preview-uris-in-content`
- `MCP_OTEL_ENABLED`: same as `--otel-enabled`
- `MCP_OTEL_ENDPOINT`: same as `--otel-endpoint`
- `MCP_OTEL_SERVICE_NAME`: same as `--otel-service-name`

## Configuration

Configuration sources are merged in this order (see SEP-1596 for config schema behavior):

1. Environment/CLI defaults
2. `--config` / `MCP_CONFIG` JSON
3. `initialize` params via `capabilities.experimental.configuration`
4. Per-call `_meta.policy` restrictions

The configuration schema is available in the initialize response (`configSchema`) and via `--print-config-schema`.
Initialize capabilities include `capabilities.experimental.policy = true` to indicate policy support.

Custom schema metadata:

- `scope`: `configuration`, `policy`, or `any` (default). Marks whether a field is only valid at configuration time, only valid in `_meta.policy`, or valid in both.
- `audience`: `human`, `llm`, or `any` (default). Indicates whether a field is intended for a human operator, an LLM policy layer, or both.

Example initialize payload:

```json
{
	"method": "initialize",
	"params": {
		"capabilities": {
			"experimental": {
				"configuration": {
					"roots": [
						{
							"path": "/home/alex",
							"default": true,
							"deny": [
								"**/.git/**"
							],
							"allow": [
								"test/**"
							]
						}
					]
				}
			}
		}
	}
}
```

Root fields:

- `path`: root path (absolute or working-directory-relative)
- `default`: true for the default root used by relative paths (if multiple defaults are set, the first is used and the rest are cleared)
- `immutable`: glob patterns that disallow write/edit/move/delete (empty means all paths are mutable; use `"**"` to freeze everything)
- `deny`: glob patterns to exclude from all operations
- `allow`: glob patterns to include (anything else is denied)

## Tools

The server exposes tools via `tools/list` and `tools/call`:

- `find_files`: fd-compatible file finder
- `search_files`: ripgrep-based text search
- `read_file`: read file contents with start_line/limit (raw content)
- `read_file_binary`: read file bytes as base64 with offset/limit (disabled by default; enable with `--enable-binary-read-tools`)
- `read_multiple_files_binary`: read multiple files as base64 with per-file offset/limit and caps (disabled by default; enable with `--enable-binary-read-tools`)
- `read_multiple_files`: read multiple files with per-file start_line/limit and caps (raw content)
- `write_file`: write changes (mode: `overwrite|append|prepend`)
- `edit_file`: replace exact matches in a file
- `list_roots`: list configured roots and per-call read grants
- `move_file`: move/rename a file or directory (fails if destination exists)
- `delete_file`: delete files or directories recursively via `paths`

Use `tools/list` to get full input/output schemas.

Notes:

- Preview mode for `edit_file` is enabled via `_meta.preview` inside `tools/call` params.
- `tools/list` includes `annotations.preview: true` for tools that support preview.
- Tools accept `_meta.allowed_scopes` to allow per-call access to additional roots (string or array).
- Binary read tools are implemented but not exposed by default because most model clients cannot reliably consume opaque binary data (e.g. Excel). Enable them with `--enable-binary-read-tools` if needed. We may re-enable/expand for images/audio/video when model support improves.

### Runtime Policy

Calls can restrict access further with `_meta.policy`. Policy roots must match configured roots by `path`.
Policy can only restrict, never expand.
Policy cannot redefine roots or change the default root; tool results are relative to configured roots and must remain stable.
Use `allow`/`deny` globs (and `immutable`) to narrow access instead.
Policy may set `blocked: true` for a root, but the default root cannot be blocked.

Example:

```json
{
	"_meta": {
		"policy": {
			"roots": [
				{
					"path": "/home/alex",
					"blocked": false,
					"allow": [
						"test/**"
					],
					"deny": [
						"**/secrets/**"
					],
					"immutable": [
						"**/*.lock"
					]
				}
			]
		}
	}
}
```

When a policy is present:

- any root with `blocked: true` is excluded entirely
- allow/deny are applied to all operations (list/read/write/edit/move/delete/search/find)
- immutable applies only to write/edit/move/delete (read/search/find still work)
- granted scopes are ignored

## MCP Extensions

### Preview Resources

Some tools expose preview artifacts through MCP resources.

- `edit_file` and `write_file` preview via `_meta.preview: true`.
- When `review_on_apply` is true (default), `edit_file` and `write_file` also generate review artifacts on non-preview calls.
- `review_on_apply` can be overridden per call via `_meta.policy`.
- By default, preview results include `_meta.ui.resourceUri` pointing to the shared static review UI at `ui://review/index.html`.
- The unified textual diff is also returned in `structuredContent.diff`.
- The review UI resource is available via `resources/list` and `resources/read`.
- `ui://` resources can be rendered using `mcp-view`, which injects them into an iframe.

### Tool Annotations

Tools include an `annotations` object to help clients choose the right tool.

- `scopes`: `read:file` or `write:file`
- `priority`: `edit_file` is `1` and `write_file` is `0` (prefer edit_file)
- `group`: `filesystem`
- `intentTemplate`: optional, user-facing EBNF-like template describing the tool's action; supports optional segments `[...]` and dotted placeholders like `{paths.path}` (middleware can expand arrays).

### Requested/Granted Scopes

When a request is outside the configured root or allowed roots (and `allow_escape` is off), the error response includes:

- `_meta.requested_scopes`: array of scope strings, e.g. `read:file:/abs/root` or `write:file:/abs/root`

Clients can retry the call with user-approved scopes by adding:

- `_meta.allowed_scopes`: string or array of scope strings

Scopes are structured as `read:file` / `write:file` so clients can grant broader scopes (e.g. `write:*`) if desired.

### Default Root

Unless an absolute path is provided, tools resolve paths relative to the default root.
Use `list_roots` to discover the default root (marked with `default: true`).

Preview resources are exposed via:

- `resources/list`: lists available static UI resources
- `resources/read`: reads a resource by URI (e.g., `ui://review/index.html`)
