# clewdr format 模块集成状态报告

## 日期: 2026-01-11

## GitHub 仓库

- **Fork 仓库**: https://github.com/rokyplay/clewdr-format-improved
- **Release**: v0.12.5-format-improved (待发布)
- **下载**: https://github.com/rokyplay/clewdr-format-improved/releases

### 编译说明

使用嵌入前端的编译方式（确保根路径 `/` 有管理界面）：
```bash
cd frontend && pnpm install && pnpm build
cargo build --release --no-default-features --features "portable,embed-resource"
```

---

## 开发环境配置

### 服务器角色

| 服务器 | 内存 | 用途 | 说明 |
|--------|------|------|------|
| **8G 服务器** | 8GB | 开发/编译 | 用于 Rust 编译和代码修改 |
| **2G 服务器** | 2GB | 部署/测试 | 运行 clewdr 收集日志 |
| **本地** | - | 开发环境 | VSCode + RooCode 测试 |

### 部署信息

- **服务路径**: `/root/clauder/versions/format-improved/`
- **端口**: 8484
- **密码**: `dyuY97Ym3uX2MnaFFN28WZvWWQNmU8ay8byU2aaQFZNfdhP3p4Y9gEGFzduqtxG7`
- **Screen 会话**: `clewdr`
- **外部访问**: `https://clewdr-gg1.204023.xyz`

### 开发流程

```
1. 在 8G 服务器修改代码
2. git push 到 GitHub
3. 在 2G 服务器 git pull 并运行
4. 收集日志分析问题
```

---

## 当前问题状态 ❌ 进行中

### 问题描述

1. **RooCode 400 错误**:
   ```
   This credential is only authorized for use with Claude Code and cannot be used for other API requests.
   ```

2. **官方 Claude Code CLI 401 错误**: 认证失败

### 请求链路分析

#### RooCode 链路
```
RooCode → NewAPI → https://clewdr-gg1.204023.xyz/code/v1/chat/completions
```

| 步骤 | 组件 | 路径 |
|------|------|------|
| 1 | RooCode | 发送 OpenAI 格式请求 |
| 2 | NewAPI | 基础URL: `https://clewdr-gg1.204023.xyz/code` |
| 3 | clewdr | 接收: `/code/v1/chat/completions` |

**clewdr 路由映射**:
- `/code/v1/chat/completions` → `api_claude_code` (ClaudeCodeProvider)
- 认证: `RequireBearerAuth`
- 格式转换: OAI → Claude

#### 官方 Claude Code CLI 链路
```
claude CLI → ANTHROPIC_BASE_URL → ???
```

**待确认**:
- 官方 CLI 实际请求的路径是什么？
- 是 `/v1/messages` 还是其他路径？

### 错误来源分析

**关键发现**: 错误信息 `This credential is only authorized for use with Claude Code` **不是 clewdr 返回的**！

- clewdr 的认证错误是: `Key/Password Invalid`
- 这个错误来自 **Anthropic 官方 API**

**结论**: 请求透传到了 Anthropic，但 Anthropic 检测到请求不符合 Claude Code 规范。

### 可能原因

1. **System Prompt 缺失或不正确**: Anthropic 检测 system prompt 内容
2. **Headers 缺失**: 缺少必要的 Claude Code 标识头
3. **OAuth Token 问题**: Token 交换过程出错

---

## v0.12.5 修复内容

### 1. 添加详细日志功能

**修改文件**:
- `src/middleware/claude/request.rs`
- `src/claude_code_state/chat.rs`

**日志文件位置** (`log/` 目录):
| 文件 | 内容 |
|------|------|
| `claude_code_incoming_request.json` | 客户端发来的原始请求 |
| `claude_code_processed_request.json` | 注入 system prompt 后的请求 |
| `claude_code_outgoing_request.json` | 发送给 Anthropic 的最终请求 |

**日志标签**:
- `[CLAUDE_CODE_PREPROCESS]` - 请求预处理阶段
- `[CLAUDE_CODE]` - 发送请求阶段

### 2. System Prompt 注入逻辑

检测逻辑:
```rust
// 检查 system prompt 是否已包含 "Claude Code"
let has_claude_code_system = match &body.system {
    Some(Value::String(s)) => s.contains("Claude Code"),
    Some(Value::Array(arr)) => arr.iter().any(|v| {
        v.get("text")
            .and_then(|t| t.as_str())
            .map(|s| s.contains("Claude Code"))
            .unwrap_or(false)
    }),
    _ => false,
};
```

注入内容:
```
You are an agent for Claude Code, Anthropic's official CLI for Claude. Given the user's message, you should use the tools available to complete the task. Do what has been asked; nothing more, nothing less. When you complete the task simply respond with a detailed writeup.
```

---

## 路由配置参考

| 路径 | Handler | Provider | 认证 | 用途 |
|------|---------|----------|------|------|
| `/v1/messages` | `api_claude_web` | ClaudeWebProvider | X-API-Key | Claude Web (Cookie) |
| `/code/v1/messages` | `api_claude_code` | ClaudeCodeProvider | X-API-Key | Claude Code (OAuth) |
| `/v1/chat/completions` | `api_claude_web` | ClaudeWebProvider | Bearer | OpenAI 兼容 Web |
| `/code/v1/chat/completions` | `api_claude_code` | ClaudeCodeProvider | Bearer | OpenAI 兼容 Code |

---

## 调试步骤

### 1. 部署新版本
```bash
# 在 2G 服务器
cd /root/clauder/versions/format-improved
git pull
# 重新编译或下载新的二进制
./clewdr
```

### 2. 发送测试请求
```bash
# 从 RooCode 或 Claude Code CLI 发送请求
```

### 3. 查看日志
```bash
# 查看实时日志
tail -f log/clewdr.log.$(date +%Y-%m-%d)

# 查看请求内容
cat log/claude_code_incoming_request.json
cat log/claude_code_processed_request.json
cat log/claude_code_outgoing_request.json
```

### 4. 分析日志

检查点:
- [ ] User-Agent 是什么
- [ ] 原始 system prompt 内容
- [ ] 是否成功注入 Claude Code prelude
- [ ] 发送给 Anthropic 的最终请求格式

---

## 历史问题 (已修复)

### OAI tool role 问题 ✅

**问题**: `unknown variant 'tool', expected one of 'system', 'user', 'assistant'`

**原因**: 导入了错误的类型 `CreateMessageParams as OaiCreateMessageParams`

**修复**: 改为正确导入 `oai::OaiCreateMessageParams`

### 功能测试结果 (2026-01-08)

| 功能 | 状态 |
|------|------|
| Write (写入文件) | ✅ |
| Read (读取文件) | ✅ |
| Glob (文件搜索) | ✅ |
| Bash (执行命令) | ✅ |
| 图片识别 | ✅ |
| WebSearch (网络搜索) | ✅ |
| 根路径前端界面 | ✅ |

---

## 模块概览

`src/format/` 模块:

| 模块 | 功能 | 状态 |
|------|------|------|
| `signature_store.rs` | 思考模式签名存储 | ✅ |
| `schema_cleaner.rs` | JSON Schema 清理 | ✅ |
| `param_remapper.rs` | 参数名重映射 | ✅ |
| `thinking_utils.rs` | Thinking 模式工具 | ✅ |
| `web_search.rs` | Web 搜索结果格式化 | ✅ |
| `image_converter.rs` | 图片格式转换 | ✅ |

---

## Release 历史

| 版本 | 日期 | 主要修复 |
|------|------|----------|
| v0.12.2 | 2026-01-09 | OAI tool role 支持、format 模块集成 |
| v0.12.3 | 2026-01-11 | Claude Code system prompt 检测逻辑改进 |
| v0.12.4 | 2026-01-11 | 认证中间件同时支持 Bearer Token 和 X-API-Key |
| v0.12.5 | 2026-01-11 | 添加完整请求日志功能 |

---

## 相关文件

- **认证中间件**: `src/middleware/auth.rs`
- **请求处理**: `src/middleware/claude/request.rs`
- **Claude Code 聊天**: `src/claude_code_state/chat.rs`
- **路由配置**: `src/router.rs`

---

## 参考项目

- [antigravity-claude-proxy](https://github.com/badri-s/antigravity-claude-proxy) - 格式转换模式
- [claude-code-router](https://github.com/musistudio/claude-code-router) - Schema 清理和 Web Search 格式化

---

*更新时间: 2026-01-11 14:30*
