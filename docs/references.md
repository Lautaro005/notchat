# Reference projects

Projects that speak the WhatsApp multi-device protocol directly (not web wrappers), useful when checking how something should behave. Notes are what we took from each; read their code for details.

| Project | Language | Why it is useful |
|---|---|---|
| [whatsmeow](https://github.com/tulir/whatsmeow) | Go | The most complete protocol library. `receipt.go`: a `read-self` receipt for a one-to-one chat names our own address, and the real chat is in `recipient`. `BuildHistorySyncRequest` asks the phone for older history; `DeviceProps.RequireFullSync` + `HistorySyncConfig` ask for the full history at link time. |
| [mautrix-whatsapp](https://github.com/mautrix/whatsapp) | Go | A long-lived bridge built on whatsmeow, so it has met every real-world edge case: history backfill (`historysync.go`), contact vs. push names (`puppet.go`), unread state, the LID migration (v0.2511.0 fixed receipts after DM LID migration; PR #906 uses LID-indexed app state). |
| [wacli](https://github.com/openclaw/wacli) | Go | On-demand history backfill from the phone; notes that the phone must be online and results are best-effort. |
| [slidge-whatsapp](https://git.deuill.org/deuill/slidge-whatsapp) | Go/Python | History sync on first login; on-demand sync needs a full reference to the oldest known message. |
| [ZapFast](https://github.com/crmne/zapfast) | Rust (egui) | A desktop companion client on the same library we use, `whatsapp-rust`. |
| [Rust Meow](https://github.com/thinkter/rust-meow) | Rust + Go | Tauri app with a whatsmeow sidecar. |
| [Whatevr](https://www.linuxlinks.com/whatevr-linux-first-native-client-whatsapp/) | — | Early native Linux client on whatsmeow. |
| [whatsapp-rust](https://docs.rs/whatsapp-rust) | Rust | Our library. `fetch_message_history` (on-demand history), `DevicePropsOverride` (full sync at link), `get_lid_pn_entry` (its own LID ↔ phone book). |

What the official desktop app does that we follow:

- Contacts show their address-book name; others show `~` + the name they chose.
- Reading a chat on the phone clears it everywhere, including chats read while this app was closed (the server queues the receipts).
- History beyond the initial sync is fetched from the phone on demand, newest chats first.
- Messages you send from the phone appear in the chat you sent them to, never in your own chat.
