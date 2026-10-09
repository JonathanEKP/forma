/** The export side panel (plan section 71). */

import { useState } from "react";

import { ApiError, exportDownloadUrl, type ExportResult, type ExportScope, type ValidationReport } from "../api";
import { useCreateExport } from "../queries";
import { Panel } from "./Panels";

export function ExportPanel({
  open,
  designId,
  designName,
  format,
  pageId,
  pageCount,
  version,
  report,
  disabled,
  onClose,
  onOpenChecks,
}: {
  open: boolean;
  designId: string;
  designName: string;
  format: { width: number; height: number };
  pageId: string | null;
  pageCount: number;
  version: number;
  report: ValidationReport | undefined;
  disabled: boolean;
  onClose: () => void;
  onOpenChecks: () => void;
}) {
  const [scope, setScope] = useState<ExportScope>("all_pages");
  const [scale, setScale] = useState(1);
  const [result, setResult] = useState<ExportResult | null>(null);
  const create = useCreateExport(designId);
  const errors = report?.issues.filter((issue) => issue.severity === "error") ?? [];
  const orientation = format.width === format.height
    ? "square"
    : format.width > format.height
      ? "landscape"
      : "portrait";

  if (!open) return null;

  const run = () => {
    setResult(null);
    create.mutate({ scope, pageId: scope === "single_page" ? pageId : null, scale }, { onSuccess: setResult });
  };

  if (create.isPending) {
    return (
      <Panel id="editor-export-panel" title="Export" onClose={onClose}>
        <div role="status" className="space-y-3 px-1 py-2">
          <h2 className="text-sm font-medium text-paper">Rendering…</h2>
          <p className="text-xs text-muted">{designName} · v{version}</p>
          <div className="grid gap-2 rounded-lg border border-edge bg-surface-2 p-3">
            <progress aria-label="Rendering PNG files" className="h-1 w-full accent-signal" />
            <p className="text-[11px] text-muted">Preparing {scope === "all_pages" ? pageCount : 1} PNG file{scope === "all_pages" && pageCount !== 1 ? "s" : ""}.</p>
          </div>
        </div>
      </Panel>
    );
  }

  if (result) {
    return (
      <Panel id="editor-export-panel" title="Export" onClose={onClose} footer={
        <button
          type="button"
          disabled={disabled}
          onClick={() => setResult(null)}
          className="w-full rounded-lg bg-signal px-3 py-2 text-xs font-semibold text-ink disabled:opacity-40"
        >
          Export again
        </button>
      }>
        <section className="space-y-3 px-1">
          <h2 className="text-sm font-medium text-paper">Your files are ready</h2>
          <p className="text-xs leading-relaxed text-muted">Made from v{result.export.design_version}. These files keep that version, even if the design changes.</p>
          <div className="space-y-2">
            {result.files.map((file) => (
              <div key={file.filename} className="border-b border-edge py-3 last:border-0">
                <a
                  href={exportDownloadUrl(result.export.id, file.filename)}
                  download={file.filename}
                  className="block min-h-11 break-all py-1 text-xs text-paper underline-offset-4 hover:underline"
                >
                  Download {file.filename} ↓
                </a>
                <span className="font-mono text-[10px] text-muted">
                  {format.width * scale} × {format.height * scale} · PNG
                </span>
              </div>
            ))}
          </div>
        </section>
      </Panel>
    );
  }

  return (
    <Panel id="editor-export-panel" title="Export" onClose={onClose} footer={
      <button
        type="button"
        disabled={disabled || create.isPending}
        onClick={run}
        className="w-full rounded-lg bg-signal px-3 py-2 text-xs font-semibold text-ink disabled:opacity-40"
      >
        {errors.length > 0 ? "Export anyway" : "Export PNG"}
      </button>
    }>
      <div className="space-y-5 px-1 pb-2">
        <section className="space-y-1">
          <h2 className="text-sm font-medium text-paper">{designName}</h2>
          <p className="font-mono text-[10px] text-muted">{format.width} × {format.height} · {orientation}</p>
          <p className="text-xs text-muted">PNG · one file for each page.</p>
        </section>

        {errors.length > 0 && (
          <section className="space-y-2 rounded-lg border border-signal/35 bg-signal/6 p-3">
            <p className="text-xs leading-relaxed text-paper">
              {errors.length} error{errors.length === 1 ? " needs" : "s need"} attention. You can still export.
            </p>
            <button
              type="button"
              onClick={onOpenChecks}
              className="min-h-9 px-0 text-left text-xs text-muted hover:text-paper"
            >
              Review errors →
            </button>
          </section>
        )}

        {create.error instanceof ApiError && (
          <p role="alert" className="rounded-lg border border-signal/45 bg-signal/6 px-2.5 py-2 text-[11px]">
            <code className="text-signal">{create.error.code}</code>{" "}
            <span className="text-muted">{create.error.message}</span>
          </p>
        )}

        <section className="space-y-2 border-t border-edge pt-4" aria-labelledby="export-pages-heading">
          <h3 id="export-pages-heading" className="text-xs font-medium text-paper">Pages</h3>
          {pageCount === 1 ? (
            <p className="text-xs text-muted">The page</p>
          ) : (
            <div className="flex gap-2">
              <Choice pressed={scope === "all_pages"} onClick={() => setScope("all_pages")}>All {pageCount}</Choice>
              <Choice pressed={scope === "single_page"} onClick={() => setScope("single_page")} disabled={pageId === null}>This page</Choice>
            </div>
          )}
        </section>

        <section className="space-y-2 border-t border-edge pt-4" aria-labelledby="export-size-heading">
          <h3 id="export-size-heading" className="text-xs font-medium text-paper">Size</h3>
          <div className="grid gap-2">
            {[1, 2, 4].map((factor) => (
              <Choice
                key={factor}
                pressed={scale === factor}
                onClick={() => setScale(factor)}
                className="text-left font-mono tabular-nums"
              >
                {factor}× · {format.width * factor} × {format.height * factor}
              </Choice>
            ))}
          </div>
        </section>

        <p className="border-t border-edge pt-4 text-[11px] leading-relaxed text-muted">
          The export keeps v{version}, exactly as it is when you start rendering.
        </p>
      </div>
    </Panel>
  );
}

function Choice({
  pressed,
  onClick,
  disabled = false,
  className = "",
  children,
}: {
  pressed: boolean;
  onClick: () => void;
  disabled?: boolean;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={pressed}
      disabled={disabled}
      onClick={onClick}
      className={`min-h-10 flex-1 rounded-lg border px-2.5 py-2 text-left text-[11px] disabled:opacity-40 ${
        pressed ? "border-signal/45 bg-signal/8 text-paper" : "border-edge bg-surface-2 text-muted"
      } ${className}`}
    >
      {children}
    </button>
  );
}
