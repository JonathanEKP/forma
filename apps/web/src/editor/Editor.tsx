/**
 * The editor shell: the layout of plan section 10.
 *
 * Top bar, pages and layers on the left, canvas in the middle, and one of
 * properties, validation or assets on the right. V1 does not try to compete
 * with Figma.
 */

import { useEffect, useMemo } from "react";

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
import { findPage } from "../types";
import { Assets } from "./Assets";
import { Canvas } from "./Canvas";
import { addImageOperation } from "./insert";
import { PagesAndLayers, Properties } from "./Panels";
import { Validation } from "./Validation";

export function Editor({ designId, onBack }: { designId: string; onBack: () => void }) {
  const { data: record, isLoading, error } = useDesign(designId);
  const { selectedPageId, selectPage, panel, setPanel, zoom, setZoom } = useEditor();

  const apply = useApplyOperations(designId);
  const undo = useUndo(designId);

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

  return (
    <div className="flex h-full flex-col">
      <header className="flex h-[54px] shrink-0 items-center gap-3 border-b border-edge bg-surface px-4">
        <button
          type="button"
          onClick={onBack}
          className="text-xs text-muted hover:text-paper"
        >
          ← Projects
        </button>
        <span className="text-[13px] font-medium">{record.name}</span>
        <span className="text-[11px] text-faint tabular-nums">v{record.version}</span>

        <div className="flex-1" />

        <ZoomControl zoom={zoom} setZoom={setZoom} />

        <button
          type="button"
          disabled={busy || record.version <= 1}
          onClick={() => undo.mutate(record.version)}
          className="rounded-lg border border-edge bg-surface-2 px-2.5 py-1.5 text-xs
                     text-muted enabled:hover:text-paper disabled:opacity-40"
        >
          Undo
        </button>

        {(["properties", "validation", "assets"] as const).map((name) => (
          <button
            key={name}
            type="button"
            onClick={() => setPanel(name)}
            className={`rounded-lg border px-2.5 py-1.5 text-xs capitalize ${
              panel === name
                ? "border-signal bg-signal text-ink font-semibold"
                : "border-edge bg-surface-2 text-muted hover:text-paper"
            }`}
          >
            {name}
          </button>
        ))}
      </header>

      {/* The conflict is not a toast: the edit did not happen, and the user has
          to know that before they carry on typing (plan section 27). */}
      {apply.error instanceof ApiError && apply.error.isConflict && (
        <Conflict error={apply.error} onReload={() => apply.reset()} />
      )}

      {apply.error instanceof ApiError && !apply.error.isConflict && (
        <Rejected error={apply.error} onDismiss={() => apply.reset()} />
      )}

      <div className="flex min-h-0 flex-1">
        <PagesAndLayers
          record={record}
          onOperations={send}
          disabled={busy}
          fontFamily={fontFamily}
        />

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

        {panel === "validation" && (
          <Validation report={validation.data} loading={validation.isLoading} />
        )}

        {panel === "assets" && (
          <Assets
            projectId={record.project_id}
            onPlace={placeAsset}
            disabled={busy || page === null}
          />
        )}

        {panel === "properties" && (
          <Properties
            record={record}
            page={page}
            onOperations={send}
            disabled={busy}
          />
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
  return (
    <div className="flex items-center gap-1.5 text-xs text-muted">
      <button type="button" onClick={() => setZoom(zoom / 1.25)} className="px-1.5 hover:text-paper">
        −
      </button>
      <span className="w-11 text-center tabular-nums">{Math.round(zoom * 100)}%</span>
      <button type="button" onClick={() => setZoom(zoom * 1.25)} className="px-1.5 hover:text-paper">
        +
      </button>
    </div>
  );
}

export function Centered({ children }: { children: React.ReactNode }) {
  return (
    <div className="grid flex-1 place-items-center p-10 text-sm text-muted">{children}</div>
  );
}
