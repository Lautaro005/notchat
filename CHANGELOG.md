# Changelog

## 0.1.4 — 2026-11-19

Names you recognise, read state that follows your phone, and a smarter assistant.

- **Contact names.** Chats, group senders, quotes and mentions use the name saved in your contacts. People you haven't saved show the name they set in WhatsApp with a ~ in front (~Juan). Group members that WhatsApp identifies by a private id are matched to their phone number when possible.
- **Mentions.** @mentions in groups show the person's name instead of a number or id. Click one, or a sender's name, to see their name and phone number, copy the number or start a chat.
- **Read on your phone, read here.** Messages you read on the phone or another device stop counting as unread in Notchat, even if it was closed at the time.
- **Save stickers.** Right-click a sticker in a chat → Save sticker. It goes to your favourites in the Stickers tab.
- **Lists in the chat header.** If a chat belongs to one of your lists, the list shows next to its name; click it to see that list.
- **Even spacing.** Consecutive messages from the same person now sit as far apart as messages from different people.
- **Right-click menus close** after you pick an option in a submenu (mute, add to list).
- **Notchat AI understands WhatsApp better.** A new tool lists your unread messages; chat names are matched loosely (accents, ~, phone numbers); chat lines carry replies, attachments, edits and reactions; and the instructions are rewritten so small local models choose the right tool.

## 0.1.3 — 2026-11-18

Message actions, a notes chat and a new look.

- **Message menu.** Hover a message for the chevron (or right-click): reply, react, star, pin for everyone (7 days), forward to up to five chats, copy, edit your own text (first 15 minutes), save to Downloads, save to Notes, message info, delete for me or for everyone, and select several messages at once for copy, star, forward, save or delete. Quick react and reply buttons sit next to each message.
- **Replies show what they answer.** Quoted messages appear inside the bubble with who wrote them; click one to jump to the original. Replies you send are real WhatsApp replies.
- **Reactions, edits and deletions** from your phone and other people show up live. Pinned messages get a bar at the top of the chat.
- **Stickers from your account.** The emoji panel has a Stickers tab with your phone's recent stickers (imported when linking) and every sticker you send or receive. Right-click to favourite or remove.
- **Notes.** A private chat pinned under Notchat AI for things you want at hand: text, files and messages saved from your chats. Three views: chat, board (drag between Pinned, To do and Done) and sticky notes with colours. Notes never go to WhatsApp unless you forward one.
- **New palette and design.** Porcelain neutrals with one iris accent; your messages in iris bubbles; tighter grouping with clear gaps between runs of messages.
- **Notch panel redesign.** Shows the contact's photo, sits flush with the notch with only the bottom corners rounded, and no longer has light edges.
- **Group previews** in the chat list say who sent the last message; long names and previews end in "…" instead of widening the list.
- **Esc** closes the open chat (after any menu, dialog, reply or selection).
- **Settings → AI → Local ports** to point Notchat at Ollama, LM Studio or llama.cpp on non-default ports.
- The Settings button is now a gear; the assistant's message box is as wide as the chats'.

## 0.1.2 — 2026-11-18

A much fuller chat app, with a new look.

- **History, groups and photos.** Linking now imports your chats, groups and recent history from the phone, including archived, pinned and muted state. Every group you belong to shows up, contacts no longer fill the list as empty chats, and profile pictures load (cached for a day).
- **Media.** See photos, stickers and videos, play and record voice notes, open documents (saved to Downloads), and send photos, videos and files from the attach button, by pasting or by drag-and-drop. Emoji picker in the composer.
- **Right-click menu** on every chat: archive, pin (up to 3), mute for 8 hours / 1 week / always, mark as read or unread, add to a list, chat or group info, export as text, clear, exit group and delete. Destructive actions ask first. Changes sync to your phone.
- **Lists and filters.** Filter by All, Unread and Groups, or make your own lists (Work, Family…) and filter by them. Archived chats have their own view.
- **Chat and group info panel** with participants, description and quick actions.
- **Settings window** with sections: General (theme, notifications), AI, Auto-reply, Storage, Updates and Account.
- **Storage.** See what Notchat keeps on this Mac, per chat, and clear downloaded files, profile pictures or old messages.
- **Updates.** Notchat checks GitHub at launch and every three hours (can be turned off), and installs a new version in one click after verifying its SHA-256.
- **New design.** Resizable chat list, floating conversation card, light / dark / system theme switch next to Settings, coloured names in groups, dotted chat wallpaper.
- Muted chats no longer send notifications.
- Fixed: the one-line installer failed with `VERSION: unbound variable` in some shells.


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
