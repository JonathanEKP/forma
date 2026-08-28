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

### 1. Playwright end-to-end — §67

The plan lists it as a testing layer. No phase was ever given it, and it does
not exist. It is what would have caught the Brand panel gap: it walks §79's script,
which starts with creating a project and configuring a Brand Kit.

### 2. Default font — §94 (93.2)

Still an open decision, which the plan says must be closed by reviewing licence,
required weights, `usvg` compatibility and visual result. `data/fonts` currently
ships both DejaVu and Geist because each arrived for a different reason; the
review that decides which is the default, and why, has not been written down.

The other two decisions from that section are closed: 93.1 resolved to Option A
in Phase 0, and 93.3's pixel-diff threshold was measured in Phase 10 rather than
invented.

## Agreed order

1. ~~**Brand panel**~~ — done. Phase 6 is closed and §79 step 4 is reachable.
2. ~~**Export from the editor**~~ — done. §10's layout is complete.
3. **Playwright end-to-end** — §67, walking §79's twenty steps, so this class of
   drift is caught by CI instead of by an audit.
4. **Default font decision** — §94 (93.2), written down.
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
