/**
 * The canvas: an authoritative SVG from the backend, with an interaction layer
 * drawn on top (plan section 37).
 *
 * The editor never reimplements typography. What it draws is the normalized
 * tree usvg produced, with text already flattened to paths, so what the user
 * sees is what the export will contain. The overlay's geometry comes from the
 * `DesignDocument`, not from measuring the SVG.
 */

import { useCallback, useRef, useState } from "react";

import type { DesignRecord, Page } from "../types";
import { layerFrame, layerId, layerName } from "../types";
import { useEditor, type DraftFrame } from "../store";

/** The eight-pixel square a resize grabs, in screen pixels. */
const HANDLE = 9;

type Handle = "nw" | "ne" | "sw" | "se";

interface Props {
  record: DesignRecord;
  page: Page;
  svg: string | undefined;
  onCommit: (frame: DraftFrame) => void;
  disabled: boolean;
  onSelectLayer: () => void;
  lockedLayerIds?: ReadonlySet<string>;
}

interface Drag {
  layerId: string;
  handle: Handle | null;
  startX: number;
  startY: number;
  origin: { x: number; y: number; width: number; height: number };
}

export function Canvas({ record, page, svg, onCommit, disabled, onSelectLayer, lockedLayerIds = new Set<string>() }: Props) {
  const { width, height } = record.document.format;
  const zoom = useEditor((s) => s.zoom);
  const selectedLayerId = useEditor((s) => s.selectedLayerId);
  const selectLayer = useEditor((s) => s.selectLayer);
  const draft = useEditor((s) => s.draft);
  const setDraft = useEditor((s) => s.setDraft);
  const approvalHighlight = useEditor((s) => s.approvalHighlight);
  const highlightedLayerIds = new Set(
    approvalHighlight?.designId === record.id
      ? Object.values(approvalHighlight.layerIdsByPage).flat()
      : [],
  );

  const surface = useRef<HTMLDivElement>(null);
  const [drag, setDrag] = useState<Drag | null>(null);

  /** Pointer delta in canvas units, which is screen pixels divided by zoom. */
  const toCanvas = useCallback((dx: number, dy: number) => ({ dx: dx / zoom, dy: dy / zoom }), [zoom]);

  const begin = (event: React.PointerEvent, id: string, handle: Handle | null) => {
    event.stopPropagation();
    event.preventDefault();
    if (disabled || lockedLayerIds.has(id)) return;

    const layer = page.layers.find((l) => layerId(l) === id);
    if (!layer) return;

    selectLayer(id);
    onSelectLayer();
    (event.target as Element).setPointerCapture(event.pointerId);

    setDrag({
      layerId: id,
      handle,
      startX: event.clientX,
      startY: event.clientY,
      origin: layerFrame(layer),
    });
  };

  /**
   * Plan section 38: the pointer move only touches local state. Sending a
   * request per frame would make one drag into fifty versions.
   */
  const move = (event: React.PointerEvent) => {
    if (!drag) return;
    const { dx, dy } = toCanvas(event.clientX - drag.startX, event.clientY - drag.startY);
    const { x, y, width: w, height: h } = drag.origin;

    const next: DraftFrame = drag.handle
      ? resize(drag.handle, { x, y, width: w, height: h }, dx, dy, drag.layerId)
      : { layerId: drag.layerId, x: x + dx, y: y + dy, width: w, height: h };

    setDraft(next);
  };

  /** One pointer-up, one operation, one version. */
  const end = () => {
    if (drag && draft) onCommit(draft);
    setDrag(null);
    setDraft(null);
  };

  const pageNumber = record.document.pages.findIndex((candidate) => candidate.id === page.id) + 1;
  const selectedLayer = page.layers.find((layer) => layer.id === selectedLayerId);
  const status = selectedLayer
    ? `${layerName(selectedLayer)} selected · page ${pageNumber}`
    : "Select a layer to inspect it.";

  return (
    <div className="editor-canvas-stage">
      <div className="editor-canvas-caption">
        {String(pageNumber).padStart(2, "0")} {page.name} / {width} × {height}
      </div>
      <div
        className="editor-canvas-scroll min-w-0 min-h-0 overflow-auto scroll-thin grid place-items-center p-6"
        onPointerDown={() => selectLayer(null)}
      >
        <div
          ref={surface}
          className="relative shadow-2xl shrink-0"
          style={{ width: width * zoom, height: height * zoom }}
          onPointerMove={move}
          onPointerUp={end}
          onPointerCancel={end}
        >
          {/*
            The backend's SVG, scaled by zoom. It is inert: every pointer event
            belongs to the overlay, so a click never lands on a glyph path.
          */}
          <div
            className="absolute inset-0 pointer-events-none bg-paper"
            style={{
              width,
              height,
              transform: `scale(${zoom})`,
              transformOrigin: "top left",
            }}
            // The SVG comes from our own backend, which built it from the
            // document rather than from anything a user typed as markup.
            dangerouslySetInnerHTML={svg ? { __html: svg } : undefined}
          />

          <svg
            className="absolute inset-0 h-full w-full"
            viewBox={`0 0 ${width} ${height}`}
            preserveAspectRatio="none"
          >
            {page.layers.map((layer) => {
              const id = layerId(layer);
              const frame =
                draft?.layerId === id ? draft : layerFrame(layer);
              const selected = id === selectedLayerId;
              const highlighted = highlightedLayerIds.has(id);
              const locked = lockedLayerIds.has(id);

              return (
                <g key={id}>
                  <rect
                    x={frame.x}
                    y={frame.y}
                    width={Math.max(frame.width, 0)}
                    height={Math.max(frame.height, 0)}
                    fill="transparent"
                    stroke={selected || highlighted ? "var(--color-signal)" : "transparent"}
                    strokeWidth={(selected ? 1 : highlighted ? 2 : 0) / zoom}
                    vectorEffect="non-scaling-stroke"
                    className={locked ? "cursor-not-allowed" : "cursor-move"}
                    data-approval-highlight={highlighted ? "true" : undefined}
                    data-approval-locked={locked ? "true" : undefined}
                    style={{ pointerEvents: disabled || locked ? "none" : "all" }}
                    onPointerDown={(e) => begin(e, id, null)}
                  >
                    <title>{locked ? `${layerName(layer)} · Read-only while approval is pending` : layerName(layer)}</title>
                  </rect>

                  {selected && !locked &&
                    handlesOf(frame).map(({ handle, cx, cy, cursor }) => (
                      <rect
                        key={handle}
                        x={cx - HANDLE / (2 * zoom)}
                        y={cy - HANDLE / (2 * zoom)}
                        width={HANDLE / zoom}
                        height={HANDLE / zoom}
                        fill="var(--color-signal)"
                        stroke="var(--color-paper)"
                        strokeWidth={1.5 / zoom}
                        style={{ cursor, pointerEvents: disabled ? "none" : "all" }}
                        onPointerDown={(e) => begin(e, id, handle)}
                      />
                    ))}
                </g>
              );
            })}
          </svg>
        </div>
      </div>
      <div className="editor-canvas-status">{status}</div>
    </div>
  );
}

interface Frame {
  x: number;
  y: number;
  width: number;
  height: number;
}

/**
 * Resizes from one corner, keeping the opposite one fixed.
 *
 * Width and height are floored at one unit: the server rejects a
 * non-positive dimension, and letting the pointer produce one would turn an
 * ordinary drag into a rejected batch.
 */
function resize(handle: Handle, f: Frame, dx: number, dy: number, id: string): DraftFrame {
  const left = handle === "nw" || handle === "sw";
  const top = handle === "nw" || handle === "ne";

  const width = Math.max(1, left ? f.width - dx : f.width + dx);
  const height = Math.max(1, top ? f.height - dy : f.height + dy);

  return {
    layerId: id,
    x: left ? f.x + (f.width - width) : f.x,
    y: top ? f.y + (f.height - height) : f.y,
    width,
    height,
  };
}

function handlesOf(f: Frame): Array<{ handle: Handle; cx: number; cy: number; cursor: string }> {
  return [
    { handle: "nw", cx: f.x, cy: f.y, cursor: "nwse-resize" },
    { handle: "ne", cx: f.x + f.width, cy: f.y, cursor: "nesw-resize" },
    { handle: "sw", cx: f.x, cy: f.y + f.height, cursor: "nesw-resize" },
    { handle: "se", cx: f.x + f.width, cy: f.y + f.height, cursor: "nwse-resize" },
  ];
}
