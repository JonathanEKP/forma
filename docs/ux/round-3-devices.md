# UX round 3: devices and pairing

**2026-10-06.** Third UX round, after [round 2](round-2-chat.md). The subject is the **Devices** section of the Agents
page (`apps/web/src/Agents.tsx`, `apps/web/src/pontis/devices-panel.tsx`): where a person pairs the machine that runs
their agent for the in-editor chat. Prototyped in OpenDesign, in a new conversation of the same project and in a new
file, `devices.html`, against the [Forma editor design system](opendesign/forma-editor-DESIGN.md). **Decisions are
recorded here.**

## The problems (browser findings, [10-pairing-devices.md](pontis-browser-findings/10-pairing-devices.md))

- **F-10-5:** the pairing instructions stop one step early and use words a first-time person does not have.
  - The page never says what a device is *for*: chatting with an agent inside the editor, as opposed to the MCP URL
    above it.
  - "Pontis" is never explained or linked, and is unpublished.
  - "daemon stopped" is jargon.
  - After `pontis pair`, the device stays offline until `pontis run` is started, and the page never says so.
  - The copied command names every machine "My laptop".
  - "expires in minutes" sits next to an absolute UTC time, and the real figure is 10 minutes.
  - Revoke does not say what happens to a running turn, nor that the machine needs `pontis unpair` before it can pair
    again.
- **F-10-6:** pairing a second machine silently breaks the chat. With two devices online the sidecar refuses with
  `ambiguous_device` (F-00-6, one online device per person by design). The page invites pairing a second one and never
  says so, and the chat offers no way to choose.
- **F-10-9:** the list never prunes and cannot tell two machines apart.
  - Every pairing stays a card forever (13 after forty minutes of testing), with revoked and live ones interleaved.
  - A card shows no paired-at date and no id, and cannot be renamed or removed.
  - Every Revoke button's accessible name is the same.
- Related, already fixed but shaping the page:
  - F-10-3: the panel now refreshes.
  - F-10-7: "last seen" is the last connect or use, not the last sign of life.
  - Provider rows show raw values (`not_applicable`, `test_only`, `execution in_process`).

## Starting recommendations (to be validated visually)

1. **Say what it is for, first.** One sentence: "Pair a computer to chat with an agent inside the editor. The agent
   runs on your computer, with your own subscription." Then a plain choice between the two ways in: chat in the editor
   (Devices) or use Forma from your AI app (the MCP URL).
2. **Pairing as three numbered steps**, with a state that follows the device:
   1. Install Pontis. The link and source are a placeholder until Pontis is published.
   2. Run the pairing command. It is copy-safe and carries the name the person typed in a field first.
   3. Start it with `pontis run`.

   The panel shows "Waiting for the pairing…", then "Paired, waiting for it to start…", then "Online". Before the
   code is used, the expiry is relative ("expires in 9:41").
3. **One device at a time, said up front, with a choice when it happens (F-10-6).** Two options to compare:
   - **(a) Choose in the chat.** With two online, the chat shows "Run on: [Laptop ▾]" and the sidecar routes to the
     chosen device. This changes F-00-6's rule from "exactly one online" to "exactly one chosen", and is a sidecar
     change.
   - **(b) Keep the rule and warn.** The page says "Only one computer can run your agent at a time" before creating a
     second code. The chat's notice names both machines and says which to stop.
4. **A list that reads.**
   - Live devices come first, each with name, platform, "Paired 4 Oct", and presence. "Online" carries no time;
     "Offline since 02:41", or just "Offline" when that time is not known, given F-10-7.
   - Providers are summarised in one line ("Codex 0.42 · signed in · sandboxed") with details on expand.
   - Revoked devices collapse into a "Revoked (12)" group, hidden by default.
   - Revoke says what happens: "The agent stops at once, any running turn ends, and that computer needs `pontis unpair`
     before it can pair again."
   - Each Revoke button names its device by name and pairing date.
   - Rename and remove are shown only if Pontis can do them. Otherwise they are recorded as an upstream request.

## Decisions

*(filled in as the round is reviewed)*
