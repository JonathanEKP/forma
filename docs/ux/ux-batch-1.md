# UX batch 1: building what rounds 1 to 3 decided

Agreed with the owner on 2026-10-06, after the three UX rounds closed. This page is the **specification** the Codex
workers implement from, so they implement rather than decide. If something here turns out to be wrong against the
code, or needs a change in Pontis, the worker stops and asks the coordinator rather than improvising.

The decisions themselves are in the round documents, and they win over this page where the two disagree:

- [round 1: the approval card and the Calm editor](round-1-approval-card.md), *Decisions*;
- [round 2: the chat](round-2-chat.md), *Decisions*;
- [round 3: devices and the MCP page](round-3-devices.md), *Decisions*, plus the two answers below.

The visual guide is the four prototypes in [opendesign/prototypes/](opendesign/prototypes/README.md). Take layout,
wording, states and spacing from them; take behaviour and data from the app. The design system is
[forma-editor-DESIGN.md](opendesign/forma-editor-DESIGN.md).

## Scope

In: every decision of rounds 1 to 3.

Out, deliberately:

- **Everything waiting on Pontis:** the visual-result slot (CR-2.3), anything that masks the five-minute session death
  (F-30-5, F-30-10), and the countdown after the person has answered (round 1 decision 8, upstream item 11). The card
  keeps today's behaviour there: the countdown is hidden while sending.
- **"Resume an earlier conversation"** (round 2, *Later*).
- The Pontis repository, production, the real Supabase project, Phase 12.

**Owner add-on (2026-10-07):** Task P implements the researched per-app MCP steps as eight app tabs; its app order,
OAuth-first paths, and token rules are recorded in [Task P](#task-p-per-app-mcp-setup-owner-2026-10-07).

## Decisions taken for this batch (owner, 2026-10-06)

| # | Decision |
|---|---|
| U-1 | **The default computer is the last one the person chose in this browser.** It is remembered in `localStorage` (the device id only). With exactly one computer online, that one is used and nothing is asked. With two or more online and the remembered one not among them, or nothing remembered, the chat asks: "Run on" is open and Send is disabled with the reason "Choose a computer to run the agent". |
| U-2 | **Changing "Run on" during a conversation starts a new session on the chosen computer.** An agent lives on one computer and is not moved. The old transcript stays above a divider that names the computer: "New session on MacBook Air · 14:02". There is no confirmation, as with New session. |
| U-3 | **A chosen computer that goes offline during a session** gives the round 2 "stopped" notice with New session. The chat never switches computers by itself, and the sidecar never routes a session's input to a different device. |
| U-4 | **All workers are Codex** (`gpt-6-luna`, extra-high effort). |

Coordinator decisions inside the scope (low-risk, not the owner's):

| # | Decision |
|---|---|
| C-1 | **Approval chips are built in the editor; the sentence in the sidecar.** The sidecar writes the summary and reason with names (round 1 decision 1), because that sentence is what Pontis stores and shows on any surface. The chips, the "what disappears" count and "Show on canvas" need the design's structure, which the editor reads through the REST API as the person, from the approval's operations. If the approval reaching the editor does not carry the operations, stop and ask. |
| C-2 | **D-2's refusal is the sidecar's.** A `ui.*` action whose invoking session has no connected window bound to it is answered `ui_unavailable`, even when exactly one other window is connected (Pontis's rule 2). Find the seam (the UI capabilities' `authorize`, or the registration) in `ui.ts` / `sidecar.ts`; if no Forma-side seam can see the invoking session, stop and ask. |
| C-3 | **`ambiguous_device` is retired.** The sidecar answers `409 device_choice_required` when a `session.start` names no device and more than one is online, and `409 device_offline` when the named or the session's device is not online. `session.input`, `session.cancel` and approval answers go to the session's own device, checked with `authoritativeSessionContext`, never to "the online one". |
| C-4 | **The calm layout replaces the old one; there is no toggle.** Every existing test that drives the editor is updated to the new layout rather than kept on a hidden old one. |
| C-5 | **No modals in the product UI.** No `<dialog>`, no `aria-modal`, no backdrop. Flyouts, popovers and inline confirmations only (round 2). |
| C-6 | **The prototypes' review rails and fake data are not built.** |

## Tasks

Six tasks. S1 and S2 are the sidecar, W1 to W4 the web app. Each owns its files; where two must touch the same file,
the line says so and the merge order settles it.

### S1: the sidecar's words (approval sentence, project in the view line)

Owns `apps/pontis-sidecar/src/capabilities.ts` and its tests; in `intents.ts`, only `parseView`, `viewLine` and the
name reads; `guidance.ts` if the agent's guidance needs a line.

1. **Round 1 decision 1:** `requiresApproval` writes the summary and reason with names, read through Boundary F as
   the person, with a bounded timeout. It never blocks the approval on the read.
   - The sentence forms are in round 1 decision 1: one layer, one page with its layers, and several layers across
     pages.
   - The fallback when the read fails is "Delete 1 layer from “…”" (or "from this design"), followed by "Forma
     couldn't load the layer's name. Details shows its id."
   - `affectedResourceCount` is what disappears: pages, plus the layers on them, plus loose layers.
   - Names are untrusted text: quote them, cap their length, and keep the sentence plain text.
2. **F-chat-17:** with a project and no design, the editor's view carries the project id, and the line becomes
   `[forma-view] project=… name="…"`, with the name read through Boundary F. W2 sends the project id; S1 accepts and
   writes it. The rules of the existing line hold: UUIDs only, the name only through Boundary F, and the person's text
   always below the line.

### S2: the sidecar's routing (choose the computer, D-2 refusal)

Owns the routing part of `intents.ts` (from `const online = …` to the delivery), `ui.ts`, `windows.ts`,
`sidecar.ts`, `ui.test.ts`, and the routing tests.

1. **Round 3 option (a), U-1 to U-3, C-3.** `session.start` takes an optional `deviceId`, kept only if it is one of
   this person's own devices, not revoked, and online. Inputs, cancels and approval answers go to the session's
   device. The errors are as in C-3. The editor learns which devices are online from the presence the Devices route
   and UI stream already carry; add nothing the browser could use to learn about another person's devices.
2. **D-2, C-2.** The other window does not move and shows nothing. The agent receives `ui_unavailable`. Extend
   `ui.test.ts`: the bound window closes, one other window of the same person stays open, and the action is refused
   and never `handled`.
3. Update the guidance only if the agent needs to tell "I can't reach your window" apart from other refusals.

### W1: the Calm editor

Owns `apps/web/src/editor/*`, the editor's part of `App.tsx`, and `index.css`. It does not edit `apps/web/src/pontis/*`;
it mounts the chat through the props `Editor` already has.

Round 1, *The editor: adopt Calm*, as in `index.html`'s Calm layout and `chat.html`:

- **Top bar:** the "f." mark, "Projects /" breadcrumb, design name, quiet version tag; Undo/Redo icons, a validation
  chip ("✓ Valid" / "2 issues") that opens a popover, and Export as the one primary action. With no design open, Undo,
  Redo, the chip and Export are hidden (round 2).
- **Left:** a slim icon rail with tooltips (Layers & pages, Assets, Brand). One flyout at a time, closed by ×, by its
  icon again, or with Escape. Pages are a switcher on top of the layers. There is exactly one active rail item.
- **Canvas:** regions separated by tone, not borders. Properties appears only while something is selected. Zoom is a
  floating control at the bottom-right of the visible canvas showing the real percentage, and is stable for the same
  artboard and window: it does not move after an agent write or an approval.
- **The agent:** collapsed, a floating "Ask the agent…" composer at the bottom-centre of the canvas. Focusing it
  expands the chat docked on the right, with the chat's composer focused. Expanded, the chat sits in the dock. W1
  builds the slot and the collapsed composer, and W2 builds what is inside the chat.
- The Geist faces as the design system declares them, and no modals (C-5).

### W2: the chat

Owns `apps/web/src/pontis/` except `ChatApproval.tsx`, `devices*` and `channel.ts`. It sends the project id in the
view for S1, and builds the Run on selector against S2.

Round 2's decisions, as in `chat.html`:

1. **Formatted text,** the subset of recommendation 1, parsed into React nodes and never through `innerHTML`. Hostile
   markup shows literally. Links are `http(s)` only, open in a new tab with `rel="noopener noreferrer"`, and show
   their host.
2. **UI action cards** with the four headings by outcome, names in the person's words, and ids only inside Details.
   Long text wraps, and the transcript has no horizontal scroll. A schema refusal is a quiet card, never JSON.
3. **Session notices:** one component in three tones (info, needs you, stopped), each with what happened, what to do,
   and one action. The strings in `chat.ts` are rewritten into that shape; the `ambiguous_device` string goes (C-3),
   replaced by U-1 to U-3's states. Text no agent said is never voiced as "Forma Agent".
4. **New session** starts at once, with a divider (`New session · hh:mm`, or U-2's form naming the computer) and the
   composer re-enabled and focused.
5. **Context chip:** design, page and layer; `Project “…”` on a project page; nothing on the projects list.
6. **Controls:**
   - Send is disabled while empty, and controls say why they are unavailable (round 1 decision 7).
   - The transcript follows the newest entry, with "Jump to latest" when the person has scrolled up. The prototype
     could not verify this, so test it here in a real browser.
7. **Session info** is a small popover (device, started, "This window's session"), with no backdrop.
8. **Run on** (round 3 option a, U-1 to U-3):
   - The selector sits in the composer's footer while two or more computers are online. It opens a popover listing
     the online computers with their presence.
   - The choice is remembered in `localStorage`, and changing it mid-session starts a new session on the chosen
     computer.
   - The Devices page's link "choose which one runs the agent in the chat" is W4's.

### W3: the approval card

Owns `apps/web/src/pontis/ChatApproval.tsx` and the approval parts of `chat.ts` and `Chat.tsx` (rebases on W2). It
touches W1's canvas and layers only to draw the highlight (rebases on W1).

Round 1, *The approval card*, decisions 2 to 7, as in `index.html`:

- the card's content in order, with chips, "+N more" and page suffixes;
- **Show on canvas** as a highlight toggle, not a selection, with the affected layers read-only while pending;
- the countdown from `expiresAt`, hidden while sending and urgent under 30 s;
- when the panel is closed, the Calm bubble above the collapsed composer and the tab title "(1) Approval needed ·
  Forma";
- every state, including repeat-after-deny ("You declined this earlier in this session, so it wasn't asked again.");
- the agent's one-line confirmation after an approved deletion is guidance (S1's file). If it needs a guidance line,
  W3 writes it after S1 merges.

### W4: the two connection pages

Owns `apps/web/src/Agents.tsx` (split as it needs), `apps/web/src/pontis/devices*`, and the `/agents` routes in
`App.tsx` (a small overlap with W1, settled by the merge order).

Round 3, as in `devices.html` and `mcp.html`:

- **"Chat in the editor":**
  - the intro sentence;
  - the three numbered steps with the states "Waiting for the pairing…", then "Paired. Waiting for it to start…",
    then "Online", plus "Expired" with "Create a new code";
  - the name field that goes into the command (`pairCommand` no longer hardcodes "My laptop"; the name is escaped);
  - the relative expiry, ticking;
  - the steps fold into the list once a computer is paired;
  - the list, with live devices first, the provider summary with its details on expand, and "Revoked (n)" collapsed;
  - specific accessible names, and the inline revoke with focus on Keep and Escape meaning Keep;
  - the option (a) sentence about several computers.
  - Rename and remove are shown only if Pontis offers them. If it does not, record that as an upstream request in
    [pontis-upstream-report.md](../pontis-upstream-report.md) and build neither.
- **"Use Forma from your AI app (MCP)":**
  - the intro for someone who has never heard of MCP;
  - "Your MCP URL" with Copy;
  - "Pick your app" tabs with concise per-host steps from [the MCP host research](mcp-host-instructions.md);
  - connected applications with an inline Disconnect;
  - personal access tokens, with a new token shown **in full**, once, with Copy.
- **Navigation:** a two-item switcher, "Chat in the editor" / "AI apps (MCP)", with the current page marked. These are
  real routes, not anchors styled as tabs. Each page links to the other in one line.

## Merge order

**S1 → S2 → W4 → W1 → W2 → W3.** All but W3 start at once. W3 starts when W1 and S1 have merged. Each later task
rebases on `main` before `worker_done`. After W3 merges, one Codex worker re-tests the whole batch in the browser (T),
on the browser stack, against the round documents.

## Common rules for every worker

- **Read first:** `CLAUDE.md`, this page, the round documents your task implements, the prototype it names, the §11
  *Progress* table in [pontis-integration-plan.md](../pontis-integration-plan.md), and
  `apps/pontis-sidecar/README.md` for S1 and S2.
- **Tests:** write the test first, then the change, in small commits. Record your task as a sub-row "UX batch 1 · …"
  in §11 *Progress*.
- **Gates before `worker_done`:**
  - `pnpm lint` (max warnings 0), typecheck, and the web and sidecar suites;
  - rebuild `apps/pontis-sidecar/test-driver` first if its source changed;
  - if Rust is touched: `cargo test --workspace`, Clippy with `-D warnings`, and `scripts/check-bindings.sh`.
- **Browser proof:** W1 to W4 show their screens on their own browser-stack instance
  ([pontis-browser-testing.md](../pontis-browser-testing.md)), in their own Orca tab, on FakeProvider, and stop the
  instance afterwards. Compare against the prototype by screenshot, and keep screenshots out of the repo unless they
  are evidence.
- **Real Codex turns** only where the model's behaviour is what is checked: at most 2 per task, and about 6 for T.
- **Never:**
  - log or commit tokens, auth headers, credentials, asset bytes or `~/.codex/auth.json` content;
  - touch production, the real Supabase project, `~/.codex/config.toml`, the global `codex`, or anything under
    `/home/vertex/projects/pontis`;
  - expose the Runtime's routes (`/pons/v1/pairing/claim`, `/pons/v1/device/*`) through the proxy;
  - stop processes you did not start.
- **No `any`,** and no new dependency for markdown or UI without asking. A small parser for the formatting subset is
  expected to be Forma's own.

## Task P: per-app MCP setup (owner, 2026-10-07)

- Show Claude, ChatGPT, Claude Code, Codex CLI, Gemini CLI, OpenCode, Cursor, and VS Code (GitHub Copilot), in that
  order. Do not include Windsurf.
- Use one row of app tabs and one active panel; Claude is selected initially. Follow the accessible tabs pattern with
  arrow keys, Home/End, visible focus, and wrapping on narrow screens.
- Lead with browser OAuth wherever supported. Offer “Use a token instead” only for apps that accept a PAT. Do not
  offer it for ChatGPT; Gemini CLI's token command saves the expanded bearer token in Gemini's config. Store PATs in
  `FORMA_TOKEN` where the host documents an environment reference; VS Code's documented password prompt is the
  secure equivalent.
- Each panel uses a concise plan/admin requirement, 2–4 numbered steps, the live MCP URL in any snippet, a Copy
  button for each command/config block, and an official vendor guide. Token instructions link to the existing
  Personal access tokens section on the page.
