# 30 — Approvals

Area T3: approval of risky operations in the chat, tried in the Orca browser on **instance 4**
(2026-10-04), FakeProvider unless a row says Codex. Format: [README](README.md). The harness is
[pontis-browser-testing.md](../pontis-browser-testing.md).

Harness note for whoever repeats this: the FakeProvider removal scripts name `base_version = 2`, so a
removal sent *before* "Raise the headline to 96pt." (design at v1) is approved and then fails with a
version conflict (case A02). Run the write first. The removal scripts are also keyed to fixed layers
(headline, tagline) of one design.

## Cases run

| id | what | result |
|---|---|---|
| A01 | "Remove the headline." → approval card appears | pass (card content: see F-30-1) |
| A02 | approve a removal whose `base_version` (2) is stale (design at v1) | pass: "Approved", then "Failed: design version changed: expected 2, found 1"; design stays v1 |
| A03 | "Raise the headline to 96pt." in the same session after a failed turn | pass: v2, session still usable |
| A04 | deny "Remove the tagline." | pass: card "Denied", result "you declined this action, so nothing was changed", design still v2 with 4 layers |
| A05 | approve "Remove the headline." | pass: exactly one new version (v3), layer gone, history v1 human, v2/v3 agent |
