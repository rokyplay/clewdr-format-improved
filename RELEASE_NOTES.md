# Release Notes v0.12.8

## Claude Opus 4.6 Compatibility
- Fix tool_use.id format validation (sanitize invalid characters like dots in OAI tool call IDs)
- Fix orphaned tool_result blocks by reconstructing proper ServerToolUse + WebSearchToolResult pairs for web search annotations
- Add tool_result/tool_use pairing validation to prevent API errors
- Fix Opus 4.6 assistant prefill not supported (auto-append user continue message)
- Fix Claude 4+ temperature and top_p conflict (remove top_p when both are set)

## 1M Context Improvements
- Simplified 1M context logic: only use 1M when explicitly requested with `-1M` model suffix

## Auto-Update
- Add periodic auto-update check every 30 minutes (portable builds)
- Add remote trigger update API endpoint (`POST /api/trigger-update`)

## Cookie Analytics
- Track request count per cookie (`request_count`, `first_request_at`, `last_request_at`)
- Track cookie lifecycle timestamps (`added_at`, `invalidated_at`)
- Preserve analytics data when cookie is invalidated

## Model List
- Update fallback static model list to include Claude Opus 4.5, Opus 4.6 and variants

## Other
- README download link now uses `/releases/latest/download/` for always-latest binary
