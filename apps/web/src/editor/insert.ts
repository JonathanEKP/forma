/**
 * Building the operations that add a layer.
 *
 * Phase 6's acceptance is that a complete piece can be built by hand without
 * MCP, which needs a way to put something on the page. These construct the same
 * typed operations an agent would send — there is no separate authoring path
 * for the editor (plan section 28).
 */

import type { DesignOperation } from "../api";
import type { DesignFormat } from "@forma/bindings/DesignFormat";

/**
 * Fields `serde` defaults on the Rust side are still required in the generated
 * TypeScript, so they are written out rather than omitted. Being explicit here
 * also means the editor never relies on a default it cannot see.
 */
const common = (id: string, name: string, frame: Frame) => ({
  id,
  name,
  visible: true,
  locked: false,
  opacity: 1,
  transform: { ...frame, rotation_deg: 0 },
});

interface Frame {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** A frame roughly centred on the canvas, so a new layer lands in view. */
function centred(format: DesignFormat, width: number, height: number): Frame {
  return {
    x: Math.round((format.width - width) / 2),
    y: Math.round((format.height - height) / 2),
    width,
    height,
  };
}

/** The default copy, and the number of lines its box has to hold. */
const SAMPLE = "Give visual ideas\na form.";
const SAMPLE_LINES = SAMPLE.split("\n").length;
const LINE_HEIGHT = 1.2;

export function addTextOperation(
  pageId: string,
  format: DesignFormat,
  fontFamily: string,
): DesignOperation {
  const width = Math.round(format.width * 0.8);

  /*
   * Sized to fit rather than to look dramatic. The editor cannot measure text —
   * shaping belongs to the renderer (plan section 35) — so an inserted layer
   * that guessed large would arrive already reporting TEXT_OVERFLOW, and the
   * first thing a user saw would be two errors they did not cause. A
   * conservative size is one they can grow deliberately.
   */
  const fontSize = Math.round(format.width * 0.055);
  const height = Math.ceil(fontSize * LINE_HEIGHT * SAMPLE_LINES * 1.3);

  return {
    op: "add_layer",
    page_id: pageId,
    layer: {
      type: "text",
      ...common(crypto.randomUUID(), "Text", centred(format, width, height)),
      // Plan section 18: no implicit wrapping, so the break is a real newline.
      text: SAMPLE,
      font_family: fontFamily,
      font_size: fontSize,
      font_weight: 600,
      line_height: LINE_HEIGHT,
      letter_spacing: 0,
      align: "left",
      color: "#171717",
    },
  };
}

export function addShapeOperation(pageId: string, format: DesignFormat): DesignOperation {
  const size = Math.round(format.width * 0.35);

  return {
    op: "add_layer",
    page_id: pageId,
    layer: {
      type: "shape",
      ...common(crypto.randomUUID(), "Rectangle", centred(format, size, size)),
      kind: "rectangle",
      fill: "#FF5A36",
      stroke: null,
      stroke_width: 0,
      corner_radius: 0,
    },
  };
}

export const removeLayerOperation = (layerId: string): DesignOperation => ({
  op: "remove_layer",
  layer_id: layerId,
});

export const addPageOperation = (name: string): DesignOperation => ({
  op: "add_page",
  page: { id: crypto.randomUUID(), name, layers: [] },
});
