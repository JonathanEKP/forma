import { useState, type FormEvent } from "react";

import { useApiKeys, useCreateApiKey, useRevokeApiKey } from "./queries";

export function Agents() {
  const keys = useApiKeys();
  const create = useCreateApiKey();
  const revoke = useRevokeApiKey();
  const [name, setName] = useState("");
  const mcpUrl = `${window.location.origin}/mcp`;

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const trimmed = name.trim();
    if (trimmed === "") return;

    create.mutate(trimmed, {
      onSuccess: () => setName(""),
    });
  };

  return (
    <main className="mx-auto w-full max-w-3xl px-6 py-12">
      <header className="mb-10">
        <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.12em] text-signal">
          Forma
        </div>
        <h1 className="text-4xl font-medium tracking-tight">Agent connections</h1>
        <p className="mt-3 max-w-2xl text-sm leading-6 text-muted">
          Connect an OAuth-capable agent with Forma&apos;s MCP URL. Use a personal
          access token only for command-line and automation clients that need a
          bearer credential.
        </p>
      </header>

      <section className="mb-10 rounded-2xl border border-edge bg-surface p-5">
        <h2 className="text-lg font-medium">OAuth connections</h2>
        <p className="mt-2 text-sm leading-6 text-muted">
          Provider authorization and connected-application management will appear
          here when hosted identity is configured.
        </p>
        <div className="mt-4 rounded-lg bg-surface-2 px-3 py-2 font-mono text-xs text-paper">
          {mcpUrl}
        </div>
      </section>

      <section className="rounded-2xl border border-edge bg-surface p-5">
        <h2 className="text-lg font-medium">Personal access tokens</h2>
        <p className="mt-2 text-sm leading-6 text-muted">
          Name each key after the client or machine that will hold it. Revoking one
          key does not disconnect any other client.
        </p>

        <form className="mt-5 flex gap-2" onSubmit={submit}>
          <input
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="Key name"
            aria-label="Key name"
            className="min-w-0 flex-1 rounded-lg border border-edge bg-surface-2 px-3 py-2 text-sm
                       text-paper outline-none placeholder:text-faint focus:border-signal/60"
          />
          <button
            type="submit"
            disabled={create.isPending || name.trim() === ""}
            className="rounded-lg bg-signal px-4 py-2 text-sm font-medium text-ink
                       disabled:cursor-not-allowed disabled:opacity-45"
          >
            {create.isPending ? "Creating…" : "Create API key"}
          </button>
        </form>

        {create.isError && (
          <p className="mt-3 text-sm text-danger">{create.error.message}</p>
        )}

        {create.data && (
          <div
            role="status"
            className="mt-4 rounded-xl border border-signal/40 bg-signal/8 p-4"
          >
            <p className="text-sm font-medium text-paper">Copy this token now.</p>
            <p className="mt-1 text-xs leading-5 text-muted">
              Forma stores only its hash. This complete value will not appear in
              the list again.
            </p>
            <code className="mt-3 block overflow-x-auto rounded-lg bg-ink px-3 py-2 text-xs text-paper">
              {create.data.token}
            </code>
          </div>
        )}

        <div className="mt-6 space-y-2">
          {keys.isLoading && <p className="text-sm text-muted">Loading keys…</p>}
          {keys.isError && <p className="text-sm text-danger">{keys.error.message}</p>}
          {keys.data?.length === 0 && (
            <p className="text-sm text-muted">No personal access tokens yet.</p>
          )}
          {keys.data?.map((key) => (
            <div
              key={key.id}
              className="flex items-center justify-between gap-4 rounded-xl border border-edge
                         bg-surface-2 px-4 py-3"
            >
              <div className="min-w-0">
                <p className="truncate text-sm text-paper">{key.name}</p>
                <p className="mt-1 font-mono text-[11px] text-faint">
                  {key.token_prefix}… · created {key.created_at.slice(0, 10)}
                  {key.last_used_at ? ` · used ${key.last_used_at.slice(0, 10)}` : ""}
                </p>
              </div>
              {key.revoked_at ? (
                <span className="text-xs text-faint">Revoked</span>
              ) : (
                <button
                  type="button"
                  disabled={revoke.isPending}
                  onClick={() => revoke.mutate(key.id)}
                  className="rounded-lg border border-edge px-3 py-1.5 text-xs text-muted
                             hover:border-danger/50 hover:text-danger disabled:opacity-45"
                >
                  Revoke
                </button>
              )}
            </div>
          ))}
        </div>

        <div className="mt-6 border-t border-edge pt-5">
          <p className="text-xs font-medium text-paper">Manual setup</p>
          <p className="mt-2 font-mono text-[11px] leading-5 text-muted">
            MCP URL: {mcpUrl}
            <br />
            Header: Authorization: Bearer {"${FORMA_MCP_TOKEN}"}
          </p>
        </div>
      </section>
    </main>
  );
}
