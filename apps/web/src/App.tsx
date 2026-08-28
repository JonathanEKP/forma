import { useEffect, useState } from "react";

import { ApiError, getStatus, type Status } from "./api";

type Load =
  | { state: "loading" }
  | { state: "ready"; status: Status }
  | { state: "failed"; code: string; message: string };

export function App() {
  const [load, setLoad] = useState<Load>({ state: "loading" });

  useEffect(() => {
    let cancelled = false;
    getStatus()
      .then((status) => {
        if (!cancelled) setLoad({ state: "ready", status });
      })
      .catch((error: unknown) => {
        if (cancelled) return;
        setLoad(
          error instanceof ApiError
            ? { state: "failed", code: error.code, message: error.message }
            : {
                state: "failed",
                code: "SERVER_UNREACHABLE",
                message: "forma-server is not running",
              },
        );
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <main>
      <h1>Forma</h1>
      <p>Local design runtime for agents.</p>

      {load.state === "loading" && <p>Checking forma-server…</p>}

      {load.state === "failed" && (
        <p role="alert">
          {load.code}: {load.message}
        </p>
      )}

      {load.state === "ready" && (
        <dl>
          <dt>server</dt>
          <dd>
            {load.status.name} {load.status.version}
          </dd>
          <dt>document schema</dt>
          <dd>v{load.status.schema_version}</dd>
          <dt>fonts</dt>
          <dd>{load.status.fonts.join(", ") || "none"}</dd>
          <dt>projects</dt>
          <dd>{load.status.projects}</dd>
        </dl>
      )}
    </main>
  );
}
