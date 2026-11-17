# Design

## World

**The notch is the brand.** Notchat lives in the black cut-out at the top of the screen, so the interface borrows its material: ink-black capsules on quiet neutral surfaces. Your own messages are notch-black capsules (inverted in dark mode); everything else recedes.

Mode: Operate. Familiar Mac messaging layout, no decoration, the conversation leads.

## Colour

- Neutrals carry the interface; they are slightly warm so black capsules do not feel cold.
- One accent, **signal orange** (`--accent`), used only for: primary action, unread count, focus ring, and marking anything the AI did. Deliberately not green or blue, so the app is never mistaken for WhatsApp, iMessage or Telegram.
- Light and dark follow the system. All colours are tokens in `ui/src/app.css`; components never hard-code colour.

## Type

System font (`-apple-system`). Fixed scale: 11 / 12 / 13 / 15 / 17 / 22 px. Weight steps 400 / 500 / 600. Numerals tabular in times and counts.

## Shape and space

- Capsule radius for bubbles and inputs (18px), 10px for rows and controls, 22px for the pairing card.
- 4px spacing grid. Rows 60px. Sidebar 300px on the second neutral layer.
- Depth is rare: one soft offset shadow for floating things (sheet, notch panel). No borders where spacing can separate.

## Motion

150–220 ms, exponential ease-out, state changes only. New messages rise 6px and fade in. Everything is disabled under `prefers-reduced-motion`.

## Icons

Authored inline SVG, 20px box, 1.6 stroke, round caps. No emoji as icons.

## Layout

Chat list · conversation · settings as an inline right sheet (not a modal). The assistant chat is pinned first in the list and marked with the accent.
