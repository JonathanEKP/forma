# Testing the Pontis → Forma connection in a real browser

`apps/pontis-sidecar/src/browser-stack.ts` brings up one **isolated instance** of the whole
chain — forma-server, the sidecar, the editor on Vite, and a paired Runtime — so a person (or an
agent driving a browser, such as Orca's) can use the editor the way a user would and try to break
it. Several instances run side by side; the instance number decides every port.

```text
browser ── Vite (apps/web) ─┬─ /api ──────────▶ forma-server  (hosted mode, fixture identity)
   │                        └─ /pons, /agent ─▶ sidecar ── device socket ──▶ test-driver (Runtime)
   │                                                                         FakeProvider or real Codex
   └─ supabase-js ── session refresh ──▶ fake identity provider  (the fixture JWKS; nothing else)
```

It is the same chain as [`local-run.test.ts`](../apps/pontis-sidecar/src/local-run.test.ts), where
the person is code and the browser is absent; here the person is at the editor. Product code is not
forked: the editor, the sidecar, forma-server and the driver are the real ones.

## What is real, what is fake

| | |
|---|---|
| forma-server | the real binary, `FORMA_AUTH_MODE=supabase`, a temp data dir per instance |
| Identity | **fake**: a local server publishing `fixtures/identity/test-jwks.json`, answering supabase-js's refresh/user/logout calls. Tokens are signed with the committed, public test key. There is no sign-in route: a browser gets a session only through the snippet below |
| Sidecar | the real `src/main.ts`, with a service credential minted for the instance |
| Editor | the real `apps/web` on Vite, with `/api` and the Pontis paths proxied to *this* instance |
| Runtime | the Rust `test-driver` (public Pontis crates), one per paired person, FakeProvider or the real Codex |

**Nothing contacts a real Supabase project.** Vite is started with `VITE_SUPABASE_URL` /
`VITE_SUPABASE_PUBLISHABLE_KEY` in its process environment, which outranks `apps/web/.env`
(measured: with an `.env` pointing at a canary host the editor still got the fake URL). After
Vite is up the script fetches the editor's own `identity.ts` as served and **refuses to continue**
unless it names the fake provider and no `supabase.co` host. The browser stack's identity log
(`logs/identity.log`) lists every request the editor made to it, and the editor's network tab
shows only `127.0.0.1` / `localhost` hosts.

## Prerequisites

Once per worktree:

```bash
pnpm install
# the Pontis seam: link the checkout as the sidecar README says
cargo build -p forma-server
cargo build --manifest-path apps/pontis-sidecar/test-driver/Cargo.toml
```

For real-Codex mode, once per machine (idempotent, safe to re-run):

```bash
node apps/pontis-sidecar/src/browser-stack.ts setup-codex
```

It installs Codex **0.158.0** (the release Pontis accepts for `capability_only`) into
`<main repo>/.codex-pinned` — the *main* checkout, shared by every worktree, gitignored — with a
dedicated `CODEX_HOME` there (`model = "gpt-6-luna"`, `model_reasoning_effort = "low"`),
`auth.json` **symlinked** to `~/.codex/auth.json` (never copied or read), and builds
`pontis-mcp-bridge` from the Pontis checkout into the driver's own target directory. The global
`codex` and `~/.codex/config.toml` are never touched. `FORMA_CODEX_PINNED` moves the install.
**Real Codex spends the owner's tokens**, but only when a message is sent.

## Starting an instance

```bash
node apps/pontis-sidecar/src/browser-stack.ts start 1                          # FakeProvider, person A paired
node apps/pontis-sidecar/src/browser-stack.ts start 1 --provider codex         # real Codex for A
node apps/pontis-sidecar/src/browser-stack.ts start 2 --provider-a codex --provider-b fake --devices a,b
# or: pnpm --filter forma-pontis-sidecar browser-stack start 1
```

It stays in the foreground, prints a summary and `READY <editor url>`, and stops on Ctrl-C / SIGTERM.
To background it: `… start 1 > /tmp/stack1.out 2>&1 &` and wait for the `READY` line. (Do not wrap it
in `timeout`: that is a stop signal.)

| Flag | |
|---|---|
| `--provider fake\|codex` | the Runtime's provider for every paired person (default `fake`) |
| `--provider-a …`, `--provider-b …` | per person |
| `--devices a`, `b`, `a,b`, `none` | whose Runtime to pair (default `a`) |

### Ports

Instance `N` (0–99) owns `4800 + 10N … 4800 + 10N + 9`:

| | |
|---|---|
| `+0` | forma-server |
| `+1` | sidecar |
| `+2` | editor (Vite) |
| `+3` | fake identity provider |

So instance 1 is 4810–4813 (editor `http://127.0.0.1:4812`), instance 2 is 4820–4823. The owner's own
servers on 4765/4766/4767/5173 are never touched. A start fails with a clear message, and starts
nothing, if any of the four ports is taken.

State lives in `/tmp/forma-browser-stack/N/` and is **wiped on the next start of that number**:
`forma/` (database and assets), `logs/`, `state.json` (ports, ids, pids — no credential),
`signin-a.js`, `signin-b.js`.

### What is seeded

Two people in **two different accounts** (A owns the bootstrap account; B is invited and creates
their own through `POST /api/v1/accounts`), each with a project, a Brand Kit, and two designs with
text and shape layers. Texts name their owner (`ALPHA …`, `BRAVO …`) so a leak across accounts is
visible on sight. In FakeProvider mode each person also gets a 120-page design for the cancel
script. Ids are printed in the summary and kept in `state.json`.

## Signing the browser in

The editor opens the Pontis channel and the chat only when it holds a Supabase session
(`access.at === "ready"`). Mint one for A or B:

```bash
node apps/pontis-sidecar/src/browser-stack.ts signin 1 a        # prints a one-line JS expression
orca tab create --url http://127.0.0.1:4812 --json              # note the browserPageId
orca eval --page <id> --expression "$(cat /tmp/forma-browser-stack/1/signin-a.js)" --json
```

The expression writes the supabase-js session into `localStorage` (`sb-127-auth-token`) and reloads.
It must run on a page **at the editor's origin**, so open the editor first.

- The session lasts six hours. supabase-js refreshes against the fake provider when it nears expiry
  (verified: an aged session was renewed and the person stayed signed in).
- The subjects are fixed per instance number, so a signed-in tab survives a restart of the same
  instance (the data is wiped, the identities are not). `signin` mints a fresh token.
- **One browser origin holds one identity.** To have A and B signed in at once use the two names the
  sidecar allows for the same server: A on `http://127.0.0.1:<editor>`, B on
  `http://localhost:<editor>`.
- The editor's magic-link sign-in screen cannot work (it would need a mail provider). After a
  sign-out, use the snippet again.

### Driving the editor with Orca

`orca skills get orca-cli --reference references/browser.md` is the reference. Two things learned
building this: `orca fill` works on the chat box; and `orca click` sometimes does nothing on the
first try after a navigation, so when a click does not take, click through the DOM instead:

```bash
orca eval --page <id> --expression "(() => { const b=[...document.querySelectorAll('button')].find(x=>x.textContent.includes('ALPHA project')); b.click(); return 'ok'; })()" --json
```

## The Runtime, and the chat

A paired driver is a Device that the sidecar routes the chat to. The chat starts a session, sends
input, cancels and answers approvals through the same `/agent/intent` route, so **the browser's chat
is the interactive path** — the driver needs nothing extra. The Agents page (`/agents`) lists the
device and its provider.

**One online device per person.** The sidecar answers `ambiguous_device` when a person has two;
the script pairs one per person.

### FakeProvider

Scripted: it answers only these exact inputs (`test-driver/src/main.rs`), anything else ends the
turn with no answer. Free, deterministic, for testing the plumbing and the UI.

| Input | What it does |
|---|---|
| `Which projects do I have, and what is in my design?` | `project.list`, `design.get` |
| `Read the manual first.` | `forma.guide` |
| `Read my brand, and ask me for a hero photo.` | `brand.get`, `assetRequest.create` |
| `Raise the headline to 96pt.` | one `patch_text_style` (works once: the script names version 1) |
| `Validate the long design.` | validates the 120-page design (slow on purpose: cancel it) |
| `Remove the tagline. I will say no.` | approval card; the script expects a refusal |
| `Remove the headline.` | approval card; the script expects consent |

### Real Codex

`--provider codex`: Codex 0.158.0 under `capability_only`, `gpt-6-luna`, effort `low`, answering
anything. Each chat session spawns its own App Server. On a brand-new `CODEX_HOME` the first
session's `thread/start` times out; the script therefore opens one free warm-up session when it sees
a cold home (no prompt is sent). Concurrent instances share the one `CODEX_HOME`.

The editor shows Codex as `codex · unknown` with "Sign-in not verified": Pontis's Codex probe cannot
report authentication, and the chat treats *cannot be known* as usable (see
[the findings](pontis-browser-findings/00-harness.md)).

## Stopping

Ctrl-C in the foreground, or `node apps/pontis-sidecar/src/browser-stack.ts stop N`. Children run in
their own process groups and the whole group is stopped (SIGTERM, then SIGKILL after 5 s); the driver
also exits when its stdin closes. If the harness itself was killed with SIGKILL, `stop N` stops the
recorded process groups through `state.json` (not exercised yet). Logs stay in `/tmp/forma-browser-stack/N/logs/` until the next start of
that number: `forma-server`, `sidecar`, `vite`, `identity`, `driver-a`, `driver-b`.

## Known limitations

- Magic-link sign-in, the OAuth consent page and OAuth grants are not faked (the grants list answers
  empty). Everything that needs *a* session works; flows that need Supabase itself do not.
- The editor is Vite's dev server (HMR, source maps, unbundled modules), not the built app.
- Two accounts only, fixed shape of seed data. `POST /api/v1/projects` etc. let a tester make more.
- FakeProvider writes are one-shot (`base_version` is fixed in the script).
- Concurrent Codex sessions share one `CODEX_HOME` and one `~/.codex/auth.json`; heavy parallel use
  could surface contention that production (one device per person) would not.
- A restart wipes the instance's data. Keep a note of ids from `state.json` if a finding needs them.
- It only runs where `target/debug` binaries and the Pontis checkout exist; CI never runs it.

Findings go in [`pontis-browser-findings/`](pontis-browser-findings/README.md).
