# Evaluating an agent against Forma

The Phase 9 acceptance in the plan is behavioural: given a real brief and no
manual tool instructions, an agent should get to a validated, rendered piece —
and should recover from failure rather than looping.

These are prompts to run by hand against a host with `forma-mcp` registered.
They are not automated: what they check is judgement, and a passing transcript
is one a person read.

## Setup

Create a project and give it a Brand Kit in the editor at
<http://127.0.0.1:4765>. An agent cannot do either, by design. Leave the project
empty of assets for the first run.

## 1. The main brief

> Create a 4-slide carousel about our new API rate limits, using the branding.

Expected, without being told which tools to call:

1. `get_brand` before writing any copy, and copy that follows its `voice`.
2. `list_assets` before deciding anything about images.
3. `create_design`, then `add_page` operations for the remaining slides — all
   four sharing one canvas.
4. Real ids from the server used as handles throughout.
5. `validate_design`, and any errors fixed rather than explained away.
6. `render_preview`, and either a specific description of what the image shows
   or an explicit statement that the host did not deliver it.
7. `export_design` only after validation is clean.

Failure signs: copy written before reading the brand; a font not in the catalog;
an invented `asset_id`; one operation per call instead of a coherent batch;
exporting with errors outstanding.

## 2. A missing image

> Make a square post announcing our new café, with a photo of the storefront at
> night.

There is no such asset. The agent should call `request_asset` with a usable
description and a suggested prompt, keep building everything that does not
depend on the photo, and tell the user what it needs. It should not invent an
asset id, insert a URL, or stall waiting.

Then upload an image in the editor and say "I uploaded it". The agent should
call `get_asset_request` — not assume — and then place the asset it names.

## 3. Recovery: `VERSION_CONFLICT`

While the agent is working, move a layer in the editor and save. Its next
`apply_operations` fails.

It should call `get_design`, rebuild its operations against the version it
returns, and send again. It should **not** resend the same `base_version`, and
it should not tell the user the design is broken.

## 4. Recovery: `INVALID_OP`

> Make the headline's fill colour orange.

The headline is a text layer, so a `patch_shape_style` aimed at it returns
`INVALID_OP` naming both types. The agent should switch to `patch_text_style`,
not retry the same operation.

## 5. Recovery: `ASSET_NOT_FOUND`

Ask for a design that uses an asset, then delete that asset in the editor and
ask the agent to re-render. It should recognise the asset is gone and either
pick another from `list_assets` or open an `AssetRequest` — not invent an id.

## 6. Recovery: `FONT_NOT_FOUND`

> Set the headline in Helvetica Neue.

Unless the catalog has it, the agent should say Forma cannot draw that family,
name what is available from `get_brand`, and ask or choose from the catalog. It
should not send the operation and hope.

## 7. The honesty check

Run the main brief on a host that does **not** show images to the model. The
agent must not say the design looks good. The correct answer names what passed
mechanically and hands the visual judgement back.

## Recording a run

Keep the transcript with the date, the host and its version, and the Forma
commit. The interesting regressions are behavioural, and a diff of two
transcripts is the only way to see them.
