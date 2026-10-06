# UX batch 1 browser re-test

**Run:** 2026-10-06, local browser stack instance 25, own Orca browser tabs, FakeProvider. The app was served from this worktree at `127.0.0.1:5052`; the checked screens were the seeded ALPHA project and its cover design. No real Codex turns were run: FakeProvider supplied fixed replies and could not exercise model guidance or `ui_unavailable`. The stack and the test driver started for this run were stopped after the checks. No product code was changed and no lint/typecheck/test gates were run for this browser-only report.

## Checklist

Each row is one decision or implementation item. Counts below include all 39 rows.

### Round 1 — approval card and Calm editor

| Check | Status | How checked |
|---|---|---|
| R1 decision 1 — named approval sentence | pass | In the editor, requested removal of Tagline; the pending card said `Delete the layer “Tagline” from “ALPHA Launch cover”`. The design view line was also visible in the FakeProvider test-driver log. |
| R1 decision 2 — card content and order | pass | Inspected a one-layer pending card: approval/risk/deadline, sentence and count, affected layer, Show on canvas, Details, then Approve once and Deny. Multi-page and overflow counts were not scripted. |
| R1 decision 3 — Show on canvas highlights without selecting | pass | Used Show on canvas; Signal outline appeared, no selection or Properties panel opened, and the affected layer was read-only until settlement. |
| R1 decision 4 — countdown and closed-panel signal | not testable here | Saw the countdown and urgent color below 30 seconds. I did not capture the closed-panel bubble and tab title together on this run. |
| R1 decision 5 — approval lifecycle states | not testable here | Pending, approved, denied, repeat-after-deny and expiry were exercised. FakeProvider did not expose separate sending, confirmation-timeout or cancellation cases. |
| R1 decision 6 — post-approval agent guidance | not testable here | After approval, FakeProvider returned its fixed reply, “The headline is gone.” It cannot establish whether a real model follows the required kept/Undo/validation guidance. |
| R1 decision 7 — unavailable control reasons | pass | Verified Send disabled when empty and the working-state explanation “The agent is working. Stop it to send now.” Stop was available during the turn. |
| R1 decision 8 — countdown after answering | not testable here | The decision is explicitly left to the Pontis resolve timeout; this browser run did not control or instrument that upstream wait. |
| Calm editor, rail tooltips and stable zoom | pass | Compared the editor at 1280×900 and 390×844; rail buttons expose tooltips, and zoom stayed at 30% through an approved deletion and Undo. |

### Round 2 — chat

| Check | Status | How checked |
|---|---|---|
| R2 decision 1 — D-2 refusal with `ui_unavailable` | not testable here | This needs an agent to invoke a UI action after its bound window closes. FakeProvider had no such scripted action and no real Codex turn was used. |
| R2 decision 2 — safe formatted text and hostile markup | not testable here | No FakeProvider fixture returned Markdown or HTML-like agent text. The hostile pairing name was rendered literally, but that does not cover the chat formatter. |
| R2 decision 3 — outcome cards, schema refusal and long action text | not testable here | No UI-action or schema-refusal fixture was available. A 4,000-character composer draft was checked separately; it stayed in the textarea with internal scrolling and no horizontal page overflow. |
| R2 decision 4 — session notices and New session | pass | The fixture-error needs-you state and the revoke/offline stopped notices each stated what happened and what to do; stopped states disabled the composer. New session started immediately with a divider and an enabled composer. |
| R2 decision 5 — read-only context chip above composer | fail | With a design open, the design/page context appeared on each sent message, but there was no current-context chip above the composer before sending. See UX1-2. |
| R2 decision 6 — composer and editor chrome controls | pass | Empty Send was disabled with its reason; the no-design Agents pages did not show design-only Undo/Redo, validation or Export controls. |
| R2 decision 7 — reserved visual-result slot | not testable here | No visual result was emitted by the scripted provider, so a reserved transcript slot could not be distinguished from an unbuilt one. |
| R2 decision 8 — no modals; flyouts and session-info popover | fail | No `<dialog>` or `aria-modal` UI appeared; the rail flyout closed with Escape. The session-info popover did not close on Escape or outside click (UX1-1). |
| R2 decision 9 — transcript follows newest entry | fail | Started at the bottom of the active transcript, then let a FakeProvider reply arrive without scrolling. The transcript remained above the bottom and showed Jump to latest; see UX1-3. |

### Round 3 — devices and MCP

| Check | Status | How checked |
|---|---|---|
| R3 decision 1 — choose among online computers (option a) | pass | With two online FakeProvider devices, Run on opened a chooser. Selecting the second began a new session under a named divider and its driver log received the design view. Duplicate test-driver labels limited visual identification. |
| R3 decision 2 — separate Chat and MCP pages | pass | Navigated `/agents` and `/agents/mcp`; both routes had the two-item navigation and a link back to Projects. |
| R3 decision 3 — MCP layout and researched instructions | pass | Compared `/agents/mcp` with `prototypes/mcp.html`: intro, MCP URL/copy, app cards, connected applications and tokens follow the same order. The app uses its actual host wording, not the prototype’s generic placeholder steps. |
| R3 decision 4 — new token shown in full once | pass | Created a disposable local test token; a DOM-only check confirmed the one-time card contained the full 74-character value without truncation and exposed Copy. I dismissed it, verified the full card did not reappear, then revoked the test token; the value was never printed or saved. |
| R3 decisions A/B — pairing steps, name and pairing states | pass | Opened the inline three-step pairing flow, used a hostile HTML-like computer name, checked the shell-quoted command and relative code expiry, and paired a second FakeProvider computer. The name appeared as text; no script ran. |
| R3 decisions C/D — device list and inline revoke | pass | Checked live-first ordering, platform/date/presence, provider summary/details, device-specific Revoke names, inline confirmation, Escape as Keep and the collapsed revoked group. The harness hardcoded the same name for both online test drivers. |

### UX batch 1 items

| Check | Status | How checked |
|---|---|---|
| U-1 — remember the chosen computer; auto-use a sole online device | pass | With two online, the app required a choice; the chosen device id was stored and selected. After revoking that device, the sole online computer was used without asking. |
| U-2 — switching computers starts a named new session | pass | Selected the second online computer during chat; the transcript stayed above `NEW SESSION ON FORMA LOCAL RUN`. |
| U-3 — offline computer stops without failover | pass | Killed the test-driver child of instance 25 during an approval. The turn stopped, its approval expired without a write, the notice named the offline computer, and no other device was selected. |
| C-1 — sidecar sentence, editor-built approval details | pass | The pending FakeProvider approval used the layer/design names and the editor showed the count, chip, highlight control and Details disclosure. |
| C-2 — Forma-side D-2 refusal | not testable here | Same constraint as R2 decision 1: no real agent UI action was available after closing its bound window. |
| C-3 — explicit device routing and errors | pass | Checked chooser-required behavior with two online devices, routing to the selected driver, and stopped state when its device was revoked or offline; there was no automatic failover. |
| C-4 — Calm replaces old layout | pass | Inspected the editor at desktop and phone widths; there was one Calm layout and no layout toggle. |
| C-5 — no product modals | pass | Checked the editor and connection pages: no dialog, `aria-modal` overlay or backdrop. The separate session-info dismissal issue is UX1-1. |
| C-6 — no prototype review rails or fake review controls | pass | The running app had no prototype scenario rail or review controls; only the app’s normal editor and connection pages were present. |

### Tasks S1 through W4

| Check | Status | How checked |
|---|---|---|
| S1 — approval wording and project-only view | not testable here | Named approval wording and the design view were observed. I did not send from a selected project with no design open, so the project-only view line remains unverified. |
| S2 — sidecar device routing and D-2 | pass | Selected device routing, session divider, revocation and offline stop were exercised through the browser; D-2 itself remains untestable here as noted above. |
| W1 — Calm editor | pass | Compared editor layout at 1280×900 and 390×844; no horizontal overflow, modal, or material layout divergence was observed. |
| W2 — chat | fail | The context chip was attached to sent messages instead of shown above the composer, and the transcript did not follow the newest entry (UX1-2, UX1-3). |
| W3 — approval card | pass | Exercised named destructive approval, highlight, read-only layer, urgency, deny/repeat, expiry, approval, version increment and Undo. Real-model confirmation wording remains untested. |
| W4 — connection pages | not testable here | Both routes, navigation, pairing, device list, provider details, revoke and the one-time token flow were checked. No OAuth grant fixture was available to exercise connected-app Disconnect. |

**Checklist totals:** 24 pass, 4 fail, 11 not testable here (39 rows). **Findings:** 0 high, 2 medium, 1 low, 0 info.

## First-time-user break tests

| Scenario | Result and how checked |
|---|---|
| Hostile computer name | pass — entered `Laptop '<img src=x onerror=alert(1)>`; the UI showed literal text and generated a quoted pair command; no script executed. |
| Long text | pass — filled the composer with 4,000 unbroken characters; the field scrolled internally, its width stayed within the composer, and the document stayed 1278px wide at a 1278px viewport. |
| Narrow and desktop windows | pass — set 390×844 mobile and 1280×900; no horizontal overflow. At phone width the canvas and chat stack vertically and the tall artboard continues below the fold. |
| Keyboard only | pass — entered a message, tabbed to Send and activated it with Enter; the FakeProvider turn completed. |
| Reduced motion | not testable here — the Orca browser controls exposed viewport/device emulation but no `prefers-reduced-motion` setting. |
| Reload mid-turn | pass — reloaded with an approval pending; the app returned to Projects with an approval count, and reopening the design showed the pending approval with its live countdown and Review action. No write occurred. |
| Two tabs | pass — opened the same project in a second tab. Each tab had its own transcript/session as specified; both showed the same design version and the browser-level device choice persisted. |
| Revoke while chatting | pass — revoked the computer selected for a pending approval from `/agents`; the chat stopped, marked the approval expired, kept the design, and offered a new session after pairing. |
| Computer goes offline mid-session | pass — terminated the test-driver process started under instance 25 during a pending approval; the chat stopped with an offline-specific notice and did not switch devices. The approval expired without a write. |

## Prototype comparison

Compared the running editor/chat, `/agents`, and `/agents/mcp` at 1280×900 with `prototypes/index.html`, `chat.html`, `devices.html`, and `mcp.html`. The prototype’s review rail, mock browser chrome, seeded artboard and scenario controls are not product UI and were absent from the app. The Calm editor, chat dock, connection-page hierarchy and dark palette otherwise aligned; the seeded app artboard is 1080×1920 while the approval prototype artboard is 1080×1440, so the canvas contents and fit differ. On the chat screen, the context-chip placement and transcript-follow behavior are material divergences recorded as UX1-2 and UX1-3. The devices prototype used distinct names/icons and one online state; the app’s cards were structurally aligned, while duplicate `Forma local run` labels came from the test-driver metadata and the offline timestamps were unavailable. The MCP page’s local URL and current host instructions differ from prototype examples as expected; no other material visual divergence was observed.

## Findings

### UX1-1 — low — session-info popover stays open

- **Steps:** Open Agent chat, open Session information, press Escape; repeat and click the canvas outside the popover.
- **Expected:** The small popover closes by Escape or outside click, with no backdrop.
- **Actual:** It stayed open for both actions; clicking Session information again closed it. No modal/backdrop was present.
- **Evidence:** [session info open](evidence/ux1/ux1-1-session-info-open.png)

### UX1-2 — medium — current context is missing above the composer

- **Steps:** Open a design and Agent chat; inspect the composer before sending, then send a message.
- **Expected:** A read-only design/page/layer context chip appears above the composer.
- **Actual:** No chip appeared above the composer. The sent message later carried “Sent with: ALPHA Launch cover, Cover” inside its message card.
- **Evidence:** [chat context](evidence/ux1/ux1-2-chat-context.png)

### UX1-3 — medium — transcript does not follow new entries from the bottom

- **Steps:** In the active app tab, scroll the transcript to its bottom, send a message, and let the FakeProvider response arrive without touching the transcript.
- **Expected:** The transcript follows the newest entry; Jump to latest appears only if the person had scrolled up.
- **Actual:** It stopped above the bottom with Jump to latest visible. At capture, `scrollTop=696`, `clientHeight=505`, `scrollHeight=1364`.
- **Evidence:** [transcript not following](evidence/ux1/ux1-3-transcript-not-following.png)
