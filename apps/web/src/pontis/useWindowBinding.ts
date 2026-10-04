/**
 * Tells this window's UI channel which agent session its chat belongs to (D-2).
 *
 * An agent's `ui.*` action goes to the window whose chat started the session; the
 * channel can only claim that on the stream it opens, so the chat reports its
 * session here and the channel reopens the stream when it changes
 * (`useUiChannel`, `window.ts`). The sidecar checks the claim against the Control
 * Plane before it binds anything, so this is a hint and not an authority.
 *
 * Whatever owns the chat's session id calls this with it — `null` when there is
 * none — and nothing else needs to know the channel exists. Cleared on unmount:
 * a chat that is gone no longer speaks for its session.
 */
import { useEffect } from "react";

import { agentBinding } from "./window";

export function useWindowBinding(sessionId: string | null): void {
  useEffect(() => {
    agentBinding.set(sessionId);
    return () => agentBinding.set(null);
  }, [sessionId]);
}
