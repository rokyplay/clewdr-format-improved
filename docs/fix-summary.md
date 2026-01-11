# clewdr 修复总结

## 修复的问题

### 1. ToolChoice 反序列化错误

**错误信息**: `tool_choice: invalid type: string "auto", expected internally tagged enum ToolChoice`

**原因**: `ToolChoice` 枚举只支持对象格式 `{ "type": "auto" }`，不支持字符串格式 `"auto"`

**修复**: 将 `ToolChoice` 改为 `#[serde(untagged)]` 枚举，支持两种格式：

```rust
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum ToolChoice {
    /// Simple string format: "auto", "any", "none"
    Simple(ToolChoiceSimple),
    /// Object format with type tag
    Object(ToolChoiceObject),
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ToolChoiceSimple {
    Auto,
    Any,
    None,
}
```

### 2. ContentBlock image_url 标签不被识别

**错误信息**: `Input tag 'image_url' found using 'type' does not match any of the expected tags`

**原因**: `ContentBlock` 枚举缺少多种内容块类型

**修复**: 添加了以下内容块类型：

- `Document` - 文档内容（PDF等）
- `SearchResult` - 搜索结果
- `ServerToolUse` - 服务器工具使用
- `WebSearchToolResult` - Web 搜索工具结果

同时添加了 `DocumentSource` 结构体：

```rust
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct DocumentSource {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}
```

### 3. 工具调用丢失

**问题**: `claude2oai.rs` 中的 `InputJsonDelta` 被丢弃，导致工具调用在 OpenAI 兼容端点丢失

**修复**: 重写了 `transform_stream` 函数，添加了工具调用状态累积：

```rust
// 状态：累积工具调用的 JSON
let tool_call_buffer: Arc<Mutex<HashMap<usize, ToolCallState>>> =
    Arc::new(Mutex::new(HashMap::new()));

// 处理 ContentBlockStart - 记录工具调用开始
// 处理 InputJsonDelta - 累积参数
// 处理 ContentBlockStop - 发送完整的工具调用
```

### 4. 非流式响应工具调用丢失

**问题**: `transforms_json` 函数过滤掉了 `ToolUse` 块

**修复**: 添加了工具调用处理：

```rust
ContentBlock::ToolUse { id, name, input, signature } => {
    // 存储签名
    if let Some(sig) = signature {
        store_thought_signature(sig);
    }
    
    // 应用参数重映射
    let mut remapped_input = input.clone();
    crate::format::remap_function_call_args(name, &mut remapped_input);
    
    tool_calls.push(json!({
        "id": id,
        "type": "function",
        "function": {
            "name": name,
            "arguments": serde_json::to_string(&remapped_input).unwrap_or_default()
        }
    }));
}
```

## 新增模块

### format 模块

位置: `clewdr/src/format/`

| 文件 | 功能 |
|------|------|
| `mod.rs` | 模块导出 |
| `signature_store.rs` | 全局 Thinking 签名存储 |
| `schema_cleaner.rs` | JSON Schema 清理（移除不支持的关键字） |
| `param_remapper.rs` | 参数重映射（Grep/Glob/Read） |
| `thinking_utils.rs` | Thinking 块工具函数 |

### 签名存储

```rust
static GLOBAL_THOUGHT_SIG: OnceLock<Mutex<Option<String>>> = OnceLock::new();

pub fn store_thought_signature(sig: &str);
pub fn get_thought_signature() -> Option<String>;
pub fn clear_thought_signature();
```

### Schema 清理

移除的不支持关键字：
- `additionalProperties`, `default`, `$schema`, `$defs`
- `definitions`, `$ref`, `$id`, `$comment`, `title`
- `minLength`, `maxLength`, `pattern`, `format`
- `minItems`, `maxItems`, `examples`, `allOf`, `anyOf`, `oneOf`

### 参数重映射

| 工具 | 原参数 | 目标参数 |
|------|--------|----------|
| Grep | query | pattern |
| Glob | query | pattern |
| Read | path | file_path |

## 测试

添加了以下测试用例：

1. `deserializes_tool_choice_string_format` - 测试字符串格式的 tool_choice
2. `deserializes_tool_choice_object_format` - 测试对象格式的 tool_choice
3. `deserializes_image_url_content_block` - 测试 image_url 内容块
4. `deserializes_document_content_block` - 测试 document 内容块
5. `deserializes_thinking_content_block` - 测试 thinking 内容块

## 修改的文件

| 文件 | 修改内容 |
|------|----------|
| `clewdr/src/types/claude.rs` | 添加 ToolChoice 字符串支持、新增内容块类型 |
| `clewdr/src/middleware/claude/claude2oai.rs` | 修复工具调用处理 |
| `clewdr/src/format/thinking_utils.rs` | 更新测试代码 |
| `clewdr/src/lib.rs` | 导出 format 模块 |

## 编译和测试

### 编译

clewdr 是一个 Rust 项目，需要使用 `cargo` 编译：

```bash
cd clewdr
cargo build --release
```

如果遇到编译错误，请确保：
1. 安装了 Rust 工具链 (`rustup`)
2. 所有依赖项都已安装

### 运行测试

```bash
cd clewdr
cargo test
```

### 测试特定模块

```bash
# 测试 format 模块
cargo test format::

# 测试 types 模块
cargo test types::

# 测试 schema_cleaner
cargo test schema_cleaner

# 测试 thinking_utils
cargo test thinking_utils
```

### 手动测试

1. **启动 clewdr 服务**：
   ```bash
   cargo run --release
   ```

2. **测试 ToolChoice 字符串格式**：
   ```bash
   curl -X POST http://localhost:8080/v1/messages \
     -H "Content-Type: application/json" \
     -d '{
       "model": "claude-sonnet-4-5",
       "max_tokens": 1024,
       "messages": [{"role": "user", "content": "Hello"}],
       "tool_choice": "auto"
     }'
   ```

3. **测试 image_url 内容块**：
   ```bash
   curl -X POST http://localhost:8080/v1/messages \
     -H "Content-Type: application/json" \
     -d '{
       "model": "claude-sonnet-4-5",
       "max_tokens": 1024,
       "messages": [{
         "role": "user",
         "content": [{
           "type": "image_url",
           "image_url": {"url": "https://example.com/image.png"}
         }]
       }]
     }'
   ```

## 与 Antigravity-Manager 的对比

| 功能 | clewdr (修复后) | Antigravity-Manager |
|------|-----------------|---------------------|
| ToolChoice 字符串支持 | ✅ | ✅ |
| image_url 内容块 | ✅ | ✅ |
| document 内容块 | ✅ | ✅ |
| thinking 内容块 | ✅ | ✅ |
| 签名存储 | ✅ OnceLock<Mutex> | ✅ OnceLock<Mutex> |
| Schema 清理 | ✅ $ref 展开 + 约束迁移 | ✅ $ref 展开 + 约束迁移 |
| 参数重映射 | ✅ Grep/Glob/Read | ✅ Grep/Glob/Read |
| cache_control 清理 | ✅ | ✅ |
| Safety Settings | N/A (Claude API) | ✅ (Gemini API) |
| Web Search 支持 | ✅ (Claude 内置) | ✅ (Google Search) |

### 架构差异说明

**clewdr** 是 Claude API 代理，直接转发请求到 Anthropic API：
- Web Search 通过 Claude 内置工具 `web_search_20250305` 支持
- 不需要 Safety Settings（Claude API 自带安全过滤）
- cache_control 清理用于防止客户端发回历史消息中的缓存控制字段

**Antigravity-Manager** 是 Claude → Gemini 转换代理：
- Web Search 需要转换为 Google Search 工具
- 需要配置 Gemini Safety Settings
- 需要处理 Gemini 特有的 grounding_metadata 响应

## 参考

- [Antigravity-Manager](../Antigravity-Manager/) - Rust 实现参考
- [antigravity-claude-proxy](../antigravity-claude-proxy/) - Node.js 实现参考
- [format-conversion-analysis.md](../../format-conversion-analysis.md) - 格式转换分析报告