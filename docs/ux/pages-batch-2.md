# Pages batch 2: building the four approved page designs

Decided with the owner on 2026-10-08. The visual and interaction spec of each page is its approved prototype in
[opendesign/prototypes/](opendesign/prototypes/README.md); this document holds what the prototypes cannot: the
data each page needs, who owns which files, and the decisions taken while planning. The review rail of each
prototype is not built (as in UX batch 1, C-6).

## Decisions

- **P-1 Three tasks in parallel, each owning its files.** H builds the projects home, A the sign-in and approval
  screens, E the editor's side panels. Sign-in and approval share one task because both are small and share
  their centred frame.
- **P-2 "Edited by … · 2 h ago" comes from the server.** Every version row already records its actor and time
  (`design_versions.actor_type`, `actor_id`, `created_at`). `DesignSummary` gains the latest version's actor and
  time, read in the same list query, so the home page makes one request rather than one per design. Labels:
  a person is **"You"**; an agent is its app's name from the `agent:<client>` actor id (`claude-code` →
  "Claude Code", `codex-mcp-client` or `codex` → "Codex CLI", `claude-ai` → "Claude", `opencode` → "OpenCode",
  `cursor` → "Cursor", `chatgpt` → "ChatGPT"; check the real ids the MCP surface records before fixing the list);
  an unknown agent is **"An agent"**; `System` is **"Forma"**.
- **P-3 Thumbnails are page 1's existing preview.** `DesignSummary` gains `first_page_id`; the card shows
  `/designs/{id}/pages/{page}/preview.png` (cached per version by the server), loaded lazily
  (`loading="lazy"`) with a quiet placeholder until it arrives, and the format's aspect ratio reserved so the
  grid never jumps.
- **P-4 "Save as template" is added to the editor.** The route exists (`POST /designs/{id}/template`) and no
  screen offers it, so without it the home page's template picker would be empty. A small action beside Export,
  asking only for a name. The home page shows "Start from a template" only when the project has templates.
- **P-5 An image request opens its design with Images open.** Which design is open, and which panel, becomes part
  of the address (`/?design=<id>&panel=images`), so a reload keeps it and the home page can link to it. A request
  with no design reads "upload from any design's Images panel". Pending requests are grouped by `design_id` on
  the client from the existing list; no server change.
- **P-6 "Send again" waits 60 seconds,** the identity provider's limit per address, not the prototype's 0:45.
- **P-7 Expired or used sign-in links** are recognised from the error the provider puts in the return URL's
  fragment (`error_code=otp_expired`), shown as the prototype's state G, and the fragment is cleared.
- **P-8 Proof.** Each task tests first (Vitest/RTL), then shows its page in a real browser against its prototype,
  state by state at desktop and 390px, with screenshots in `docs/ux/evidence/pages/`. The Playwright walkthrough
  (`e2e/`) gains deleting a design and the new home page.

## Tasks and file ownership

| Task | Builds | Owns |
|---|---|---|
| **H** | `projects.html`: home, delete design and project, new design with format and template, image requests, About popover | `Projects.tsx`; `App.tsx` (address state of P-5); `DesignSummary` and its list query, bindings; the design-list, project/design delete and template list/create routes' client functions |
| **A** | `signin.html` and `consent.html` | `SignIn.tsx`, `Consent.tsx`, `identity.ts` |
| **E** | `panels.html`: Brand, Images, Export, Checks; Save as template | `editor/*` (Brand, Assets, Export, Validation, Editor, Panels); the save-template client function |

Contracts between them:
- E adds `initialTool?: "layers" | "images" | "brand"` to `Editor` (or the editor's own name for the tools);
  H passes it from the address. Merge order: E before H, or H rebases on E.
- H reuses E's Brand form for the home page's inline Brand Kit editing, unchanged.
- `api.ts` and `queries.ts` are shared and only added to; whoever merges second rebases.

## Not in this batch

Search, sharing, renaming, trash or restore (not supported by the server); a Forma-hosted per-app guide (see
plan-status); anything that needs Pontis.
