/**
 * The approval card (`pontis/docs/13` § Approval panel).
 *
 * Capability, the action in words, the parameters worth reading, the risk class
 * and how many things it touches — in front of the two buttons, so the person
 * answers what is being asked rather than "Allow tool?". `approveOnce` and `deny`
 * are the only answers: a session-wide grant is not offered in PV-1 (plan §3.1),
 * and the sidecar refuses any other decision anyway.
 */
import { RISK_LABEL, type ApprovalStatus, type ApprovalView } from "./chat";
import type { CapabilityRisk } from "@forma/bindings/CapabilityRisk";

const SETTLED: Record<Exclude<ApprovalStatus, "pending">, string> = {
  approved: "Approved",
  denied: "Denied",
  expired: "Expired before you answered",
  cancelled: "Cancelled",
};

function riskLabel(risk: string): string {
  return risk in RISK_LABEL ? RISK_LABEL[risk as CapabilityRisk] : risk;
}

export function ApprovalCard({
  approval,
  status,
  answering,
  unconfirmed,
  onDecide,
}: {
  approval: ApprovalView;
  status: ApprovalStatus;
  /** An answer was sent and the Runtime has not confirmed it yet. */
  answering: boolean;
  /** The approval stayed pending after the confirmation window elapsed. */
  unconfirmed: boolean;
  onDecide: (decision: "approveOnce" | "deny") => void;
}) {
  const heavy = approval.risk === "destructive" || approval.risk === "external_side_effect" || approval.risk === "sensitive";

  return (
    <div className="rounded-lg border border-edge-strong bg-surface-2 p-3 text-xs">
      <div className="mb-1 flex items-center gap-2">
        <span className="text-[10px] font-semibold uppercase tracking-[0.1em] text-muted">Needs your approval</span>
        <span
          className={`rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide ${
            heavy ? "bg-paper text-ink" : "border border-edge-strong text-muted"
          }`}
        >
          {riskLabel(approval.risk)}
        </span>
      </div>

      <p className="text-[13px] leading-5">{approval.action}</p>
      <code className="mt-1 block text-[11px] text-faint">{approval.capabilityId}</code>

      {approval.parameters.length > 0 && (
        <dl className="mt-2 space-y-1 border-t border-edge pt-2">
          {approval.parameters.map((parameter) => (
            <div key={parameter.name} className="grid grid-cols-[88px_1fr] gap-2">
              <dt className="truncate font-mono text-[11px] text-faint">{parameter.name}</dt>
              <dd className="break-words text-muted">{parameter.value}</dd>
            </div>
          ))}
        </dl>
      )}

      <p className="mt-2 text-[11px] text-muted">
        {approval.affected === null
          ? "How many things this changes isn't stated."
          : `Affects ${approval.affected} ${approval.affected === 1 ? "item" : "items"}.`}
      </p>

      {status === "pending" ? (
        <div className="mt-3 flex gap-2">
          <button
            type="button"
            disabled={answering}
            onClick={() => onDecide("approveOnce")}
            className="rounded-lg bg-signal px-2.5 py-1.5 text-xs font-semibold text-ink disabled:opacity-40"
          >
            Approve once
          </button>
          <button
            type="button"
            disabled={answering}
            onClick={() => onDecide("deny")}
            className="rounded-lg border border-edge bg-surface px-2.5 py-1.5 text-xs text-muted enabled:hover:text-paper disabled:opacity-40"
          >
            Deny
          </button>
          {answering && <span className="self-center text-[11px] text-faint">Sending…</span>}
          {!answering && unconfirmed && <span className="self-center text-[11px] text-faint">Not confirmed by your device</span>}
        </div>
      ) : (
        <p className="mt-3 text-[11px] font-semibold text-muted">{SETTLED[status]}</p>
      )}
    </div>
  );
}
