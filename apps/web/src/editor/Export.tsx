/**
 * Exporting from the editor (plan section 10's top-right Export, and section
 * 71).
 *
 * Phase 6's acceptance is that a whole piece can be built by hand without MCP.
 * Until this existed that was only half true: you could build one and had no
 * way to get a PNG out of it.
 *
 * The export pins the version it came from, so what you download stays what you
 * saw even after the design moves on.
 */

import { useState } from "react";

import {
  ApiError,
  exportDownloadUrl,
  type ExportResult,
  type ExportScope,
  type ValidationReport,
} from "../api";
import { useCreateExport } from "../queries";

export function ExportButton({
  designId,
  pageId,
  pageCount,
  report,
  disabled,
}: {
  designId: string;
  pageId: string | null;
  pageCount: number;
  report: ValidationReport | undefined;
  disabled: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [scope, setScope] = useState<ExportScope>("all_pages");
  const [scale, setScale] = useState(1);
  const [result, setResult] = useState<ExportResult | null>(null);
  const create = useCreateExport(designId);

  // Section 42: errors are the ones that mean the design is broken. The editor
  // says so and still lets a person decide, which is the difference between a
  // human and the agent — the skill tells the agent to fix them first.
  const errors = report?.issues.filter((issue) => issue.severity === "error") ?? [];

  const run = () => {
    setResult(null);
    create.mutate(
      { scope, pageId: scope === "single_page" ? pageId : null, scale },
      { onSuccess: setResult },
    );
  };

  return (
    <div className="relative">
      <button
        type="button"
        disabled={disabled}
        onClick={() => setOpen((was) => !was)}
        aria-expanded={open}
        aria-controls="editor-export-options"
        className="editor-export-button rounded-lg px-4 py-2 text-xs font-semibold disabled:opacity-40"
      >
        Export
      </button>

      {open && (
        <div
          id="editor-export-options"
          role="region"
          aria-label="Export options"
          className="absolute right-0 top-[38px] z-20 w-64 rounded-xl border border-edge
                     bg-surface p-3 shadow-lg"
        >
          {errors.length > 0 && (
            <div className="mb-2.5 rounded-lg border border-signal/45 bg-signal/6 px-2.5 py-2 text-[11px] leading-snug">
              <strong className="font-semibold">
                {errors.length} validation error{errors.length === 1 ? "" : "s"}.
              </strong>{" "}
              <span className="text-muted">
                Exporting now ships {errors.length === 1 ? "it" : "them"}. The
                Validation panel has the detail.
              </span>
            </div>
          )}

          <div className="mb-1.5 text-[10px] text-faint">PAGES</div>
          <div className="mb-3 flex gap-1">
            {(
              [
                ["all_pages", pageCount === 1 ? "The page" : `All ${pageCount}`],
                ["single_page", "This page"],
              ] as const
            ).map(([value, label]) => (
              <button
                key={value}
                type="button"
                onClick={() => setScope(value)}
                disabled={value === "single_page" && pageId === null}
                className={`flex-1 rounded-lg border px-2 py-1.5 text-[11px] disabled:opacity-40 ${
                  scope === value
                    ? "border-signal/45 bg-signal/8 text-paper"
                    : "border-edge bg-surface-2 text-muted"
                }`}
              >
                {label}
              </button>
            ))}
          </div>

          <div className="mb-1.5 text-[10px] text-faint">SIZE</div>
          <div className="mb-3 flex gap-1">
            {[1, 2, 4].map((factor) => (
              <button
                key={factor}
                type="button"
                onClick={() => setScale(factor)}
                className={`flex-1 rounded-lg border px-2 py-1.5 text-[11px] tabular-nums ${
                  scale === factor
                    ? "border-signal/45 bg-signal/8 text-paper"
                    : "border-edge bg-surface-2 text-muted"
                }`}
              >
                {factor}×
              </button>
            ))}
          </div>

          {create.error instanceof ApiError && (
            <div className="mb-2 rounded-lg border border-signal/45 bg-signal/6 px-2.5 py-2 text-[11px]">
              <code className="text-[10px] text-signal">{create.error.code}</code>{" "}
              <span className="text-muted">{create.error.message}</span>
            </div>
          )}

          <button
            type="button"
            disabled={create.isPending}
            onClick={run}
            className="w-full rounded-lg bg-signal px-2.5 py-2 text-xs font-semibold text-ink
                       disabled:opacity-40"
          >
            {/* Says what it will produce, which also stops it echoing the
                header button it sits under. */}
            {create.isPending
              ? "Rendering…"
              : errors.length > 0
                ? "Export anyway"
                : scope === "single_page"
                  ? "Export this page"
                  : `Export ${pageCount} page${pageCount === 1 ? "" : "s"}`}
          </button>

          {result && (
            <div className="mt-3">
              <div className="mb-1.5 text-[10px] text-faint">
                VERSION {result.export.design_version} · {result.files.length} FILE
                {result.files.length === 1 ? "" : "S"}
              </div>
              {result.files.map((file) => (
                <a
                  key={file.filename}
                  href={exportDownloadUrl(result.export.id, file.filename)}
                  download={file.filename}
                  className="mb-1 flex items-center justify-between gap-2 rounded-lg border
                             border-edge bg-surface-2 px-2.5 py-2 text-[11px] text-muted
                             transition-colors hover:border-edge-strong hover:text-paper"
                >
                  <span className="truncate">{file.filename}</span>
                  <span className="shrink-0 text-[10px] text-faint tabular-nums">
                    {Math.round(file.byte_size / 1024)} KB
                  </span>
                </a>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
