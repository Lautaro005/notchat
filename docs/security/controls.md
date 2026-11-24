# Security controls mapping

> **Aligned, not certified.** Certifications such as ISO/IEC 27001 or SOC 2 apply
> to audited organizations, not to a piece of software. This table shows how
> Notchat's design supports the relevant controls so that individuals and
> organizations using it can assess it against their own programs.

## ISO/IEC 27001:2022 — Annex A (technological and relevant organizational controls)

| Control | Title | How Notchat addresses it |
|---|---|---|
| A.5.7 | Threat intelligence | Dependabot, `cargo audit` (RustSec) and CodeQL in CI |
| A.5.12 | Classification of information | All chat data treated as confidential by default; untrusted vs. trusted inputs separated in the AI design |
| A.5.14 | Information transfer | Only to WhatsApp (E2E protocol) and user-initiated web lookups; AI stays on localhost |
| A.5.23 | Use of cloud services | None. No Notchat backend |
| A.5.24–A.5.26 | Incident management | [SECURITY.md](../../SECURITY.md): private reporting, response targets, advisories |
| A.5.34 | Privacy and protection of PII | Data minimization (no history import, no telemetry), local storage, user can unlink and erase |
| A.8.1 | User endpoint devices | Runs entirely on the user's Mac with per-user file permissions |
| A.8.3 | Information access restriction | Database files `0600`, folder `0700`; Keychain-held key |
| A.8.5 | Secure authentication | Device linking through WhatsApp's QR / pair-code flow; no passwords handled by Notchat |
| A.8.7 | Protection against malware | Downloads restricted to two hosts and pinned by SHA-256 |
| A.8.8 | Management of technical vulnerabilities | Locked dependencies, automated audits, supported-versions policy |
| A.8.9 | Configuration management | Safe defaults: auto-reply off, send confirmation on, notifications local |
| A.8.10 | Information deletion | "Unlink this device" deletes stored chats and the session database |
| A.8.12 | Data leakage prevention | AI isolation (T3–T6 in the threat model), loopback-only providers, SSRF guard |
| A.8.15 | Logging | Operational logging without message content or identifiers |
| A.8.20–A.8.22 | Network security / segregation | Outbound guard blocks private, loopback, link-local and CGNAT ranges |
| A.8.24 | Use of cryptography | SQLCipher (AES-256), raw 256-bit random keys, TLS for all downloads |
| A.8.25 | Secure development life cycle | Threat model, code review, CI gates (fmt, clippy `-D warnings`, tests, audit, deny) |
| A.8.26 | Application security requirements | Input validation on every IPC command; strict CSP |
| A.8.28 | Secure coding | Rust; no `unwrap` outside tests; `unsafe` limited to documented AppKit/libc calls |
| A.8.29 | Security testing | Unit tests for the policy engine, agent isolation, SSRF guard, encryption at rest |
| A.8.30 | Outsourced development | Third-party code limited to audited, locked dependencies |
| A.8.32 | Change management | Pull requests, CI required, signed provenance on releases |

## OWASP ASVS 5.0 (selected requirements)

| Area | Notchat |
|---|---|
| V1 Encoding and sanitization | Svelte escapes all text; no `{@html}` with message content; QR SVG generated in Rust |
| V2 Validation and business logic | Typed IPC payloads, length and charset checks, rate limits on automation |
| V4 API and web service | IPC surface is an explicit allowlist of commands |
| V5 File handling | Attachments (button, paste or drag-and-drop): 15 MB cap, type sniffing, path components stripped, PDF parser panics contained; dropped folders skipped and drops outside the conversation cancelled |
| V8 Authorization | The assistant cannot send without the user's confirmation once untrusted content is in play |
| V11 Cryptography | Vetted implementations only (SQLCipher, rustls, sha2) |
| V12 Secure communication | TLS 1.2+ via rustls for every remote request; HTTPS-only downloads |
| V13 Configuration | No secrets in source; dependencies pinned; CSP enforced |
| V14 Data protection | Encrypted at rest, minimal retention, user-triggered deletion |
| V15 Secure coding and architecture | Core logic isolated from UI; threat model maintained |
| V16 Security logging | No sensitive data in logs |
