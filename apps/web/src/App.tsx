import { useState } from "react";

import { Editor } from "./editor/Editor";
import { Projects } from "./Projects";

export function App() {
  const [designId, setDesignId] = useState<string | null>(null);

  // These entry points are part of authentication hand-offs and setup
  // instructions, not an in-app navigation graph. Reading the browser path
  // directly keeps four stable URLs from bringing in a routing dependency.
  switch (window.location.pathname) {
    case "/login":
      return (
        <Placeholder
          title="Sign in to Forma"
          detail="This page will start the hosted identity provider's sign-in flow."
        />
      );
    case "/oauth/consent":
      return (
        <Placeholder
          title="Authorize an agent"
          detail="This page will show the requesting agent and ask you to allow or deny access."
        />
      );
    case "/agents":
      return (
        <Placeholder
          title="Agent connections"
          detail="Connect agents and manage their personal access tokens here."
        />
      );
    case "/":
      break;
    default:
      return <Placeholder title="Page not found" detail="Return to Forma to continue." />;
  }

  return designId === null ? (
    <Projects onOpen={setDesignId} />
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
