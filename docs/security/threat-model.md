# Threat model

Method: STRIDE over the data flows below. Reviewed on every release that changes
a trust boundary.

## Assets

| Asset | Where it lives |
|---|---|
| WhatsApp session keys (Signal identity, sessions, pre-keys) | `session.db` (SQLCipher) |
| Messages, contacts, chat names | `notchat.db` (SQLCipher) |
| Assistant conversation and attachments' extracted text | `notchat.db` |
| Downloaded media and profile pictures | `media/`, `avatars/` (files `0600`, hashed names, not encrypted) |
| Notes (text) and the account's sticker list (media keys) | `notchat.db` |
| Files attached to notes | `notes/` (files `0600`, hashed names, not encrypted; not cleared by Settings → Storage) |
| Database key | macOS Keychain, item `app.notchat.desktop / db-key-v1` |
| Settings (AI provider, auto-reply rules) | `notchat.db` |

All files live in `~/Library/Application Support/app.notchat.desktop/` (`0700`).

## Trust boundaries and data flows

```
 WhatsApp servers ──TLS/Noise──► Rust core ◄──IPC (validated)── Webview UI
                                   │   │
             localhost only ◄──────┘   └──────► public web (assistant only, SSRF-guarded)
        (Ollama / LM Studio / llama.cpp)
                                   │
                    github.com / huggingface.co (downloads you start, SHA-256 pinned)
                                   │
                    api.github.com / github.com (update checks, optional, SHA-256 verified)
```

1. **WhatsApp ↔ core** — end-to-end encrypted protocol handled by `whatsapp-rust`.
2. **Webview ↔ core** — Tauri IPC. The webview is treated as less trusted than the core.
3. **Core ↔ AI provider** — HTTP on loopback only; remote endpoints are refused.
4. **Core ↔ web** — only when the user asks the assistant; public addresses only.
5. **Core ↔ download hosts** — pinned URLs and hashes in `models.json`.
6. **Core ↔ GitHub Releases** — update checks at launch and every 3 hours (user toggle, on by default). An update installs only after the user clicks *Install* and its SHA-256 matches the release's `SHA256SUMS`.
7. **Core ↔ WhatsApp CDN** — media and profile pictures. Picture URLs must be `https://*.whatsapp.net`; redirects are not followed.

## Threats and mitigations

| # | STRIDE | Threat | Mitigation |
|---|---|---|---|
| T1 | Information disclosure | Another app or user reads the databases | SQLCipher AES-256, key in Keychain, `0600`/`0700` permissions |
| T2 | Information disclosure | Backups or disk images leak messages | Databases are encrypted; key stays in the Keychain, not in backups of the data folder |
| T3 | Tampering / elevation | **Prompt injection** in an incoming message makes the AI act | Auto-replies: one chat of context, zero tools, output length-capped, `SKIP` allowed, rate-limited, off by default |
| T4 | Tampering / elevation | Injection via chats or web pages read by the assistant makes it message someone | Tool results labelled as untrusted; any send after reading untrusted content **always** requires user confirmation; ambiguous recipients are refused |
| T5 | Information disclosure | Assistant exfiltrates data through web requests | Web access is a user toggle; requests only on user turns; SSRF guard blocks private networks so local services cannot be probed |
| T6 | Spoofing | A remote "AI provider" receives chat content | Provider URL must be `http://` on a loopback host; validated on save and on use |
| T7 | Tampering | Malicious llama.cpp binary or model | HTTPS-only from `github.com` / `huggingface.co`, size and SHA-256 verified before use, partial files discarded on mismatch |
| T8 | Elevation | Compromised webview calls privileged commands | Minimal capabilities (events, window drag); every command validates its inputs (JID charset/length, sizes, enums); CSP `default-src 'self'`, no remote scripts |
| T9 | Information disclosure | Logs leak messages | Logs carry events and counts only; dependency logs capped at `warn`; no message text, numbers or keys |
| T10 | Denial of service | Message floods trigger runaway AI replies | Per-chat hourly cap, one pending reply per chat, quiet hours, global kill switch in the menu bar |
| T11 | Repudiation | User cannot tell what the AI sent | AI-sent messages are flagged and marked in the conversation; assistant actions are kept in its history |
| T12 | Tampering | Compromised release artifact | Checksums published per release, installer verifies them, build provenance attestation, SBOM |
| T14 | Information disclosure | Downloaded photos and files read from disk | Owner-only `0600` files in a `0700` folder, names are hashes (no numbers or chat ids); can be cleared from Settings → Storage |
| T15 | Tampering | Malicious update | HTTPS only, release tag must be newer, archive SHA-256 checked against the published `SHA256SUMS` before unpacking; bundle swap runs with paths passed as arguments |
| T16 | Information disclosure | Media rendered as active content in the webview | Only `image/`, `video/` and `audio/` types are handed to the webview, as `data:` URLs in media elements; CSP `media-src` limited to `self`, `data:` and `blob:` |
| T17 | Tampering / spoofing | A link in a message or in an AI answer runs code or opens a local file | Links open only when clicked, in the default browser; only `http(s)` and `mailto:` are accepted (checked in the webview and again in the `open_url` command), passed to `/usr/bin/open` as a single argument, never through a shell. AI answers are rendered from escaped Markdown, with no raw HTML |
| T18 | Information disclosure | The assistant's history grows past the model's window and it fills gaps by inventing | Conversation trimmed to a fixed budget (oldest first), tool results capped, repeated tool calls reuse the first result, earlier tool calls remembered as one-line notes; the prompt tells the model to call tools again instead of guessing |
| T19 | Tampering / spoofing | Forged or replayed poll votes, or a poll's secret leaking to the interface | Votes are AES-GCM encrypted with a key derived from the poll's secret and the creator and voter ids; votes that do not decrypt are dropped, only option hashes that match the poll count, and a person's newest vote replaces older ones. The secret stays in the encrypted database and is never sent to the webview. Voting is only done when the user clicks an option |
| T20 | Tampering | A file dropped on the window opens as a page in the webview | Native drop handling is off and the webview reads dropped files itself; drops outside the conversation are cancelled, folders are skipped, and files go through the same size checks as the attach button. Nothing is sent until the user presses send |
| T21 | Tampering | A patched protobuf crate hides a behaviour change | `vendor/waproto` is byte-for-byte `waproto` 0.7.0 except `build.rs`, which only declares two message fields (`Message.audioStickerMessage = 134`, `StickerMessage.audioMessage = 26`); the crate's own descriptor checksum still runs; documented in `vendor/waproto/NOTCHAT.md` and dropped once upstream declares the fields. Sticker sounds reach the webview only as `audio/ogg` `data:` URLs (T16) |
| T13 | Account risk | WhatsApp restricts the number | Clear warning at pairing and before enabling auto-reply; conservative defaults |

## Residual risks

- Notchat is an **unofficial client**; protocol changes can break it and WhatsApp may act against accounts using it.
- Local models are weaker at resisting prompt injection than large hosted models. The tool-less auto-reply design limits impact to the reply text itself.
- Until releases are signed with an Apple Developer ID and notarized, users must trust the GitHub release and checksum rather than Gatekeeper.
- Downloaded media is stored unencrypted (permissions only), so another process running as the same user can read it.
- A malicious process running as the same user with Keychain access can read data. This is outside what an app can defend against.
