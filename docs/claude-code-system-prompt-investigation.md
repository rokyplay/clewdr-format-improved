# Claude Code API System Prompt 调查

## 背景

用户报告在使用 clewdr 的 `/code` 路径时，偶尔会收到错误：
```
This credential is only authorized for use with Claude Code
```

这个错误是 Claude API 返回的，说明 OAuth token 被用在了不正确的上下文中。

## 猜想

Claude Code 有官方的系统提示词，可能是固定的，不支持修改。如果尝试修改系统提示词，可能会触发这个错误。

## 调查方向

1. **Claude Code 官方系统提示词**
   - Claude Code CLI 使用的默认系统提示词是什么？
   - 是否可以通过 API 修改？

2. **System 字段处理**
   - Claude API 的 `system` 字段是作为数组还是字符串？
   - 是否可以追加内容到系统提示词？

3. **clewdr 的处理方式**
   - clewdr 如何处理 system 消息？
   - 是否有将 system 转换为 user 的逻辑？

## 测试计划

### 测试 1: 基础请求（无自定义 system）

```bash
curl -X POST https://api.anthropic.com/v1/messages \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "anthropic-beta: oauth-2025-04-20" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4-20250514",
    "max_tokens": 100,
    "messages": [{"role": "user", "content": "Hi"}]
  }'
```

### 测试 2: 带 system 字符串

```bash
curl -X POST https://api.anthropic.com/v1/messages \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "anthropic-beta: oauth-2025-04-20" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4-20250514",
    "max_tokens": 100,
    "system": "You are a helpful assistant.",
    "messages": [{"role": "user", "content": "Hi"}]
  }'
```

### 测试 3: 带 system 数组

```bash
curl -X POST https://api.anthropic.com/v1/messages \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "anthropic-beta: oauth-2025-04-20" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4-20250514",
    "max_tokens": 100,
    "system": [{"type": "text", "text": "You are a helpful assistant."}],
    "messages": [{"role": "user", "content": "Hi"}]
  }'
```

### 测试 4: 带 cache_control 的 system

```bash
curl -X POST https://api.anthropic.com/v1/messages \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "anthropic-beta: oauth-2025-04-20" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4-20250514",
    "max_tokens": 100,
    "system": [{"type": "text", "text": "You are a helpful assistant.", "cache_control": {"type": "ephemeral"}}],
    "messages": [{"role": "user", "content": "Hi"}]
  }'
```

## clewdr 当前处理逻辑

### Claude Code 路径 (`/code/v1/messages`)

在 `src/middleware/claude/request.rs` 的 `ClaudeCodePreprocess` 中：

1. 检测 User-Agent 是否来自 Claude Code CLI
2. 如果不是来自 Claude Code CLI，添加一个 prelude 到 system：
   ```rust
   const PRELUDE_TEXT: &str = "You are Claude Code, Anthropic's official CLI for Claude.";
   ```
3. 将 prelude 插入到 system 数组的开头

### 潜在问题

1. **system 格式不一致**
   - 如果客户端发送的是字符串格式的 system，clewdr 会转换为数组格式
   - 这可能导致格式不兼容

2. **cache_control 处理**
   - 客户端可能发送带有 cache_control 的 system
   - clewdr 可能没有正确处理这些字段

3. **OAuth token 限制**
   - Claude Code 的 OAuth token 可能只允许特定的 system prompt 格式
   - 修改 system prompt 可能触发 "credential only authorized for Claude Code" 错误

## 测试结果

（待填写）

## 结论

（待填写）

## 建议修复

（待填写）