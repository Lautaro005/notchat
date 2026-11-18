# Changelog

## 0.1.1 — 2026-11-18

Maintenance release: updated dependencies and a hardened build pipeline. No feature changes.

- Updated Rust dependencies: rand 0.10, scraper 0.29 (drops the unmaintained fxhash), base64 and sha2.
- Updated the interface toolchain: Vite 8, vite-plugin-svelte 7 and TypeScript 6.
- CI and release workflows moved to current GitHub Actions (Node 24).
- Release builds only sign or notarize when Apple credentials are configured.
- Supply-chain checks run cargo audit and cargo-deny on every push; shipped npm packages must have no known high-severity issues.

## 0.1.0 — 2026-11-17

First version.

- Link your phone with a QR code or a pair code generated inside the app.
- Read and answer chats and groups; encrypted local storage.
- Notchat AI: a local assistant that reads your chats, searches the web, reads attached files and sends messages for you with confirmation.
- Use Ollama, LM Studio or llama.cpp, or let Notchat install llama.cpp with a lightweight model.
- Auto-reply for all, selected or no chats and groups, with limits and a kill switch.
- Notifications in the notch or as system notifications, both with inline reply.
