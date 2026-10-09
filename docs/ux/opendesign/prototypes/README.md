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
| `mcp-v2.html` | [mcp-host-instructions](../../mcp-host-instructions.md) | "Use Forma from your AI app", second version, approved by the owner on 2026-10-08: two-column picker, inline code per step, token instructions. The step text is real; one fix applies when building: VS Code's token example must use the portable `.mcp.json` format (`mcpServers` + `inputs`), not `servers`. Its official guide links are the research's |
| `projects.html` | — | The home page: projects sidebar, design cards with page-1 thumbnails and last editor, Brand Kit row, new design with format or template, image requests per design, inline delete for a design (on its card) and a project (type the name), server unreachable. Approved by the owner on 2026-10-08. Built in pages batch 2 |
| `signin.html` | — | Sign-in and first visit: email link with no password, arriving from an AI app, check your inbox with resend wait, sending failed, invited (create my workspace), not invited, expired or used link. Approved by the owner on 2026-10-08. Built in pages batch 2 (A) |
| `consent.html` | — | Approving an AI app's connection: what it can and cannot do, the unverified name and the return address to check, sending, allowed, denied, cannot be shown. Approved by the owner on 2026-10-08. Built in pages batch 2 (A) |
| `panels.html` | — | The editor's side panels on the Calm layout: Brand (eight colours, type, logo, rules, voice; save only when changed), Images (agent requests first, then the library), Export (pages, 1×/2×/4× with pixels, export anyway, one PNG per page) and Checks (plain messages, error and warning dots, technical detail folded). Approved by the owner on 2026-10-08. Built in pages batch 2 |

Open one directly in a browser. Each has a review rail outside the frame that switches states. They load
`assets/geist-variable.ttf` and `assets/geist-mono-variable.ttf`, which are not copied here; without them the page
falls back to a system font. The real app's fonts are in `apps/web/src/fonts/`.

They are prototypes, not code to port: plain HTML and inline script, with fake data. Take layout, wording,
states and spacing from them; take behaviour and data from the app.
