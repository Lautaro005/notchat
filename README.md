<div align="center">

<img src="docs/assets/icon.png" width="112" height="112" alt="Notchat icon">

# Notchat

**WhatsApp on your Mac, with a private AI that lives in the notch.**

Light, fast and local. Written in Rust.

[![CI](https://github.com/Lautaro005/notchat/actions/workflows/ci.yml/badge.svg)](https://github.com/Lautaro005/notchat/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Lautaro005/notchat?label=release)](https://github.com/Lautaro005/notchat/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-black)](LICENSE)
![macOS 13+](https://img.shields.io/badge/macOS-13%2B-black)

[Install](#install) · [Features](#features) · [Privacy](#privacy-and-security) · [Español](README.es.md)

</div>

---

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/Lautaro005/notchat/main/install.sh | sh
```

The script downloads the latest release, checks it against the published SHA-256, and puts `Notchat.app` in `/Applications`. Prefer to build it yourself?

```bash
curl -fsSL https://raw.githubusercontent.com/Lautaro005/notchat/main/install.sh | sh -s -- --source
```

Then open Notchat, go to **WhatsApp → Settings → Linked devices → Link a device** on your phone and scan the code. The QR code is generated inside the app; nothing goes through a third party.

> [!NOTE]
> Releases are signed ad-hoc for now (not yet notarized by Apple). The installer verifies the checksum instead; the release pipeline is ready for Developer ID signing.

## Features

|  |  |
|---|---|
| **Notch replies** | New messages drop out of the notch. Hit *Reply*, type, done — without leaving what you are doing. Prefer system notifications? Switch in Settings; both support inline replies. |
| **Notchat AI** | Your own assistant chat. It reads your chats, searches the web, reads files you attach and sends messages for you: *"tell Ana I'm running late"*. You confirm before anything is sent. |
| **Auto-reply** | Let the AI answer all chats, selected ones, or none — separately for chats and groups, with per-chat instructions, hourly limits, quiet hours and a kill switch in the menu bar. |
| **Bring your own model** | Uses what you already have — Ollama, LM Studio or llama.cpp. Nothing installed? One click downloads llama.cpp and a lightweight model (Gemma 4, Qwen3.5) and runs it locally. |
| **The full chat app** | Your chats, groups and history from your phone, with profile pictures. Photos, videos, voice notes (record and play), documents, polls (see results and vote), drag files from Finder straight into a chat, and emoji. Pin, mute, archive, mark unread, export and clear chats from a right-click menu; archived chats stay quiet. |
| **Every message action** | Reply (with the quoted message shown), react (click a reaction to see who left it), star, pin, forward, copy, edit, delete for everyone, save to Notes and select many at once. Send the stickers from your account. |
| **Notes** | A private chat just for you to keep text, files and saved messages — as a chat, a board or sticky notes. It never reaches WhatsApp unless you forward something. |
| **Your own lists** | Group chats into lists like *Work* or *Family* and filter by them, next to *Unread* and *Groups*. Resize the chat list to taste. |
| **Light by design** | Native Rust core, system webview, a ~90 KB interface bundle. Runs quietly in the menu bar. |
| **Bilingual** | Spanish and English, following your system language. |

## Privacy and security

- **Everything stays on your Mac.** No Notchat servers, accounts or telemetry. The AI runs on `localhost`; remote AI endpoints are refused.
- **Encrypted at rest** with SQLCipher (AES-256); the key lives in your Keychain.
- **Prompt-injection aware.** Auto-replies see only the chat they answer and have no tools. The assistant always asks before sending once it has read chats or web pages.
- **Verified downloads.** llama.cpp and models are pinned by SHA-256; updates are checked against the release's published SHA-256 before installing.
- **Updates on your terms.** Notchat checks GitHub for a new version at launch and every three hours; turn it off in Settings → Updates.
- **Audited supply chain.** `cargo audit`, `cargo deny`, CodeQL, SBOM and signed build provenance for every release.

Read the [security policy](SECURITY.md), the [threat model](docs/security/threat-model.md) and the [ISO 27001 / OWASP ASVS controls mapping](docs/security/controls.md). Notchat is aligned with those frameworks — it is software, so it is not itself certified.

## Requirements

- macOS 13 Ventura or later, Apple Silicon or Intel
- A WhatsApp account on your phone
- For AI features: 8 GB of RAM (16 GB for the larger models)

## Build from source

```bash
git clone https://github.com/Lautaro005/notchat.git && cd notchat
npm run setup
npm run dev      # development, hot reload
npm run build    # Notchat.app in target/release/bundle/macos
```

Needs Rust stable, Node.js 22+ and the Xcode Command Line Tools. See [CONTRIBUTING.md](CONTRIBUTING.md).

## How it is built

```
crates/core   Rust: WhatsApp session, encrypted store, AI providers, agent, auto-reply policy
src-tauri     Tauri v2 app: commands, notch panel, notifications, menu bar
ui            Svelte 5 interface
```

WhatsApp connectivity comes from the open-source [`whatsapp-rust`](https://github.com/jlucaso1/whatsapp-rust) library.

## Disclaimer

Notchat is an independent, **unofficial** client. It is not affiliated with, endorsed by or connected to WhatsApp or Meta. "WhatsApp" is a trademark of its owner. Using unofficial clients — and automating replies in particular — can lead WhatsApp to restrict or ban your number. Use it at your own risk.

## License

[MIT](LICENSE)
