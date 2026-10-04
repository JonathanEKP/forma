/**
 * Holding the channel open for as long as the editor is on screen.
 *
 * Mounted once, high up: a UI action is addressed to *this window*, not to a
 * particular panel, and the registry replaces a second connection from the same
 * browser context rather than accumulating — so a component that remounted
 * would churn the registration for no reason.
 *
 * The handlers are read through a ref so the channel is opened once per session
 * rather than per render. They close over `App`'s state setter, which changes
 * identity on every render; reconnecting the stream each time would have
 * deregistered and re-registered the client continuously, and an action arriving
 * in one of those gaps answers `ui_action_not_registered`.
 */
import { useEffect, useRef } from "react";

import { accessToken, currentSubject, onSessionChange } from "../identity";
import { liveFeed, openChannel, type ActionHandler, type ChannelState } from "./channel";
import { agentBinding } from "./window";

export interface UiChannelOptions {
  /** `false` where there is nothing to connect to: local mode has no sidecar. */
  readonly enabled: boolean;
  readonly handlers: Record<string, ActionHandler>;
  readonly onState?: (state: ChannelState) => void;
}

export function useUiChannel({ enabled, handlers, onState }: UiChannelOptions): void {
  const current = useRef(handlers);
  current.current = handlers;
  const report = useRef(onState);
  report.current = onState;

  // The handler ids are what gets registered, so a change in *which* actions
  // exist does have to reopen the channel. Their implementations do not.
  const registered = Object.keys(handlers).sort().join(",");

  useEffect(() => {
    if (!enabled) return;
    const stable: Record<string, ActionHandler> = {};
    for (const id of registered.split(",")) {
      if (id.length > 0) stable[id] = (input) => current.current[id]?.(input) ?? { outcome: "rejected", detail: `${id} is no longer registered` };
    }
    const channel = openChannel({
      handlers: stable,
      token: accessToken,
      subject: currentSubject,
      onState: (state) => {
        liveFeed.setState(state);
        report.current?.(state);
      },
      onEvent: (frame) => liveFeed.publish(frame),
    });

    // The chat started or re-attached to a session: say so on a fresh stream, so
    // an agent's actions go to this window and not to whichever is ambiguous.
    const unbind = agentBinding.subscribe(() => channel.refresh());

    // The stored session may now be somebody else's — another tab signed in as
    // them. The provider announces that to its own listeners, and a plain
    // `storage` event covers a session written some other way; either way the
    // channel compares who is signed in with who the stream was opened for.
    // Deferred a turn: the provider's callback runs inside its own lock, and
    // asking it for the session from there would wait on itself.
    const recheck = () => void setTimeout(() => void channel.recheck(), 0);
    const stopListening = onSessionChange(recheck);
    window.addEventListener("storage", recheck);

    return () => {
      unbind();
      stopListening();
      window.removeEventListener("storage", recheck);
      channel.close();
      liveFeed.setState({ at: "offline" });
    };
  }, [enabled, registered]);
}
