/**
 * Narrowing helpers over the generated bindings.
 *
 * The types themselves are never redeclared here — they come from Rust and CI
 * fails when the committed bindings drift. What this file adds is the small
 * amount of narrowing TypeScript needs to work with a tagged union whose common
 * fields arrive flattened.
 */

import type { DesignRecord } from "@forma/bindings/DesignRecord";
import type { ImageLayer } from "@forma/bindings/ImageLayer";
import type { Layer } from "@forma/bindings/Layer";
import type { Page } from "@forma/bindings/Page";
import type { ShapeLayer } from "@forma/bindings/ShapeLayer";
import type { TextLayer } from "@forma/bindings/TextLayer";
import type { Transform } from "@forma/bindings/Transform";

export type { DesignRecord, ImageLayer, Layer, Page, ShapeLayer, TextLayer, Transform };

/** `LayerCommon` is `#[serde(flatten)]`ed, so these sit on every variant. */
export const layerId = (layer: Layer): string => layer.id;
export const layerName = (layer: Layer): string => layer.name;
export const layerTransform = (layer: Layer): Transform => layer.transform;

export interface Frame {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** The rectangle an overlay draws, taken from the document rather than the SVG. */
export function layerFrame(layer: Layer): Frame {
  const { x, y, width, height } = layer.transform;
  return { x, y, width, height };
}

export const isText = (layer: Layer): layer is { type: "text" } & TextLayer =>
  layer.type === "text";

export const isShape = (layer: Layer): layer is { type: "shape" } & ShapeLayer =>
  layer.type === "shape";

export const isImage = (layer: Layer): layer is { type: "image" } & ImageLayer =>
  layer.type === "image";

/** What a human calls each kind, for the layer list. */
export function layerKindLabel(layer: Layer): string {
  switch (layer.type) {
    case "text":
      return "Text";
    case "image":
      return "Image";
    case "shape":
      return layer.kind;
  }
}

export const findPage = (record: DesignRecord, pageId: string | null): Page | null =>
  record.document.pages.find((page) => page.id === pageId) ?? null;

export const findLayer = (page: Page | null, layerId: string | null): Layer | null =>
  page?.layers.find((layer) => layer.id === layerId) ?? null;
