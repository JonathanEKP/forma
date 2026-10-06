/**
 * The validation panel (plan sections 41 and 42).
 *
 * The report comes from the server, measured against real glyph bounds. The
 * editor never decides whether something overflows — it only shows what the
 * render authority found, and severity is what decides whether an export is
 * blocked.
 */

import type { ValidationReport } from "../api";
import { useEditor } from "../store";

export function Validation({
  report,
  loading,
  onSelect,
}: {
  report: ValidationReport | undefined;
  loading: boolean;
  onSelect?: () => void;
}) {
  const selectLayer = useEditor((s) => s.selectLayer);
  const selectPage = useEditor((s) => s.selectPage);

  if (loading || !report) {
    return <p className="px-1 py-2 text-xs text-faint">Checking…</p>;
  }

  const errors = report.issues.filter((i) => i.severity === "error");
  const warnings = report.issues.filter((i) => i.severity === "warning");

  return (
    <div>
      <div
        className={`mb-3 rounded-lg border px-2.5 py-2 text-xs ${
          errors.length > 0
            ? "border-signal/45 bg-signal/6 text-paper"
            : "border-edge bg-surface-2 text-muted"
        }`}
      >
        {errors.length === 0
          ? "No errors. This design can be exported."
          : `${errors.length} error${errors.length === 1 ? "" : "s"} block export.`}
        {warnings.length > 0 && (
          <span className="block mt-1 text-faint">
            {warnings.length} warning{warnings.length === 1 ? "" : "s"}.
          </span>
        )}
      </div>

      {/* Checks never short-circuit on the server, so the whole list is here:
          an agent or a human fixing a design wants all of it at once. */}
      {report.issues.map((issue, index) => (
        <button
          key={`${issue.code}:${index}`}
          type="button"
          className="mb-1.5 w-full rounded-lg border border-edge bg-surface-2 px-2.5 py-2
                     text-left transition-colors hover:border-edge-strong"
          onClick={() => {
            if (issue.page_id) selectPage(issue.page_id);
            if (issue.layer_id) selectLayer(issue.layer_id);
            onSelect?.();
          }}
          disabled={!issue.layer_id && !issue.page_id}
        >
          <div className="flex items-center gap-2">
            <span
              className={`h-[7px] w-[7px] shrink-0 rounded-full ${
                issue.severity === "error" ? "bg-signal" : "bg-graphite"
              }`}
            />
            <code className="text-[10px] text-muted">{issue.code}</code>
          </div>
          <p className="mt-1 text-[11px] leading-snug text-muted">{issue.message}</p>
        </button>
      ))}
    </div>
  );
}
