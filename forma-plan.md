# Forma
## Complete execution plan — React/TypeScript + Rust + MCP

**Status:** proposed architecture for implementation  
**Technical reference date:** August 27, 2026  
**Goal:** build a local programmatic graphic design application controllable by agents such as Codex and Claude through MCP, without directly consuming model APIs or adding separate billing.

---

# 1. Executive summary

The product will be a **design runtime for AI agents**.

The agent will not directly generate the final pixels. Instead:

1. The user asks their usual agent for a piece.
2. The agent queries the Brand Kit and the available assets.
3. The agent creates or modifies a structured design document.
4. The Rust backend validates the document.
5. The backend generates a deterministic SVG.
6. That same SVG is rasterized to PNG.
7. The agent can receive the preview as an image through MCP, inspect it and correct it.
8. The user can open the React application and manually edit the same piece.
9. If a base image is missing, V1 creates an `AssetRequest` and waits for the user to upload the image.
10. In the future, `AssetRequest` will be resolvable through search or generation on the AI host without modifying the core of the system.

The application will have **no OpenAI API, no Anthropic API and no other integrated LLM provider**.

The intelligence lives on the host:

- Codex
- Claude Code / Claude Desktop
- other compatible MCP clients
- ChatGPT, when the MCP capabilities of the plan and of the product allow it

The backend will be responsible exclusively for:

- documents;
- branding;
- assets;
- rendering;
- validation;
- versions;
- storage;
- MCP;
- export.

---

# 2. Scope of V1

## 2.1 Must exist in V1

V1 must allow:

- creating projects;
- defining a Brand Kit;
- uploading images;
- creating single-page or multi-page pieces;
- adding text;
- adding images;
- adding rectangles, circles/ellipses and lines;
- ordering layers;
- moving and resizing layers;
- editing properties from React;
- saving automatically;
- maintaining optimistic versioning;
- validating the design;
- rendering a preview;
- exporting PNG;
- connecting Codex through local MCP;
- connecting Claude Code through local MCP;
- returning PNG previews inside the MCP result;
- allowing an agent to modify the design through structured operations;
- asking the user for an image through `AssetRequest`;
- continuing the flow after the user uploads the missing asset.

## 2.2 Will not be implemented in V1

V1 will **not** include:

- automatic image generation;
- OpenAI Image API;
- Anthropic API;
- automation of `chatgpt.com`;
- Playwright/Selenium against external AI services;
- automatic stock search;
- Google Images scraping;
- publishing to Instagram;
- social calendar;
- real-time multi-user collaboration;
- OAuth — **amended by §105**: in scope from Phase 11.5, for hosted deployments only;
- payments;
- WASM;
- Yew;
- video;
- animations;
- Figma-style auto-layout;
- complex photographic filters;
- advanced masks;
- pixel editing;
- 3D composition;
- automatic template generation;
- mandatory integration with Supabase — this exclusion **stands**. Phase 11.5
  introduces Supabase for hosted identity only, behind `FORMA_AUTH_MODE`, and
  local Forma must never require it.

These exclusions are intentional, to keep V1 finishable and to leave clean extension points.

---

# 3. Architectural principles

## 3.1 The model lives outside the application

There will be no:

```text
Forma -> OpenAI API
Forma -> Anthropic API
Forma -> Gemini API
```

There will be:

```text
Codex / Claude / other MCP host
              |
              | MCP
              v
          Forma MCP
              |
              | internal API
              v
         Forma Server
```

The AI host decides:

- copy;
- visual hierarchy;
- position;
- size;
- colors;
- asset selection;
- when to review a preview;
- when to correct a design.

Forma executes those decisions deterministically.

---

## 3.2 Rust is the source of truth

All authoritative logic lives in Rust:

- schema;
- invariants;
- versions;
- validation;
- rendering;
- asset management;
- persistence;
- export.

React holds only temporary interaction state:

- current selection;
- zoom;
- pan;
- drag in progress;
- resize in progress;
- open panel;
- form that has not been saved yet.

If React and Rust disagree, Rust wins.

---

## 3.3 The document is not HTML nor arbitrary code

The agent will not be able to send arbitrary HTML/CSS/JS.

It will use a limited schema:

```text
DesignDocument
  -> Page
      -> Layer
          -> Text
          -> Image
          -> Shape
```

This enables:

- validation;
- versioning;
- human editing;
- reproducible rendering;
- a smaller security surface;
- compatibility across agents;
- schema migrations.

---

## 3.4 The final render is deterministic

The output will not depend on the browser.

Pipeline:

```text
DesignDocument
      |
      v
SVG authored by Rust
      |
      v
usvg
      |
      +--> layout/bounding boxes
      |
      +--> normalized/flattened SVG
      |
      v
resvg
      |
      v
PNG
```

`usvg`/`resvg` will be part of the authoritative renderer.

---

# 4. Final local architecture

```text
+---------------------------------------------------------+
|                    AI HOST                              |
|                                                         |
|          Codex / Claude Code / MCP client               |
+---------------------------+-----------------------------+
                            |
                            | stdio MCP
                            v
+---------------------------------------------------------+
|                    forma-mcp                            |
|                         Rust                            |
|                                                         |
| - exposes MCP tools                                     |
| - validates MCP schemas                                 |
| - translates tool calls into the internal API           |
| - returns JSON + image previews                         |
+---------------------------+-----------------------------+
                            |
                            | HTTP localhost
                            v
+---------------------------------------------------------+
|                   forma-server                          |
|                  Rust + Actix Web                       |
|                                                         |
|  Application Services                                   |
|  +----------------+ +--------------+ +---------------+  |
|  | Design Core    | | Assets       | | Validator     |  |
|  +----------------+ +--------------+ +---------------+  |
|           \               |                /             |
|            \              |               /              |
|             +-------- Renderer -----------+              |
|                       |                                  |
|                  usvg / resvg                            |
|                       |                                  |
|                 SVG / PNG                               |
|                                                         |
|  Persistence                                             |
|  +----------------+ +-------------------------------+   |
|  | SQLite         | | Local filesystem              |   |
|  +----------------+ +-------------------------------+   |
+---------------------------+-----------------------------+
                            |
                            | REST
                            | optional SSE
                            v
+---------------------------------------------------------+
|                   forma-web                             |
|             React + TypeScript + Vite                   |
|                                                         |
| Editor / Layers / Assets / Brand / Preview / Export     |
+---------------------------------------------------------+
```

Proposed ports:

```text
127.0.0.1:4765   forma-server + web
127.0.0.1:4766   reserved for future MCP HTTP
```

The local MCP in V1 will use **stdio**.

---

# 5. Why two processes: `forma-server` and `forma-mcp`

The MCP process will not be allowed to open SQLite directly.

Reason:

```text
React -> forma-server -> SQLite
MCP   -> SQLite
```

would create two independent mutation paths.

The architecture will be:

```text
React --------\
               > forma-server -> DB
forma-mcp ----/
```

Benefits:

- a single authority for versions;
- a single validation layer;
- a single transaction per operation;
- the same behavior from the UI and from the agent;
- no duplicated logic;
- the agent can be restarted without restarting Forma.

`forma-mcp` will be an internal client of the `forma-server` API.

---

# 6. Future remote architecture

For external testing:

```text
                     Internet
                        |
                      HTTPS
                        |
                      Caddy
                  /             \
                 /               \
                v                 v
       forma-server         forma-mcp
          :4765                :4766
             \                   /
              \                 /
               +------ Rust ----+
                        |
              +---------+----------+
              |                    |
          PostgreSQL           Object Store
```

There will be two possible modes.

## 6.1 Minimal remote deploy

Oracle VM:

```text
Oracle VM
  - Caddy
  - forma-server
  - forma-mcp --transport http
  - SQLite
  - local asset storage
```

This is the first option for testing because it replicates the local environment almost exactly.

## 6.2 Deploy with Supabase

```text
Oracle VM
  - Caddy
  - forma-server
  - forma-mcp

Supabase
  - PostgreSQL
  - Storage
```

Supabase will not be a dependency of the domain.

It will be an adapter.

---

# 7. Monorepo

```text
forma/
|
|-- apps/
|   |-- server/
|   |   |-- Cargo.toml
|   |   `-- src/
|   |
|   |-- mcp/
|   |   |-- Cargo.toml
|   |   `-- src/
|   |
|   `-- web/
|       |-- package.json
|       |-- vite.config.ts
|       `-- src/
|
|-- crates/
|   |-- design-core/
|   |-- application/
|   |-- renderer/
|   |-- validator/
|   |-- assets/
|   |-- persistence/
|   |-- mcp-api/
|   `-- common/
|
|-- bindings/
|   `-- generated/
|
|-- migrations/
|   |-- sqlite/
|   `-- postgres/
|
|-- data/
|   |-- forma.db
|   |-- assets/
|   |-- previews/
|   |-- exports/
|   `-- fonts/
|
|-- fixtures/
|   |-- designs/
|   |-- assets/
|   `-- golden/
|
|-- scripts/
|-- deploy/
|   |-- docker/
|   |-- caddy/
|   `-- systemd/
|
|-- docs/
|   |-- architecture.md
|   |-- design-schema.md
|   |-- mcp.md
|   `-- renderer.md
|
|-- Cargo.toml
|-- Cargo.lock
|-- package.json
|-- pnpm-workspace.yaml
|-- AGENTS.md
|-- .env.example
`-- README.md
```

---

# 8. Rust workspace

## 8.1 `design-core`

Does not depend on:

- Actix;
- SQLx;
- MCP;
- filesystem;
- HTTP.

It contains only domain and types.

```text
design-core/src/
|-- project.rs
|-- brand.rs
|-- document.rs
|-- page.rs
|-- layer.rs
|-- geometry.rs
|-- color.rs
|-- typography.rs
|-- operation.rs
|-- version.rs
|-- asset.rs
|-- asset_request.rs
`-- error.rs
```

It must be simple to test.

## 8.2 `application`

Contains use cases.

```text
application/src/
|-- projects/
|-- brands/
|-- designs/
|-- assets/
|-- renders/
|-- exports/
|-- asset_requests/
`-- services.rs
```

Use cases:

```text
CreateDesign
GetDesign
ApplyDesignOperations
ValidateDesign
RenderPreview
ExportDesign
CreateAssetRequest
ResolveAssetRequest
```

It must not know about Actix or MCP.

## 8.3 `renderer`

Responsible for transforming:

```text
DesignDocument -> SVG -> PNG
```

```text
renderer/src/
|-- scene.rs
|-- svg.rs
|-- text.rs
|-- images.rs
|-- shapes.rs
|-- fonts.rs
|-- flatten.rs
|-- raster.rs
|-- bounds.rs
`-- error.rs
```

Main dependencies:

- `usvg`
- `resvg`
- `tiny-skia`, indirectly through resvg

## 8.4 `validator`

Deterministic checks.

```text
validator/src/
|-- document.rs
|-- bounds.rs
|-- text.rs
|-- assets.rs
|-- brand.rs
|-- geometry.rs
`-- severity.rs
```

## 8.5 `assets`

Responsible for:

- uploads;
- hashing;
- MIME;
- dimensions;
- logical storage;
- sanitization;
- future thumbnails.

It knows nothing about agents.

## 8.6 `persistence`

Interfaces and adapters.

```text
persistence/src/
|-- traits/
|   |-- project_repository.rs
|   |-- design_repository.rs
|   |-- asset_repository.rs
|   `-- object_store.rs
|
|-- sqlite/
|-- local_fs/
|
|-- postgres/       # deploy phase
`-- supabase/       # deploy phase
```

## 8.7 `mcp-api`

Defines the schemas and handlers that call `forma-server`.

It does not implement design logic directly.

---

# 9. Frontend

Stack:

```text
React
TypeScript
Vite
TanStack Query
Zustand
Tailwind CSS
```

No WASM.

No Yew.

## 9.1 Responsibilities

TanStack Query:

- projects;
- designs;
- Brand Kit;
- assets;
- versions;
- pending requests;
- server mutations.

Zustand:

- selected page;
- selected layer;
- zoom;
- pan;
- temporary drag;
- temporary resize;
- active panel;
- active tool.

---

# 10. UI layout

```text
+-------------------------------------------------------------------+
| Project        Design         Undo Redo             Preview Export |
+-------------+---------------------------------------+-------------+
|             |                                       |             |
| PAGES       |                                       | PROPERTIES  |
|             |                                       |             |
| Page 1      |                                       | Position    |
| Page 2      |                CANVAS                 | Size        |
| Page 3      |                                       | Rotation    |
|             |                                       | Typography  |
|-------------|                                       | Color       |
| LAYERS      |                                       | Asset       |
|             |                                       |             |
| Headline    |                                       |             |
| Hero image  |                                       |             |
| CTA         |                                       |             |
+-------------+---------------------------------------+-------------+
| ASSETS              | BRAND                | VALIDATION           |
+-------------------------------------------------------------------+
```

V1 does not try to compete with Figma.

---

# 11. Domain model

## 11.1 IDs

All IDs will be UUIDs.

Rust will use wrapper types instead of interchangeable strings:

```rust
ProjectId(Uuid)
DesignId(Uuid)
PageId(Uuid)
LayerId(Uuid)
AssetId(Uuid)
BrandId(Uuid)
AssetRequestId(Uuid)
```

The TypeScript bindings will be generated from Rust.

---

# 12. `Project`

```rust
Project {
    id,
    name,
    brand_id,
    created_at,
    updated_at,
}
```

A Project represents a workspace.

V1: one active Brand Kit per Project.

Future: multiple Brand Kits.

---

# 13. `BrandKit`

Conceptual shape:

```rust
BrandKit {
    id,
    project_id,
    name,
    colors,
    typography,
    logo_asset_id,
    voice,
    rules,
    version,
}
```

JSON example:

```json
{
  "name": "Default",
  "colors": {
    "primary": "#FF5436",
    "secondary": "#FDBA2D",
    "accent": "#7C3AED",
    "background": "#101010",
    "surface": "#191919",
    "text": "#FFFFFF",
    "text_inverse": "#101010",
    "muted": "#A3A3A3"
  },
  "typography": {
    "display": "Forma Sans",
    "body": "Forma Sans",
    "mono": "Forma Mono"
  },
  "voice": "Direct, technical and brief. Do not use hype.",
  "rules": {
    "safe_margin": 80,
    "minimum_font_size": 24
  }
}
```

Token names will be stable.

```text
$brand.primary
$brand.secondary
$brand.accent
$brand.background
$brand.surface
$brand.text
$brand.textInverse
$brand.display
$brand.body
$brand.mono
```

Rules:

- the agent reads the Brand Kit before writing copy or designing;
- `voice` is a user instruction and takes precedence over editorial defaults;
- values chosen by the agent must use tokens whenever an appropriate semantic token exists;
- a literal color explicitly requested by the user is kept as a literal and may produce a warning; it is not silently substituted;
- a font must exist in the `FontRegistry`; the agent must not invent families that the renderer cannot draw.

The list of actually available fonts will be exposed by `get_brand`/`get_font_catalog`.

---

# 14. `DesignRecord` and `DesignDocument`

The persistent identity and the portable document are deliberately separated.

```rust
DesignRecord {
    id,
    project_id,
    name,
    version,
    created_at,
    updated_at,
    document: DesignDocument,
}
```

```rust
DesignDocument {
    schema_version,
    format,
    pages,
    metadata,
}
```

`DesignDocument` **does not contain**:

- `project_id`;
- `design_id`;
- database version;
- local paths;
- remote URLs;
- secrets or credentials.

It only contains portable references:

- `AssetId` for images;
- `$brand.*` tokens for brand colors/fonts;
- internal page/layer IDs for operations.

This allows the same document to become a template, be duplicated, or be moved between projects through an explicit asset/token resolution phase.

It must support carousels from the start; do not model a piece as a single canvas.

## `DesignFormat`

The format belongs to the whole design, not to each page.

```rust
DesignFormat {
    preset,
    width,
    height,
}
```

Initial presets:

```text
Instagram portrait  1080 x 1350
Square              1080 x 1080
Story               1080 x 1920
Landscape           1920 x 1080
Custom
```

All pages/slides of a carousel share the same canvas.

The renderer knows `width/height`; `preset` is useful semantics for the UI, safe areas and future transformations such as `set_format`.

---

# 15. `Page`

```rust
Page {
    id,
    name,
    background,
    layers,
}
```

The order of `layers` is the z-order: the last element is on top.

Coordinates may fall outside the canvas to allow bleed/crops. The renderer clips to the canvas. `OUTSIDE_CANVAS` must not automatically treat every case as an error: the validator will distinguish allowed visual content from lost critical text.

---

# 16. Layers

Serde-tagged enum.

```rust
enum Layer {
    Text(TextLayer),
    Image(ImageLayer),
    Shape(ShapeLayer),
}
```

V1 will not have a `GroupLayer`.

---

# 17. Common Layer properties

```rust
LayerCommon {
    id,
    name,
    role,
    visible,
    locked,
    opacity,
    transform,
}
```

```rust
Transform {
    x,
    y,
    width,
    height,
    rotation_deg,
}
```

V1 limits transforms to:

```text
rotation
translate
width
height
```

`role` is optional semantic metadata for the agent/editor:

```text
headline
kicker
body
cta
stat
photo
logo
decoration
```

By itself it does not affect the renderer, but it enables templates, character budgets and smarter editing tools without selecting layers by position or by text content.

Skew is postponed.

---

# 18. `TextLayer`

```rust
TextLayer {
    common,
    text,
    box_width,
    box_height,
    font_family,
    font_size,
    font_weight,
    line_height,
    letter_spacing,
    align,
    color,
}
```

## V1: deliberately simple wrapping

Text will **not** perform implicit word-wrap.

The string uses explicit line breaks.

```text
BUILD
WITH AI
```

Reasons:

- it avoids implementing two typographic engines;
- the agent can insert line breaks;
- the validator can detect overflow;
- `usvg` can provide real bounding boxes of the rendered text.

`TextWrapMode::Word` will be added later.

---

# 19. `ImageLayer`

```rust
ImageLayer {
    common,
    asset_id,
    fit,
    focal_point,
}
```

`fit` in V1:

```text
contain
cover
fill
```

`focal_point`:

```rust
{x: 0.5, y: 0.5}
```

Allows `cover` to prioritize a region.

---

# 20. `ShapeLayer`

```rust
enum ShapeKind {
    Rectangle,
    Ellipse,
    Line,
}
```

```rust
ShapeLayer {
    common,
    kind,
    fill,
    stroke,
    stroke_width,
    corner_radius,
}
```

`corner_radius` only applies to Rectangle.

---

# 21. Assets

```rust
Asset {
    id,
    project_id,
    hash,
    kind,
    mime_type,
    width,
    height,
    byte_size,
    storage_key,
    original_filename,
    origin,
    attribution,
    created_at,
}
```

```rust
enum AssetOrigin {
    UserUpload,
    RemoteUrl,
    HostGenerated,
    Stock,
}
```

V1 implements only:

```text
UserUpload
```

The other values allow the schema to evolve without making V1 depend on those features.

## Portability rule

An `ImageLayer` never contains a remote URL or a local path; it contains only:

```text
asset_id
```

Downloading/importing is a separate operation that first creates an `Asset`.

Reasons:

- external URLs die or change;
- it enables hashing/deduplication;
- the renderer does not need arbitrary network access;
- local and remote deployments use the same document;
- provenance/license stays associated with the asset and not with the layer.

Before creating an `AssetRequest`, the agent must review `list_assets` to reuse existing material when appropriate.

---

# 22. Asset Requests

A critical element for not blocking the flow.

```rust
AssetRequest {
    id,
    project_id,
    design_id,
    purpose,
    description,
    suggested_prompt,
    requested_width,
    requested_height,
    status,
    resolved_asset_id,
    created_at,
    resolved_at,
}
```

States:

```rust
enum AssetRequestStatus {
    Pending,
    Resolved,
    Cancelled,
}
```

V1:

```text
Pending
   |
   | user uploads image
   v
Resolved
```

A complex workflow engine will not be modeled yet.

---

# 23. Flow when an image is missing

```text
Agent
 |
 | request_asset
 v
AssetRequest(PENDING)
 |
 +------> React shows the request
              |
              | user drags an image
              v
          Upload Asset
              |
              v
      Resolve AssetRequest
              |
              v
        AssetRequest(RESOLVED)
              |
              v
Agent checks the request / continues
```

In V1 the agent's continuation may require a message from the user:

> I already uploaded the image.

We will not attempt to implement autonomous resumption of the host.

---

# 24. Future evolution of Asset Resolution

Future conceptual interface:

```rust
trait AssetResolver {
    async fn resolve(
        request: &AssetRequest
    ) -> AssetResolution;
}
```

Possible implementations:

```text
UserUploadResolver
RemoteUrlResolver
StockResolver
HostImageGenerationResolver
```

But V1 must not build all these abstractions if they are not needed yet.

Rule:

> Do not build interfaces prematurely unless they isolate a real infrastructure boundary or multiple implementations already exist.

For V1, `AssetRequestService` is enough.

---

# 25. Persistence

## 25.1 Local SQLite

V1:

```text
data/forma.db
```

SQLx + migrations.

Configure SQLite for a server process with reasonable concurrency:

```text
WAL
foreign_keys
busy_timeout
```

The server will be the only writer.

---

# 26. Initial tables

```text
projects
brand_kits
designs
design_versions
assets
asset_requests
exports
```

## `designs`

```text
id
project_id
name
schema_version
version
document_json
created_at
updated_at
```

## `design_versions`

```text
id
design_id
version
document_json
actor_type
actor_id
created_at
```

In V1 a full JSON snapshot will be stored per version.

Reasons:

- small documents;
- simple implementation;
- trivial undo;
- clear auditing.

Do not use event sourcing in V1.

---

# 27. Optimistic versioning

Every mutation includes:

```text
base_version
```

Example:

```json
{
  "design_id": "...",
  "base_version": 12,
  "operations": []
}
```

Conceptual SQL:

```sql
UPDATE designs
SET version = version + 1,
    document_json = ?
WHERE id = ?
  AND version = ?;
```

If zero rows change:

```text
VERSION_CONFLICT
```

The client must:

1. fetch the design again;
2. re-evaluate the change;
3. resend it against the new version.

This applies equally to React and to MCP.

---

# 28. Design operations

We will not expose arbitrary mutations of the full JSON. Operations must express intent and validate the real type of the layer.

V1:

```rust
enum DesignOperation {
    AddPage,
    RemovePage,
    ReorderPages,
    SetBackground,

    AddLayer,
    RemoveLayer,
    MoveLayer,
    ReorderLayer,

    SetText,
    PatchTextStyle,
    SetImage,
    PatchShapeStyle,

    SetFormat,
}
```

There will be no public `UpdateLayer { arbitrary_json }`. A patch sent to the wrong type returns `INVALID_OP` instead of becoming a silent no-op.

The main tool will be:

```text
apply_operations
```

Conceptual example:

```json
{
  "design_id": "...",
  "base_version": 4,
  "operations": [
    {
      "op": "set_text",
      "layer_id": "...",
      "content": "BUILD\nWITH AI",
      "font_size": 104
    },
    {
      "op": "move_layer",
      "layer_id": "...",
      "frame": {
        "x": 80,
        "y": 160
      }
    }
  ]
}
```

## Atomicity

The whole batch:

- is validated;
- is applied in order onto an in-memory copy;
- is validated structurally;
- is persisted in a single transaction;
- increments a single version;
- if one structural operation fails, none of the previous ones are saved.

V1 fixes:

```text
MAX_OPS_PER_BATCH = 20
```

This is not a renderer limitation; it is an operational limit to keep payloads, errors and retries manageable. It may change after measuring real usage.

A coherent modification that touches several pages must be sent in a single batch whenever it fits within the limit.

## Policy for invalid values

We will not copy The Agentcy's lenient behavior of silently degrading many cosmetic values.

V1:

- structural errors: reject the batch;
- out-of-range values: reject with an error/hint;
- warnings: only for valid but likely problematic conditions;
- normalizations/clamps: only where explicitly documented.

The priority is that agent and human know exactly what was persisted.

---

# 29. Undo / Redo

React will not implement an undo stack separate from the server.

Undo:

```text
design version 10
       |
       v
restore snapshot version 9
       |
       v
new version 11
```

We do not decrement the version.

History always moves forward.

This avoids breaking optimistic concurrency.

---

# 30. TypeScript types generated from Rust

Rust will be the source of truth.

Use `ts-rs`.

```rust
#[derive(Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TextLayer {
    // ...
}
```

It generates bindings in:

```text
bindings/generated/
```

The frontend does not manually maintain duplicated interfaces.

CI will fail if the generated bindings are out of date.

---

# 31. REST API

Prefix:

```text
/api/v1
```

## Projects

```text
GET    /projects
POST   /projects
GET    /projects/:id
PATCH  /projects/:id
DELETE /projects/:id
```

## Brand

```text
GET    /projects/:id/brand
PUT    /projects/:id/brand
```

## Designs

```text
GET    /projects/:id/designs
POST   /projects/:id/designs
GET    /designs/:id
POST   /designs/:id/operations
POST   /designs/:id/undo
GET    /designs/:id/history
```

## Render

```text
POST /designs/:id/validate
POST /designs/:id/render
GET  /designs/:id/pages/:page_id/preview.svg
GET  /designs/:id/pages/:page_id/preview.png
```

## Assets

```text
GET    /projects/:id/assets
POST   /projects/:id/assets
GET    /assets/:id
DELETE /assets/:id
```

## Asset Requests

```text
GET  /projects/:id/asset-requests
POST /projects/:id/asset-requests
GET  /asset-requests/:id
POST /asset-requests/:id/resolve
POST /asset-requests/:id/cancel
```

## Export

```text
POST /designs/:id/exports
GET  /exports/:id
GET  /exports/:id/download
```

---

# 32. HTTP responses

All errors will have a stable shape:

```json
{
  "error": {
    "code": "VERSION_CONFLICT",
    "message": "Design version changed",
    "details": {
      "expected": 12,
      "actual": 13
    }
  }
}
```

Do not return ad-hoc strings.

---

# 33. Rendering

The renderer must be completely independent of Actix.

Conceptual API:

```rust
trait DesignRenderer {
    fn render_svg(...) -> Result<RenderedSvg>;
    fn render_png(...) -> Result<RenderedPng>;
    fn inspect(...) -> Result<RenderInspection>;
}
```

---

# 34. Fonts

Do not depend on fonts arbitrarily installed on the system.

The following will be created:

```text
data/fonts/
```

plus a `FontRegistry`.

V1 will have a default font with a license compatible with redistribution **or** will require the user to install/configure an allowed font.

Before including a font in the repository, its license must be reviewed.

The renderer must load the same fonts regardless of:

- local machine;
- CI;
- Docker;
- Oracle.

---

# 35. Text rendering

Conceptual pipeline:

```text
TextLayer
   |
   v
SVG <text>/<tspan>
   |
   v
usvg + FontRegistry
   |
   +--> Text bounding box
   |
   +--> text flattened to paths
   |
   v
normalized SVG
```

This enables:

- measuring overflow;
- removing the dependency on browser font layout;
- rendering the PNG from the same tree.

V1 uses explicit line breaks.

---

# 36. Mandatory technical spike for the renderer

Before developing the full editor, a small spike must be carried out.

It must demonstrate:

1. loading a font from `data/fonts`;
2. creating an SVG with text;
3. processing it with `usvg`;
4. obtaining a bounding box;
5. serializing text converted to paths;
6. rendering the result with `resvg`;
7. obtaining a PNG;
8. verifying that SVG and PNG match visually;
9. verifying the behavior of IDs/groups during normalization.

### Conditional decision

`usvg` may simplify/remove groups that do not affect rendering.

For that reason it will **not be assumed** that layer IDs survive automatically.

The spike will decide between:

### Option A

The necessary groups survive:

```text
<g id="layer-...">
```

and are used directly in the editor.

### Option B

The renderer flattens each layer separately and Forma re-wraps the result:

```text
<g id="layer-...">
    ...normalized paths...
</g>
```

Option B guarantees stable IDs even if `usvg` simplifies internally.

Do not continue with the editor until this spike is closed.

---

# 37. Editor preview

The UI will not reimplement typography.

It will receive an authoritative SVG from the backend.

```text
Rust
 |
 v
flattened SVG
 |
 v
React
```

An interactive layer is drawn on top of the SVG:

```text
selection boxes
resize handles
safe area
alignment guides
```

The logical bounds come from the `DesignDocument`.

---

# 38. Drag

During pointer move:

```text
React local state only
```

Do not send a request per frame.

```text
pointerDown
  |
pointerMove x N
  |
pointerUp
  |
  v
POST /operations
```

Afterwards:

```text
Rust validates
Rust saves
Rust increments the version
React refreshes the preview
```

---

# 39. Resize

Same mechanism.

The frontend provisionally computes:

```text
x
y
width
height
```

On `pointerUp` it sends an operation.

Rust:

- normalizes;
- validates;
- saves.

---

# 40. Text editing

V1:

- select a TextLayer;
- edit the content from Properties;
- short debounce for the UI, optional;
- save on confirm / blur;
- refresh the SVG.

Do not implement inline editing on the canvas initially.

V2 may add double-click inline editing.

---

# 41. Validator

```rust
ValidationReport {
    valid,
    issues,
}
```

```rust
ValidationIssue {
    code,
    severity,
    page_id,
    layer_id,
    message,
    details,
}
```

---

# 42. V1 checks

## Structural

```text
INVALID_PAGE_SIZE
DUPLICATE_LAYER_ID
MISSING_LAYER
INVALID_OPACITY
INVALID_DIMENSION
INVALID_COLOR
UNKNOWN_BRAND_TOKEN
```

## Bounds

```text
OUTSIDE_CANVAS
OUTSIDE_SAFE_AREA
```

## Text

```text
TEXT_OVERFLOW_WIDTH
TEXT_OVERFLOW_HEIGHT
FONT_NOT_FOUND
FONT_TOO_SMALL
```

## Assets

```text
ASSET_NOT_FOUND
ASSET_UNREADABLE
INVALID_ASSET_TYPE
```

## Brand

```text
COLOR_OUTSIDE_BRAND
MISSING_BRAND_FONT
```

Warnings do not necessarily block export.

Errors do.

---

# 43. Brand validation

A color can be:

```rust
enum DesignColor {
    Literal(Color),
    BrandToken(BrandColorToken),
}
```

The agent must prefer tokens.

But the human editor may allow a literal.

The validator can emit a warning:

```text
COLOR_OUTSIDE_BRAND
```

Do not forbid it rigidly.

---

# 44. Asset upload V1

Expected formats:

```text
PNG
JPEG
WebP
SVG
```

For raster files:

1. enforce a configurable size limit;
2. verify magic bytes;
3. do not trust the extension;
4. obtain dimensions;
5. compute BLAKE3;
6. store by hash.

Layout:

```text
data/assets/ab/cd/<hash>.<ext>
```

---

# 45. SVG uploads

SVG is potentially dangerous active content.

The original upload will not be served without processing it.

Pipeline:

```text
SVG upload
   |
   v
usvg parse
   |
   | external resources disabled
   | scripts/events unsupported
   v
normalized static SVG
   |
   v
store canonical result
```

If it cannot be normalized:

```text
reject
```

---

# 46. Remote URL imports

Not to be implemented in V1.

When it is implemented:

- HTTPS only;
- block loopback;
- block private networks;
- block link-local;
- resolve DNS and check the IP;
- limit redirects;
- limit bytes;
- verify MIME/magic;
- timeout;
- record the source URL;
- review license/provenance.

This avoids SSRF and arbitrary downloads.

---

# 47. MCP V1

SDK:

```text
rmcp
```

Initial transport:

```text
stdio
```

The MCP server does not access the DB directly.

```text
AI host
  |
  v
forma-mcp
  |
  v
forma-server REST
```

---

# 48. MCP tools in V1

Keep few tools and make them composable.

## Reading

```text
forma_status
list_projects
get_project
get_brand
list_assets
get_design
get_asset_request
```

## Writing

```text
create_design
apply_operations
request_asset
```

## Quality/output

```text
validate_design
render_preview
export_design
```

Initial total:

```text
13 tools
```

We do not need `add_text`, `move_layer`, `set_color`, etc. as individual tools because `apply_operations` covers those actions in a batch.

---

# 49. `render_preview`

It must return:

1. a structured result;
2. a PNG inside MCP.

MCP supports base64 image content blocks.

Conceptual structured response:

```json
{
  "design_id": "...",
  "page_id": "...",
  "version": 7,
  "validation": {
    "valid": true
  }
}
```

plus:

```text
ImageContent(image/png)
```

This lets MCP clients with multimodal support inspect the piece.

Do not rely solely on local paths.

---

# 50. Agent flow

Prompt:

```text
Create a 5-slide carousel announcing X.
```

Expected flow:

```text
get_brand
   |
list_assets
   |
   +-- a suitable asset exists --> continue
   |
   +-- asset missing ----------> request_asset
                                       |
                                 user resolves it
                                       |
                                   continue
   |
create_design
   |
apply_operations
   |
validate_design
   |
   +-- errors --> apply_operations
   |
render_preview
   |
visual critique
   |
apply_operations
   |
validate_design
   |
export_design
```

---

# 51. Skill / instructions for agents

The skill will not be a decorative list of tools. It will be the **operational contract of the agent**: mental model, schema, design rules, workflow, error recovery and a full session.

The main structural reference for this phase will be The Agentcy's public skill provided during the design of the project. We will take the useful patterns, not their product, naming, pricing or features outside our scope.

The repo will have:

```text
skills/
└── forma/
    └── SKILL.md

docs/
└── agent-design-reference.md

AGENTS.md
```

`AGENTS.md` must stay short: it identifies when to load the skill and how to start the server/MCP. The full guide lives in `SKILL.md`.

## 51.1 Goals of `SKILL.md`

It must allow a new host to:

1. understand what a Design is and what it can modify;
2. read the Brand Kit before designing;
3. know exactly which fields are valid;
4. use real IDs as the only handles for pages/layers/assets;
5. create or edit in atomic batches;
6. resolve images without inventing IDs or URLs inside the document;
7. validate;
8. render and observe a preview when the host can see it;
9. correct;
10. export;
11. recover from conflicts and errors without entering loops.

## 51.2 Mandatory structure

```text
# Forma

1. Mental model
2. DesignRecord vs portable DesignDocument
3. Format / pages / z-order
4. Brand tokens, voice and font catalog
5. Layers and valid fields
6. Assets and AssetRequest
7. MCP tools
8. apply_operations and baseVersion
9. Validation rules
10. Preview / visual inspection
11. Error codes and recovery
12. Design heuristics
13. Full end-to-end session
```

## 51.3 Mental model the agent will see

The skill must make explicit:

```text
DesignRecord
├── id / project / version       server
└── document                     portable
    ├── schemaVersion
    ├── format
    └── pages[]
        ├── background
        └── layers[]             order = z-order
```

Rules:

- `DesignDocument` contains no URLs and no workspace identity;
- IDs are the only valid handles;
- never use implicit selectors such as "the headline of the second page" when a `layer_id` is already available;
- after creating/mutating, use the outline/document returned by the server as the authority;
- coordinates may go outside the canvas for visual bleed;
- critical text outside the safe area produces a warning/error depending on the rule, but a photo bleeding outside the canvas is valid.

## 51.4 Brand

Flow:

```text
get_brand
    |
    +--> resolved colors
    +--> tokens
    +--> fonts/font catalog
    +--> logo asset
    +--> voice
```

Rules:

- read it once at the start of the relevant task/session;
- follow `voice`;
- use `$brand.*` when the agent chooses the value;
- keep a literal explicitly requested by the user;
- do not invent a font outside the catalog;
- do not modify the global Brand Kit as a side effect of creating a post.

## 51.5 Assets

The skill must repeat this boundary because it is easy for an LLM to break it:

```text
ImageLayer
   └── asset_id

NOT:
ImageLayer
   └── https://...
```

V1 flow:

```text
list_assets
    |
    +--> a suitable asset exists -> set_image/add_layer
    |
    └--> asset missing -> request_asset
                             |
                             v
                      WAITING FOR USER
```

Do not assert what an asset shows if the agent could not inspect it.

## 51.6 Mutations

All modifications go through:

```text
apply_operations({
  design_id,
  base_version,
  operations: [...]
})
```

The skill must explain:

- an initial maximum of 20 ops;
- execution order;
- atomicity;
- IDs;
- the fields accepted by each op;
- that a coherent batch is preferable to several round trips;
- that `base_version` must be the version actually read.

## 51.7 Recommended workflow

```text
get_brand
   |
list_assets
   |
create_design / get_design
   |
apply_operations
   |
validate_design
   |
   +--- errors ---> get_design/apply_operations
   |
render_preview
   |
visual inspection when the host supports images
   |
apply_operations if needed
   |
validate_design
   |
export_design
```

In V2, once templates exist:

```text
get_brand
   |
list_templates
   |
suitable template? -- yes --> create from template
        |
        no
        v
design from scratch
```

## 51.8 Preview: honesty rule

If `render_preview` returns an image and the host can see it:

- inspect it;
- critique the composition;
- correct it if needed.

If the host does not receive / cannot inspect the image:

- do not claim "it looks good";
- report that the mechanical validation passed;
- leave the visual inspection to the user.

Mechanical validation and visual approval are different concepts.

## 51.9 Error recovery

All relevant MCP/REST errors must have:

```json
{
  "error": "VERSION_CONFLICT",
  "message": "...",
  "hint": "..."
}
```

The skill will include a recovery table at least for:

```text
VERSION_CONFLICT
DESIGN_NOT_FOUND
PAGE_NOT_FOUND
LAYER_NOT_FOUND
ASSET_NOT_FOUND
INVALID_OP
INVALID_DOCUMENT
LIMIT_EXCEEDED
FONT_NOT_FOUND
TEXT_OVERFLOW
ASSET_REQUEST_PENDING
RENDER_FAILED
STORAGE_ERROR
```

Example:

```text
VERSION_CONFLICT
→ get_design
→ look at the new version
→ rebase/recreate the operations
→ resend with the new base_version
→ never blindly repeat the same batch/version
```

## 51.10 Design heuristics

These rules belong to the skill, not to the renderer:

- one main idea per page;
- clear hierarchy: headline > supporting copy > metadata;
- respect safe areas;
- do not invent facts, statistics, names or claims;
- do not invent schema fields that the renderer does not support;
- text over photography requires a real contrast solution;
- keep copy within budgets when a template/role defines them;
- prefer your own shapes/type/assets before asking for a generic image when the piece works without it;
- use explicit line breaks in V1;
- do not export with validation errors.

## 51.11 Separable content heuristics

Recommendations such as "hook", story arc or cover length are useful for social media, but must not be mixed with renderer invariants.

They will be organized as a subsection or a future additional skill:

```text
skills/
├── forma/SKILL.md
└── social-carousel/SKILL.md   # future
```

That way the core can also serve posters/covers without imposing carousel editorial rules.

## 51.12 Full session

`SKILL.md` must end with a real end-to-end example that includes:

```text
get_brand
list_assets
request_asset (if applicable)
create_design
apply_operations
validate_design
fix
render_preview
export_design
```

The example must show coherent fictional real-looking versions and IDs, and a recovery from `VERSION_CONFLICT`.

---

# 52. Initial rules for the agent

```text
1. Read the Brand Kit before writing or designing.
2. Treat IDs returned by the server as the only valid handles.
3. Reuse existing assets when suitable.
4. Never fabricate an asset ID.
5. Never put a remote URL directly inside an ImageLayer.
6. If a required visual does not exist, call request_asset.
7. Prefer brand tokens for values you choose.
8. Preserve explicit user-requested literals when supported.
9. Use only fonts returned by the font catalog.
10. Use explicit line breaks in V1.
11. Group coherent edits into one atomic apply_operations batch.
12. Use the version you actually read as base_version.
13. Validate after meaningful mutations.
14. Render a preview before final export.
15. Never claim to have visually inspected a preview you could not see.
16. Fix validation errors before export.
17. On VERSION_CONFLICT, refetch and rebase; never blind-retry.
18. Never invent facts or unsupported schema fields.
```

---

# 53. Codex integration

V1 must be tested with Codex as an MCP client.

Goal:

```text
Codex
  |
stdio
  |
forma-mcp
```

The exact configuration will be documented according to the current version of Codex at implementation time.

Do not hardcode personal paths inside the repository.

The following will be added:

```text
docs/integrations/codex.md
```

---

# 54. Claude Code integration

Claude Code supports local MCP servers over stdio.

The following will be documented:

```text
claude mcp add --transport stdio ...
```

File:

```text
docs/integrations/claude-code.md
```

It will not require the Anthropic API if the user operates Claude through their normal compatible authentication.

---

# 55. ChatGPT

Not a V1 target.

Current reasons:

- ChatGPT does not connect directly to a local MCP;
- full write MCP is conditioned by plan/product;
- a remote connection requires additional infrastructure.

The following is left prepared:

```text
forma-mcp --transport http
```

for a later phase.

V1 will not be designed around ChatGPT's Developer Mode.

---

# 56. Remote MCP HTTP

`rmcp` exposes Streamable HTTP as a Tower service.

Since the main backend uses Actix, an unnecessary adapter will not be forced.

`forma-mcp` will have two modes:

```text
forma-mcp --transport stdio
forma-mcp --transport http --listen 127.0.0.1:4766
```

HTTP mode uses its own HTTP runtime compatible with `rmcp`.

On deploy:

```text
Caddy /mcp -> 127.0.0.1:4766
Caddy /    -> 127.0.0.1:4765
```

---

# 57. Remote auth

Do not implement OAuth in V1.

First remote testing:

```text
Authorization: Bearer <token>
```

Token generated with enough entropy.

The server stores/compares the hash of the token.

Config:

```text
FORMA_REMOTE_TOKEN
```

Later it can be migrated to OAuth if a specific integration requires it.

---

# 58. React build

Development:

```text
pnpm dev
```

Local production:

```text
pnpm build
```

produces:

```text
apps/web/dist
```

`forma-server` serves those static files.

Result:

```text
http://127.0.0.1:4765
```

is the whole application.

---

# 59. Config

Variables:

```text
FORMA_BIND=127.0.0.1:4765
FORMA_DATA_DIR=./data
FORMA_DATABASE_URL=sqlite://data/forma.db
FORMA_FONT_DIR=./data/fonts
FORMA_LOG=info
```

Future:

```text
FORMA_DATABASE_URL=postgres://...
FORMA_OBJECT_STORE=supabase
FORMA_REMOTE_TOKEN=...
```

---

# 60. Logging

Rust:

```text
tracing
tracing-subscriber
```

Structured logs.

Important fields:

```text
request_id
project_id
design_id
version
tool_name
render_ms
actor_type
actor_id
```

Never log:

- tokens;
- auth headers;
- binary asset content.

---

# 61. Actor metadata

Every mutation records:

```rust
enum ActorType {
    Human,
    Agent,
    System,
}
```

And optionally:

```text
actor_id
```

MCP can send:

```text
agent:codex
agent:claude
```

This enables history:

```text
v3 Human
v4 Codex
v5 Codex
v6 Human
```

---

# 62. Rust testing

## Unit

`design-core`:

- serialization;
- invalid dimensions;
- operations;
- tokens;
- version semantics.

`validator`:

- outside bounds;
- missing assets;
- text overflow;
- brand violations.

`renderer`:

- shapes;
- text;
- image fit;
- alpha;
- rotation.

---

# 63. Golden rendering tests

Fixtures:

```text
fixtures/designs/
```

Golden PNGs:

```text
fixtures/golden/
```

Test:

```text
DesignDocument -> render -> compare
```

Do not rely solely on an exact hash if there are legitimate renderer changes.

Use a pixel diff with a documented threshold.

Every golden change requires visual review.

---

# 64. Integration tests

Real test:

```text
temp SQLite
   |
CreateProject
   |
CreateDesign
   |
ApplyOperations
   |
Validate
   |
Render
   |
Export
```

It must exercise the full transaction.

---

# 65. MCP tests

The MCP SDK supports in-process/worker transports in addition to stdio/HTTP.

Tests:

```text
list tools
call get_brand
create design
apply operations
version conflict
render preview
image content returned
```

Plus a smoke test that launches the binary over stdio.

---

# 66. Frontend tests

```text
Vitest
React Testing Library
```

Cases:

- layer selection;
- property update;
- drag state;
- resize state;
- version conflict UX;
- pending AssetRequest;
- asset upload.

---

# 67. E2E of our own app

Playwright can indeed be used to test **Forma**.

This is unrelated to automating ChatGPT.

E2E flow:

```text
start forma-server
open browser
create project
upload asset
create design
move layer
export PNG
verify download
```

---

# 68. CI

GitHub Actions:

```text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
pnpm lint
pnpm typecheck
pnpm test
pnpm build
bindings check
```

Afterwards:

```text
renderer golden tests
```

---

# 69. Reproducible build

Rust:

```text
Cargo.lock committed
```

Node:

```text
pnpm-lock.yaml committed
```

Do not use ambiguous ranges in deployment.

---

# 70. Local security

Local server:

```text
127.0.0.1
```

Not:

```text
0.0.0.0
```

by default.

Remote mode must be enabled explicitly.

---

# 71. Export

`export_design` can export:

```text
single page
all pages
```

Single:

```text
design-name-page-01.png
```

Carousel:

```text
design-name/
  01.png
  02.png
  03.png
  04.png
  05.png
```

ZIP can be added later.

---

# 72. Render cache

Key:

```text
design_id
version
page_id
format
```

Path:

```text
data/previews/<design>/<version>/<page>.png
```

A version never changes.

That is why it can be cached until cleanup.

---

# 73. Asset deduplication

BLAKE3:

```text
same bytes -> same physical object
```

V1 may simplify this to per-project deduplication.

---

# 74. Garbage collection

Do not immediately delete physical files when an Asset is removed.

Reason:

historical versions may reference them.

V1:

```text
soft-delete metadata
```

Real GC is postponed.

---

# 75. Schema version

`DesignDocument` includes:

```text
schema_version
```

Initial:

```text
1
```

When the format changes:

```text
v1 -> v2 migration
```

Do not rely solely on SQL migrations.

JSON documents also need a migration path.

---

# 76. Handling schema changes

Implement:

```rust
fn migrate_document(
    value: serde_json::Value
) -> Result<DesignDocument>
```

V1:

```text
schema 1 -> parse
```

Future:

```text
1 -> 2 -> 3
```

---

# 77. Execution phases

## Phase 0 — Repo + blocking spikes

### Goal

Eliminate technical uncertainty before building UI.

### Tasks

- create the monorepo;
- configure the Cargo workspace;
- configure the pnpm workspace;
- minimal React/Vite;
- Actix health endpoint;
- minimal SQLite migration;
- `usvg/resvg` spike;
- local font spike;
- bounding box spike;
- text flattening spike;
- PNG spike;
- layer IDs/groups spike;
- `rmcp` stdio spike;
- return a dummy PNG image through MCP.

### Acceptance

From Codex/Claude it must be possible to call:

```text
forma_status
```

and:

```text
render_test_card
```

and receive a PNG.

Do not continue if the renderer/preview is not technically clear.

---

## Phase 1 — Domain Core

### Tasks

- typed IDs;
- Project;
- BrandKit;
- DesignDocument;
- Page;
- Layer enums;
- operations;
- schema version;
- errors;
- `ts-rs` bindings;
- tests.

### Acceptance

A JSON fixture:

```text
deserialize
apply operations
serialize
```

with no semantic loss.

---

## Phase 2 — Renderer

### Tasks

- FontRegistry;
- color tokens;
- page background;
- Rectangle;
- Ellipse;
- Line;
- ImageLayer;
- TextLayer;
- explicit multiline;
- bbox inspection;
- SVG normalization;
- PNG rasterization;
- render cache.

### Acceptance

An Instagram 4:5 fixture with:

```text
background
image
headline
subtitle
CTA
```

produces correct SVG and PNG.

---

## Phase 3 — Validator

### Tasks

- structural validator;
- canvas bounds;
- safe area;
- missing asset;
- font missing;
- text overflow;
- brand token resolution;
- report format.

### Acceptance

Unit tests for every code.

---

## Phase 4 — Persistence

### Tasks

- migrations;
- repositories;
- SQLite;
- design snapshots;
- optimistic concurrency;
- asset metadata;
- asset requests;
- exports.

### Acceptance

Two mutations with the same `base_version`:

```text
first  -> success
second -> VERSION_CONFLICT
```

---

## Phase 5 — REST server

### Tasks

- Actix routes;
- DTOs;
- error mapper;
- multipart upload;
- static files;
- preview endpoints;
- export endpoints.

### Acceptance

Every main operation works with curl/an HTTP client without a frontend.

---

## Phase 6 — React editor

### Tasks

- project view;
- editor layout;
- page list;
- layer list;
- preview SVG;
- selection overlay;
- move;
- resize;
- properties;
- assets panel;
- Brand panel;
- validation panel;
- autosave/mutations;
- conflict UI.

### Acceptance

A complete piece can be built manually without MCP.

---

## Phase 7 — AssetRequest UX

### Tasks

- pending request list;
- modal/panel;
- suggested prompt;
- copy prompt;
- drag/drop;
- upload;
- resolve request.

### Acceptance

Agent creates a request -> the UI shows it -> the user uploads -> the request becomes resolved.

---

## Phase 8 — MCP

### Tasks

- `forma-mcp`;
- internal REST client;
- tools;
- structured results;
- MCP image content;
- tool annotations where appropriate;
- stdio integration tests.

### Acceptance

From a host:

```text
get_brand
create_design
apply_operations
validate_design
render_preview
```

produces a real design.

---

## Phase 9 — Agent Skill

### Tasks

- minimal `AGENTS.md`;
- `skills/forma/SKILL.md`;
- `DesignRecord` vs `DesignDocument` mental model;
- schema documented with valid fields;
- Brand Tokens, voice and font catalog;
- tool workflow;
- atomic batch rules;
- asset request rules;
- preview honesty rule;
- validation loop;
- error recovery table;
- design heuristics;
- full end-to-end example;
- documented `VERSION_CONFLICT` case;
- evaluation prompts for Codex and Claude.

### Acceptance

Target prompt:

```text
Create a 4-slide carousel about X using the branding.
```

Without manual tool instructions, the agent:

1. reads the Brand Kit;
2. reviews the assets;
3. requests an asset if one is missing;
4. creates/mutates through real IDs;
5. validates;
6. inspects the preview if it can;
7. corrects;
8. exports.

It must also pass conversational recovery tests:

```text
VERSION_CONFLICT
INVALID_OP
ASSET_NOT_FOUND
FONT_NOT_FOUND
```

The agent must not enter blind retries nor claim to have seen a preview that the host did not deliver to it.

---

## Phase 10 — Hardening

### Tasks

- golden tests;
- upload security;
- SVG sanitization;
- large file handling;
- error UX;
- backup;
- cleanup;
- tracing;
- graceful shutdown.

---

## Phase 11 — Remote deploy

### Tasks

- aarch64 build;
- optional Docker;
- Oracle VM;
- systemd;
- Caddy;
- TLS;
- MCP HTTP;
- bearer auth;
- external smoke tests.

### Acceptance

An authorized remote MCP host can create/render a piece.

---

## Phase 12 — Optional Supabase adapter

Only after the SQLite deploy works.

### Tasks

- PostgreSQL repository;
- Supabase Storage object store;
- Postgres migrations;
- env configuration;
- integration tests.

Do not migrate local to Postgres out of obligation.

---

# 78. Realistic recommended order for a first weekend

## Day 1

```text
Repo
Actix
DesignDocument
Renderer spike
Text
Images
SVG
PNG
```

Goal:

> manual JSON -> real PNG.

## Day 2

```text
SQLite
Operations
Validator
REST
MCP stdio
```

Goal:

> Codex/Claude -> tools -> real PNG.

## Day 3 if it exists

```text
Minimal React editor
Asset upload
AssetRequest
polish
```

Goal:

> AI and human edit the same document.

Do not attempt a deploy during the first block until the core is stable.

---

# 79. Definition of Done for V1

V1 is finished when exactly this can be done:

1. start `forma-server`;
2. open React;
3. create a project;
4. configure the Brand Kit;
5. upload a photo;
6. connect Codex or Claude Code to `forma-mcp`;
7. ask: `Create a 5-slide carousel about X`;
8. the agent queries branding/assets;
9. creates the design;
10. validates;
11. generates a preview;
12. inspects the preview;
13. corrects it if needed;
14. exports five PNGs;
15. the user opens the same design in React;
16. moves an element;
17. saves;
18. the agent receives `VERSION_CONFLICT` if it was working on an old version;
19. refreshes and continues;
20. no LLM API was called by Forma.

---

# 80. Definition of Done for AssetRequest

Case:

```text
Create an ad for a futuristic coffee shop.
```

No suitable photo exists.

This must happen:

```text
request_asset
```

UI:

```text
Image required
Description
Suggested prompt
Upload
```

The user provides a file.

Then:

```text
AssetRequest = Resolved
resolved_asset_id = ...
```

The design can continue.

That completely replaces image generation in V1.

---

# 81. What is left ready for future generation

When a host can generate and transfer assets in a supported way:

```text
Host image generation
        |
        v
image attachment/file
        |
        v
resolve_asset_request
        |
        v
Asset
```

These would not change:

- DesignDocument;
- renderer;
- validator;
- editor;
- persistence;
- export.

Only who resolves the `AssetRequest` changes.

---

# 82. Future auto-wrap

V1:

```text
explicit \n
```

V2:

```text
TextWrapMode::Word
```

It must be implemented in Rust.

Do not accept browser layout as the authority.

The algorithm will have to use the same typographic stack as the renderer to measure lines.

No concrete implementation is fixed yet, until the typographic spike is completed and the real required APIs are measured.

---

# 83. Future WASM

Do not use WASM in V1.

Only reconsider it if a real problem appears, such as:

- heavy geometry;
- complex snapping;
- hundreds of layers;
- a layout solver;
- the need to share exactly the same geometry between Rust/browser.

In that case a pure crate would be extracted:

```text
design-geometry
```

compilable to:

```text
native
wasm32
```

React would remain the UI framework.

---

# 84. Future grouping

No `GroupLayer` in V1.

Reason:

- nested selection;
- composed transforms;
- resize;
- z-order;
- bounds;
- undo;
- rendering.

V2 may add it only after stabilizing transforms.

---

# 85. Future templates

A template will essentially be:

```text
DesignDocument
+
role/slot metadata
```

Each slot should be able to expose:

```rust
TemplateRole {
    page_index,
    role,
    max_chars,
    sample_text,
}
```

Example:

```text
page 0  headline    maxChars=55
page 0  hero_image
page 1  body        maxChars=180
page 3  cta         maxChars=80
```

This turns a template into a **structural brief for the agent**, not merely a duplicated document.

The agent:

1. reads the roles;
2. decides whether the shape of the template matches the content;
3. duplicates;
4. replaces copy/assets;
5. validates overflow;
6. adjusts only when necessary.

It does not need a second renderer.

---

# 86. Future stock search

It will not be in the backend initially.

Future architecture:

```text
AssetRequest
   |
StockResolver
   |
provider adapter
```

It must store:

- source page;
- original URL;
- license;
- attribution;
- provider.

Never treat an image found on the Internet as automatically reusable.

---

# 87. Future Artifact Layers

The Agentcy's skill exposes an idea that is especially useful for our original problem of depending on base images: representing UI, dashboards, chats, receipts, charts or comparisons as **components drawn by code**, not as screenshots.

It is not part of V1.

V2/V3 may add:

```rust
Layer::Artifact(ArtifactLayer)
```

Possible initial `kind` values:

```text
browser
window
chat
chart
table
receipt
notification
comparison
arrow
```

Advantages:

- zero external assets;
- fully editable;
- scalable;
- deterministic branding;
- ideal for technical/software content;
- reduces how often the workflow needs an `AssetRequest`.

Important restriction:

> Artifact must not become a "mini arbitrary HTML".

Each kind will have a closed schema and its own Rust renderer.

Conceptual example:

```json
{
  "type": "artifact",
  "kind": "browser",
  "role": "product_demo",
  "frame": { "x": 120, "y": 320, "w": 840, "h": 620 },
  "title": "Dashboard",
  "subtitle": "localhost:4765",
  "rows": [
    { "label": "Designs", "value": "12" },
    { "label": "Ready", "value": "9", "strong": true }
  ],
  "accent": "$brand.primary"
}
```

This feature must be evaluated after stabilizing:

- base layers;
- text metrics;
- transforms;
- selection;
- validation.

---

# 88. Future social publishing

It will be a completely separate layer:

```text
ExportedArtifact
      |
PublishingService
      |
Instagram / LinkedIn / etc.
```

Do not add social logic inside `DesignDocument`.

---

# 89. Deployment details

## Oracle

Likely target if Ampere A1 is used:

```text
aarch64-unknown-linux-gnu
```

Do not assume x86 in Docker.

Build multiarch or compile directly on ARM.

---

# 90. Oracle Free Tier — current figure to verify at deploy time

Oracle's current Always Free documentation indicates, for Always Free accounts:

```text
2 OCPU
12 GB RAM
```

equivalent to:

```text
1500 OCPU-hours
9000 GB-hours / month
```

for Ampere A1.

It also warns that there may be a lack of capacity in specific regions.

Therefore the plan does not depend on having a VM available.

If Oracle has no capacity:

```text
the remote deploy is postponed or another host is used
```

without changing the architecture.

---

# 91. Supabase Free — current figure

As of the date of this document, the Free plan advertises:

```text
500 MB database
1 GB file storage
5 GB egress
5 GB cached egress
2 active projects
pause after 1 week inactivity
```

It is adequate for small testing; unlimited free production must not be assumed.

---

# 92. Researched dependencies

Baselines verified as of 2026-08-27:

```text
Actix Web      4.15.0
rmcp           3.1.x
resvg          0.48.1
usvg           0.48.1
ts-rs          12.0.1
```

This does not mean the project should use floating versions.

Versions will be pinned in lockfiles at initialization.

---

# 93. Decisions that are NOT left open

```text
Frontend:            React + TypeScript
Backend:             Rust
HTTP backend:        Actix Web
WASM V1:             no
Yew:                 no
LLM APIs:            no
Image generation V1: no
Local MCP:           stdio
Local persistence:   SQLite
Local assets:        filesystem
Render authority:    Rust
Raster:              resvg
SVG normalization:   usvg
Type bindings:       Rust -> TypeScript
```

---

# 94. Decisions that require a spike before being frozen

## 93.1 Text layer flattening/editor DOM

Validate:

- the exact behavior of groups/IDs after `usvg`;
- the final interactive SVG strategy.

Known fallback:

```text
flatten each layer
+
rewrap with Forma-owned IDs
```

## 93.2 Default font

Select only after checking:

- license;
- required weights;
- compatibility with `usvg`;
- visual result.

## 93.3 Pixel diff threshold

Do not invent a threshold before having real renders.

Create the baseline during the renderer phase.

---

# 95. Main risks

## R1 — quality of the agent's design

Mitigation:

- Brand Kit;
- design guide;
- batch operations;
- deterministic validator;
- visual preview;
- examples;
- future templates.

## R2 — text overflow

Mitigation:

- server-side bounding boxes;
- explicit line breaks in V1;
- validator;
- future auto-wrap.

## R3 — preview/export mismatch

Mitigation:

- a single backend renderer;
- flattened SVG;
- PNG from the same render tree;
- golden tests.

## R4 — human/agent concurrency

Mitigation:

- optimistic versioning;
- a single server authority;
- version conflict.

## R5 — MCP host differences

Mitigation:

- the core does not depend on the host;
- small tools;
- standard stdio;
- structured results;
- MCP image content.

## R6 — missing images

Mitigation:

- AssetRequest;
- the user as a universal fallback.

## R7 — scope creep

Mitigation:

- image generation outside V1;
- no social publishing;
- no WASM;
- no collaboration;
- no local auth.

---

# 96. Initial metrics

Do not fix performance numbers without a benchmark.

Do record:

```text
render duration
PNG encode duration
SVG size
PNG size
DB mutation duration
asset upload duration
MCP tool duration
```

After Phase 2, create a baseline.

Only then define budgets.

---

# 97. First useful milestone

The first important milestone is not React.

It is:

```text
design.json
    |
forma renderer
    |
final.png
```

With:

```text
text
image
shape
brand token
```

If that is not solid, do not build the editor.

---

# 98. Second useful milestone

```text
Codex / Claude
      |
      | MCP
      v
create_design
apply_operations
validate
render_preview
      |
      v
PNG
```

That demonstrates the thesis of the project.

---

# 99. Third useful milestone

```text
Agent edits
     |
same DesignDocument
     |
Human edits in React
     |
version conflict safe
```

This turns the experiment into a real tool.

---

# 100. Criterion for adding a feature

A feature is accepted when it meets at least one of these:

1. it unblocks the main workflow;
2. it eliminates real repetitive manual work;
3. it improves output consistency;
4. it fixes a failure observed in tests;
5. it enables a concrete integration.

It is not accepted merely because "it would be interesting".

---

# 101. Expected result

```text
                     subscription AI
                           |
                  Codex / Claude
                           |
                          MCP
                           |
                     Forma Runtime
                           |
          +----------------+----------------+
          |                |                |
        Brand            Assets          Design
          |                |                |
          +----------------+----------------+
                           |
                        Validate
                           |
                         Render
                           |
                      SVG -> PNG
                           |
          +----------------+----------------+
          |                                 |
        Agent                             React
      inspection                         editor
```

Without:

```text
LLM API billing
image API billing
cloud dependency
browser automation against AI services
```

And with a clean path to later add:

```text
stock
host image generation
templates
remote MCP
Supabase
social publishing
WASM geometry
```

without rewriting the core.

---

# 102. Reference skill studied

During the design, The Agentcy's public skill provided by the user was reviewed:

```text
https://theagentcy.app/skill
```

Patterns explicitly adopted:

- a portable document separated from server identity/versioning;
- IDs as the only editing handles;
- z-order defined by layer order;
- Brand Tokens + voice + font catalog;
- assets imported first and referenced by `asset_id`, never URLs inside the design;
- atomic batches with `base_version`;
- the prescribed workflow `brand -> assets -> create/edit -> validate -> preview -> fix -> export`;
- error responses with `code/message/hint`;
- honesty about whether the agent could actually inspect the preview;
- future templates with roles and character budgets;
- artifact layers as a future extension.

Patterns that are **not** automatically copied:

- pricing/plan gates;
- their product's onboarding;
- Instagram scheduling/publishing;
- video clipping;
- reach/follow automation;
- their exact catalog of ops;
- exact limits that only answer to their implementation;
- lenient normalization of cosmetic values.

Our skill must describe our schema and real behavior, and must never claim compatibility with The Agentcy.

---

# 103. Verified technical sources

## Actix Web

- Docs.rs — Actix Web 4.15.0  
  https://docs.rs/crate/actix-web/latest

- Actix Getting Started  
  https://actix.rs/docs/getting-started/

## Rust MCP SDK

- Official Rust MCP SDK / `rmcp`  
  https://github.com/modelcontextprotocol/rust-sdk

- `rmcp` docs  
  https://docs.rs/rmcp/latest/rmcp/

The current documentation confirms support for:

- stdio;
- Streamable HTTP server/client;
- tool schemas;
- image content;
- structured content.

## MCP image results

- MCP Tools specification  
  https://modelcontextprotocol.io/specification/2025-11-25/server/tools

- `rmcp` ImageContent  
  https://docs.rs/rmcp/latest/rmcp/model/struct.RawImageContent.html

## SVG / rendering

- resvg  
  https://github.com/linebender/resvg

- resvg 0.48.1  
  https://docs.rs/crate/resvg/latest

- usvg  
  https://docs.rs/usvg/latest/usvg/

- usvg Text bounding boxes  
  https://docs.rs/usvg/latest/usvg/struct.Text.html

- usvg Tree serialization  
  https://docs.rs/usvg/latest/usvg/struct.Tree.html

- usvg WriteOptions (`preserve_text = false` by default)  
  https://docs.rs/usvg/latest/usvg/struct.WriteOptions.html

## Rust -> TypeScript

- ts-rs  
  https://docs.rs/crate/ts-rs/latest

## Claude Code MCP

- Claude Code MCP docs  
  https://code.claude.com/docs/en/mcp

The current documentation confirms local MCP servers over stdio and remote HTTP.

## ChatGPT MCP

- Developer mode and MCP apps in ChatGPT  
  https://help.openai.com/en/articles/12584461

The current documentation indicates that ChatGPT does not connect directly to a local MCP server; the availability of write/full MCP depends on the plan/product.

## Supabase

- Pricing  
  https://supabase.com/pricing

## Oracle Cloud Always Free

- Always Free Resources  
  https://docs.oracle.com/en-us/iaas/Content/FreeTier/freetier_topic-Always_Free_Resources.htm

---

# 104. Recommended next step

The next piece of work must not be starting to build the whole UI.

It must be creating a **Phase 0 implementation brief** and executing the blocking spikes:

```text
1. monorepo
2. Actix
3. minimal DesignDocument
4. usvg text
5. text bbox
6. SVG flatten
7. resvg PNG
8. rmcp stdio
9. MCP image result
```

When those nine points work together, the main architecture stops being hypothetical and product implementation can begin.

---

# 105. Amendment: OAuth enters scope for hosted deployments

**Technical reference date:** August 30, 2026.

§2.2 excluded OAuth from V1. That exclusion was correct for a runtime one person
runs on their own machine, and it stops being correct the moment the deployment
is meant to be reached by an agent belonging to a named person.

The §100 criterion admitting it is **(5) it enables a concrete integration**:
Claude custom connectors, and ChatGPT MCP apps, complete a standards-based OAuth
flow against a remote MCP URL. Verified on the reference date: ChatGPT's
documented MCP authentication options are **OAuth or none** — it accepts no
manual bearer header — so for that host OAuth is not the preferred route, it is
the only one.

Criterion **(1) it unblocks the main workflow** applies as well. Phase 11 left
the browser editor unusable remotely, deliberately and with the reason recorded:
the deployment-wide bearer protected the whole origin and the SPA had nowhere to
hold a credential.

What this amendment does **not** change:

- Supabase remains excluded as a mandatory dependency. It is introduced for
  identity only, behind `FORMA_AUTH_MODE`, and local Forma must run without it.
- Phase 12 remains the optional persistence adapter phase. Application data stays
  in SQLite and the local filesystem.
- Forma still integrates no LLM provider.

Sections 106 to 117 specify the phase. They are an addition; no earlier section
is rewritten.

---

# 106. Phase 11.5 — hosted identity and agent distribution

## 106.1 Purpose

The boundary moves from **deployment-wide secret** to **hosted user identity plus
account-scoped authorization**.

A person must be able to:

```text
1. open the hosted Forma editor
2. sign in
3. see only their own projects, assets and designs
4. connect a supported agent with the public MCP URL and that
   provider's normal OAuth flow
5. use a manual token only where the host cannot do OAuth
6. revoke one agent without rotating credentials for the rest
7. and local Forma keeps working with no Supabase at all
```

```text
person
  |
  v
https://forma.hintly.online
  |
  | sign in
  v
Forma account
  |
  +---------------------------+
  |                           |
  | Connect Claude            | Developer / CLI
  | URL only, OAuth           | API key, bearer PAT
  v                           v
Claude / remote host      Codex CLI / Claude Code / Gemini CLI
          \                   /
           +--------+--------+
                    |
                    v
         https://forma.hintly.online/mcp
                    |
                    v
             account-scoped Forma
```

## 106.2 Decisions frozen here

**Identity provider.** Supabase Auth, for human identity and as the upstream
OAuth 2.1 Authorization Server. Forma implements no password database, no
password reset, no authorization-code issuer, no refresh-token issuer, no client
registry, no OIDC discovery and no JWKS signing.

**Application data.** Unchanged: SQLite plus the local filesystem. This phase
moves no design, asset or brand data anywhere.

**Browser authentication.** The normal Supabase browser session. React obtains a
Supabase access token and sends `Authorization: Bearer <token>` to
`/api/v1/*`; Forma validates the JWT locally through JWKS. There is no Forma
password, no deployment token in the SPA, and **no second Forma session cookie
layered on top of the Supabase session**.

**Remote MCP authentication.** Two credential types: a Supabase OAuth access
token, or a Forma personal access token. OAuth is the consumer path; the PAT is
for CLI and automation hosts.

**Deployment mode.**

```text
FORMA_AUTH_MODE=local      local-first, no Internet identity
FORMA_AUTH_MODE=supabase   hosted accounts, browser auth, OAuth MCP, PATs
```

## 106.3 Provider maturity, recorded

Supabase's OAuth 2.1 server was **in beta** on the reference date, and free to
use during the beta period across all plans. The resource-server boundary
specified below is standard OAuth and survives a provider change, but the phase
rests on a beta feature with undefined future pricing and that is a known,
accepted risk rather than an oversight.

MCP `2026-07-28` deprecates Dynamic Client Registration in favour of Client ID
Metadata Documents — "DCR continues to work for backward compatibility, but will
be removed in a future version" — and its stateless protocol core removes the
`initialize`/`initialized` handshake and the `Mcp-Session-Id` header that
`forma-mcp` uses today. Forma implements no client-registration endpoint of its
own; DCR is enabled in Supabase for present interoperability and that assumption
stays isolated to auth configuration. **The transport migration and the CIMD
migration age together**; whoever does one should expect the other on the same
trip. Neither blocks this phase.

---

# 107. Account model

## 107.1 Types and tables

```rust
AccountId(Uuid)
AuthUserId(Uuid)
ApiKeyId(Uuid)
```

```text
Account
  |
  +-- AccountMembership
  |      user_id
  |      email
  |      role
  |
  +-- Project
         |
         +-- Brand
         +-- Assets
         +-- Designs
         +-- Templates
         +-- AssetRequests
```

```sql
accounts
--------
id
name
legacy              -- the pre-accounts deployment, claimable once
claimed_at
created_at
updated_at

account_memberships
-------------------
account_id
user_id             -- authoritative: the Supabase subject
email               -- descriptive only, never used to authorize
role
created_at

account_invites
---------------
email               PRIMARY KEY
created_at
claimed_at
claimed_by_user_id

api_keys
--------
id
account_id
created_by_user_id
name
token_prefix
token_hash
created_at
last_used_at
revoked_at
```

`projects.account_id` becomes `NOT NULL`.

`account_id` is **not** duplicated onto assets, designs, templates, brand kits or
asset requests. Each already carries `project_id ... REFERENCES projects (id) ON
DELETE CASCADE`, so ownership is one hop away and is recorded in exactly one
place. A second copy is a second thing that can disagree, and disagreement about
who owns a row is the failure this phase exists to prevent.

The cost is a JOIN on scoped reads. That is the right price.

`email` on a membership exists so a human reading the table after the identity
provider is gone can tell whose row is whose, and so §113.3's recovery command
can rebind an account to a fresh subject. It authorizes nothing.

## 107.2 One user, one account

For this phase: one authenticated user resolves to one active account. No account
switcher, no invitations UI, no organization management, no team membership.

`account_memberships` exists rather than an `owner_user_id` column on `accounts`
so team membership can arrive later without changing how a Project is owned.

## 107.3 Who is allowed in

**Signing in is not the gate. Owning an account is.**

A stranger who completes Supabase sign-up holds an identity and reaches nothing:
no project, no asset, no render, no tool. Open registration at the identity
provider is therefore not a data risk, and Supabase's signup setting is not where
this is decided.

The gate is account creation, and it requires an unclaimed invite matching the
authenticated user's verified email. Invites are minted on the machine:

```text
forma-server invite <email>
```

Everyone else receives `ACCOUNT_REQUIRED`, worded as a state and not a failure:
*this Forma is invite-only*.

**The gate lives in Forma, not in the provider**, because Forma owns membership
and a client never mints server-owned identity — the same reason
`PUT /projects/:id/brand` takes a `BrandKitDraft`. Supabase's own documentation
warns that Dynamic Client Registration allows unrestricted registration; the
invite gate is what makes that acceptable.

**Per-account quotas are deliberately out of this phase.** With an invite gate,
everyone who can consume the disk was invited by name. `DEFAULT_MAX_UPLOAD_BYTES`
still caps a single upload. §100 says a quota system waits for evidence it is
needed; revisit when an account exists that the operator did not personally
invite.

---

# 108. The client never chooses its account

`account_id` is a security boundary, not an input field. It must never be
accepted from a REST body, a query parameter, an MCP tool schema or an agent
prompt.

Every authenticated request resolves:

```rust
AuthContext {
    account_id: AccountId,
    user_id: Option<AuthUserId>,
    auth_kind: AuthKind,
    client_id: Option<String>,
    api_key_id: Option<ApiKeyId>,
}
```

```rust
enum AuthKind {
    Local,
    BrowserUser,
    McpOAuth,
    McpPat,
}
```

`get_project(project_id)` means:

```sql
SELECT ... FROM projects WHERE id = ? AND account_id = current_account;
```

It never means `get_project(account_id_from_client, project_id)`.

MCP tool schemas do not gain `account_id`. Existing project, design and asset ids
remain the handles exposed to agents.

## 108.1 The scope is carried by the type system

Auditing 36 repository methods across 7 traits is not a mechanism; it is hoping.

> Every repository method that reads or writes project-owned state takes an
> `AccountId` as a parameter — not an `Option<AccountId>`, not a field on a
> context struct a caller may forget to populate.

A method that omits the scope does not compile. This is the same device as
*repositories stamp their own `created_at`*: the type carries the invariant so no
call site can be careless. It is also what satisfies the requirement that the
scope live below the HTTP handler — after this there is no unscoped method left
to call from anywhere.

The object store is the one exception. It addresses bytes by hash, knows nothing
about projects, and is reachable only through an asset row that is already
scoped.

## 108.2 Cross-account access is not found — unconditionally

Every scoped read filters by `account_id`, and a miss returns
`PROJECT_NOT_FOUND` / `DESIGN_NOT_FOUND` — the identical code, message and status
as an id that was never issued. There is no branch that can tell them apart,
because after §108.1 there is no unscoped read to compare against.

**Never `403` for an object in another account.** A `403` is an admission that
the id exists. `403` appears exactly once in this phase, for `ACCOUNT_REQUIRED`:
a valid identity with no membership at all, which leaks nothing about any object.

Do not rely on UUID secrecy as a substitute for the filter.

---

# 109. Prerequisites that sit below identity

These three are consequences of the existing code. They are not identity work,
they belong to the first execution slice of §117, and each is a way the phase
fails silently if it is skipped.

## 109.1 The bind guard stops meaning anything

`Config::binding_is_safe` refuses to start when the bind address is not loopback
and no `FORMA_REMOTE_TOKEN` is set. It rests on a premise: *loopback means only
this machine can reach it.*

The deployment binds `127.0.0.1:4765` with Caddy in front. **That premise has
been false since Caddy went in front of it** — the internet reaches loopback
through the proxy. It is masked only because a token happens to be set, so the
guard returns on its first line and its reasoning is never reached.

§113.3 retires that token. At that moment the guard sees loopback and approves,
`require_bearer` treats *no token configured* as its **pass-through** case — which
is correct today, because no token means local — and a hosted deployment with
`FORMA_AUTH_MODE` unset or `FORMA_SUPABASE_JWKS_URL` missing starts cleanly and
serves the whole API unauthenticated, with nothing in any log saying so.

Both halves change together:

1. `binding_is_safe` becomes a function of the auth mode. In `supabase` mode it
   requires the Supabase issuer and JWKS configuration and refuses without them.
   The existing check stays for `local`.
2. `require_bearer`'s open case is gated on `FORMA_AUTH_MODE=local`, not on the
   absence of a token. **"No credential configured" must stop being a synonym for
   "no credential required."**

This is the same class of failure §70 exists to prevent, arriving by the opposite
route: not a public bind with no token, but a loopback bind the world can reach.

## 109.2 `forma-mcp` holds one credential for its lifetime

`ServerClient::new(base_url).with_token(token)` is constructed once from the
process environment, and the HTTP session factory clones the same constant for
every session. That is right for one deployment with one credential.

After this phase every inbound MCP request carries a **different person's**
identity. A process-wide credential cannot express that. From the outside
`forma-mcp` is a resource server; internally it is also a client, and both halves
change:

- inbound: validate the OAuth JWT or the PAT and resolve an `AuthContext`;
- outbound: forward that identity **per request**, so `forma-server` scopes on the
  caller and not on the proxy.

The form that preserves *`forma-mcp` must never open SQLite; a single mutation
path keeps versioning and validation in one authority* is for `forma-mcp` to pass
the caller's credential through and for `forma-server` to verify it once for both
entry points. Two verifiers can disagree about who someone is.

## 109.3 The error envelope has no 403

`ApiError` offers `bad_request`, `unauthorized`, `payload_too_large` and
`internal`, and the code-to-status map contains no `FORBIDDEN` at all.
`403 ACCOUNT_REQUIRED` is the one status this phase genuinely needs and the
codebase has never had. Add the constructor and the mapping.

Without it an implementer reaches for `unauthorized`, and a caller cannot tell
*your credential is bad* from *your credential is fine and you have no
workspace*. Those require different actions from the person reading them.

---

# 110. The hosted browser

## 110.1 The security boundary

The React bundle is public. The data is not.

```text
PUBLIC
GET /
GET /assets/*
GET /favicon*
GET /login
GET /oauth/consent
GET /agents
GET /skill

PROTECTED HUMAN API
/api/v1/*

PROTECTED AGENT API
/mcp
```

Serving `index.html`, JavaScript, CSS and public brand assets without
authentication is intentional; the shell is what a visitor with no credential is
*for*. No secret may be embedded in those files.

This replaces Phase 11's assertion that the origin refuses without a credential:

```text
GET /                      -> 200
GET /assets/<bundle>.js    -> 200
GET /api/v1/projects       -> 401 without user auth
POST /api/v1/designs       -> 401 without user auth
POST /mcp                  -> 401 without MCP auth
```

The frontend may contain the Supabase project URL and the publishable key. It
must never contain a `service_role` key, `FORMA_REMOTE_TOKEN`, a PAT, an OAuth
client secret, or database credentials.

## 110.2 Sign-in

The minimum hosted method is Supabase email magic-link / OTP. Additional
providers may be enabled without changing this architecture and are not required
to close the phase. Configure a real mail provider before treating email auth as
production-ready.

```text
no session      -> render login / onboarding
valid session   -> Authorization: Bearer <token> on /api/v1/*
401 from REST   -> refresh the Supabase session;
                   if none remains, return to login
```

## 110.3 Routing — three required routes that 404 today

The phase requires `/login`, `/oauth/consent` and `/agents`. Supabase redirects
the user to `https://<domain>/oauth/consent?authorization_id=<id>`.

`App.tsx` states it plainly — *"Two views, no router"* — and the static service is
mounted with `index_file` and **no `default_handler`**. `index_file` answers a
directory request only. Measured against the built app:

```text
GET /                -> 200
GET /oauth/consent   -> 404
GET /login           -> 404
GET /agents          -> 404
```

So the OAuth flow does not fail at the token exchange or a signature check. It
fails when Supabase hands the person back and Forma answers 404 — a step that
only appears when a real provider drives a real browser, which is the most
expensive place in this phase to discover anything.

Two changes, neither optional:

- a `default_handler` on the static service returning `index.html` for any
  non-`/api/` path that is not a file;
- a router in the SPA. V1 deliberately went without one and `App.tsx` anticipated
  this exact moment: *"it becomes worth adding when a URL has to be shareable,
  which is a Phase 11 concern."*

## 110.4 Content Security Policy

Browser bearer tokens make XSS credential theft, so the hosted editor carries a
strict CSP. It is nearly free: the editor's fonts are self-hosted woff2, there is
no CDN and no Google Fonts link, and there is no `fetch` to any absolute URL
anywhere in the frontend. Supabase adds exactly one origin, to `connect-src`.

```text
default-src    'self';
script-src     'self';
style-src      'self';
style-src-attr 'unsafe-inline';
img-src        'self' data: blob:;
font-src       'self';
connect-src    'self' https://<project-ref>.supabase.co;
frame-ancestors 'none';
base-uri       'none';
object-src     'none';
```

`style-src-attr 'unsafe-inline'` is required and is not a loophole: React writes
inline `style` attributes and attribute styles cannot execute script.
`img-src data:` is required because the renderer embeds asset bytes as data URIs.

**Set it in `forma-server`, not in Caddy.** A header present only in the hosted
deployment is a header no test ever sees; one served by the binary is exercised
by the Playwright walkthrough on every run.

**The one injection point, named.** The editor writes the renderer's SVG with
`dangerouslySetInnerHTML`. That SVG is produced by our own renderer from the
document, and uploaded SVG assets are sanitized by re-serializing through usvg —
a whitelist by construction, which drops scripts and handlers because usvg never
understood them rather than because a filter matched them. The CSP here is
defence in depth over a boundary that is already sound. It stays required: after
this phase, what sits behind that boundary is a credential.

---

# 111. Forma as an OAuth resource server

## 111.1 Supabase configuration

Enable the OAuth 2.1 Authorization Server with an **asymmetric** signing key
(RS256 or ES256). This is required, not preferred: asymmetric signing is
mandatory whenever the `openid` scope is requested, which the consent flow does.

```text
Site URL:            https://forma.hintly.online
Authorization Path:  /oauth/consent
```

Supabase provides `/oauth/authorize`, `/oauth/token`, authorization-server and
OIDC discovery, JWKS, authorization-code with PKCE, and refresh tokens. Enable
Dynamic Client Registration for current MCP-client compatibility, subject to
§107.3.

## 111.2 Protected Resource Metadata

Canonical protected resource:

```text
https://forma.hintly.online/mcp
```

```text
GET /.well-known/oauth-protected-resource/mcp
```

```json
{
  "resource": "https://forma.hintly.online/mcp",
  "authorization_servers": [
    "https://<project-ref>.supabase.co/auth/v1"
  ],
  "bearer_methods_supported": ["header"]
}
```

A root alias at `/.well-known/oauth-protected-resource` is permitted for client
interoperability; the canonical resource identifier stays `/mcp`.

An unauthenticated `/mcp` request answers HTTP `401` — **not** an MCP tool-level
error, because provider discovery depends on that distinction — with:

```http
WWW-Authenticate: Bearer resource_metadata="https://forma.hintly.online/.well-known/oauth-protected-resource/mcp"
```

```text
POST /mcp -> 401 + WWW-Authenticate
   -> Protected Resource Metadata
   -> Supabase authorization-server metadata
   -> client registration / metadata
   -> PKCE authorization
   -> Forma consent UI
   -> Supabase token exchange
   -> POST /mcp with Bearer <oauth-token>
```

## 111.3 The consent screen

Supabase redirects to `/oauth/consent?authorization_id=<id>`. The page preserves
the id, requires a valid session, returns to the same URL after login if needed,
calls `supabase.auth.oauth.getAuthorizationDetails(authorization_id)`, shows the
requesting client and the requested scopes, offers Allow and Deny, calls
`approveAuthorization` or `denyAuthorization`, and redirects to the exact
`redirect_url` Supabase returns.

**The consent screen must not imply that Supabase's scopes are Forma
permissions.** Supabase supports `openid`, `email`, `profile` and `phone` for
this flow; those govern identity data, not access to Forma projects.
Forma-specific authorization is the account boundary. An authorized connection
receives the existing MCP tool surface for that account, and the screen says so
plainly:

```text
Claude wants to use Forma.

This connection can use Forma's agent tools on projects in your account,
including reading branding and assets and creating or editing designs,
previews and exports.

It cannot access projects belonging to another Forma account.

[ Deny ]  [ Allow ]
```

Do not present fine-grained read/write scope control until Forma implements it.

## 111.4 Token validation

Forma validates Supabase JWTs **locally** through JWKS. Do not call Supabase per
request to ask whether a token is valid. Verification covers: signature against
JWKS, an approved asymmetric algorithm, issuer, expiry, not-before when present,
subject, and audience. Keys are cached and refreshed on normal JWKS semantics.

For an MCP access token, require the `client_id` claim — Supabase populates it for
OAuth clients and leaves it null for direct user sessions, which is exactly the
distinction §112 rests on — and map:

```text
sub        -> AuthUserId
client_id  -> external agent client identity
```

then resolve `AuthUserId -> AccountMembership -> AccountId`. No membership means a
valid Supabase user who is not an authorized Forma account: reject.

**Audience.** Configure a Custom Access Token Hook so MCP tokens carry an
audience for the Forma MCP resource, following Supabase's own MCP example of a
per-client audience:

```text
https://forma.hintly.online/mcp/{client_id}
```

Validate the prefix and accept the client suffix. The per-client form gives finer
revocation and carries the approved-agent gate in the hook itself. Browser
sessions keep Supabase's normal authenticated-user audience.

**Do not accept an arbitrary Supabase access token at `/mcp` because its
signature is valid.** Without the audience check, any user's browser token from
the same project would be a universal MCP credential.

---

# 112. Credentials, and what each one opens

```text
/api/v1/*
    accepts: Supabase browser user session JWT, or a Forma PAT
    rejects: unauthenticated calls

/mcp
    accepts: Supabase OAuth MCP JWT, or a Forma PAT
    rejects: unauthenticated calls
```

A browser's Supabase token does not become an MCP credential merely because it
exists in the same browser. `/mcp` requires its own OAuth client identity or a
PAT. This keeps browser authorization, agent authorization and revocation
separate.

**Amended during slice 2A.** This section first said `/api/v1/*` *rejects* a
Forma PAT, which read literally contradicts §109.2: `forma-mcp` forwards the
caller's credential to `forma-server`, and every MCP tool call is therefore a
REST call carrying a PAT. Taken at its word it would need a second verifier
inside `forma-mcp`, and two verifiers can disagree about who someone is.

What the original line was protecting is still true and still enforced: **a
browser session is never a PAT.** The SPA authenticates with the Supabase
session it already holds and never asks anyone for a token, so the credential
classes stay separate where that separation does work — in what each product
surface uses, and in what revoking each one disconnects.

A PAT reaching REST directly is the same account with the same permissions as
that PAT reaching it through `/mcp`, and it is useful for scripting. Refusing it
there would buy nothing and cost a second implementation of identity.

## 112.1 Personal access tokens

OAuth is preferred where the host supports it. It does not everywhere, and the
PAT is the compatibility path for CLI and automation hosts — Codex CLI, Claude
Code, Gemini CLI.

**It is not a fallback for ChatGPT.** ChatGPT accepts OAuth or nothing, so there
is no manual-header path there at all.

```text
forma_pat_<random-high-entropy-secret>
```

At least 32 random bytes before encoding. Stored: id, account, creator, name,
safe prefix, hash, timestamps, revocation state — never the token. Shown exactly
once. Because the secret is high entropy a SHA-256 hash is sufficient; compare
safely and never log a presented token.

A revoked PAT fails on the next MCP request. Revoking one PAT must not disconnect
browser sessions, OAuth-connected agents, or other PATs.

## 112.2 OAuth grants

The Agents page lists the OAuth applications the current user has authorized, via
`supabase.auth.oauth.getUserGrants()`, showing client name, a safe identifier and
timestamps. Revocation uses `supabase.auth.oauth.revokeGrant(clientId)`.

**Revocation is immediate**, not eventual: Supabase marks the consent revoked,
deletes active sessions for that client and invalidates its refresh tokens at
once. Re-use requires a new authorization.

Do not invent a second grant database in Forma, and do not confuse grant
revocation with PAT revocation. They are separate credential systems.

---

# 113. Migration of the existing deployment

## 113.1 The legacy account

The deployed database holds projects created before accounts existed. **They must
not be assigned to the first person who signs up.**

```text
1. create one account row marked legacy
2. assign every existing Project to it
3. create no membership for it automatically
4. keep FORMA_REMOTE_TOKEN as the way in while nobody owns the workspace
5. the operator hands it to a person with
   `forma-server grant-owner <email>`, on the machine
6. the first membership marks the account owned, and the token stops
   being accepted as general REST or MCP authentication
```

**Amended while implementing.** Steps 5 and 6 first specified a
`POST /api/v1/bootstrap/claim` that took a valid session *and* the deployment
token, so a person pasted the server's own secret into a browser field. That was
designed before accounts existed; once they do, it is a toll on the way to
something the account already gives. **An account owns projects, and that
relation is the ownership** — there is nothing separate left to claim.

So the handover moved to `grant-owner`, which was already built as §113.3's
recovery path. It is the better shape for the same job: its boundary is shell
access on the machine rather than a secret that travels through a browser and
into somebody's clipboard.

`claimed_at` survives and means what it always meant, set by the first
membership rather than by a ritual — and it is still what retires the token, so
§113.1's actual guarantee is unchanged. Recording it there rather than at a call
site is deliberate: a caller who forgot would leave a shared secret working
forever.

The old token becomes a migration secret, not the new long-term credential.

A user who owns no account may create a personal one instead, subject to the
invite gate of §107.3. The onboarding UI states the difference, and offers the
claim only while an unclaimed legacy account exists:

```text
No Forma account yet
[ Create my account ]

Existing deployment?
[ Claim existing workspace ]
```

## 113.2 The schema migration is a table rebuild

SQLite has no `ALTER COLUMN`. `projects` is populated on the deployed machine, so
adding a `NOT NULL` column is the documented create-copy-drop-rename procedure,
in this order, in one migration file:

```text
1. create accounts, account_memberships, account_invites, api_keys
2. insert the legacy account row
3. ALTER TABLE projects ADD COLUMN account_id TEXT REFERENCES accounts (id)
   -- nullable
4. backfill every existing project to the legacy account
5. rebuild projects with account_id TEXT NOT NULL, keeping the table name
   so existing ON DELETE CASCADE references from templates, designs and
   assets continue to resolve
6. recreate idx_projects_updated_at
```

A `NOT NULL` constraint never meets a row that has not been backfilled.

## 113.3 Losing the identity provider must not lose the workspace

`account_memberships.user_id` points at a subject living in another system. The
claim is one-time and then the old token stops being accepted. If the Supabase
project is deleted, rotated to a new ref, or migrated, every membership points at
a subject that no longer resolves and **there is no way back into data that is
sitting intact in `forma.db` on a disk the operator owns**.

The hardening that makes the claim safe is also what makes that failure
permanent. So:

```text
forma-server grant-owner <email>
```

attaches an owner membership on the machine. Shell access is the security
boundary, exactly as it is for `forma-server backup` — a procedure that already
exists in this shape and needs no new trust. With the `email` column of §107.1 it
rebinds an account to a fresh subject from a new Supabase project.

Recovery is: point `FORMA_SUPABASE_*` at the new project, sign in, run
`grant-owner`.

---

# 114. Configuration and routing

## 114.1 Configuration

```text
FORMA_AUTH_MODE=supabase

FORMA_SUPABASE_URL=https://<project-ref>.supabase.co
FORMA_SUPABASE_ISSUER=https://<project-ref>.supabase.co/auth/v1
FORMA_SUPABASE_JWKS_URL=https://<project-ref>.supabase.co/auth/v1/.well-known/jwks.json

FORMA_MCP_RESOURCE_URL=https://forma.hintly.online/mcp

VITE_SUPABASE_URL=https://<project-ref>.supabase.co
VITE_SUPABASE_PUBLISHABLE_KEY=<publishable-key>
```

The publishable key belongs in the browser. No `service_role` key is required to
validate user JWTs or to use the user-facing OAuth consent APIs; if a future
server-side admin operation needs one, add it for that operation alone and never
expose it to the SPA.

`FORMA_REMOTE_TOKEN` is retained only long enough to claim the legacy account,
then removed as a general authentication mechanism — subject to §109.1, which
must land first or its removal opens the deployment.

## 114.2 Caddy

```text
https://forma.hintly.online/                                  -> forma-server :4765
https://forma.hintly.online/api/v1/*                          -> forma-server :4765
https://forma.hintly.online/mcp                               -> forma-mcp    :4766
https://forma.hintly.online/.well-known/oauth-protected-resource*
                                                              -> forma-mcp    :4766
```

**The well-known path needs its own `handle` block, without `strip_prefix`.** The
existing MCP block matches `/mcp*` and strips that prefix;
`/.well-known/oauth-protected-resource/mcp` does not match it, falls through to
`forma-server`, and 404s exactly as §110.3 measures. Discovery would fail with no
error anyone reads.

rmcp's allowed-host validation stays enabled. Adding Supabase must never lead to
`disable_allowed_hosts()` or an allow-all policy — an empty list means allow
everything. TLS remains mandatory.

---

# 115. Error semantics

```text
AUTH_REQUIRED  -- do not add; UNAUTHORIZED already means this
INVALID_TOKEN
ACCOUNT_REQUIRED
LEGACY_ACCOUNT_ALREADY_CLAIMED
INVALID_BOOTSTRAP_TOKEN
API_KEY_REVOKED
SERVICE_CREDENTIAL_REVOKED
SERVICE_CREDENTIAL_OUT_OF_SCOPE
```

```text
missing or invalid credential       -> 401
valid identity, no Forma account    -> 403 ACCOUNT_REQUIRED
service credential, route outside
  Boundary F and resolve            -> 403 SERVICE_CREDENTIAL_OUT_OF_SCOPE
object outside the current account  -> the same not-found as an unknown id
```

Two codes for one condition is how a stable contract rots; `UNAUTHORIZED` already
covers a missing or bad credential and `AUTH_REQUIRED` must not be added beside
it. The rest slot into the existing envelope of §32 with no structural change,
plus the `FORBIDDEN` mapping of §109.3.

For `/mcp`, a missing or invalid credential is an HTTP authorization failure and
never an MCP tool result.

---

# 116. Tests

## 116.1 Unit

JWT verification: valid, expired, not-yet-valid, wrong issuer, wrong audience,
wrong algorithm, missing subject, missing `client_id` on the MCP OAuth path, JWKS
rotation and cache behaviour.

PAT: create, hash-only storage, authenticate, `last_used` update, revoke, revoked
rejected, two keys independent.

Account resolution: user with membership, user without, PAT account, legacy claim,
second claim rejected, invite required, invite consumed once.

## 116.2 Tenant isolation — mandatory

Two accounts, each with a project, brand, asset, design, template and asset
request. For **every** externally reachable read and write:

```text
A can access A
B can access B
A cannot read B by known id
A cannot mutate B by known id
B cannot read A by known id
B cannot mutate A by known id
```

Do not rely on random UUID secrecy.

## 116.3 Browser end-to-end

Playwright, against a real server: open `/` unauthenticated, log in, resolve the
account, create a project, configure a Brand Kit, upload an asset, create and
open a design, edit, export, log out, and confirm the protected API is no longer
usable. A second user must not see the first user's projects. The consent page's
own behaviour — details rendered, approve, deny, redirect to the exact
`redirect_url` — belongs here too: it is our page.

## 116.4 The OAuth protocol flow is a script, not a CI test

Every test in this repository runs against a temporary SQLite and a locally built
binary. CI has no deployment, no domain and no certificate. Calling the OAuth
flow a CI test would produce a test skipped on every run, which is worse than a
script someone executes.

Its home is `scripts/smoke-remote.py`, or beside it: run from off the machine
against the real domain, reporting pass and fail counts, with the result recorded
in `docs/deploy/status.md` under *verified here*. It must observe the
unauthenticated `401`, the `WWW-Authenticate` challenge, Protected Resource
Metadata, authorization-server discovery, registration, PKCE, the code exchange,
`tools/list`, a tool call reaching only that account, refresh, and a revoked grant
failing to reconnect. An in-process MCP client cannot prove the `401 -> discovery`
path.

The magic link is avoided with a Supabase test user holding a password. That is a
fixture, not a weakening of production auth.

## 116.5 The provider smoke tests are manual, by rule

From a normal Claude custom connector UI: add the MCP URL, complete OAuth, list
projects, create and render a test design, **with no manual bearer configured**.
Then the same for ChatGPT where the account surface exposes it; if it does not,
record a provider availability limitation and do not fabricate a workaround that
changes Forma's auth model.

**These are not automated, and not for pragmatic reasons: §2.2 forbids driving
external AI services with Playwright, and §67 confines it to testing our own
app.** They are written procedures, executed by a person, recorded in
`docs/deploy/status.md`.

## 116.6 PAT live smoke

Create one PAT in the browser, use it from an external CLI host against the real
`/mcp`, revoke it, and confirm that host fails afterwards while the browser
session and the OAuth connector keep working.

---

# 117. Execution, acceptance and non-goals

## 117.1 Two slices, in this order

**The account boundary requires no Supabase at all.** Sections 107, 108, 109 and
113.2 can be built and shipped under `FORMA_AUTH_MODE=local` against a singleton
account, with no provider, no JWT and no OAuth.

That is also where all the security risk lives: 36 repository methods, 8 use-case
modules, and the question of whether one person's work can reach another's.
Everything the identity half depends on was, on the reference date, in beta or
formally deprecated or both.

```text
Slice 1 — the boundary, no provider
  accounts, memberships, invites
  AccountId on every scoped repository method
  the projects table rebuild and the legacy account
  the bind guard and require_bearer gated on auth mode   (109.1)
  per-request identity in forma-mcp                      (109.2)
  403 in the error envelope                              (109.3)
  actor derived from AuthContext                         (117.2)
  tenant isolation tests                                 (116.2)

Slice 2 — identity and distribution
  Supabase Auth, JWKS validation, the audience hook
  the SPA router, default_handler and CSP
  /login, /oauth/consent, /agents
  Protected Resource Metadata, the 401 challenge, Caddy
  PATs, grants, revocation
  GET /skill
  the remote OAuth script and the manual provider smokes
```

Build the boundary first, with its tests. Plug identity into a boundary that is
already proven.

## 117.2 Actor metadata

Keep the `Human | Agent | System` history of §61, and populate it from
authenticated context.

```text
REST browser mutation   actor_type = Human
                        actor_id   = user:<supabase-sub>

OAuth MCP mutation      actor_type = Agent
                        actor_id   = oauth:<client_id>:<supabase-sub>

PAT MCP mutation        actor_type = Agent
                        actor_id   = pat:<api-key-id>
```

Today the actor comes from an `x-forma-actor` header that any caller can write,
and `forma-mcp` fills it from the host's own `clientInfo.name`. That is not a
vulnerability with one credential and one person; with accounts it is a forgeable
audit trail. The change is small because the helper is already the single choke
point — no handler reads the header itself. It takes the `AuthContext` and derives
the actor from it. The host-supplied name may survive as descriptive client
metadata beside the derived actor, never as its source.

Never log raw JWTs, refresh tokens or PATs.

## 117.3 The hosted skill

```text
GET /skill
```

returns the canonical `skills/forma/SKILL.md`, public, containing no account data
and no credentials. It is a manual fallback for hosts that cannot install a
package, a canonical URL a person can hand an agent, and one source of truth
rather than copies drifting per host.

**Compile it in with `include_str!`**, exactly as the existing skill test already
does. Read from disk it becomes a deploy artifact that can go missing, go stale
against the binary, or 404 after a partial deploy. Served by `forma-server`, which
already owns the web origin, so Caddy needs no new route. The existing skill test
gains one assertion: the bytes the route serves are the bytes it checks. The
repository symlinks for local discovery are unaffected.

## 117.4 Agent connection UX

```text
Use Forma with your favourite AI

Claude
  Connect Forma as a remote connector.
  MCP URL: https://forma.hintly.online/mcp
  [ Copy URL ]  [ Instructions ]

ChatGPT / Codex
  Connect through the provider's MCP app flow.   [ Instructions ]

Developer agents
  Codex CLI · Claude Code · Gemini CLI
  [ Create API key ]  [ Manual setup ]

Connected applications      <OAuth grants, with revoke>
API keys                    <PATs, with revoke>
```

For an OAuth-capable provider the person needs only the public URL and that
provider's own "add connector" action. **Do not ask anyone to copy an OAuth access
token.** For PAT clients, generate host-specific snippets where they are reliable
and always show the protocol-neutral fallback:

```text
MCP URL: https://forma.hintly.online/mcp
Header:  Authorization: Bearer ${FORMA_MCP_TOKEN}
```

Snippets prefer an environment variable over a secret written into a committed
config file.

Provider UIs change independently of Forma, so provider-specific setup is a thin
distribution layer over the stable MCP URL and the resource-server contract. A
provider's plan limitation or directory approval must never determine Forma's
internal auth architecture.

## 117.5 Acceptance

```text
[ ] The hosted SPA loads without a deployment bearer.
[ ] Unauthenticated REST data access is rejected.
[ ] Unauthenticated /mcp returns a standards-usable 401 challenge.
[ ] Supabase Auth login works in the hosted editor.
[ ] /login, /oauth/consent and /agents resolve rather than 404.
[ ] Every hosted Project belongs to an Account.
[ ] Existing deployed Projects are migrated to a claimable legacy Account.
[ ] The legacy Account is claimable exactly once with the old token.
[ ] Account creation requires an invite.
[ ] forma-server grant-owner recovers an account without the provider.
[ ] A second user cannot reach the first user's data with known ids.
[ ] The browser can create, edit and export after login.
[ ] Supabase OAuth 2.1 is enabled with asymmetric signing.
[ ] /oauth/consent handles authorization_id, approve and deny.
[ ] Forma publishes Protected Resource Metadata, routed to forma-mcp.
[ ] OAuth tokens are verified locally through JWKS.
[ ] MCP tokens are restricted to the Forma MCP resource audience.
[ ] A real OAuth flow reaches tools/list and tools/call.
[ ] Refresh-token reconnection works.
[ ] Grant revocation works, and is immediate.
[ ] Claude's custom connector succeeds on URL plus OAuth alone.
[ ] The ChatGPT flow is tested, or its unavailability is documented.
[ ] PAT creation, one-time display, use and revocation work.
[ ] Revoking one PAT revokes nothing else.
[ ] /skill serves the canonical skill.
[ ] The Agents page offers OAuth before manual token setup.
[ ] Hosted auth breaks neither local mode nor stdio MCP.
[ ] binding_is_safe and require_bearer are gated on FORMA_AUTH_MODE.
[ ] The remote smoke suite tests the public-shell / private-data boundary.
```

The criterion is not "OAuth works". It is:

> A person can use the hosted editor, connect a mainstream agent through that
> provider's normal connection flow, let the agent work only inside that person's
> Forma account, see the result in the editor, and revoke the connection without
> affecting anyone else.

## 117.6 Security checks before acceptance

No secret in the Vite bundle; no `service_role` key in frontend code; no token or
auth-header logging; strict same-origin REST; the CSP of §110.4; HTTPS-only hosted
auth; asymmetric signing; issuer validation; MCP audience validation; account
scope enforced below the HTTP handlers; constant-time comparison for the bootstrap
token and PAT hashes; high-entropy PAT generation; PATs shown once; explicit
consent; revocable grants; rmcp host validation still enabled; cross-account ids
leaking nothing.

## 117.7 Non-goals

Not in this phase: Supabase PostgreSQL or Storage migration; billing;
subscriptions; teams; invitations UI; account switching; RBAC beyond the owner
model; a custom Forma OAuth server; custom OAuth scopes; per-account quotas;
social publishing; provider marketplace approval as a hard dependency; automatic
image generation; replacing local stdio MCP; requiring Supabase for local
installations.

Each requires separate evidence and a separate phase.

## 117.8 Local mode is unchanged

Under `FORMA_AUTH_MODE=local`, Forma uses a singleton Account bootstrapped at
startup; existing local projects belong to it. Local React and stdio MCP keep the
trusted-local behaviour they have now. There is no OAuth header on stdio: its
boundary remains the process that launched it. Hosted-only tables may exist in
SQLite as long as local startup bootstraps the singleton account automatically.

No local user is ever required to create a Supabase account to run Forma on their
own machine.
