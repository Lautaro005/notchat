# Security Policy

Notchat handles private conversations, so security reports get priority over everything else.

## Supported versions

| Version | Supported |
|---|---|
| Latest release | Yes |
| Previous minor release | Security fixes for 30 days after a new minor ships |
| Anything older | No — please update |

## Reporting a vulnerability

**Do not open a public issue.** Report privately through
[GitHub Security Advisories](https://github.com/Lautaro005/notchat/security/advisories/new).

Please include:

- what an attacker can do, and what they need to do it;
- steps to reproduce, or a proof of concept;
- the Notchat version and macOS version.

### What to expect

| Step | Target |
|---|---|
| Acknowledgement | within 3 days |
| Initial assessment and severity (CVSS 4.0) | within 7 days |
| Fix for critical / high severity | within 30 days |
| Fix for medium / low severity | next scheduled release |
| Public advisory and credit | when the fix ships, unless you ask to stay anonymous |

We will keep you informed throughout and agree on a disclosure date with you.
We do not take legal action against good-faith research that respects this policy:
test only against your own accounts and devices, do not access other people's data,
and give us reasonable time to fix before disclosure.

## Scope

In scope: the Notchat app, its installer (`install.sh`), the release pipeline and
anything in this repository.

Out of scope: WhatsApp's own servers and protocol, third-party AI runtimes
(Ollama, LM Studio, llama.cpp) and the models themselves, and issues that need
an already-compromised Mac or physical access to an unlocked one.

## How Notchat protects your data

A summary — details in [docs/security/threat-model.md](docs/security/threat-model.md)
and [docs/security/controls.md](docs/security/controls.md).

- **Local only.** No Notchat servers, no accounts, no telemetry. Network traffic is
  limited to WhatsApp, your local AI on `localhost`, web lookups you ask the
  assistant for, and downloads you start (llama.cpp and models, checksum-verified).
- **Encrypted at rest.** Messages and the WhatsApp session are stored in SQLCipher
  databases (AES-256). The 256-bit key is random and kept in the macOS Keychain.
  Data files are readable only by your user (`0600`, folder `0700`).
- **AI isolation.** Incoming messages are treated as untrusted. Auto-replies see only
  the chat they answer and have no tools: no internet, no other chats, no sending
  elsewhere. The assistant asks before sending a message, and always asks when it
  read chats or web pages in that request.
- **Hardened webview.** Strict Content Security Policy, no remote content,
  minimal Tauri capabilities, every command input validated in Rust.
- **Outbound web guard.** Assistant web requests can reach only public addresses;
  private, loopback and link-local ranges are blocked, including after DNS
  resolution and redirects.
- **Supply chain.** Locked dependencies, `cargo audit` and `cargo deny` in CI,
  CodeQL, Dependabot, a CycloneDX SBOM and signed build provenance for every release.

## Certifications

Notchat is open-source software, not an organization, so it is **not certified**
under ISO/IEC 27001, SOC 2 or similar schemes. Its controls are designed to be
**aligned** with ISO/IEC 27001:2022 Annex A and OWASP ASVS 5.0; the mapping is in
[docs/security/controls.md](docs/security/controls.md).

## Unofficial client

Notchat is not affiliated with WhatsApp or Meta. It uses the unofficial
multi-device protocol through the open-source `whatsapp-rust` library. Using
unofficial clients, and automating replies in particular, can lead WhatsApp to
restrict or ban your number.
