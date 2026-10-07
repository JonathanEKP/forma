/**
 * The side panels of the layout in plan section 10.
 *
 * Signal orange marks one thing only: what is selected or active. If everything
 * were orange nothing would read as active (docs/brand/README.md).
 */

import type { DesignOperation } from "../api";
import { addPageOperation, addShapeOperation, addTextOperation, removeLayerOperation } from "./insert";
import type { DesignRecord, Layer, Page } from "../types";
import { findLayer, isImage, isShape, isText, layerKindLabel } from "../types";
import { useEditor } from "../store";

const row =
  "w-full text-left px-2.5 py-2 rounded-lg text-xs flex items-center gap-2.5 transition-colors";
const idle = "text-muted hover:text-paper hover:bg-surface-2";
const active = "text-paper bg-signal/10 outline outline-signal/40";

export function PagesAndLayers({
  record,
  onOperations,
  disabled,
  fontFamily,
  onClose,
  onSelectLayer,
  lockedLayerIds = new Set<string>(),
}: {
  record: DesignRecord;
  onOperations: (operations: DesignOperation[]) => void;
  disabled: boolean;
  fontFamily: string;
  onClose: () => void;
  onSelectLayer: () => void;
  lockedLayerIds?: ReadonlySet<string>;
}) {
  const { selectedPageId, selectedLayerId, selectPage, selectLayer } = useEditor();
  const approvalHighlight = useEditor((state) => state.approvalHighlight);
  const highlightedLayerIds = new Set(
    approvalHighlight?.designId === record.id
      ? Object.values(approvalHighlight.layerIdsByPage).flat()
      : [],
  );
  const page = record.document.pages.find((p) => p.id === selectedPageId) ?? null;
  const format = record.document.format;

  return (
    <aside className="editor-pane editor-flyout-pane w-64 shrink-0 overflow-y-auto scroll-thin" aria-label="Layers & pages">
      <header className="editor-pane-heading">
        <div className="text-[11px] font-medium text-paper">Layers &amp; pages</div>
        <button type="button" className="editor-close-button" onClick={onClose} aria-label="Close Layers & pages">
          ×
        </button>
      </header>
      <div className="px-3 pb-3">
        <section className="mb-5">
          <div className="editor-section-heading">
            <span>Pages</span>
            <span className="editor-section-count">{record.document.pages.length}</span>
          </div>
          <div className="flex items-center gap-2">
            <select
              id="current-page"
              aria-label="Current page"
              value={selectedPageId ?? ""}
              onChange={(event) => selectPage(event.target.value)}
              className="editor-page-select min-w-0 flex-1"
            >
              {record.document.pages.map((p, index) => (
                <option key={p.id} value={p.id}>
                  {String(index + 1).padStart(2, "0")} {p.name}
                </option>
              ))}
            </select>
            <button
              type="button"
              disabled={disabled}
              title="Add a page"
              aria-label="Add a page"
              className="editor-icon-button"
              onClick={() =>
                onOperations([addPageOperation(`Page ${record.document.pages.length + 1}`)])
              }
            >
              +
            </button>
          </div>
        </section>

        <section aria-label="Layers">
          <div className="editor-section-heading">
            <span>Layers</span>
            <span className="editor-section-count">{page?.layers.length ?? 0}</span>
          </div>

      {/* Without these there is no way to build a piece by hand, which is what
          Phase 6 has to prove. They send the same typed operations an agent
          would — the editor has no private authoring path. */}
      {page && (
        <div className="mb-2 flex gap-1.5 px-1">
          {(
            [
              ["Text", () => addTextOperation(page.id, format, fontFamily)],
              ["Rect", () => addShapeOperation(page.id, format)],
            ] as const
          ).map(([name, build]) => (
            <button
              key={name}
              type="button"
              disabled={disabled}
              className="flex-1 rounded-lg border border-edge bg-surface-2 py-1.5 text-[11px]
                         text-muted enabled:hover:border-edge-strong enabled:hover:text-paper
                         disabled:opacity-40"
              onClick={() => onOperations([build()])}
            >
              + {name}
            </button>
          ))}
        </div>
      )}

      {page === null && <Empty>Select a page.</Empty>}
      {page?.layers.length === 0 && <Empty>This page has no layers yet.</Empty>}

      {/* Reversed: the last layer paints on top, and that is how a designer
          expects to read the stack. */}
      {page?.layers
        .slice()
        .reverse()
        .map((layer) => {
          const selected = layer.id === selectedLayerId;
          const highlighted = highlightedLayerIds.has(layer.id);
          const locked = lockedLayerIds.has(layer.id);
          return (
            <button
              key={layer.id}
              type="button"
              disabled={disabled || locked}
              title={locked ? "This layer is read-only while an approval is pending." : undefined}
              aria-label={locked ? `${layer.name}, read-only while an approval is pending` : `${layer.name}, ${layerKindLabel(layer)} layer`}
              data-approval-highlight={highlighted ? "true" : undefined}
              data-approval-locked={locked ? "true" : undefined}
              className={`${row} ${selected ? active : highlighted ? "text-paper bg-signal/8 outline outline-1 outline-signal/35" : idle} ${locked ? "cursor-not-allowed opacity-70" : ""}`}
              onClick={() => {
                selectLayer(layer.id);
                onSelectLayer();
              }}
            >
              <LayerIcon layer={layer} active={selected || highlighted} />
              <span className="truncate flex-1">{layer.name}</span>
              {locked && <span className="text-faint text-[10px]">Read-only</span>}
            </button>
          );
        })}
        </section>
      </div>
    </aside>
  );
}

function LayerIcon({ layer, active }: { layer: Layer; active: boolean }) {
  const className = `editor-layer-icon${active ? " is-active" : ""}`;
  if (isText(layer)) return <span className={`${className} editor-layer-icon-text`} aria-hidden="true">T</span>;
  if (isShape(layer)) return <span className={`${className} editor-layer-icon-shape`} aria-hidden="true">□</span>;
  if (isImage(layer)) {
    return (
      <svg className={className} viewBox="0 0 16 16" fill="none" aria-hidden="true">
        <rect x="2" y="2.5" width="12" height="11" rx="1.5" />
        <circle cx="5.5" cy="6" r="1" />
        <path d="m3.5 11 3-3 2 2 1.5-1.5 2.5 2.5" />
      </svg>
    );
  }
  return null;
}

function Empty({ children }: { children: React.ReactNode }) {
  return <p className="px-2.5 py-2 text-xs text-faint">{children}</p>;
}

interface PropertiesProps {
  record: DesignRecord;
  page: Page | null;
  onOperations: (operations: DesignOperation[]) => void;
  disabled: boolean;
  onClose: () => void;
}

export function Properties({ record, page, onOperations, disabled, onClose }: PropertiesProps) {
  const selectedLayerId = useEditor((s) => s.selectedLayerId);
  const draft = useEditor((s) => s.draft);
  const layer = findLayer(page, selectedLayerId);

  if (!layer) {
    return (
      <Panel title="Properties" onClose={onClose}>
        <Empty>Select a layer to edit it.</Empty>
      </Panel>
    );
  }

  // While a pointer is down the numbers come from the draft, so the panel and
  // the canvas never disagree about where the layer is.
  const frame = draft?.layerId === layer.id ? draft : layer.transform;

  return (
    <Panel title="Properties" onClose={onClose}>
      <div className="editor-property-name">
        <strong className="truncate">{layer.name}</strong>
        <span>{page?.name ?? "Page"} · {layerKindLabel(layer)}</span>
      </div>

      <div className="editor-property-grid" role="group" aria-label="Layer position and size">
        <PropertyValue label="X" value={Math.round(frame.x)} />
        <PropertyValue label="Y" value={Math.round(frame.y)} />
        <PropertyValue label="Width" value={Math.round(frame.width)} />
        <PropertyValue label="Height" value={Math.round(frame.height)} />
      </div>

      {isText(layer) && (
        <TextProperties
          layer={layer}
          disabled={disabled}
          onOperations={onOperations}
        />
      )}

      {isShape(layer) && (
        <Field label="Shape">
          <span>{layer.kind}</span>
          <span className="text-faint">{Math.round(layer.corner_radius)}px radius</span>
        </Field>
      )}

      <button
        type="button"
        disabled={disabled}
        onClick={() => onOperations([removeLayerOperation(layer.id)])}
        className="w-full rounded-lg border border-edge bg-surface-2 py-2 text-[11px]
                   text-muted enabled:hover:border-signal/45 enabled:hover:text-paper
                   disabled:opacity-40"
      >
        Delete layer
      </button>

      <p className="mt-4 px-1 text-[11px] leading-relaxed text-faint">
        Version {record.version}. Every edit is one version, and undo adds
        another rather than going back to one.
      </p>
    </Panel>
  );
}

/**
 * Plan section 40: V1 edits text content from the properties panel rather than
 * in place on the canvas, and commits on blur — one edit, one version.
 */
function TextProperties({
  layer,
  disabled,
  onOperations,
}: {
  layer: { type: "text" } & import("../types").TextLayer;
  disabled: boolean;
  onOperations: (operations: DesignOperation[]) => void;
}) {
  return (
    <>
      <div className="editor-property-field">
        <label htmlFor="editor-layer-text">Text</label>
        <textarea
          id="editor-layer-text"
          key={`${layer.id}:${layer.text}`}
          defaultValue={layer.text}
          rows={3}
          disabled={disabled}
          aria-label="Text"
          className="w-full rounded-lg border border-edge bg-surface-2 px-2.5 py-2 text-xs
                     text-paper outline-none focus:border-signal disabled:opacity-50"
          onBlur={(event) => {
            const content = event.target.value;
            if (content === layer.text) return;
            onOperations([{ op: "set_text", layer_id: layer.id, content }]);
          }}
        />
        {/* Plan section 18: no implicit wrapping, so a line break is a real \n. */}
        <p className="mt-1.5 text-[10px] text-faint">
          Line breaks are literal. Text does not wrap on its own.
        </p>
      </div>

      <PropertyValue label="Font size" value={`${Math.round(layer.font_size)} px`} />
      <PropertyValue label="Font family" value={layer.font_family} />
    </>
  );
}

function PropertyValue({ label: name, value }: { label: string; value: string | number }) {
  return (
    <div className="editor-property-value">
      <span>{name}</span>
      <span className="tabular-nums">{value}</span>
    </div>
  );
}

export function Panel({
  title,
  children,
  onClose,
}: {
  title: string;
  children: React.ReactNode;
  onClose?: () => void;
}) {
  return (
    <aside
      className={`editor-pane ${onClose ? "editor-flyout-pane" : "editor-properties-pane"} w-64 shrink-0 overflow-y-auto scroll-thin`}
      aria-label={title}
    >
      <header className="editor-pane-heading">
        <div className="editor-panel-title">{title}</div>
        {onClose && (
          <button type="button" className="editor-close-button" onClick={onClose} aria-label={`Close ${title}`}>
            ×
          </button>
        )}
      </header>
      <div className="px-3 pb-4">{children}</div>
    </aside>
  );
}

function Field({ label: name, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="mb-3.5">
      <div className="text-faint text-[10px] mb-1.5">{name.toUpperCase()}</div>
      <div className="flex items-center justify-between gap-2 rounded-lg border border-edge
                      bg-surface-2 px-2.5 py-2 text-xs text-muted">
        {children}
      </div>
    </div>
  );
}

/** Which layers exist is the document's business; this is only presentation. */
export const layerCount = (page: Page | null): number => page?.layers.length ?? 0;

export type { Layer };
