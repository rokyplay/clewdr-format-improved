# clewdr 重写计划

## 基于 Antigravity-Manager 的功能增强

本文档详细描述了如何将 Antigravity-Manager 的完善功能整合到 clewdr 项目中。

---

## 一、当前 clewdr 架构分析

### 1.1 项目结构

```
clewdr/src/
├── api/                    # API 端点处理
│   ├── claude_code.rs      # Claude Code API
│   ├── claude_web.rs       # Claude Web API
│   └── ...
├── claude_code_state/      # Claude Code 状态管理
│   ├── chat.rs             # 聊天处理（直接透传）
│   ├── exchange.rs         # OAuth2 认证
│   └── organization.rs     # 组织管理
├── claude_web_state/       # Claude Web 状态管理
│   ├── chat.rs             # 聊天处理
│   └── transform.rs        # 请求转换
├── middleware/claude/      # Claude 中间件
│   ├── claude2oai.rs       # Claude → OpenAI 转换（有缺陷）
│   ├── request.rs          # 请求预处理
│   └── response.rs         # 响应后处理
├── types/                  # 类型定义
│   ├── claude.rs           # Claude API 类型
│   └── oai.rs              # OpenAI API 类型
└── ...
```

### 1.2 当前问题

| 问题 | 位置 | 严重程度 |
|------|------|----------|
| **工具调用丢失** | `claude2oai.rs:94` - `InputJsonDelta` 被丢弃 | ❌ 严重 |
| **非流式工具调用丢失** | `claude2oai.rs:103-106` - `ToolUse` 被过滤 | ❌ 严重 |
| **无签名管理** | 缺少全局签名存储 | ❌ 严重 |
| **无 Schema 清理** | 缺少 JSON Schema 清理逻辑 | ⚠️ 中等 |
| **无参数重映射** | 缺少 Grep/Glob/Read 参数重映射 | ⚠️ 中等 |
| **图片支持有限** | 仅支持 base64，不支持 URL | ⚠️ 中等 |
| **无 Thinking 恢复** | 缺少工具循环中的 thinking 恢复 | ⚠️ 中等 |

---

## 二、重写计划

### 2.1 新增模块结构

```
clewdr/src/
├── format/                         # 新增：格式转换模块
│   ├── mod.rs                      # 模块导出
│   ├── signature_store.rs          # 全局签名存储
│   ├── schema_cleaner.rs           # JSON Schema 清理
│   ├── param_remapper.rs           # 参数重映射
│   ├── thinking_utils.rs           # Thinking 工具函数
│   └── content_converter.rs        # 内容块转换
├── middleware/claude/
│   ├── claude2oai.rs               # 重写：完整的 Claude → OpenAI 转换
│   └── ...
└── ...
```

### 2.2 实现优先级

| 优先级 | 任务 | 预计工作量 |
|--------|------|------------|
| P0 | 签名管理模块 | 1 小时 |
| P0 | 修复工具调用支持 | 2 小时 |
| P1 | JSON Schema 清理 | 2 小时 |
| P1 | 参数重映射 | 1 小时 |
| P2 | Thinking 恢复机制 | 3 小时 |
| P2 | 完善图片支持 | 1 小时 |
| P3 | 测试和验证 | 2 小时 |

---

## 三、详细实现方案

### 3.1 签名管理模块

**文件**: `clewdr/src/format/signature_store.rs`

参考 Antigravity-Manager 的实现：

```rust
use std::sync::{Mutex, OnceLock};

static GLOBAL_THOUGHT_SIG: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn get_thought_sig_storage() -> &'static Mutex<Option<String>> {
    GLOBAL_THOUGHT_SIG.get_or_init(|| Mutex::new(None))
}

/// 存储签名（只存储更长的签名）
pub fn store_thought_signature(sig: &str) {
    if let Ok(mut guard) = get_thought_sig_storage().lock() {
        let should_store = match &*guard {
            None => true,
            Some(existing) => sig.len() > existing.len(),
        };
        if should_store {
            tracing::debug!(
                "[ThoughtSig] Storing new signature (length: {})",
                sig.len()
            );
            *guard = Some(sig.to_string());
        }
    }
}

/// 获取存储的签名
pub fn get_thought_signature() -> Option<String> {
    get_thought_sig_storage().lock().ok()?.clone()
}

/// 清除签名
pub fn clear_thought_signature() {
    if let Ok(mut guard) = get_thought_sig_storage().lock() {
        *guard = None;
    }
}
```

### 3.2 修复工具调用支持

**文件**: `clewdr/src/middleware/claude/claude2oai.rs`

#### 3.2.1 流式响应修复

```rust
pub fn transform_stream<I, E>(s: I) -> impl Stream<Item = Result<Event, E>>
where
    I: Stream<Item = Result<eventsource_stream::Event, E>>,
{
    // 状态：累积工具调用的 JSON
    let tool_call_buffer: Arc<Mutex<HashMap<usize, ToolCallState>>> = 
        Arc::new(Mutex::new(HashMap::new()));
    
    s.try_filter_map(async move |eventsource_stream::Event { data, .. }| {
        let Ok(parsed) = serde_json::from_str::<StreamEvent>(&data) else {
            return Ok(None);
        };
        
        match parsed {
            StreamEvent::ContentBlockStart { index, content_block } => {
                // 处理工具调用开始
                if let ContentBlock::ToolUse { id, name, .. } = content_block {
                    let mut buffer = tool_call_buffer.lock().unwrap();
                    buffer.insert(index, ToolCallState {
                        id,
                        name,
                        arguments: String::new(),
                    });
                }
                Ok(None)
            }
            StreamEvent::ContentBlockDelta { index, delta } => {
                match delta {
                    ContentBlockDelta::TextDelta { text } => {
                        Ok(Some(build_event(EventContent::Content { content: text })))
                    }
                    ContentBlockDelta::ThinkingDelta { thinking } => {
                        Ok(Some(build_event(EventContent::Reasoning {
                            reasoning_content: thinking,
                        })))
                    }
                    ContentBlockDelta::InputJsonDelta { partial_json } => {
                        // 累积工具调用参数
                        let mut buffer = tool_call_buffer.lock().unwrap();
                        if let Some(state) = buffer.get_mut(&index) {
                            state.arguments.push_str(&partial_json);
                        }
                        Ok(None)
                    }
                    ContentBlockDelta::SignatureDelta { signature } => {
                        // 存储签名到全局存储
                        crate::format::signature_store::store_thought_signature(&signature);
                        Ok(None)
                    }
                }
            }
            StreamEvent::ContentBlockStop { index } => {
                // 工具调用完成，发送完整的 tool_call
                let mut buffer = tool_call_buffer.lock().unwrap();
                if let Some(state) = buffer.remove(&index) {
                    return Ok(Some(build_tool_call_event(state)));
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    })
}

struct ToolCallState {
    id: String,
    name: String,
    arguments: String,
}

fn build_tool_call_event(state: ToolCallState) -> Event {
    let data = serde_json::json!({
        "choices": [{
            "delta": {
                "tool_calls": [{
                    "index": 0,
                    "id": state.id,
                    "type": "function",
                    "function": {
                        "name": state.name,
                        "arguments": state.arguments
                    }
                }]
            }
        }]
    });
    Event::default().json_data(data).unwrap()
}
```

#### 3.2.2 非流式响应修复

```rust
pub fn transforms_json(input: CreateMessageResponse) -> Value {
    let mut content_parts = Vec::new();
    let mut tool_calls = Vec::new();

    for block in input.content.iter() {
        match block {
            ContentBlock::Text { text } => {
                content_parts.push(text.clone());
            }
            ContentBlock::ToolUse { id, name, input } => {
                tool_calls.push(json!({
                    "id": id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": serde_json::to_string(input).unwrap_or_default()
                    }
                }));
            }
            ContentBlock::Thinking { thinking, signature } => {
                // 存储签名
                if let Some(sig) = signature {
                    crate::format::signature_store::store_thought_signature(sig);
                }
                // 可选：将 thinking 添加到内容中
            }
            _ => {}
        }
    }

    let content = content_parts.join("");
    
    let usage = input.usage.as_ref().map(|u| {
        json!({
            "prompt_tokens": u.input_tokens,
            "completion_tokens": u.output_tokens,
            "total_tokens": u.input_tokens + u.output_tokens
        })
    });

    let finish_reason = match input.stop_reason {
        Some(StopReason::EndTurn) => "stop",
        Some(StopReason::MaxTokens) => "length",
        Some(StopReason::StopSequence) => "stop",
        Some(StopReason::ToolUse) => "tool_calls",
        Some(StopReason::Refusal) => "content_filter",
        None => "stop",
    };

    let mut message = json!({
        "role": "assistant",
        "content": if content.is_empty() { Value::Null } else { json!(content) }
    });

    if !tool_calls.is_empty() {
        message["tool_calls"] = json!(tool_calls);
    }

    json!({
        "id": input.id,
        "object": "chat.completion",
        "created": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        "model": input.model,
        "choices": [{
            "index": 0,
            "message": message,
            "finish_reason": finish_reason
        }],
        "usage": usage
    })
}
```

### 3.3 JSON Schema 清理

**文件**: `clewdr/src/format/schema_cleaner.rs`

```rust
use serde_json::Value;

/// 不支持的 JSON Schema 关键字
const UNSUPPORTED_KEYWORDS: &[&str] = &[
    "additionalProperties", "default", "$schema", "$defs",
    "definitions", "$ref", "$id", "$comment", "title",
    "minLength", "maxLength", "pattern", "format",
    "minItems", "maxItems", "examples", "allOf", "anyOf", "oneOf"
];

/// 清理 JSON Schema 以兼容 Gemini API
pub fn clean_json_schema(schema: &mut Value) {
    if !schema.is_object() {
        return;
    }

    let obj = schema.as_object_mut().unwrap();

    // 移除不支持的关键字
    for keyword in UNSUPPORTED_KEYWORDS {
        obj.remove(*keyword);
    }

    // 处理类型数组 ["string", "null"] -> "string"
    if let Some(type_val) = obj.get_mut("type") {
        if let Some(arr) = type_val.as_array() {
            let non_null: Vec<_> = arr.iter()
                .filter(|v| v.as_str() != Some("null"))
                .cloned()
                .collect();
            if let Some(first) = non_null.first() {
                *type_val = first.clone();
            }
        }
    }

    // 递归处理 properties
    if let Some(props) = obj.get_mut("properties") {
        if let Some(props_obj) = props.as_object_mut() {
            for (_, prop_schema) in props_obj.iter_mut() {
                clean_json_schema(prop_schema);
            }
        }
    }

    // 递归处理 items
    if let Some(items) = obj.get_mut("items") {
        clean_json_schema(items);
    }
}

/// 为空 schema 生成占位符
pub fn ensure_valid_schema(schema: &mut Value) {
    if !schema.is_object() {
        *schema = serde_json::json!({
            "type": "object",
            "properties": {
                "reason": {
                    "type": "string",
                    "description": "Reason for calling this tool"
                }
            },
            "required": ["reason"]
        });
        return;
    }

    let obj = schema.as_object_mut().unwrap();
    
    // 确保有 type
    if !obj.contains_key("type") {
        obj.insert("type".to_string(), json!("object"));
    }

    // 如果是 object 类型但没有 properties，添加占位符
    if obj.get("type").and_then(|v| v.as_str()) == Some("object") {
        if !obj.contains_key("properties") || 
           obj.get("properties").and_then(|v| v.as_object()).map(|o| o.is_empty()).unwrap_or(true) {
            obj.insert("properties".to_string(), json!({
                "reason": {
                    "type": "string",
                    "description": "Reason for calling this tool"
                }
            }));
            obj.insert("required".to_string(), json!(["reason"]));
        }
    }
}
```

### 3.4 参数重映射

**文件**: `clewdr/src/format/param_remapper.rs`

```rust
use serde_json::Value;

/// 重映射工具调用参数
/// Gemini 有时使用与 Claude Code 期望不同的参数名
pub fn remap_function_call_args(tool_name: &str, args: &mut Value) {
    let Some(obj) = args.as_object_mut() else {
        return;
    };

    match tool_name {
        "Grep" => {
            // Gemini 使用 "query", Claude Code 期望 "pattern"
            if let Some(query) = obj.remove("query") {
                if !obj.contains_key("pattern") {
                    obj.insert("pattern".to_string(), query);
                    tracing::debug!("[ParamRemap] Grep: query → pattern");
                }
            }
        }
        "Glob" => {
            if let Some(query) = obj.remove("query") {
                if !obj.contains_key("pattern") {
                    obj.insert("pattern".to_string(), query);
                    tracing::debug!("[ParamRemap] Glob: query → pattern");
                }
            }
        }
        "Read" => {
            // Gemini 可能使用 "path" vs "file_path"
            if let Some(path) = obj.remove("path") {
                if !obj.contains_key("file_path") {
                    obj.insert("file_path".to_string(), path);
                    tracing::debug!("[ParamRemap] Read: path → file_path");
                }
            }
        }
        _ => {}
    }
}
```

### 3.5 Thinking 工具函数

**文件**: `clewdr/src/format/thinking_utils.rs`

```rust
use crate::types::claude::{ContentBlock, Message, MessageContent};

/// 最小签名长度
pub const MIN_SIGNATURE_LENGTH: usize = 10;

/// 检查消息是否有有效的 thinking 块
pub fn message_has_valid_thinking(message: &Message) -> bool {
    match &message.content {
        MessageContent::Blocks { content } => {
            content.iter().any(|block| {
                if let ContentBlock::Thinking { signature, .. } = block {
                    signature.as_ref()
                        .map(|s| s.len() >= MIN_SIGNATURE_LENGTH)
                        .unwrap_or(false)
                } else {
                    false
                }
            })
        }
        _ => false,
    }
}

/// 检查消息是否有工具调用
pub fn message_has_tool_use(message: &Message) -> bool {
    match &message.content {
        MessageContent::Blocks { content } => {
            content.iter().any(|block| matches!(block, ContentBlock::ToolUse { .. }))
        }
        _ => false,
    }
}

/// 检查是否应该因为历史消息禁用 thinking
pub fn should_disable_thinking_due_to_history(messages: &[Message]) -> bool {
    // 逆序查找最后一条 Assistant 消息
    for msg in messages.iter().rev() {
        if msg.role == crate::types::claude::Role::Assistant {
            let has_tool_use = message_has_tool_use(msg);
            let has_thinking = message_has_valid_thinking(msg);
            
            // 如果有工具调用，但没有 Thinking 块 -> 不兼容
            if has_tool_use && !has_thinking {
                tracing::info!("[Thinking] Detected ToolUse without Thinking in history");
                return true;
            }
            return false;
        }
    }
    false
}

/// 检查是否有有效的签名用于函数调用
pub fn has_valid_signature_for_function_calls(
    messages: &[Message],
    global_sig: &Option<String>,
) -> bool {
    // 1. 检查全局存储
    if let Some(sig) = global_sig {
        if sig.len() >= MIN_SIGNATURE_LENGTH {
            return true;
        }
    }

    // 2. 检查消息历史中的 thinking 块
    for msg in messages.iter().rev() {
        if msg.role == crate::types::claude::Role::Assistant {
            if message_has_valid_thinking(msg) {
                return true;
            }
        }
    }
    false
}
```

### 3.6 扩展类型定义

**文件**: `clewdr/src/types/claude.rs` (修改)

需要添加 `Thinking` 内容块和 `signature` 字段：

```rust
/// Content block in a message
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(tag = "type")]
pub enum ContentBlock {
    /// Text content
    #[serde(rename = "text")]
    Text { text: String },
    /// Image content
    #[serde(rename = "image")]
    Image { source: ImageSource },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrl },
    /// Tool use content
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        signature: Option<String>,  // 新增
    },
    /// Tool result content
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: serde_json::Value,
    },
    /// Thinking content (新增)
    #[serde(rename = "thinking")]
    Thinking {
        thinking: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
    /// Redacted thinking (新增)
    #[serde(rename = "redacted_thinking")]
    RedactedThinking {
        data: String,
    },
}
```

---

## 四、测试计划

### 4.1 单元测试

1. **签名存储测试**
   - 存储和获取签名
   - 只存储更长的签名
   - 清除签名

2. **Schema 清理测试**
   - 移除不支持的关键字
   - 处理类型数组
   - 生成占位符 schema

3. **参数重映射测试**
   - Grep: query → pattern
   - Glob: query → pattern
   - Read: path → file_path

### 4.2 集成测试

1. **工具调用流程**
   - 流式响应中的工具调用
   - 非流式响应中的工具调用
   - 多工具调用

2. **Thinking 模式**
   - 带签名的 thinking 块
   - 工具循环中的 thinking
   - 跨请求签名恢复

### 4.3 端到端测试

1. **Claude Code 兼容性**
   - 使用 Claude Code CLI 测试
   - 验证工具调用正常工作
   - 验证 thinking 模式正常工作

2. **OpenAI 兼容端点**
   - 使用 OpenAI SDK 测试
   - 验证工具调用格式正确
   - 验证流式响应正确

---

## 五、实施时间表

| 阶段 | 任务 | 时间 |
|------|------|------|
| 第 1 阶段 | 创建 format 模块结构 | Day 1 |
| 第 2 阶段 | 实现签名存储 | Day 1 |
| 第 3 阶段 | 修复工具调用支持 | Day 1-2 |
| 第 4 阶段 | 实现 Schema 清理 | Day 2 |
| 第 5 阶段 | 实现参数重映射 | Day 2 |
| 第 6 阶段 | 实现 Thinking 工具 | Day 3 |
| 第 7 阶段 | 扩展类型定义 | Day 3 |
| 第 8 阶段 | 测试和验证 | Day 4 |

---

## 六、风险和缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 破坏现有功能 | 高 | 增量修改，保持向后兼容 |
| 签名过期 | 中 | 实现签名刷新机制 |
| 性能下降 | 低 | 使用高效的数据结构 |
| 类型不匹配 | 中 | 完善类型定义，添加测试 |

---

## 七、参考资料

- [Antigravity-Manager 源码](../Antigravity-Manager/src-tauri/src/proxy/mappers/claude/)
- [antigravity-claude-proxy 源码](../antigravity-claude-proxy/src/format/)
- [Claude API 文档](https://docs.anthropic.com/claude/reference/messages)
- [OpenAI API 文档](https://platform.openai.com/docs/api-reference/chat)