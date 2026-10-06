# OpenDesign prototypes (reference copies)

Copied from the local OpenDesign project on 2026-10-06, at the state each UX round closed with. They are the
**visual guide** for [UX batch 1](../../ux-batch-1.md); the decisions are in the round documents, which win where
the two disagree.

| File | Round | What it shows |
|---|---|---|
| `index.html` | [1](../../round-1-approval-card.md) | The approval card, its states and cases, and the Calm editor layout |
| `chat.html` | [2](../../round-2-chat.md) | The chat: formatting, UI action cards, session notices, context chip, rail flyouts, session info popover |
| `devices.html` | [3](../../round-3-devices.md) | "Chat in the editor": pairing steps, device list, revoke, Run on |
| `mcp.html` | [3](../../round-3-devices.md) | "Use Forma from your AI app": the layout only; the per-app step text is a placeholder |

Open one directly in a browser. Each has a review rail outside the frame that switches states. They load
`assets/geist-variable.ttf` and `assets/geist-mono-variable.ttf`, which are not copied here; without them the page
falls back to a system font. The real app's fonts are in `apps/web/src/fonts/`.

They are prototypes, not code to port: plain HTML and inline script, with fake data. Take layout, wording,
states and spacing from them; take behaviour and data from the app.
