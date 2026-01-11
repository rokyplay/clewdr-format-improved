# clewdr format 模块集成状态报告

## 日期: 2026-01-09

## GitHub 仓库

- **Fork 仓库**: https://github.com/rokyplay/clewdr-format-improved
- **Release**: v0.12.2-format-improved
- **下载**: https://github.com/rokyplay/clewdr-format-improved/releases/download/v0.12.2-format-improved/clewdr-linux-x64

### 编译说明

使用嵌入前端的编译方式（确保根路径 `/` 有管理界面）：
```bash
cd frontend && pnpm install && pnpm build
cargo build --release --no-default-features --features "portable,embed-resource"
```

## 当前问题状态 ✅ FIXED

### 核心问题
OAI `role: "tool"` 消息解析失败，错误信息：
```
unknown variant `tool`, expected one of `system`, `user`, `assistant`
```

### 根本原因 🔍

**导入错误！** `request.rs` 中导入了错误的类型：

```rust
// 错误的导入 (request.rs:29)
use crate::types::oai::CreateMessageParams as OaiCreateMessageParams;

// 正确的导入
use crate::types::oai::OaiCreateMessageParams;
```

**问题分析**：
- `oai.rs` 中有两个结构体：
  1. `CreateMessageParams` (363行) - 用 `Vec<Message>` (Claude 类型，**不支持 tool role**)
  2. `OaiCreateMessageParams` (439行) - 用 `Vec<OaiMessage>` (OAI 类型，**支持 tool role**)
- 原代码导入了 `CreateMessageParams` 并重命名为 `OaiCreateMessageParams`
- 这导致解析时使用了 Claude 的 `Message` 类型，其 `role` 字段是 `Role` 枚举（只有 system/user/assistant）

### 排查过程

1. **错误信息分析**：`unknown variant 'tool', expected one of 'system', 'user', 'assistant'`
   - 这说明解析器使用的是 Claude 的 `Role` 枚举，而不是 `OaiRole`

2. **检查 OaiRole 定义**：确认 `OaiRole` 已包含 `Tool` 变体 ✅

3. **检查 OaiMessage 定义**：确认使用 `pub role: OaiRole` ✅

4. **检查 OaiCreateMessageParams 定义**：确认使用 `pub messages: Vec<OaiMessage>` ✅

5. **检查 request.rs 导入**：发现问题！
   ```rust
   // 第 29 行
   oai::CreateMessageParams as OaiCreateMessageParams  // ← 错误！
   ```
   
6. **检查 oai.rs 中的结构体**：
   - `CreateMessageParams` (363行): `pub messages: Vec<Message>` ← Claude 类型
   - `OaiCreateMessageParams` (439行): `pub messages: Vec<OaiMessage>` ← OAI 类型

### 修复方案

修改 `src/middleware/claude/request.rs` 第 29 行：
```rust
// Before
oai::CreateMessageParams as OaiCreateMessageParams,

// After
oai::OaiCreateMessageParams,
```

### 已完成的修复

1. **OaiRole 枚举** - ✅ 已添加 `Tool` 变体
2. **OaiMessageContent 枚举** - ✅ 新增支持 String/Array/Null
3. **OaiMessage 结构体** - ✅ 更新使用新的 content 类型
4. **tool_choice 格式转换** - ✅ 已实现 `to_object_format()` 方法
5. **convert_oai_message 函数** - ✅ 已更新
6. **request.rs 导入修复** - ✅ 已修复
7. **OaiCreateMessageParams.tools 类型** - ✅ 改为 `Vec<OaiTool>`
8. **OaiCreateMessageParams.tool_choice 转换** - ✅ 添加 `.map(|tc| tc.to_object_format())`
9. **tool_result.content 格式** - ✅ 保持字符串格式，不解析为 JSON 对象
10. **web_search 工具转换** - ✅ 转换为 Claude 内置 `KnownTool::WebSearch20250305`

### 2026-01-08 测试结果

| 功能 | 状态 | 备注 |
|------|------|------|
| Write (写入文件) | ✅ 成功 | |
| Read (读取文件) | ✅ 成功 | |
| Glob (文件搜索) | ✅ 成功 | |
| Bash (执行命令) | ✅ 成功 | |
| 图片识别 | ✅ 成功 | |
| WebSearch (网络搜索) | ✅ 成功 | 2026-01-08 验证通过 |
| 根路径前端界面 | ✅ 成功 | 需使用 embed-resource 编译 |

### 调试文件
- **原始请求**: `versions/format-improved/log/debug_raw_request.json`
- **日志**: `versions/format-improved/log/clewdr.log.2026-01-07`

### 部署信息
- **服务路径**: `/root/clauder/versions/format-improved/`
- **端口**: 8484
- **密码**: `dyuY97Ym3uX2MnaFFN28WZvWWQNmU8ay8byU2aaQFZNfdhP3p4Y9gEGFzduqtxG7`
- **Screen 会话**: `clewdr`

---

## 一、模块概览

`src/format/` 模块包含以下子模块：

| 模块 | 功能 | 状态 |
|------|------|------|
| `signature_store.rs` | 思考模式签名存储 | ✅ 已集成 |
| `schema_cleaner.rs` | JSON Schema 清理 | ✅ 已集成 |
| `param_remapper.rs` | 参数名重映射 | ✅ 已集成 |
| `thinking_utils.rs` | Thinking 模式工具 | ✅ 已集成 |
| `web_search.rs` | Web 搜索结果格式化 | ✅ 已集成 |
| `image_converter.rs` | 图片格式转换 | ✅ 已集成 |

## 二、基础功能测试结果

| 测试项 | 结果 | 详情 |
|--------|------|------|
| Claude 原生格式认证 (x-api-key) | ✅ 通过 | |
| OpenAI 格式认证 (Bearer) | ✅ 通过 | |
| Claude 格式消息（无工具） | ✅ 通过 | 测试消息正确响应 |
| OpenAI 格式消息（无工具） | ✅ 通过 | 测试消息正确响应 |
| 图片 (OAI image_url data:URI) | ✅ 通过 | 成功识别 1x1 像素图片 |
| **OAI tool role 消息** | ✅ 通过 | 已修复导入和类型问题 |
| **工具调用 (Write/Read/Glob/Bash)** | ✅ 通过 | 2026-01-07 23:30 验证 |
| **WebSearch** | ✅ 通过 | 2026-01-08 验证 |

## 三、潜在风险分析

### Cookie 封号风险排查

如果使用修改版后出现 Cookie 被封情况，可能的原因：

1. **Claude Web 路径 (`/v1/`)**: `transform.rs` 中的 Thinking 内容会被转换为 `<thinking>` 标签发送
2. **IP/代理问题**: 与代码无关，需检查 IP 环境
3. **使用频率**: 高频请求可能触发风控

**排查建议**:
- 确认使用的是 Claude Code 路径 (`/code/v1/`) 还是 Claude Web 路径 (`/v1/`)
- Claude Code 路径直接调用 Anthropic API，不涉及 transform 逻辑
- 如使用 Claude Web 并怀疑 `<thinking>` 标签导致问题，可在 `transform.rs` 中移除相关代码

## 四、参考项目

本 fork 的格式转换思路参考：
- [antigravity-claude-proxy](https://github.com/badri-s/antigravity-claude-proxy) - 格式转换模式
- [claude-code-router](https://github.com/musistudio/claude-code-router) - Schema 清理和 Web Search 格式化

---

*报告更新时间: 2026-01-09*