/** Reader-facing checks for the current design version. */

import type { ValidationReport } from "../api";
import { useEditor } from "../store";
import type { DesignRecord } from "../types";

type Issue = ValidationReport["issues"][number];

const MESSAGES: Readonly<Record<string, string>> = {
  INVALID_PAGE_SIZE: "The canvas size is invalid.",
  MISSING_LAYER: "This page has no layers, so it will export blank.",
  DUPLICATE_LAYER_ID: "Two layers share the same identity.",
  INVALID_OPACITY: "The layer opacity must be between 0% and 100%.",
  INVALID_DIMENSION: "The layer has an invalid size or position.",
  OUTSIDE_CANVAS: "This layer extends past the canvas edge.",
  OUTSIDE_SAFE_AREA: "This layer extends beyond the safe margin.",
  MISSING_BRAND_FONT: "A Brand Kit font is missing from Forma’s font catalog.",
  UNKNOWN_BRAND_TOKEN: "A brand colour or font token cannot be resolved.",
  COLOR_OUTSIDE_BRAND: "This colour is not part of the Brand Kit.",
  ASSET_NOT_FOUND: "An image used by this layer is missing.",
  ASSET_UNREADABLE: "An image used by this layer cannot be read.",
  INVALID_ASSET_TYPE: "This layer uses a file that cannot be displayed as an image.",
  FONT_NOT_FOUND: "This font is not available in Forma.",
  TEXT_INVALID_CHARACTER: "This text contains a character Forma cannot render.",
  FONT_TOO_SMALL: "This text is below the Brand Kit’s minimum font size.",
  TEXT_LOW_CONTRAST: "This text is difficult to read against its background.",
  TEXT_OVERFLOW_WIDTH: "This text is too wide for its box.",
  TEXT_OVERFLOW_HEIGHT: "This text is too tall for its box.",
};

/** Maps every validator code to copy that does not expose implementation terms. */
export function validationMessage(issue: Issue): string {
  if (issue.code === "OUTSIDE_CANVAS" && issue.severity === "error" && issue.message.includes("entirely outside")) {
    return "This layer is outside the canvas.";
  }
  return MESSAGES[issue.code] ?? "This item needs attention.";
}

export function Validation({
  record,
  report,
  loading,
  onSelect,
  onCheckAgain,
}: {
  record: DesignRecord;
  report: ValidationReport | undefined;
  loading: boolean;
  onSelect?: () => void;
  onCheckAgain?: () => void;
}) {
  const selectLayer = useEditor((state) => state.selectLayer);
  const selectPage = useEditor((state) => state.selectPage);

  if (loading || !report) {
    return (
      <div role="status" className="grid gap-3 py-2 text-xs text-muted">
        <h2 className="font-medium text-paper">Checking…</h2>
        <progress aria-label="Checking design" className="h-1 w-full accent-signal" />
        <p>Checking text, layout and Brand Kit rules.</p>
      </div>
    );
  }

  const errors = report.issues.filter((issue) => issue.severity === "error");
  const warnings = report.issues.filter((issue) => issue.severity === "warning");

  return (
    <div className="space-y-4">
      <section className="space-y-1.5">
        <h2 className="text-sm font-medium text-paper">
          {errors.length === 0
            ? "No errors."
            : `${errors.length} error${errors.length === 1 ? " needs" : "s need"} attention`}
        </h2>
        {errors.length === 0 && <p className="text-xs leading-relaxed text-muted">This design can be exported.</p>}
        {warnings.length > 0 && (
          <p className="text-xs leading-relaxed text-muted">
            {warnings.length} warning{warnings.length === 1 ? "" : "s"} · review these before exporting. Export anyway is available.
          </p>
        )}
      </section>

      <div className="divide-y divide-edge">
        {report.issues.map((issue, index) => {
          const page = record.document.pages.find((item) => item.id === issue.page_id);
          const layer = page?.layers.find((item) => item.id === issue.layer_id);
          const target = layer
            ? `${layer.name} · ${page?.name ?? "Page"}`
            : page
              ? `Page · ${page.name}`
              : "Design";

          return (
            <article key={`${issue.code}:${issue.layer_id ?? issue.page_id ?? "design"}:${index}`} className="space-y-1.5 py-4 first:pt-0 last:pb-0">
              <button
                type="button"
                disabled={!issue.layer_id && !issue.page_id}
                onClick={() => {
                  if (issue.page_id) selectPage(issue.page_id);
                  if (issue.layer_id) selectLayer(issue.layer_id);
                  onSelect?.();
                }}
                className="grid w-full min-h-11 gap-1.5 rounded-lg text-left disabled:cursor-default"
              >
                <span className="flex items-center gap-2 text-[10px] uppercase tracking-wide text-muted">
                  <span
                    aria-label={issue.severity === "error" ? "Error" : "Warning"}
                    className={`h-[7px] w-[7px] shrink-0 rounded-full ${issue.severity === "error" ? "bg-signal" : "bg-graphite"}`}
                  />
                  {issue.severity === "error" ? "Error" : "Warning"}
                </span>
                <span className="text-[13px] leading-snug text-paper">{validationMessage(issue)}</span>
                <span className="text-[11px] text-muted">{target}</span>
              </button>
              <details className="text-[10px] text-muted">
                <summary className="min-h-8 py-1.5">Technical detail</summary>
                <code className="block break-all pb-2 font-mono text-faint">{issue.code}</code>
              </details>
            </article>
          );
        })}
      </div>

      <button
        type="button"
        onClick={onCheckAgain}
        className="min-h-10 text-left text-xs text-muted hover:text-paper"
      >
        Check again
      </button>
    </div>
  );
}
