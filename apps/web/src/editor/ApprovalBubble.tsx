import type { ApprovalCardView } from "../pontis/ChatApproval";
import { ApprovalCountdown } from "../pontis/ApprovalCountdown";

export function approvalWindowTitle(designName: string, pendingCount: number, chatOpen: boolean): string {
  return pendingCount > 0 && !chatOpen
    ? `(${pendingCount}) Approval needed · Forma`
    : `${designName} · Forma`;
}

export function ApprovalBubble({
  approval,
  answering,
  deviceName,
  onReview,
}: {
  approval: ApprovalCardView;
  answering: boolean;
  deviceName?: string;
  onReview: () => void;
}) {
  return (
    <section className="editor-approval-bubble" aria-label="Pending approval" role="status">
      <div className="editor-approval-bubble-head">
        <strong>Needs your approval</strong>
        {!answering && approval.expiresAt && <ApprovalCountdown expiresAt={approval.expiresAt} />}
      </div>
      <p>{approval.action}</p>
      {answering && <p className="editor-approval-bubble-sending">Sending your answer… Waiting for confirmation from {deviceName ?? "your device"}.</p>}
      <button type="button" onClick={onReview}>Review</button>
    </section>
  );
}
