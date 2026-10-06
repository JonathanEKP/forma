/**
 * The editor shell: the layout of plan section 10.
 *
 * Calm's top bar, slim tool rail, optional tool flyout, centered canvas, and
 * contextual properties and chat. V1 does not try to compete with Figma.
 */

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

import { ApiError, conflictDetails, type Asset, type DesignOperation } from "../api";
import {
  useApplyOperations,
  useBrand,
  useDesign,
  usePreview,
  useStatus,
  useUndo,
  useValidation,
} from "../queries";
import { useEditor, type DraftFrame } from "../store";
import { ChatPanel } from "../pontis/Chat";
import type { Chat } from "../pontis/useChat";
import { findLayer, findPage } from "../types";
import { Assets } from "./Assets";
import { Brand } from "./Brand";
import { Canvas } from "./Canvas";
import { ExportButton } from "./Export";
import { addImageOperation } from "./insert";
import { PagesAndLayers, Panel, Properties } from "./Panels";
import { Validation } from "./Validation";

/** How long the agent-updated notice stays. */
const NOTICE_MS = 6_000;

type RailTool = "layers" | "assets" | "brand";

export function Editor({
  designId,
  onBack,
  chat,
  chatOpen = false,
  onToggleChat = () => undefined,
}: {
  designId: string;
  onBack: () => void;
  /**
   * The agent chat, which belongs to the app and not to this design: it is held above the
   * editor so a conversation survives opening another design or going back to the projects
   * (D-1). Absent where there is no agent to talk to — a local Forma, or nobody signed in.
   */
  chat?: Chat;
  chatOpen?: boolean;
  onToggleChat?: () => void;
}) {
  const { data: record, isLoading, error, refetch } = useDesign(designId);
  const { selectedPageId, selectedLayerId, selectPage, panel, setPanel, zoom, setZoom } = useEditor();
  const [activeTool, setActiveTool] = useState<RailTool | null>(() =>
    panel === "assets" || panel === "brand" ? panel : panel === "validation" ? null : "layers",
  );
  const [validationOpen, setValidationOpen] = useState(panel === "validation");
  const editorRoot = useRef<HTMLDivElement>(null);
  const fittedOnOpen = useRef(false);
  const observedPanel = useRef(panel);
  const internalPanelChange = useRef(false);

  const setEditorPanel = useCallback((next: typeof panel) => {
    if (panel === next) return;
    internalPanelChange.current = true;
    setPanel(next);
  }, [panel, setPanel]);

  const closePanels = useCallback(() => {
    setActiveTool(null);
    setValidationOpen(false);
    setEditorPanel("properties");
  }, [setEditorPanel]);

  const toggleTool = useCallback((tool: RailTool) => {
    const next = activeTool === tool ? null : tool;
    setActiveTool(next);
    setValidationOpen(false);
    setEditorPanel(next === "assets" || next === "brand" ? next : "properties");
  }, [activeTool, setEditorPanel]);

  const toggleValidation = useCallback(() => {
    setActiveTool(null);
    setValidationOpen((open) => !open);
    setEditorPanel("properties");
  }, [setEditorPanel]);

  // Keep panel requests from the editor channel wired to the corresponding
  // Calm surface. Local rail clicks set the same store value themselves.
  useEffect(() => {
    if (panel === observedPanel.current) return;
    observedPanel.current = panel;
    if (internalPanelChange.current) {
      internalPanelChange.current = false;
      return;
    }

    if (panel === "assets" || panel === "brand") {
      setActiveTool(panel);
      setValidationOpen(false);
    } else if (panel === "validation") {
      setActiveTool(null);
      setValidationOpen(true);
    } else {
      setActiveTool(null);
      setValidationOpen(false);
    }
  }, [panel]);

  // The dock mounts with the editor. Move focus to its real composer as soon as
  // it exists, including when the person opens it from the floating composer.
  useLayoutEffect(() => {
    if (!chatOpen || !chat?.enabled) return;
    editorRoot.current
      ?.querySelector<HTMLTextAreaElement>('textarea[aria-label="Message the agent"]')
      ?.focus({ preventScroll: true });
  }, [chatOpen, chat?.enabled]);

  // Fit the artboard into the canvas once after the design first paints. The
  // stored zoom then stays fixed through server refetches, agent writes, and
  // approval UI changes; later canvas resizes can scroll instead of jumping.
  useLayoutEffect(() => {
    if (isLoading || !record || fittedOnOpen.current) return;
    const canvas = editorRoot.current?.querySelector<HTMLElement>(".editor-canvas-room");
    if (!canvas) return;

    const { width, height } = record.document.format;
    const horizontalPadding = 48;
    const verticalPadding = 48;
    const controlBand = chat?.enabled && !chatOpen ? 104 : 64;
    const availableWidth = canvas.clientWidth - horizontalPadding;
    const availableHeight = canvas.clientHeight - verticalPadding - controlBand;
    const fittedZoom = Math.min(1, availableWidth / width, availableHeight / height);
    if (!Number.isFinite(fittedZoom) || fittedZoom <= 0) return;

    setZoom(fittedZoom);
    fittedOnOpen.current = true;
  }, [chat?.enabled, chatOpen, isLoading, record, setZoom]);

  const apply = useApplyOperations(designId);
  const undo = useUndo(designId);

  const notice = useEditor((state) => state.notice);
  const dismissNotice = useEditor((state) => state.dismissNotice);
  // Said for a few seconds and gone: it tells the person something happened that they did not do,
  // and is not a state to be left in.
  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(dismissNotice, NOTICE_MS);
    return () => clearTimeout(timer);
  }, [notice, dismissNotice]);

  const page = useMemo(() => (record ? findPage(record, selectedPageId) : null), [record, selectedPageId]);

  // A design always has at least one page, so there is always something to
  // select; landing on an empty canvas would just look broken.
  useEffect(() => {
    if (record && !page && record.document.pages.length > 0) {
      selectPage(record.document.pages[0].id);
    }
  }, [record, page, selectPage]);

  const preview = usePreview(designId, page?.id ?? null, record?.version ?? null);
  const validation = useValidation(designId, record?.version ?? null);
  const status = useStatus();
  const brand = useBrand(record?.project_id ?? null);

  /**
   * A new text layer must name a family the renderer actually has, or it would
   * be created and then fail validation with FONT_NOT_FOUND. The project's own
   * brand wins; otherwise the first family the server reports.
   */
  const fontFamily =
    brand.data?.typography.display ?? status.data?.fonts[0] ?? "Geist";

  const busy = apply.isPending || undo.isPending;

  useEffect(() => {
    if (!activeTool && !validationOpen) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      closePanels();
    };
    document.addEventListener("keydown", closeOnEscape);
    return () => document.removeEventListener("keydown", closeOnEscape);
  }, [activeTool, validationOpen, closePanels]);

  const send = (operations: DesignOperation[]) => {
    if (!record || operations.length === 0) return;
    apply.mutate({ baseVersion: record.version, operations });
  };

  /**
   * Puts an uploaded asset on the current page.
   *
   * It goes through the same add_layer operation an agent would send, so the
   * editor still has no private authoring path (plan section 28).
   */
  const placeAsset = (asset: Asset) => {
    if (!record || !page) return;
    send([
      addImageOperation(
        page.id,
        record.document.format,
        asset.id,
        asset.original_filename ?? "Image",
      ),
    ]);
  };

  /** Plan section 38: one pointer-up becomes one move_layer, not one per frame. */
  const commitFrame = (frame: DraftFrame) => {
    send([
      {
        op: "move_layer",
        layer_id: frame.layerId,
        frame: {
          x: frame.x,
          y: frame.y,
          width: frame.width,
          height: frame.height,
        },
      },
    ]);
  };

  if (isLoading) return <Centered>Loading the design…</Centered>;
  if (error || !record) {
    return <Centered>{error instanceof Error ? error.message : "Design not found."}</Centered>;
  }

  const selectedLayer = findLayer(page, selectedLayerId);
  const validationCount = validation.data?.issues.length ?? 0;
  const validationLabel = validation.data
    ? validationCount === 0
      ? "✓ Valid"
      : `${validationCount} ${validationCount === 1 ? "issue" : "issues"}`
    : "Checking…";

  return (
    <div ref={editorRoot} className="editor-root flex h-full min-h-0 flex-col">
      <header className="editor-topbar relative flex h-[56px] shrink-0 items-center justify-between gap-4 bg-ink px-4">
        <div className="flex min-w-0 items-center gap-2.5">
          <span className="editor-mark" aria-label="Forma">f.</span>
          <button
            type="button"
            onClick={onBack}
            aria-label="Open Projects"
            className="shrink-0 text-xs text-muted hover:text-paper"
          >
            Projects /
          </button>
          <span className="truncate text-[13px] font-medium" title={record.name}>{record.name}</span>
          <span className="editor-version">v{record.version}</span>
        </div>

        <div className="editor-top-actions flex shrink-0 items-center gap-2">
          <button
            type="button"
            disabled={busy || record.version <= 1}
            onClick={() => undo.mutate(record.version)}
            title="Undo"
            aria-label="Undo"
            className="editor-icon-button"
          >
            <EditorIcon name="undo" />
          </button>
          <button
            type="button"
            disabled
            title="Redo is not available yet"
            aria-label="Redo"
            className="editor-icon-button"
          >
            <EditorIcon name="redo" />
          </button>
          <div className="relative">
            <button
              type="button"
              onClick={toggleValidation}
              aria-expanded={validationOpen}
              aria-controls="editor-validation-popover"
              aria-label={
                validationCount === 0 && validation.data
                  ? "Open validation: valid"
                  : `Open validation: ${validationLabel}`
              }
              title="Validation"
              className="editor-validation-chip"
            >
              {validationLabel}
            </button>
            {validationOpen && (
              <section
                id="editor-validation-popover"
                className="editor-validation-popover"
                aria-label="Validation"
              >
                <header className="flex items-center justify-between gap-3">
                  <h2 className="text-xs font-medium">Validation</h2>
                  <button
                    type="button"
                    onClick={closePanels}
                    aria-label="Close validation"
                    className="editor-close-button"
                  >
                    ×
                  </button>
                </header>
                <Validation report={validation.data} loading={validation.isLoading} onSelect={closePanels} />
              </section>
            )}
          </div>
          <ExportButton
            designId={record.id}
            pageId={page?.id ?? null}
            pageCount={record.document.pages.length}
            report={validation.data}
            disabled={busy}
          />
        </div>
      </header>

      {notice && (
        <div role="status" className="flex items-center gap-3 border-b border-edge bg-surface-2 px-4 py-2">
          <span className="h-[7px] w-[7px] rounded-full bg-signal" />
          <span className="flex-1 text-xs">{notice.text}</span>
          <button type="button" onClick={dismissNotice} className="text-xs text-muted hover:text-paper">
            Dismiss
          </button>
        </div>
      )}

      {/* The conflict is not a toast: the edit did not happen, and the user has
          to know that before they carry on typing (plan section 27). */}
      {apply.error instanceof ApiError && apply.error.isConflict && (
        <Conflict
          error={apply.error}
          onReload={() => {
            // Actually refetch. Clearing the error alone would hide the banner
            // while the editor still held the stale version, so the next edit
            // would conflict again — the blind retry the plan warns about,
            // built into the UI.
            void refetch().then(() => apply.reset());
          }}
        />
      )}

      {apply.error instanceof ApiError && !apply.error.isConflict && (
        <Rejected error={apply.error} onDismiss={() => apply.reset()} />
      )}

      <div className="editor-workspace flex min-h-0 flex-1">
        <nav className="editor-rail flex shrink-0 flex-col items-center gap-2 py-3" aria-label="Design tools">
          <RailButton tool="layers" activeTool={activeTool} onClick={toggleTool}>
            <EditorIcon name="layers" />
          </RailButton>
          <RailButton tool="assets" activeTool={activeTool} onClick={toggleTool}>
            <EditorIcon name="assets" />
          </RailButton>
          <RailButton tool="brand" activeTool={activeTool} onClick={toggleTool}>
            <EditorIcon name="brand" />
          </RailButton>
        </nav>

        {activeTool === "layers" && (
          <PagesAndLayers
            record={record}
            onOperations={send}
            disabled={busy}
            fontFamily={fontFamily}
            onClose={closePanels}
          />
        )}

        {activeTool === "assets" && (
          <Assets
            projectId={record.project_id}
            onPlace={placeAsset}
            disabled={busy || page === null}
            onClose={closePanels}
          />
        )}

        {activeTool === "brand" && (
          <Panel title="Brand" onClose={closePanels}>
            <Brand projectId={record.project_id} />
          </Panel>
        )}

        <main className="editor-canvas-room relative flex min-h-0 min-w-0 flex-1" aria-label="Design workspace">
          {page ? (
            <Canvas
              record={record}
              page={page}
              svg={preview.data}
              onCommit={commitFrame}
              disabled={busy}
            />
          ) : (
            <Centered>This design has no pages.</Centered>
          )}

          <ZoomControl zoom={zoom} setZoom={setZoom} />

          {chat && !chatOpen && (
            <div className="editor-agent-presence">
              <span className="editor-presence-dot" aria-hidden="true" />
              <textarea
                rows={1}
                readOnly
                aria-label="Ask the agent…"
                placeholder="Ask the agent…"
                onFocus={() => onToggleChat()}
              />
              <button type="button" onClick={onToggleChat} aria-label="Open Agent chat" title="Open chat" className="editor-icon-button">
                <EditorIcon name="chat" />
              </button>
            </div>
          )}
        </main>

        {selectedLayer && page && (
          <div className="editor-properties">
            <Properties
              record={record}
              page={page}
              onOperations={send}
              disabled={busy}
            />
          </div>
        )}

        {chat && chatOpen && (
          <div className="editor-docked-chat">
            <ChatPanel chat={chat} onClose={onToggleChat} />
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * Someone else moved the design while we were editing.
 *
 * The plan is explicit that the client refetches and rebases rather than
 * retrying: resending the same base_version would fail forever.
 */
function Conflict({ error, onReload }: { error: ApiError; onReload: () => void }) {
  const versions = conflictDetails(error);

  return (
    <div className="flex items-center gap-3 border-b border-signal/45 bg-signal/8 px-4 py-2.5">
      <span className="h-[7px] w-[7px] rounded-full bg-signal" />
      <div className="flex-1 text-xs">
        <strong className="font-semibold">This design changed elsewhere.</strong>{" "}
        <span className="text-muted">
          {versions
            ? `You were editing v${versions.expected}; it is now v${versions.actual}. Your last change was not applied.`
            : "Your last change was not applied."}
        </span>
      </div>
      <button
        type="button"
        onClick={onReload}
        className="rounded-lg bg-signal px-2.5 py-1.5 text-xs font-semibold text-ink"
      >
        Reload
      </button>
    </div>
  );
}

/** A rejected batch: the server understood it and refused. Nothing was saved. */
function Rejected({ error, onDismiss }: { error: ApiError; onDismiss: () => void }) {
  return (
    <div className="flex items-center gap-3 border-b border-edge-strong bg-surface-2 px-4 py-2.5">
      <div className="flex-1 text-xs">
        <code className="text-[11px] text-signal">{error.code}</code>{" "}
        <span className="text-muted">{error.message}</span>
        {error.hint && <span className="block text-faint mt-0.5">{error.hint}</span>}
      </div>
      <button type="button" onClick={onDismiss} className="text-xs text-muted hover:text-paper">
        Dismiss
      </button>
    </div>
  );
}

function ZoomControl({ zoom, setZoom }: { zoom: number; setZoom: (z: number) => void }) {
  const percent = Math.round(zoom * 100);
  return (
    <div className="editor-zoom-control flex items-center gap-1.5 text-xs text-muted" role="group" aria-label="Canvas zoom controls">
      <button type="button" onClick={() => setZoom(zoom / 1.25)} aria-label="Zoom out" title="Zoom out">
        −
      </button>
      <output className="w-12 text-center tabular-nums" aria-label={`Canvas zoom ${percent} percent`}>
        {percent}%
      </output>
      <button type="button" onClick={() => setZoom(zoom * 1.25)} aria-label="Zoom in" title="Zoom in">
        +
      </button>
    </div>
  );
}

function RailButton({
  tool,
  activeTool,
  onClick,
  children,
}: {
  tool: RailTool;
  activeTool: RailTool | null;
  onClick: (tool: RailTool) => void;
  children: React.ReactNode;
}) {
  const labels: Record<RailTool, string> = {
    layers: "Layers & pages",
    assets: "Assets",
    brand: "Brand",
  };
  const label = labels[tool];
  const active = activeTool === tool;

  return (
    <button
      type="button"
      onClick={() => onClick(tool)}
      aria-label={label}
      aria-pressed={active}
      aria-expanded={active}
      title={label}
      data-tooltip={label}
      className={`editor-rail-button grid h-8 w-8 place-items-center rounded-lg ${active ? "is-active" : ""}`}
    >
      {children}
    </button>
  );
}

function EditorIcon({ name }: { name: "layers" | "assets" | "brand" | "undo" | "redo" | "chat" }) {
  const icon = {
    layers: <><path d="m2 5 6-3 6 3-6 3z" /><path d="m2 8 6 3 6-3M2 11l6 3 6-3" /></>,
    assets: <><rect x="2" y="2" width="12" height="12" rx="2" /><circle cx="5" cy="5" r="1" /><path d="m2 11 4-4 4 4 2-2 2 2" /></>,
    brand: <><path d="m3 13 5-10 5 10M5 9h6" /></>,
    undo: <><path d="m6 3-4 4 4 4M2 7h7a4 4 0 0 1 4 4v2" /></>,
    redo: <><path d="m10 3 4 4-4 4m4-4H7a4 4 0 0 0-4 4v2" /></>,
    chat: <><path d="M3 3h10v8H7l-4 3z" /><path d="M5 6h6M5 8h4" /></>,
  }[name];

  return (
    <svg viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round">
      {icon}
    </svg>
  );
}

export function Centered({ children }: { children: React.ReactNode }) {
  return (
    <div className="grid flex-1 place-items-center p-10 text-sm text-muted">{children}</div>
  );
}
