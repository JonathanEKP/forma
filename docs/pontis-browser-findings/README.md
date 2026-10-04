# Browser findings

What testers learn while trying to break the Pontis → Forma connection in a real browser with
[the browser stack](../pontis-browser-testing.md). One file per **area**, named `NN-area.md`
(`00-harness.md` is taken; then e.g. `01-chat.md`, `02-devices.md`, `03-ui-actions.md`,
`04-approvals.md`, `05-isolation.md`). Append to your area's file; do not edit another's findings.

A finding is something observed, not something guessed. If it was not seen in the browser (or in a
log of that run), say that in *Evidence*. If it is a product bug that was fixed on the spot, keep it
and mark it `fixed` with the commit.

## Format

One section per finding, in this shape:

```markdown
## F-<area>-<n> — <title>

- **Severity:** blocker | high | medium | low | info
- **Status:** open | fixed (<commit>) | wontfix (<reason>) | not-reproducible | upstream (Pontis)
- **Instance / provider:** e.g. instance 2, FakeProvider; or instance 1, real Codex gpt-6-luna

**Steps to reproduce**
1. …
2. …

**Expected** — what the plan, the docs or common sense says should happen (cite the section).

**Actual** — what happened, exactly. Quote messages verbatim.

**Evidence** — a screenshot at `docs/pontis-browser-findings/evidence/<id>-<what>.png`, and/or a
log excerpt (a few lines, with the log's file name). Event types and ids are fine.
```

### Severity

| | |
|---|---|
| **blocker** | a person cannot do the thing the feature exists for (cannot chat, cannot sign in), or work/credentials of another account are reachable |
| **high** | the feature works only partly or lies about its state (says success when nothing happened, loses a person's work) |
| **medium** | wrong or confusing behaviour with a way around it |
| **low** | cosmetic, wording, rough edge |
| **info** | a limitation or observation worth recording that is not a defect |

### Rules

- **Never put a token, an `Authorization` header, a service credential or anything from
  `~/.codex/auth.json` in a finding or its evidence.** Screenshots are checked for them too (the
  editor does not display them; a network panel or a log could). Session JWTs minted from the test
  key are test-only, but do not commit them either.
- Redact the person's email if it is not the fixture's `person-a-N@example.test`.
- Screenshots: PNG, named `<finding id>-<what>.png`, cropped to what matters when you can.
- Say which provider the run used. A FakeProvider result says nothing about Codex and vice versa.
- Keep reproduction steps runnable from `docs/pontis-browser-testing.md` (instance number, who is
  signed in, which design) so the next person can follow them.
- If a finding needs product code to change, record it here and tell the coordinator; do not change
  product code from a testing branch without asking.
