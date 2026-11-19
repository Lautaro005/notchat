# Design

## World

**Iris on porcelain, the notch in black.** Notchat lives in the black cut-out at the top of the screen; the notch panel is always pure black, flush with the hardware notch, with only its bottom corners rounded. Inside the window the material is porcelain: quiet, slightly cool neutrals with a single iris accent. Your own messages are iris capsules; everything else recedes.

Mode: Operate. Familiar Mac messaging layout, no decoration, the conversation leads.

## Colour

- Neutrals carry the interface, tinted faintly toward violet so the iris accent sits naturally.
- One accent, **iris** (`--accent`, `#5847e0` light / `#a598ff` dark), used for: primary actions, unread counts, focus rings, selection, quotes' author, and anything the AI did. Deliberately not green or blue, so the app is never mistaken for WhatsApp, iMessage or Telegram.
- Own bubbles use `--mine` (a deeper iris in dark mode so white text keeps AA contrast).
- Notes have their own soft paper colours (`--note-*`), used only in the Notes chat.
- Light and dark follow the system or the theme switch. All colours are tokens in `ui/src/app.css`; components never hard-code colour, except the notch panel, which is black in both themes and keeps its own local tokens.

## Type

System font (`-apple-system`); `ui-rounded` for big titles, empty states and sticky notes. Scale 11 / 12 / 13 / 14 / 17 / 22 px. Weights 400 / 500 / 600 / 700. Tabular numerals in times and counts.

## Shape and space

- Bubbles: 18px corners; the first bubble of a run from one sender has a 6px tail corner at the top (left for others, right for me). Every message sits 12px from the next, whether or not the sender changes.
- Mentions and group sender names are buttons in the accent colour (underlined in your own bubbles); they open a small person card. A chat's lists show as soft accent chips in its header.
- Conversation, assistant and notes are floating cards (16px radius) on the window background.
- 4px spacing grid. Chat rows 68px. Sidebar resizable 260–520px.
- Depth is rare: one soft offset shadow for floating things (menus, dialogs, cards). No colored side stripes.

## Motion

140–320 ms, exponential ease-out, state changes only. New messages rise 8px and fade in; the notch content drops in with a short blur. Everything is disabled under `prefers-reduced-motion`.

## Icons

Authored inline SVG, 24px viewBox drawn at 13–20px, 1.6 stroke, round caps. No emoji as icons.

## Layout

Chat list · conversation · optional info panel. Notchat AI and Notes are pinned at the top of the list. Message actions appear on hover (react, reply, chevron menu) and on right-click; selection mode swaps the composer for an action bar.
