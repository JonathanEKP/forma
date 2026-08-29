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
| 8 — MCP | 7 | Complete — the thirteen tools of §48. |
| 9 — Agent skill | 15 | Complete. |
| 10 — Hardening | 9 | Complete. |
| 11 — Remote deploy | 9 | Not started. Gated on Oracle capacity (§90). |
| 12 — Supabase adapter | 5 | Not started. The plan marks it optional and only after 11. |

## Open, with the section that asks for it

### Nothing from the plan is open

§94's three decisions are all closed now: 93.1 resolved to Option A in Phase 0,
93.3's pixel-diff threshold was measured in Phase 10 rather than invented, and
93.2 — the default font — is settled in
[data/fonts/README.md](../data/fonts/README.md): Geist for text, Geist Mono for
code, DejaVu kept as the per-glyph fallback.

What remains is Phase 11 and, behind it, the optional Phase 12.

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

1. ~~**Brand panel**~~ — done. Phase 6 is closed and §79 step 4 is reachable.
2. ~~**Export from the editor**~~ — done. §10's layout is complete.
3. ~~**Playwright end-to-end**~~ — done. §79's walkthrough runs in CI.
4. ~~**Default font decision**~~ — done. §94 (93.2) is settled and written down.
5. **Phase 11, remote deploy** — after the above, and only once Oracle capacity
   is confirmed (§90). The plan says explicitly that the architecture does not
   depend on it.
6. **Phase 12, Supabase** — optional, and only after 11 works.

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
