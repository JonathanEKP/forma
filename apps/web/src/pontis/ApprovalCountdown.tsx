import { useEffect, useState } from "react";

export function remainingApprovalSeconds(expiresAt: string, now: number): number | null {
  const expires = Date.parse(expiresAt);
  if (!Number.isFinite(expires)) return null;
  return Math.max(0, Math.ceil((expires - now) / 1_000));
}

function formatRemaining(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  const rest = String(seconds % 60).padStart(2, "0");
  return `${minutes}:${rest}`;
}

export function ApprovalCountdown({
  expiresAt,
  className = "",
  now: suppliedNow,
}: {
  expiresAt: string;
  className?: string;
  now?: number;
}) {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    if (suppliedNow !== undefined) return;
    const timer = window.setInterval(() => setNow(Date.now()), 1_000);
    return () => window.clearInterval(timer);
  }, [suppliedNow]);

  const remaining = remainingApprovalSeconds(expiresAt, suppliedNow ?? now);
  if (remaining === null) return null;

  const urgent = remaining < 30;
  return (
    <span
      className={`${className} ${urgent ? "editor-approval-countdown--urgent" : ""}`.trim()}
      data-urgency={urgent ? "urgent" : "normal"}
      aria-label={`Approval expires in ${formatRemaining(remaining)}`}
    >
      Expires in {formatRemaining(remaining)}
    </span>
  );
}
