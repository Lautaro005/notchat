# waproto 0.7.0 — Notchat patch

Unmodified copy of [`waproto` 0.7.0](https://crates.io/crates/waproto) (MIT,
João Lucas), wired in with `[patch.crates-io]` in the root `Cargo.toml`.

The only change is in `build.rs`: two entries in `LOCAL_FIELDS` (and a
`type_name` so local fields can be messages) for stickers with sound, which
WhatsApp added after this release:

| Message | Field | Number | Type |
|---|---|---|---|
| `Message` | `audioStickerMessage` | 134 | `Message.FutureProofMessage` |
| `Message.StickerMessage` | `audioMessage` | 26 | `Message.AudioMessage` |

`src/whatsapp.proto` and `src/whatsapp.desc` are untouched, so the crate's own
descriptor checksum still passes. Drop this copy (and the patch) once a
`whatsapp-rust` release declares both fields.
