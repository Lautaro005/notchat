# Changelog

## 0.1.9 — 2026-11-27

Delivery ticks, a notification sound and member profiles.

- **Sent, delivered, read.** Your messages in one-to-one chats show WhatsApp's ticks: one for sent, two for delivered, two in blue for read. The chat list shows them before the preview when the last message is yours (not when it's theirs). They update live as the other person gets and reads your messages; history from the phone comes with its ticks too. Groups don't show them yet, and messages from before this version have none.
- **Notification sound.** Settings → Notifications has a sound switch (on by default): new-message notifications play macOS's "Glass" sound, or an MP3 of your own (up to 5 MB) that you choose there. "Play" previews it. Muted and archived chats stay silent, and a burst of messages plays it once.
- **Member profiles.** Clicking someone in a group's member list opens a card with their picture, name (and their own WhatsApp name if you saved them under another one), phone number with a copy button, whether they're an admin of the group, and a button to message them. Mentions open the same card.
- **Replies to photos.** The quote in a reply to a photo, video or sticker now shows a small preview of it. Clicking any quote takes you to the original message, loading older messages if needed (until now it only worked when the original was already on screen, and silently did nothing otherwise).
- **Photos open centred.** An opened photo now always fits the window and stays centred, whatever its size and the window's. Tall photos used to start halfway down and run off the bottom.

## 0.1.8 — 2026-11-26

Read state that follows your phone, stickers with sound and names in replies.

- **Read on the phone, read here.** Chats you read on your phone now stop counting as unread in Notchat, also when Notchat was closed at the time. Until now this only worked with read receipts turned off: with them on (the default), WhatsApp tells your other devices in a different way, which Notchat ignored.
- **Stickers with sound.** WhatsApp's new stickers with sound show up instead of disappearing. A small speaker button on the sticker plays the sound. Saved to your stickers, they're sent silent.
- **Mentions in replies.** The quoted message inside a reply shows `@Name` instead of `@number`, right after you send it too: the contact name if you saved them, otherwise ~ and their WhatsApp name, and the number only when there's no name. Same in the reply box above the composer.
- **Group members.** Saved contacts show only their name; the phone number appears underneath only for people you haven't saved.
- **Release notes are formatted.** The "new version available" card in Settings → Updates shows the notes with headings, bold and lists instead of raw `#` and `*`.

## 0.1.7 — 2026-11-24

Polls, drag-and-drop that works and quieter archived chats.

- **Polls.** Polls show up in chats with their question, options and live results. Click an option to vote (or click it again to take your vote back); "View votes" lists who picked what. Votes from your phone and other people update as they come in. Polls that arrived before this version were skipped at the time, so only new ones (and history fetched from now on) appear.
- **Drag files into a chat.** Drop photos, videos or documents from Finder onto a conversation and they're attached, ready to send, like with the attach button. Folders are skipped.
- **Who reacted.** Clicking a reaction under a message no longer adds the same reaction: it opens a small list of everyone who reacted and with which emoji, with tabs per emoji. Click your own row to remove your reaction.
- **Date labels don't pile up.** Each day's label ("Today", "Sunday, November 22") sticks to the top only while that day's messages are on screen, instead of stacking on top of each other.
- **Archived chats are quiet.** New messages in archived chats no longer show notifications (system or notch), like WhatsApp's "keep chats archived".
- **Group members with names.** The participants list shows each person's name as in the chat list (contact name, or ~ and their WhatsApp name) with the phone number underneath in smaller type; people without a name show only their number. You appear as "You".

## 0.1.6 — 2026-11-23

A clearer conversation and a much better-looking assistant.

- **Unread divider.** Opening a chat with unread messages lands on a "N unread messages" bar above the first one, instead of the end. It goes away once you scroll to the bottom.
- **Clickable links** in messages; they open in your browser.
- **View-once photos and videos** now show up as a message ("View-once photo · can only be opened on your phone"). WhatsApp never sends their content to linked devices, so they can't be opened on the Mac.
- **Notchat AI answers are formatted.** Markdown with small headings, bold, lists and quotes, on a light card. When the assistant offers options they appear as buttons. Chats it looked at show as small cards you can click to open; "read 30 messages from Ana" links to the chat.
- **Web sources.** Facts from the web carry a small source number (not copied with the text), and the sources appear as pills under the answer with the exact page. URLs are left out of the answer unless you ask for them.
- **Today's date, always.** The assistant gets today's date with every message and searches news with a recency filter, so "today" means today.
- **No more running out of context.** Long conversations are trimmed to fit small models, tool results are capped, repeated lookups reuse the first result, and earlier lookups are remembered as short notes, so the assistant re-checks instead of inventing.
- **@ in the assistant** suggests your chats and groups to name exactly who you mean.

## 0.1.5 — 2026-11-23

Read state, names and history that match your phone.

- **Messages you send from the phone stay in their chat.** 0.1.4 could file messages to (and from) some contacts under your own chat. Fixed, the wrong links are repaired at launch, and messages from those contacts are moved back to their chats.
- **Read on the phone means read here, also after a restart.** WhatsApp names your own number in read receipts for one-to-one chats; Notchat now finds the right chat from the message itself, and messages read before they reached the Mac arrive already read. Coming back to the window reconnects right away if the connection had dropped (sleep, network change).
- **More consistent names.** At launch Notchat looks up the phone number of every private group id it has seen (and the id of every unnamed chat) in WhatsApp's own address book, so saved names show everywhere.
- **Whole history.** In the background Notchat asks your phone for each chat's older messages, newest chats first, a little at a time. A small ring in the sidebar shows progress; turn it off in Settings → Storage. Scrolling to the top of a chat also asks the phone for older messages. New links request the full history from the start.
- **Mark all as read.** Right-click All, Unread, Groups or one of your lists.
- docs/references.md lists the open-source projects used as reference.

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
