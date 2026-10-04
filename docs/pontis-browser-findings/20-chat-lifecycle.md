# 20 — Chat lifecycle (instance 3)

Area T2 of the Pontis → Forma browser break-test: the in-editor chat, start to end. Run on instance 3
(editor `http://127.0.0.1:4832`, signed in as A, project `ALPHA project`, design `ALPHA Launch cover`),
Orca browser tab of my own, 2026-10-04. Format: [README](README.md). Provider is stated per case:
**Fake** = FakeProvider (plumbing, free), **Codex** = real Codex 0.158.0 `gpt-6-luna` low.

Notes taken before the details:
- The editor has **no route per design**: the URL stays `/` whether on the project list or inside a design.
  So a reload anywhere lands on the project list and the chat is closed (see C-reload).
- Everything below was seen in the browser unless the *Evidence* line says it was a log or `curl`.

## Cases run

| id | what | provider | result |
|---|---|---|---|
