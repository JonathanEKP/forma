/**
 * The side panels of the layout in plan section 10.
 *
 * Signal orange marks one thing only: what is selected or active. If everything
 * were orange nothing would read as active (docs/brand/README.md).
 */

import type { DesignOperation } from "../api";
import { addPageOperation, addShapeOperation, addTextOperation, removeLayerOperation } from "./insert";
import type { DesignRecord, Layer, Page } from "../types";
import { findLayer, isShape, isText, layerKindLabel } from "../types";
import { useEditor } from "../store";

const row =
  "w-full text-left px-2.5 py-2 rounded-lg text-xs flex items-center gap-2.5 transition-colors";
const idle = "text-muted hover:text-paper hover:bg-surface-2";
const active = "text-paper bg-signal/10 outline outline-signal/40";

const label = "text-faint uppercase text-[10px] tracking-[0.11em] px-2 mt-1 mb-2";

export function PagesAndLayers({
  record,
  onOperations,
  disabled,
  fontFamily,
}: {
  record: DesignRecord;
  onOperations: (operations: DesignOperation[]) => void;
  disabled: boolean;
  fontFamily: string;
}) {
  const { selectedPageId, selectedLayerId, selectPage, selectLayer } = useEditor();
  const page = record.document.pages.find((p) => p.id === selectedPageId) ?? null;
  const format = record.document.format;

  return (
    <aside className="w-56 shrink-0 border-r border-edge bg-surface overflow-y-auto scroll-thin p-1.5">
      <div className="flex items-center justify-between">
        <div className={label}>Pages</div>
        <button
          type="button"
          disabled={disabled}
          title="Add a page"
          className="mr-1.5 rounded px-1.5 text-xs text-faint hover:text-paper disabled:opacity-40"
          onClick={() =>
            onOperations([addPageOperation(`Page ${record.document.pages.length + 1}`)])
          }
        >
          +
        </button>
      </div>
      {record.document.pages.map((p, index) => (
        <button
          key={p.id}
          type="button"
          className={`${row} ${p.id === selectedPageId ? active : idle}`}
          onClick={() => selectPage(p.id)}
        >
          <Dot on={p.id === selectedPageId} />
          <span className="tabular-nums text-faint">
            {String(index + 1).padStart(2, "0")}
          </span>
          <span className="truncate">{p.name}</span>
        </button>
      ))}

      <div className={label}>Layers</div>

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
        .map((layer) => (
          <button
            key={layer.id}
            type="button"
            className={`${row} ${layer.id === selectedLayerId ? active : idle}`}
            onClick={() => selectLayer(layer.id)}
          >
            <Dot on={layer.id === selectedLayerId} />
            <span className="truncate flex-1">{layer.name}</span>
            <span className="text-faint text-[10px]">{layerKindLabel(layer)}</span>
          </button>
        ))}
    </aside>
  );
}

function Dot({ on }: { on: boolean }) {
  return (
    <span
      className={`h-[7px] w-[7px] rounded-sm shrink-0 ${on ? "bg-signal" : "bg-faint"}`}
    />
  );
}

function Empty({ children }: { children: React.ReactNode }) {
  return <p className="px-2.5 py-2 text-xs text-faint">{children}</p>;
}

interface PropertiesProps {
  record: DesignRecord;
  page: Page | null;
  onOperations: (operations: DesignOperation[]) => void;
  disabled: boolean;
}

export function Properties({ record, page, onOperations, disabled }: PropertiesProps) {
  const selectedLayerId = useEditor((s) => s.selectedLayerId);
  const draft = useEditor((s) => s.draft);
  const layer = findLayer(page, selectedLayerId);

  if (!layer) {
    return (
      <Panel title="Properties">
        <Empty>Select a layer to edit it.</Empty>
      </Panel>
    );
  }

  // While a pointer is down the numbers come from the draft, so the panel and
  // the canvas never disagree about where the layer is.
  const frame = draft?.layerId === layer.id ? draft : layer.transform;

  return (
    <Panel title="Properties">
      <Field label="Layer">
        <span className="truncate">{layer.name}</span>
        <span className="text-faint">{layerKindLabel(layer)}</span>
      </Field>

      <Field label="Position">
        <span className="tabular-nums">X {Math.round(frame.x)}</span>
        <span className="tabular-nums">Y {Math.round(frame.y)}</span>
      </Field>

      <Field label="Size">
        <span className="tabular-nums">W {Math.round(frame.width)}</span>
        <span className="tabular-nums">H {Math.round(frame.height)}</span>
      </Field>

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
      <div className="mb-4">
        <div className="text-faint text-[10px] mb-1.5 px-1">CONTENT</div>
        <textarea
          key={`${layer.id}:${layer.text}`}
          defaultValue={layer.text}
          rows={3}
          disabled={disabled}
          className="w-full rounded-lg border border-edge bg-surface-2 px-2.5 py-2 text-xs
                     text-paper outline-none focus:border-signal disabled:opacity-50"
          onBlur={(event) => {
            const content = event.target.value;
            if (content === layer.text) return;
            onOperations([{ op: "set_text", layer_id: layer.id, content }]);
          }}
        />
        {/* Plan section 18: no implicit wrapping, so a line break is a real \n. */}
        <p className="mt-1.5 px-1 text-[10px] text-faint">
          Line breaks are literal. Text does not wrap on its own.
        </p>
      </div>

      <Field label="Type">
        <span className="truncate">{layer.font_family}</span>
        <span className="text-faint tabular-nums">
          {Math.round(layer.font_size)}px / {layer.font_weight}
        </span>
      </Field>
    </>
  );
}

export function Panel({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <aside className="w-64 shrink-0 border-l border-edge bg-surface overflow-y-auto scroll-thin p-1.5">
      <div className={label}>{title}</div>
      <div className="px-1">{children}</div>
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
