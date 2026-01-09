# ClewdR

<p align="center">
  <img src="./assets/clewdr-logo.svg" alt="ClewdR" height="60">
</p>

ClewdR is a Rust proxy for Claude (Claude.ai, Claude Code).  
It keeps resource usage low, serves OpenAI-style endpoints, and ships with a small React admin UI for managing cookies and settings.

---

## Highlights

- Works with Claude web and Claude Code.
- Single static binary for Linux, macOS, Windows, and Android; Docker image available.
- Web dashboard shows live status and supports hot config reloads.
- Drops into existing OpenAI-compatible clients while keeping native Claude formats.
- Typical production footprint: `<10 MB` RAM, `<1 s` startup, `~15 MB` binary.

## Supported Endpoints

| Service | Endpoint |
|---------|----------|
| Claude.ai | `http://127.0.0.1:8484/v1/messages` |
| Claude.ai OpenAI compatible | `http://127.0.0.1:8484/v1/chat/completions` |
| Claude Code | `http://127.0.0.1:8484/code/v1/messages` |
| Claude Code OpenAI compatible | `http://127.0.0.1:8484/code/v1/chat/completions` |

Streaming responses work on every endpoint.

## Quick Start

1. Download the latest release for your platform from GitHub.
   Linux/macOS example (this fork):
   ```bash
   curl -L -o clewdr https://github.com/rokyplay/clewdr-format-improved/releases/download/v0.12.2-format-improved/clewdr-linux-x64
   chmod +x clewdr
   ```
2. Run the binary:
   ```bash
   ./clewdr
   ```
3. Open `http://127.0.0.1:8484` and enter the admin password shown in the console (or container logs if using Docker).

## Using the Web Admin

- `Dashboard` shows health, connected clients, and rate-limit status.
- `Claude` tab stores browser cookies; paste `cookie: value` pairs and save.
- `Settings` lets you rotate the admin password, set upstream proxies, and reload config without restarting.

If you forget the password, delete `clewdr.toml` and start the binary again. Docker users can mount a persistent folder for that file.

## Configure Upstreams

### Claude

1. Export your Claude.ai cookies (e.g., via browser devtools).  
2. Paste them into the Claude tab; ClewdR tracks their status automatically.  
3. Optionally set an outbound proxy or fingerprint overrides if Claude blocks your region.

## Client Examples

SillyTavern:

```json
{
  "api_url": "http://127.0.0.1:8484/v1/chat/completions",
  "api_key": "password-from-console",
  "model": "claude-3-sonnet-20240229"
}
```

Continue (VS Code):

```json
{
  "models": [
    {
      "title": "Claude via ClewdR",
      "provider": "openai",
      "model": "claude-3-sonnet-20240229",
      "apiBase": "http://127.0.0.1:8484/v1/",
      "apiKey": "password-from-console"
    }
  ]
}
```

Cursor:

```json
{
  "openaiApiBase": "http://127.0.0.1:8484/v1/",
  "openaiApiKey": "password-from-console"
}
```

## Resources

- Wiki: <https://github.com/Xerxes-2/clewdr/wiki>

## Fork Enhancements

This fork adds enhanced format conversion for better Claude Code and OpenAI compatibility:

- **OAI tool role support** - Full support for OpenAI's `role: "tool"` messages
- **Image format conversion** - Automatic conversion between data URI and Claude native formats
- **Web Search support** - Claude web search results converted to OpenAI annotations format
- **Schema cleaning** - JSON Schema sanitization for cross-provider compatibility
- **Thinking mode utilities** - Signature storage and conversation state analysis
- **Parameter remapping** - Automatic tool parameter name conversion (e.g., `query` → `pattern`)

All features tested with Claude Code (Write/Read/Glob/Bash/WebSearch).

## Thanks

- [wreq](https://github.com/0x676e67/wreq) for the fingerprinting library.
- [Clewd](https://github.com/teralomaniac/clewd) for many upstream ideas.
- [Clove](https://github.com/mirrorange/clove) for Claude Code helpers.
- [antigravity-claude-proxy](https://github.com/badri-s/antigravity-claude-proxy) for format conversion patterns.
- [claude-code-router](https://github.com/musistudio/claude-code-router) for schema cleaning and web search formatting.
