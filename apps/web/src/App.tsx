import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useState } from "react";

import { Agents } from "./Agents";
import { ApiError, getIdentity, listProjects } from "./api";
import { Consent } from "./Consent";
import { Editor } from "./editor/Editor";
import { currentSession } from "./identity";
import { uiHandlers } from "./pontis/handlers";
import { useUiChannel } from "./pontis/useUiChannel";
import { Onboard, SignIn } from "./SignIn";
import { Projects } from "./Projects";
import { useEditor } from "./store";

/**
 * What the app knows about who is using it.
 *
 * `unknown` is a real state and not a placeholder: the answer needs a round trip,
 * and drawing a sign-in screen before it arrives would flash one at a local
 * Forma that never needed one.
 */
type Access =
  | { at: "unknown" }
  | { at: "open" }
  | { at: "anonymous" }
  | { at: "accountless" }
  | { at: "ready" };

function useAccess(): [Access, () => void] {
  const [access, setAccess] = useState<Access>({ at: "unknown" });
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let live = true;
    void (async () => {
      try {
        const identity = await getIdentity();
        if (!identity.required) return live && setAccess({ at: "open" });
        if (!(await currentSession())) return live && setAccess({ at: "anonymous" });

        // A session is not an account. The server is the authority on that and
        // says so with the one 403 it has, so the cheapest authenticated read
        // doubles as the question — rather than a second endpoint that could
        // disagree with the routes it is meant to describe.
        try {
          await listProjects();
          if (live) setAccess({ at: "ready" });
        } catch (cause) {
          if (!live) return;
          if (cause instanceof ApiError && cause.code === "ACCOUNT_REQUIRED") {
            setAccess({ at: "accountless" });
          } else if (cause instanceof ApiError && cause.status === 401) {
            setAccess({ at: "anonymous" });
          } else {
            throw cause;
          }
        }
      } catch {
        // Unreachable server: the workspace view says so better than this can.
        if (live) setAccess({ at: "open" });
      }
    })();
    return () => {
      live = false;
    };
  }, [attempt]);

  return [access, () => setAttempt((n) => n + 1)];
}

export function App() {
  const [designId, setDesignId] = useState<string | null>(null);
  const [access, recheck] = useAccess();
  const queries = useQueryClient();

  // Opening a design is a navigation, so it clears what was selected in the
  // last one — a layer id from another design would otherwise survive the
  // switch. The channel calls this, and so could anything else; it is not an
  // agent-only path.
  const openDesign = useCallback((next: string) => {
    setDesignId((current) => {
      if (current !== next) useEditor.getState().selectPage(null);
      return next;
    });
  }, []);

  // The four editor actions of plan §3.2, answered by this window.
  //
  // Only where there is a sidecar to connect to: `/service/resolve` needs a
  // subject and an account, so a local Forma has neither one nor a session to
  // present, and `access.at === "ready"` is exactly the state that has both.
  const handlers = useMemo(
    () => uiHandlers({ openDesignId: () => designId, openDesign, queries }),
    [designId, openDesign, queries],
  );
  useUiChannel({ enabled: access.at === "ready", handlers });

  // These entry points are part of authentication hand-offs and setup
  // instructions, not an in-app navigation graph. Reading the browser path
  // directly keeps four stable URLs from bringing in a routing dependency.
  switch (window.location.pathname) {
    case "/login":
      return <SignIn />;
    case "/oauth/consent":
      return <Consent />;
    case "/agents":
      return <Agents />;
    case "/":
      break;
    default:
      return <Placeholder title="Page not found" detail="Return to Forma to continue." />;
  }

  // Nothing at all while the question is open. A flash of a sign-in screen on a
  // local Forma that never needed one is worse than a blank moment.
  if (access.at === "unknown") return null;
  if (access.at === "anonymous") return <SignIn />;
  if (access.at === "accountless") return <Onboard onReady={recheck} />;

  return designId === null ? (
    <Projects onOpen={openDesign} />
  ) : (
    <Editor designId={designId} onBack={() => setDesignId(null)} />
  );
}

function Placeholder({ title, detail }: { title: string; detail: string }) {
  return (
    <main className="grid min-h-screen place-items-center px-6">
      <div className="max-w-lg rounded-2xl border border-edge bg-surface p-8">
        <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.12em] text-signal">
          Forma
        </div>
        <h1 className="text-3xl font-medium tracking-tight">{title}</h1>
        <p className="mt-3 text-sm leading-6 text-muted">{detail}</p>
      </div>
    </main>
  );
}
