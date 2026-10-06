Prototype **the Devices section of Forma's "Agent connections" page** as an interactive single-page prototype in a new file, `devices.html`. Do not change `index.html` or `chat.html`. Use the selected Forma Editor design system. Add a review rail outside the frame, as in the other files.

No modals anywhere: inline panels, popovers and inline confirmations only.

## Context

Forma's "Agent connections" page (a page, not the editor) offers two ways to use an AI agent with Forma:
- **Chat inside the editor.** The person pairs a computer; a small program called Pontis runs the agent there, with their own Codex or Claude subscription, and the editor's chat talks to it.
- **Use Forma from an AI app** (Claude, ChatGPT and others) by pasting Forma's MCP URL into that app.

Below those come "Connected applications" and "Personal access tokens", which stay as quiet context here.

Today the Devices section fails a first-time person:
- It never says what a device is for.
- It never says what Pontis is or where to get it.
- It uses "daemon".
- It stops after `pontis pair` and never says to run `pontis run`, so the computer stays offline and nothing explains why.
- The copied command names every machine "My laptop".
- The list keeps every revoked device forever, interleaved with live ones, and cannot tell two machines apart.
- Only one computer may be online at a time. With two online, the chat refuses with "More than one of your devices is online", and the page never warns about this.

## What to build (switchable from the review rail)

**A. First visit (no device).**
- A header line: "Pair a computer to chat with an agent inside the editor. The agent runs on your computer, with your own subscription."
- A short, plain comparison with the MCP option: "Prefer your AI app? Use the MCP URL below."
- Then pairing as three numbered steps:
  1. **Install Pontis on that computer.** A "How to install" link that goes nowhere yet, marked as a placeholder.
  2. **Name it and pair it.** A name field ("Studio desktop") and "Create pairing code". The code and the copy-safe command appear, with the typed name in the command: `pontis pair --origin … --application forma --name "Studio desktop" FWTC-XSRX-AJ4N`. The command wraps; there is no horizontal scrollbar. Below it: "Expires in 9:41", ticking, and "Works once".
  3. **Start it:** `pontis run`, with Copy.

**B. Pairing in progress.** The steps track the device:
- "Waiting for the pairing…" while the code is unused;
- "Paired. Waiting for it to start…" after `pontis pair`;
- "Online" once `pontis run` is running.

Include an "Expired" state with "Create a new code".

**C. A list that reads.** Three online and offline devices plus 12 revoked ones.
- Each live device is a row: name, platform ("macOS · arm64"), "Paired 4 Oct", and presence. "Online" carries no time; "Offline since 02:41" when known, otherwise just "Offline".
- Providers are summarised in one line ("Codex 0.42 · signed in · sandboxed"), with an expand for the details. The raw values (`not_applicable`, `test_only`, `execution in_process`) stay hidden there in Geist Mono.
- Revoked devices collapse into "Revoked (12)", hidden by default.
- Long names truncate with a tooltip.
- Every button's accessible name is specific: "Revoke Studio desktop, paired 4 Oct".

**D. Revoke.** An inline confirmation in the row, not a modal: "Revoke Studio desktop? The agent there stops at once and any running turn ends. To pair this computer again later, run `pontis unpair` on it first." Then Revoke and Keep.

**E. Two computers online.** This needs a decision from the owner, so show both options from the review rail, each on the Devices page *and* in a small inset of the editor's chat:
- **Option (a), choose in the chat.**
  - The page says: "You can pair several computers. When more than one is online, choose which one runs the agent in the chat."
  - The chat composer shows "Run on: Studio desktop ▾" only while two or more are online.
- **Option (b), one at a time.**
  - Before creating a code while a device is online, the page says inline: "Only one computer can run your agent at a time. If both are on, the chat will ask you to stop one."
  - The chat's notice names both ("Studio desktop and MacBook Air are both online. Stop Pontis on one of them.").

## Constraints

- Same as the other files:
  - Signal orange only for the primary action, focus, selection and urgent state.
  - Panel text is 12–13px; radius 8px; hairline warm borders.
  - Plain English in sentence case; internal ids only inside details.
  - Accessible names on every control.
  - Transitions ≤150ms that respect prefers-reduced-motion.
  - Thin, warm scrollbars.
- Switching a review option resets to that state's initial view.
