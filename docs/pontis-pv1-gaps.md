# PV-1 gap closure: S4, S10 and S6 expiry

**Task G1, 2026-10-04.** Step 12 left three scenarios of the [PV-1 matrix](pontis-pv1-evidence.md)
unclaimed. All three are on Forma's side to prove; only S6 expiry can turn out to be a Pontis defect.
One Codex worker (`gpt-6-luna`, xhigh) owns all three on branch `JonathanEKP/pontis-pv1-gaps`.

## Scope

| Scenario | Definition | What is missing | Real Codex turns |
|---|---|---|---|
| S4 | [pontis-pv1-e2e.md §S4](pontis-pv1-e2e.md#s4--deterministic-validation--sidecar-contract) | A sidecar-contract test: `design.validate` through the sidecar on a design with known issues returns exactly the validator's own list (same codes, severities, layer ids, order), the bounds hold, and the same design once fixed validates clean and renders through `design.renderPreview`. | 0 |
| S10 | [§S10](pontis-pv1-e2e.md#s10--authorization-and-account-isolation--sidecar-contract) | One test that revokes a membership **during a live session**: the next invocation is refused with `authorization_denied` although the delegation is unexpired, and the refusal does not disclose whether the resource exists. Also a delegation for account A acting on account B, if no existing test already proves it end to end. | 0 |
| S6 expiry | [§S6](pontis-pv1-e2e.md#s6--approval--full-stack) | Re-run the expiry case. Step 12's single attempt saw `approval.expired` and the turn complete, then timed out waiting for the Runtime's stale-resolution refusal ([real-codex-gaps.json](evidence/step12/real-codex-gaps.json)). | ≤ 3 |

## Decisions

- **G-1. Prove S6 expiry on FakeProvider first.** If the Runtime refuses a late resolution there, the real-Codex
  run is only confirmation. If it does not refuse on FakeProvider either, the gap is not timing.
- **G-2. Separate the two halves of expiry.** The scenario's requirement is *nothing deleted, terminal*. Assert
  that directly (designs still readable, no Boundary F write after expiry, a late approve executes nothing)
  and record the Runtime's refusal diagnostic as a separate observation. If no write happens but no diagnostic
  arrives, S6 expiry **passes** on its requirement and the missing diagnostic goes to the upstream report as a
  Pontis observation (it belongs with item 11), with the exact observed sequence.
- **G-3. Never wait on a clock you could wait on a condition for.** If the old test timed out because of a fixed
  wait or a too-short timeout, fix the test; say so in the evidence.
- **G-4. Prefer the existing harness.** S4 and S10 go in the sidecar's contract tests (`apps/pontis-sidecar/src/*.test.ts`,
  or `apps/server/tests/pontis.rs` where it is Boundary F alone); S6 in `real-codex.test.ts` / `local-run.test.ts`.
- **G-5. A membership store that cannot be changed mid-session is a finding, not a reason to skip.** If the
  harness has no way to revoke a membership while a session lives, add the smallest test seam needed in test
  code only, never a production route.

## Constraints

Same as batch 2: never edit `/home/vertex/projects/pontis` (read-only through the `.pontis` symlink); never
copy, print or commit `~/.codex/auth.json`; never touch production or the real Supabase project; never log
tokens, auth headers or asset bytes; don't kill the servers on 4765/5173; use the scratchpad, not `/tmp`. If
the Pontis checkout does not compile (it is being edited), build nothing from it: use the already built
test-driver and say so.

## Outcome

Recorded by the worker in [pontis-pv1-evidence.md](pontis-pv1-evidence.md) (each row only on evidence), in
`docs/evidence/step12/pv1-gaps.json`, and in §11 of the integration plan.
