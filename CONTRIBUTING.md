# Contributing

Thanks for helping. Read [AGENT.md](AGENT.md) first: it holds the project's fixed decisions and rules, for people and AI assistants alike.

## Setup

```bash
git clone https://github.com/Lautaro005/notchat.git
cd notchat
npm run setup      # installs the UI dependencies
npm run dev        # runs the app with hot reload
```

Requirements: macOS 13+, Rust (stable, via rustup), Node.js 22+, Xcode Command Line Tools.

Development builds read the database key from `NOTCHAT_DEV_KEY` (64 hex characters) when set, to avoid a Keychain prompt on every rebuild, and keep their data in a separate `dev/` folder. Release builds always use the Keychain.

To review the interface without the app, run `npm --prefix ui run dev` and open `http://127.0.0.1:1420/?view=chats` (also `pair`, `code`, `empty`, `noai`, `aiempty`, `auto`). The browser preview uses sample data.

## Before opening a pull request

```bash
npm run check
```

runs `cargo fmt --check`, `cargo clippy -D warnings`, all Rust tests and the UI type-check. CI runs the same plus `cargo deny`, `cargo audit` and CodeQL.

## Ground rules

- Core logic goes in `crates/core`, with tests. `src-tauri` only wires it to the UI.
- Never log message text, phone numbers or keys.
- Every Tauri command validates its inputs.
- Anything an incoming message can influence must not gain new capabilities (see the threat model).
- Prefer the dependencies already in the tree; justify new ones in the PR.
- All user-facing text in Spanish and English (`ui/src/lib/i18n.ts`).

By contributing you agree your work is released under the MIT License.
