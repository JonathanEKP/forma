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
- OAuth;
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
- mandatory integration with Supabase.

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
