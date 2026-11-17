# Threat model

Method: STRIDE over the data flows below. Reviewed on every release that changes
a trust boundary.

## Assets

| Asset | Where it lives |
|---|---|
| WhatsApp session keys (Signal identity, sessions, pre-keys) | `session.db` (SQLCipher) |
| Messages, contacts, chat names | `notchat.db` (SQLCipher) |
| Assistant conversation and attachments' extracted text | `notchat.db` |
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
```

1. **WhatsApp ↔ core** — end-to-end encrypted protocol handled by `whatsapp-rust`.
2. **Webview ↔ core** — Tauri IPC. The webview is treated as less trusted than the core.
3. **Core ↔ AI provider** — HTTP on loopback only; remote endpoints are refused.
4. **Core ↔ web** — only when the user asks the assistant; public addresses only.
5. **Core ↔ download hosts** — pinned URLs and hashes in `models.json`.

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
| T13 | Account risk | WhatsApp restricts the number | Clear warning at pairing and before enabling auto-reply; conservative defaults |

## Residual risks

- Notchat is an **unofficial client**; protocol changes can break it and WhatsApp may act against accounts using it.
- Local models are weaker at resisting prompt injection than large hosted models. The tool-less auto-reply design limits impact to the reply text itself.
- Until releases are signed with an Apple Developer ID and notarized, users must trust the GitHub release and checksum rather than Gatekeeper.
- A malicious process running as the same user with Keychain access can read data. This is outside what an app can defend against.
