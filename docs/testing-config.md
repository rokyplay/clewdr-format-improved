# clewdr format-improved 测试配置

## 服务信息

- **运行位置**: `/root/clauder/versions/format-improved/`
- **端口**: 8484
- **密码**: `dyuY97Ym3uX2MnaFFN28WZvWWQNmU8ay8byU2aaQFZNfdhP3p4Y9gEGFzduqtxG7`
- **版本**: 0.12.2

## API 端点

### Claude Code 路径 (推荐测试)

- **消息**: `POST /code/v1/messages`
- **OpenAI 格式**: `POST /code/v1/chat/completions`
- **模型列表**: `GET /code/v1/models`

### Claude Web 路径

- **消息**: `POST /v1/messages`
- **OpenAI 格式**: `POST /v1/chat/completions`
- **模型列表**: `GET /v1/models`

## 认证方式

### Bearer Token (OpenAI 风格)
```bash
curl -H "Authorization: Bearer dyuY97Ym3uX2MnaFFN28WZvWWQNmU8ay8byU2aaQFZNfdhP3p4Y9gEGFzduqtxG7"
```

### x-api-key (Claude 风格)
```bash
curl -H "x-api-key: dyuY97Ym3uX2MnaFFN28WZvWWQNmU8ay8byU2aaQFZNfdhP3p4Y9gEGFzduqtxG7"
```

## 测试命令

### 1. 基础连通性测试
```bash
curl -s -H "Authorization: Bearer dyuY97Ym3uX2MnaFFN28WZvWWQNmU8ay8byU2aaQFZNfdhP3p4Y9gEGFzduqtxG7" \
  http://localhost:8484/code/v1/models
```

### 2. Claude 原生格式测试
```bash
curl -s -X POST http://localhost:8484/code/v1/messages \
  -H "Content-Type: application/json" \
  -H "x-api-key: dyuY97Ym3uX2MnaFFN28WZvWWQNmU8ay8byU2aaQFZNfdhP3p4Y9gEGFzduqtxG7" \
  -d '{
    "model": "claude-sonnet-4-20250514",
    "max_tokens": 100,
    "messages": [{"role": "user", "content": "Hi"}]
  }'
```

### 3. OpenAI 格式测试
```bash
curl -s -X POST http://localhost:8484/code/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer dyuY97Ym3uX2MnaFFN28WZvWWQNmU8ay8byU2aaQFZNfdhP3p4Y9gEGFzduqtxG7" \
  -d '{
    "model": "claude-sonnet-4-20250514",
    "max_tokens": 100,
    "messages": [{"role": "user", "content": "Hi"}]
  }'
```

### 4. 图片测试 (OpenAI image_url 格式)
```bash
curl -s -X POST http://localhost:8484/code/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer dyuY97Ym3uX2MnaFFN28WZvWWQNmU8ay8byU2aaQFZNfdhP3p4Y9gEGFzduqtxG7" \
  -d '{
    "model": "claude-sonnet-4-20250514",
    "max_tokens": 100,
    "messages": [{
      "role": "user",
      "content": [
        {"type": "text", "text": "What is in this image?"},
        {"type": "image_url", "image_url": {"url": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="}}
      ]
    }]
  }'
```

### 5. 工具调用测试
```bash
curl -s -X POST http://localhost:8484/code/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer dyuY97Ym3uX2MnaFFN28WZvWWQNmU8ay8byU2aaQFZNfdhP3p4Y9gEGFzduqtxG7" \
  -d '{
    "model": "claude-sonnet-4-20250514",
    "max_tokens": 200,
    "messages": [{"role": "user", "content": "What is the weather in Tokyo?"}],
    "tools": [{
      "type": "function",
      "function": {
        "name": "get_weather",
        "description": "Get current weather for a location",
        "parameters": {
          "type": "object",
          "properties": {
            "location": {"type": "string", "description": "City name"}
          },
          "required": ["location"]
        }
      }
    }]
  }'
```

## 日志查看

```bash
# 实时查看日志
tail -f ./versions/format-improved/log/clewdr.log*

# 查看最新日志
tail -50 ./versions/format-improved/log/clewdr.log*
```

## 服务管理

```bash
# 查看服务状态
screen -ls

# 停止服务
screen -S clewdr -X quit

# 启动服务
cd ./versions/format-improved && screen -dmS clewdr ./clewdr

# 进入服务控制台
screen -r clewdr
```

---

*创建时间: 2026-01-07 20:53*