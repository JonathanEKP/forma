/** The approval card, including its editor-only canvas cue. */
import { useEffect, useState } from "react";

import type { CapabilityRisk } from "@forma/bindings/CapabilityRisk";
import { useDesign } from "../queries";
import { useEditor } from "../store";
import type { ApprovalStatus, ApprovalView } from "./chat";
import { approvalOperationSource, buildApprovalDetails, type ApprovalChip, type ApprovalDetails } from "./approval-details";
import { ApprovalCountdown, remainingApprovalSeconds } from "./ApprovalCountdown";

const SETTLED_LABEL: Record<Exclude<ApprovalStatus, "pending">, string> = {
  approved: "Approved",
  denied: "Denied",
  expired: "Expired before you answered",
  cancelled: "Cancelled",
};

const SETTLED_HINT = "Ask the agent again if you change your mind.";

function riskLabel(risk: string): string {
  const labels: Partial<Record<CapabilityRisk, string>> = {
    read: "Read",
    write: "Write",
    destructive: "Destructive",
    external_side_effect: "External side effect",
    sensitive: "Sensitive",
  };
  return labels[risk as CapabilityRisk] ?? risk;
}

function firstQuotedName(action: string): string | null {
  return /“([^”]+)”/u.exec(action)?.[1] ?? null;
}

function pageNames(details: ApprovalDetails, recordNameById: ReadonlyMap<string, string>): string[] {
  return details.pageIds.flatMap((id) => {
    const name = recordNameById.get(id);
    return name ? [name] : [];
  });
}

function changedDescription(approval: ApprovalView, details: ApprovalDetails, record: ReturnType<typeof useDesign>["data"]): string {
  const names = new Map(record?.document.pages.map((page) => [page.id, page.name]) ?? []);
  const pageLabels = pageNames(details, names);

  if (details.pageCount === 1 && pageLabels.length > 0) {
    return `Deleted page “${pageLabels[0]}”${details.layerCount > 0 ? ` and its ${details.layerCount} ${details.layerCount === 1 ? "layer" : "layers"}` : ""}`;
  }
  if (details.pageCount > 1 && pageLabels.length > 0) {
    return `Deleted ${details.pageCount} pages${details.layerCount > 0 ? ` and ${details.layerCount} ${details.layerCount === 1 ? "layer" : "layers"}` : ""}`;
  }
  if (details.layerCount === 1) {
    const layerChip = details.chips.find((chip) => chip.kind === "layer");
    const name = layerChip?.label ?? firstQuotedName(approval.action);
    return name ? `Deleted the layer “${name}”` : "Deleted the requested layer";
  }
  if (details.layerCount > 1) {
    return pageLabels.length > 0
      ? `Deleted ${details.layerCount} layers across ${pageLabels.join(" and ")}`
      : `Deleted ${details.layerCount} layers`;
  }
  if (details.pageCount === 0 && /layer/iu.test(approval.action)) {
    const name = firstQuotedName(approval.action);
    return name ? `Deleted the layer “${name}”` : "Deleted the requested layer";
  }
  return details.pageCount > 0 ? `Deleted ${details.pageCount} pages` : "Completed the approved change";
}

function keptDescription(approval: ApprovalView, details: ApprovalDetails, record: ReturnType<typeof useDesign>["data"]): string {
  const names = new Map(record?.document.pages.map((page) => [page.id, page.name]) ?? []);
  const pageLabels = pageNames(details, names);
  if (details.pageCount === 1 && pageLabels.length > 0) {
    return `Page “${pageLabels[0]}”${details.layerCount > 0 ? ` and its ${details.layerCount} ${details.layerCount === 1 ? "layer" : "layers"}` : ""} were kept`;
  }
  if (details.layerCount === 1) {
    const layerChip = details.chips.find((chip) => chip.kind === "layer");
    const name = layerChip?.label ?? firstQuotedName(approval.action);
    return name ? `“${name}” was kept` : "The requested layer was kept";
  }
  if (details.layerCount > 1) {
    return pageLabels.length > 0
      ? `All ${details.layerCount} layers across ${pageLabels.join(" and ")} were kept`
      : `All ${details.layerCount} layers were kept`;
  }
  const name = firstQuotedName(approval.action);
  if (details.pageCount === 0 && /layer/iu.test(approval.action) && name) return `“${name}” was kept`;
  return "No changes were made";
}

function PageIcon() {
  return (
    <svg aria-hidden="true" viewBox="0 0 16 16" className="h-3 w-3 shrink-0 fill-none stroke-current">
      <path d="M4 2.5h5l3 3v8H4zM9 2.5v3h3" strokeWidth="1.2" />
    </svg>
  );
}

function Chip({ chip }: { chip: ApprovalChip }) {
  return (
    <span className="editor-approval-chip">
      {chip.kind === "page" && <PageIcon />}
      <span>{chip.label}</span>
      {chip.showPageSuffix && <small>{chip.pageName}</small>}
    </span>
  );
}

export type ApprovalCardView = ApprovalView;

export function ApprovalCard({
  approval,
  status,
  answering,
  unconfirmed,
  onDecide,
  deviceName,
}: {
  approval: ApprovalCardView;
  status: ApprovalStatus;
  /** An answer was sent and the Runtime has not confirmed it yet. */
  answering: boolean;
  /** The approval stayed pending after the confirmation window elapsed. */
  unconfirmed: boolean;
  onDecide: (decision: "approveOnce" | "deny") => void;
  deviceName?: string;
}) {
  const source = approvalOperationSource(approval);
  const { data: record, isLoading } = useDesign(source.designId);
  const openDesignId = useEditor((state) => state.openDesignId);
  const approvalHighlight = useEditor((state) => state.approvalHighlight);
  const setApprovalHighlight = useEditor((state) => state.setApprovalHighlight);
  const selectPage = useEditor((state) => state.selectPage);
  const selectLayer = useEditor((state) => state.selectLayer);
  const [showAllChips, setShowAllChips] = useState(false);
  const [now, setNow] = useState(() => Date.now());

  const details = buildApprovalDetails(source.operations, record ?? null, source.reason);
  const nameNotice = source.reason ?? (isLoading ? null : details.nameNotice);
  const isHighlighted = approvalHighlight?.approvalId === approval.approvalId && approvalHighlight.designId === source.designId;
  const canvasAvailable = status === "pending" && source.designId !== null && source.designId === openDesignId && record !== undefined && details.layerCount > 0;
  const remaining = approval.expiresAt ? remainingApprovalSeconds(approval.expiresAt, now) : null;
  const expiredLocally = remaining === 0;
  const busy = answering;
  const canAnswer = status === "pending" && !busy && !expiredLocally;
  const actionHelp = expiredLocally
    ? "This approval expired. No changes were made."
    : "This answer applies to this request only.";

  useEffect(() => {
    if (status !== "pending" && approvalHighlight?.approvalId === approval.approvalId) setApprovalHighlight(null);
  }, [approval.approvalId, approvalHighlight?.approvalId, setApprovalHighlight, status]);

  useEffect(() => {
    if (status !== "pending" || answering) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 1_000);
    return () => window.clearInterval(timer);
  }, [answering, status]);

  const toggleHighlight = () => {
    if (!source.designId || !record) return;
    if (isHighlighted) {
      setApprovalHighlight(null);
      return;
    }
    selectLayer(null);
    const firstPageId = details.pageIds[0];
    if (firstPageId) selectPage(firstPageId);
    setApprovalHighlight({
      approvalId: approval.approvalId,
      designId: source.designId,
      pageIds: details.pageIds,
      layerIdsByPage: details.layerIdsByPage,
    });
  };

  const showChip = showAllChips ? details.allChips : details.chips;
  const settledText = status === "approved"
    ? `${SETTLED_LABEL[status]}. ${changedDescription(approval, details, record)}.`
    : status === "denied" || status === "expired" || status === "cancelled"
      ? `${SETTLED_LABEL[status]}. ${keptDescription(approval, details, record)}.`
      : null;

  return (
    <article className="editor-approval-card" aria-label="Approval request">
      {status === "pending" ? (
        <>
          <header className="editor-approval-heading">
            <span className="editor-approval-kicker">NEEDS YOUR APPROVAL</span>
            {approval.risk === "destructive" && <span className="editor-approval-risk">DESTRUCTIVE</span>}
            {approval.risk !== "destructive" && <span className="editor-approval-risk editor-approval-risk--quiet">{riskLabel(approval.risk).toUpperCase()}</span>}
            <span className="editor-approval-deadline">
              {!busy && approval.expiresAt && <ApprovalCountdown expiresAt={approval.expiresAt} now={now} />}
            </span>
          </header>

          <h3 className="editor-approval-action">{approval.action}</h3>
          <p className="editor-approval-count">{details.countLabel}</p>
          {nameNotice && <p className="editor-approval-name-notice">{nameNotice}</p>}
          {isLoading && source.operations.length > 0 && <p className="editor-approval-name-notice">Loading affected layer names…</p>}
          {details.editSummary && <p className="editor-approval-edits">{details.editSummary}</p>}

          {details.chips.length > 0 && (
            <div className="editor-approval-chips" aria-label="Objects that will be deleted">
              {showChip.map((chip) => <Chip key={`${chip.kind}:${chip.id}`} chip={chip} />)}
              {details.moreCount > 0 && !showAllChips && (
                <button type="button" className="editor-approval-more" onClick={() => setShowAllChips(true)}>
                  +{details.moreCount} more
                </button>
              )}
            </div>
          )}

          {canvasAvailable && (
            <button type="button" aria-pressed={isHighlighted} onClick={toggleHighlight} className="editor-approval-canvas-toggle">
              <span aria-hidden="true">◎</span>
              Show on canvas
            </button>
          )}

          <details className="editor-approval-details">
            <summary>Details</summary>
            <dl>
              <div><dt>design_id</dt><dd>{source.designId ?? "Not provided"}</dd></div>
              {details.rawIds.map((id, index) => (
                <div key={`${id}:${index}`}><dt>{id === source.designId ? "design_id" : "affected id"}</dt><dd>{id}</dd></div>
              ))}
              <div><dt>base_version</dt><dd>{source.baseVersion ?? "Not provided"}</dd></div>
              <div className="editor-approval-raw"><dt>raw operations</dt><dd><pre>{JSON.stringify(source.operations, null, 2)}</pre></dd></div>
            </dl>
          </details>

          {busy ? (
            <p className="editor-approval-sending" role="status">
              Sending your answer… Waiting for confirmation from {deviceName ?? "your device"}.
            </p>
          ) : (
            <>
              {unconfirmed && <p className="editor-approval-unconfirmed" role="status">Not confirmed by your device</p>}
              <div className="editor-approval-actions">
                <button type="button" disabled={!canAnswer} aria-describedby="approval-answer-help" onClick={() => onDecide("approveOnce")} className="editor-approval-approve">
                  Approve once
                </button>
                <button type="button" disabled={!canAnswer} aria-describedby="approval-answer-help" onClick={() => onDecide("deny")} className="editor-approval-deny">
                  Deny
                </button>
              </div>
              <p id="approval-answer-help" className="editor-approval-help">{actionHelp}</p>
            </>
          )}
        </>
      ) : (
        <>
          <p className="editor-approval-settled">{settledText}</p>
          {status !== "approved" && <p className="editor-approval-help">{SETTLED_HINT}</p>}
        </>
      )}
    </article>
  );
}
