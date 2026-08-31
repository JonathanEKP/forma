# Plan status

[forma-plan.md](../forma-plan.md) is the spec of record and does not get
rewritten. This is the audit of what has actually been built against it, and the
agreed order for what is left.

It exists because a phase was closed with one of its own tasks undone, and
nobody noticed for three phases. Every claim below names the evidence, and
anything still open names the plan section that asks for it — so the backlog is
derived from the plan rather than from whoever last had an opinion.

**Rule adopted:** a phase is closed only when every task in its §77 list is
either built or explicitly deferred *here*, with a reason. "The acceptance test
passes" is not enough — Phase 6's acceptance passed with two panels missing.

## Phases

| Phase | Tasks | Status |
| --- | --- | --- |
| 0 — Repo + blocking spikes | 14 | Complete. §36 resolved to **Option A**: `<g id="layer-{uuid}">` survives usvg. |
| 1 — Domain core | 11 | Complete. |
| 2 — Renderer | 13 | Complete. |
| 3 — Validator | 8 | Complete. |
| 4 — Persistence | 8 | Complete. |
| 5 — REST server | 7 | Complete. |
| 6 — React editor | 14 | Complete. `Brand panel` closed the last task. |
| 7 — AssetRequest UX | 7 | Complete. Also delivered Phase 6's `assets panel`. |
| 8 — MCP | 7 | Complete. §48's thirteen tools, plus `list_templates` — see item 8 for why fourteen. |
| 9 — Agent skill | 15 | Complete. |
| 10 — Hardening | 9 | Complete. |
| 11 — Remote deploy | 9 | Complete. Deployed and accepted: `smoke-remote.py` reports 15/15 against `https://forma.hintly.online`. See item 9 below and [deploy/status.md](deploy/status.md). |
| 11.5 — Hosted identity | — | Slice 1 complete: account-scoped persistence and routes, admission/recovery CLI, local auth context, and per-request MCP credentials. Slice 2 is not started. |
| 12 — Supabase adapter | 5 | Not started. The plan marks it optional and only after 11. |

## Open, with the section that asks for it

### Nothing from the plan is open

§94's three decisions are all closed now: 93.1 resolved to Option A in Phase 0,
93.3's pixel-diff threshold was measured in Phase 10 rather than invented, and
93.2 — the default font — is settled in
[data/fonts/README.md](../data/fonts/README.md): Geist for text, Geist Mono for
code, DejaVu kept as the per-glyph fallback.

Every phase of the V1 plan is closed, Phase 11 included. What remains from the
plan itself is only the optional Phase 12 — and the items below that came out of
using the product rather than from the plan.

## What the three-model experiment settled

Recorded because it changed the order below, and an order changed without a
reason written down is an order nobody can revisit.

The same prompt, brand and photograph were run four times: three with Sonnet 5 at
low reasoning effort, one at maximum. The three low runs produced the same piece
each time — nameplate top left at 42 to 60pt, headline bottom left, a full-width
scrim over the photograph in two of them, and in one case four validation warnings
that were never mentioned. The maximum-effort run broke that skeleton on every
axis: the nameplate moved to the empty sky at top right at 100pt, no scrim at all,
zero warnings standing, and it narrated its own fixes including a rebase after a
`VERSION_CONFLICT`.

So the genericness was largely a reasoning budget, not a missing feature. That
demotes **templates (§85)**: they remain worth building, but as a way to let a
cheap model reach where an expensive one already gets on its own, not as the
ceiling on quality.

What survived the maximum-effort run was a legibility failure — two metadata lines
in `$brand.muted`, a mid grey, over mid-grey wet asphalt. The accent red beside
them stayed readable because it breaks by hue rather than by luminance. Nothing in
Forma checks this, which is why it reached a person's eye. That is what the
contrast check above is for, and it is the only one of the candidate fixes that
does not depend on the agent having a good day.

## Agreed order

Items 1 to 4 closed the V1 plan itself. Everything from 5 on came out of using
the product against a real brief — a magazine cover, four agent runs, one
photograph — and each one names the section it belongs to and the §100 criterion
that lets it in. Nothing enters this list because it would be interesting.

1. ~~**Brand panel**~~ — done. Phase 6 is closed and §79 step 4 is reachable.
2. ~~**Export from the editor**~~ — done. §10's layout is complete.
3. ~~**Playwright end-to-end**~~ — done. §79's walkthrough runs in CI.
4. ~~**Default font decision**~~ — done. §94 (93.2) is settled and written down.
5. ~~**`DELETE /designs/:id`**~~ — done. A hard delete, unlike an asset's soft
   one: an asset is pointed at by historical versions of *other* designs, while a
   design is pointed at by nothing and its snapshots exist only to undo it. It
   also clears the two kinds of derived file no SQL cascade reaches — the cached
   previews and the exported PNGs — while asset bytes stay, per §74. There is no
   MCP tool for it: §48 fixes the surface at thirteen, and deciding that a piece
   of someone's work should stop existing is not the agent's call. **Still no
   delete in the web UI**, for designs or projects; the API is what §31 asked for
   and a button is a separate decision.
6. ~~**The orphan rule, in the skill**~~ — **struck, on the evidence.** It was
   listed because a photo credit looked stranded in the far corner of the best
   cover so far. Reading the coordinates instead of the picture says otherwise:
   the credit shares its baseline with the section line (both y=1210) and is
   right-aligned to x=984, the same measure the nameplate and the headline end
   on. That is an ordinary editorial footer, one item flush left and one flush
   right on a shared measure, not a leftover. The same cover validates with no
   errors and no warnings at all. The rule the item proposed to rewrite is
   already landing; the complaint was a preference of the reviewer's, and it was
   wrong.
7. ~~**An overlay on the image layer**~~ — done. `ImageOverlay { color, opacity,
   from? }` sits beside `fit` and `focal_point`. With `from` the tint is anchored
   to an edge and fades to nothing, which is the whole point: a `ShapeLayer` is
   opaque to its own edges and leaves a seam. It covers the drawn photograph
   rather than the frame, so a `contain` image is tinted where it actually is and
   the layer's painted bounds do not grow. The validator checks the overlay's
   colour token and its opacity — an image layer carried no colour at all before
   this, and an empty arm there would have rebuilt the font-token bug exactly.
   Still not a licence to reach for one: removing both scrims from a real cover
   improved it.
8. ~~**Templates (§85)**~~ — done. A template is the
   document as it was when saved plus `TemplateRole { page_index, role,
   max_chars, sample_text }` derived from it server-side, so nobody authors
   metadata by hand and a brief cannot disagree with the layout it describes.
   Only layers carrying a `role` become slots: a role is the author saying what
   a layer is for, and a template of unlabelled boxes teaches nothing.
   Instantiating mints fresh page and layer ids, which §85's choice to address
   slots by `page_index` is what makes safe. Scoped to a project, because an
   `ImageLayer` holds an `asset_id` and assets belong to one.

   **The MCP surface is now fourteen tools.** §48 fixed it at thirteen, but
   §51.7's own V2 flow reads `get_brand → list_templates → suitable template?`,
   so the plan anticipated exactly this one. It is not a per-property tool and
   nothing else in the surface can express "a layout that already worked here".
   `create_design` takes an optional `template_id` rather than growing a
   fifteenth tool for it. There is still no way for an agent to *save* a
   template: which design is worth keeping is the person's call.

9. ~~**Phase 11, remote deploy**~~ — **done.** Its acceptance is *an authorized
   remote MCP host can create/render a piece*, and
   `./scripts/smoke-remote.py https://forma.hintly.online` reports 15 passed, 0
   failed, that check among them. Bearer auth on both processes (§57), MCP over
   Streamable HTTP on 4766 (§56), the guard that refuses to bind the network
   without a token (§70), Caddy in front, systemd units, and an aarch64 build.

   Two bugs were found by deploying rather than by reading, which is what a
   deploy is for. Caddy validated as root and created its own log file `0600
   root:root`, so the service could not reopen it — the file log is gone, since
   the journal was already doing that job. And rmcp validates the inbound `Host`
   against a loopback-only allowlist, so everything through Caddy was 403 before
   any JSON-RPC was read; the deployment now names itself through `FORMA_DOMAIN`,
   with the loopback entries kept.

   [docs/deploy/status.md](deploy/status.md) is the live state. Two of the
   three things this item once listed as open are closed. **The firewall
   survives a reboot** — the rules were saved with `netfilter-persistent`, the
   VM was rebooted, and 22/80/443 came back ahead of the final REJECT with
   Oracle's `InstanceServices` rules intact and Caddy started on its own. That
   was the largest single risk to a deployment that otherwise only looked
   finished. **Backups run on the machine** — `forma-backup.timer` is enabled
   and active, and its last run was its own rather than a person's.

   **Still open: getting backups off the machine.** No destination is chosen.
   What exists covers an accidental delete and does not cover losing the VM,
   which is the failure that would actually end the workspace.

   **Known limitation, deliberate:** the browser editor is not usable remotely.
   Every path except `/api/v1/health` requires the bearer, so the domain answers
   401 to a visitor rather than serving an app whose every call then fails. The
   SPA has nowhere to hold a token. §57 calls this "first remote testing" and the
   phase's acceptance is the agent half, which is what this delivers.
10. **Phase 11.5, hosted identity and agent distribution** — specified in
    **plan §105 to §117**, the first amendment the plan has taken. It moves the
    boundary from a deployment-wide secret to hosted identity plus account
    scoping, which is what Phase 11 deliberately stopped short of.

    §2.2 excluded OAuth; §105 records the change with the §100 criterion that
    admits it — *(5) it enables a concrete integration* — and the measurement
    behind it: **ChatGPT accepts OAuth or nothing**, with no manual bearer
    header, so for that host OAuth is not preferred but required. Supabase stays
    excluded as a *mandatory* dependency and is introduced for identity only,
    behind `FORMA_AUTH_MODE`.

    **It ships in two slices, and the order is the point.** The account boundary
    — accounts, `AccountId` on every scoped repository method, the table rebuild,
    the isolation tests — needs no Supabase, no JWT and no OAuth, and runs under
    `FORMA_AUTH_MODE=local`. That is also where all the security risk is: 36
    repository methods and the question of whether one person's work can reach
    another's. Everything the identity half rests on was in beta or formally
    deprecated on the reference date. Build the boundary first; plug identity
    into a boundary that is already proven.

    **Slice 1 is complete.** Accounts own every project and every repository
    method is scoped by `AccountId`; the real HTTP routes prove cross-account
    objects are indistinguishable from absent ids. Local auth derives the actor
    from `AuthContext`, machine-local commands control admission and owner
    recovery, and MCP credentials now travel with each request instead of with
    the process. Slice 2, including provider identity and OAuth, has not begun.

    Written against the code rather than the plan, in
    [phase-11.5-review.md](phase-11.5-review.md), which is the working paper
    behind §105–§117 and keeps the reasoning that the plan states as decisions.
    Three findings sit *below* identity and land in slice 1: the bind guard stops
    meaning anything once `FORMA_REMOTE_TOKEN` is retired (§109.1), `forma-mcp`
    holds one credential for its whole lifetime (§109.2), and the error envelope
    has no 403 at all (§109.3). A fourth was measured rather than reasoned:
    `/login`, `/oauth/consent` and `/agents` all **404 today**, so the OAuth flow
    would fail at the moment the provider hands the person back (§110.3).

11. **Phase 12, Supabase** — optional. Behind 10 rather than ahead of it: it
    unblocks nothing currently blocked, and is a persistence adapter for a scale
    problem this deployment does not have.

**Deliberately not on this list: editorial style modes in the skill.** It is the
intervention type with the worst measured record here — the scrim rule was
written twice and landed neither time, while adding two fonts moved the output
more than every prose change combined. If it is ever wanted, it belongs in a
template (item 8) or in a `social-carousel` skill per §51.11, not as more
adjectives in the core one.

Anything proposed that is not on this list goes through §100 first: a feature
enters when it unblocks the main workflow, removes real repeated manual work,
improves output consistency, fixes something a test found, or enables a concrete
integration. Not because it would be interesting.

## Decisions taken that the plan did not specify

Recorded so they are visible rather than buried in commit messages. The
reasoning for each is an invariant in [CLAUDE.md](../CLAUDE.md).

| Decision | Why |
| --- | --- |
| `render_test_card` removed from the MCP surface | Phase 0 scaffolding; §48 fixes the surface at thirteen tools. |
| `BrandKitDraft` for `PUT /brand` | A client should not mint a `BrandId` or pick a version. |
| `FormatRequest` for `POST /designs` | A client saying "square" should not have to know what square measures. |
| `INVALID_BODY` envelope for malformed JSON | §32 says every error has one shape; actix's own was plain text. |
| Two bounding boxes, `layout` and `painted` | usvg's layer box ignores clip paths, so one number would make §42 wrong either way. |
| `UNKNOWN_BRAND_TOKEN` reported with no Brand Kit | Otherwise a design validates clean and then fails to render. |
| Render cache prunes itself on write | §72 says a version never changes; nothing reads a superseded entry. |
| `checkpoint_and_close` rather than `pool.close()` | Closing checkpoints only *usually*; a backup cannot rest on usually. |
| Brand version in the render cache key | A `$brand.*` token resolves at render time, so a brand edit changes the picture without changing the design's version. |
| The Brand panel appears in the project view too | §79 configures a Brand Kit before any design exists, so the editor alone would be too late. |
| The editor warns before exporting a design with errors, but allows it | §42 says errors block export, and the server does not enforce it. The agent is told to fix them first; a person is told what they are shipping and decides. |
| The end-to-end covers the browser half of §79, not the agent half | Steps 6 to 14 are already driven against the real binaries in `apps/mcp/tests/agent_flow.rs`. Repeating them in a browser would be slower and prove less. |
| Geist is the default; DejaVu is kept as a fallback | Geist ships four weights to DejaVu's two, which is what a typographic scale needs, and covers every character a design reaches for. DejaVu's eight-fold coverage is the floor that stops a non-Latin design rendering as tofu. |
| The renderer sorts the font directory and picks the fallback face by weight | usvg's default selector takes the first face in `fontdb` order, which `read_dir` decides, and its style guard lets a Bold face answer a Regular request — so the same commit rendered Greek in different weights on two machines. Found by CI, not by review. |
| A contrast check, `TEXT_LOW_CONTRAST` | Seventeen validation codes and not one asked whether the text could be seen. Metadata in `$brand.muted` over wet asphalt survived a max-effort agent, a skill rule telling it to look, and its own visual pass; the user caught it by eye. §100 criteria 1 and 3. It measures lightness *and* colour: a luminance-only version called the accent red the worst offender on the very cover whose author said it was the one element that worked. |
| Font tokens resolve in the renderer, not only in the validator | The renderer wrote `$brand.display` into the SVG as a family name and usvg drew nothing, while the validator resolved it and passed the design. Found by an agent reading the code, not by a test. |
| `TEXT_INVALID_CHARACTER`, and the write gate that makes it rare | Found by auditing every `RenderError` against the validator's codes after the font-token bug. One hole: a control character in a string validated clean and answered 500 at render, on a design that also cannot be deleted. |
| `forma.hintly.online` as the deploy name, for now | Forma has no domain of its own and the deploy needs a name before it can have a certificate. A subdomain of one that exists, so nothing at that apex is touched. Held in `FORMA_DOMAIN` rather than written into the Caddyfile, because a temporary value hard-coded is a temporary value forever. |
| Every MCP payload is sent as text *and* as `structuredContent` | The spec says a tool returning structured content SHOULD also serialize it into a text block, and a host that reads only text is not hypothetical: opencode answered `list_projects` with "1 project(s)" and nothing else, which makes every id-taking tool after it unusable — ids are the only handles there are. Duplicated tokens on hosts that read both is the smaller problem. |
| No boolean subschema in any tool schema | `apply_operations` emitted one, and a host converting the tool list for its model failed on the whole list rather than on that tool. A schema is read by software nobody here wrote, so the conservative shape wins over the expressive one. |
| The deployment names itself to rmcp, rather than trusting any `Host` | rmcp validates the inbound `Host` against a loopback-only allowlist, so everything arriving through Caddy was 403 before any JSON-RPC was read. The allowlist is DNS-rebinding protection and worth keeping: `FORMA_DOMAIN` is *added* to the loopback entries, never `disable_allowed_hosts()`, whose empty list means allow everything. |
| The skill is symlinked into `.claude/skills/` and `.agents/skills/` | §51.2 puts it at `skills/forma/SKILL.md`, which no agent host scans: Claude Code looks in `.claude/skills`, Codex in `.agents/skills`, opencode in both. It was invisible to every host that was not told to open it by path. Symlinks rather than moving it, so the plan's path, the test and every existing link stay put. |
