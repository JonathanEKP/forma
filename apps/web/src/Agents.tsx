import { useEffect, useRef, useState, type FormEvent, type KeyboardEvent as ReactKeyboardEvent, type MouseEvent as ReactMouseEvent, type ReactNode, type RefObject } from "react";

import { getIdentity, listProjects } from "./api";
import { currentSession, listGrants, revokeGrant, supabase, type Grant } from "./identity";
import { MCP_HOST_GROUPS, MCP_HOSTS, MCP_HOSTS_IN_TAB_ORDER, MCP_OTHER_HOST, type McpCodeSample, type McpHost, type McpSetupStep } from "./mcp-hosts";
import { McpHostIcon } from "./mcp-host-icons";
import { DevicePanel } from "./pontis/devices-panel";
import { useApiKeys, useCreateApiKey, useRevokeApiKey } from "./queries";

export function Agents() {
  return window.location.pathname === "/agents/mcp" ? <McpConnections /> : <ChatConnections />;
}

function ChatConnections() {
  const pontis = usePontisEnabled();

  return (
    <main className="mx-auto w-full max-w-4xl px-6 py-12">
      <ConnectionHeader title="Chat in the editor" page="chat" />
      <div className="mb-8 max-w-2xl space-y-2 text-sm leading-6">
        <p className="text-muted">
          Pair a computer to chat with an agent inside the editor. The agent runs on your computer, with your own subscription.
        </p>
        <p className="text-muted">
          Prefer to use Forma from your AI app? <a href="/agents/mcp" className="text-paper underline underline-offset-4">Set up MCP →</a>
        </p>
      </div>
      {pontis && <DevicePanel />}
    </main>
  );
}

function McpConnections() {
  const keys = useApiKeys();
  const create = useCreateApiKey();
  const revoke = useRevokeApiKey();
  const [name, setName] = useState("");
  const [showCreatedToken, setShowCreatedToken] = useState(false);
  const [selectedHostId, setSelectedHostId] = useState(MCP_HOSTS[0].id);
  const [tokenOpen, setTokenOpen] = useState(false);
  const [tokenFocusRequest, setTokenFocusRequest] = useState(0);
  const tokenDetailsRef = useRef<HTMLDetailsElement>(null);
  const tokenSummaryRef = useRef<HTMLElement>(null);
  const mcpUrl = `${window.location.origin}/mcp`;
  const grants = useGrants();
  const selectedHost = MCP_HOSTS_IN_TAB_ORDER.find((host) => host.id === selectedHostId) ?? MCP_HOSTS[0];

  useEffect(() => {
    if (tokenFocusRequest === 0) return;
    tokenDetailsRef.current?.scrollIntoView?.({ block: "start" });
    tokenSummaryRef.current?.focus({ preventScroll: true });
  }, [tokenFocusRequest, selectedHostId]);

  function followSelectedTokenSteps(event: ReactMouseEvent<HTMLAnchorElement>) {
    event.preventDefault();
    if (!selectedHost.token) return;
    setTokenOpen(true);
    setTokenFocusRequest((request) => request + 1);
  }

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const trimmed = name.trim();
    if (trimmed === "") return;

    create.mutate(trimmed, {
      onSuccess: () => {
        setName("");
        setShowCreatedToken(true);
      },
    });
  };

  return (
    <main className="mcp-page mx-auto w-full">
      <ConnectionHeader title="Use Forma from your AI app" page="mcp" />

      <div className="mcp-intro mb-8 space-y-2 text-sm leading-6">
        <p className="text-paper">MCP is a standard way for an AI app to use tools in another app.</p>
        <p className="text-muted">Your AI app can create and edit designs in your Forma workspace, and you see them here.</p>
        <p className="text-muted">
          Want to chat with an agent in the editor? <a href="/agents" className="text-paper underline underline-offset-4">Pair a computer →</a>
        </p>
      </div>

      <section className="mcp-section" aria-labelledby="mcp-url-heading">
        <div className="mcp-section-heading">
          <h2 id="mcp-url-heading">Your MCP URL</h2>
          <p>Use this URL in each AI app you connect.</p>
        </div>
        <div className="mcp-section-content">
          <CopyableUrl url={mcpUrl} />
        </div>
      </section>

      <section className="mcp-section" aria-labelledby="pick-app-heading">
        <div className="mcp-section-heading">
          <h2 id="pick-app-heading">Pick your app</h2>
          <p>Choose an app to see its connection steps.</p>
        </div>
        <div className="mcp-section-content">
          <McpHostPicker
            mcpUrl={mcpUrl}
            selectedHostId={selectedHost.id}
            onSelectHost={setSelectedHostId}
            tokenOpen={tokenOpen}
            onTokenOpenChange={setTokenOpen}
            tokenDetailsRef={tokenDetailsRef}
            tokenSummaryRef={tokenSummaryRef}
          />
        </div>
      </section>

      {grants.available && (
        <section className="mcp-section" aria-labelledby="connected-apps-heading">
          <div className="mcp-section-heading">
            <h2 id="connected-apps-heading">Connected applications</h2>
            <p>Apps you authorize can use this workspace until you disconnect them.</p>
          </div>
          <div className="mcp-section-content">
            {grants.error && <p role="alert" className="mb-3 text-sm text-danger">{grants.error}</p>}
            {grants.list?.length === 0 && <p className="text-sm text-muted">No applications connected yet.</p>}
            <div className="space-y-2">
              {grants.list?.map((grant) => (
                <article
                  key={grant.client.id}
                  aria-label={grant.client.name + ", connected " + dateLabel(grant.granted_at)}
                  className="rounded-lg border border-edge bg-surface-2 px-4 py-3"
                >
                  <div className="flex items-start justify-between gap-4">
                    <div className="min-w-0">
                      <p className="truncate text-sm text-paper">{grant.client.name}</p>
                      <p className="mt-1 text-xs text-faint">connected {dateLabel(grant.granted_at)}</p>
                    </div>
                    <InlineConfirmAction
                      action="Disconnect"
                      subject={grant.client.name}
                      specific={grant.client.name + ", connected " + dateLabel(grant.granted_at)}
                      detail="This app will lose access to your Forma workspace. You can authorize it again later."
                      pending={grants.pending === grant.client.id}
                      onConfirm={() => void grants.disconnect(grant.client.id)}
                    />
                  </div>
                </article>
              ))}
            </div>
          </div>
        </section>
      )}

      <section id="tokens" className="mcp-section" aria-labelledby="tokens-heading">
        <div className="mcp-section-heading">
          <h2 id="tokens-heading">Personal access tokens</h2>
          <p>For apps that need a token instead of browser sign-in.</p>
        </div>
        <div className="mcp-section-content">
          <McpTokenGuidance host={selectedHost} onFollowTokenSteps={followSelectedTokenSteps} />

          <form id="token-form" className="mcp-token-form" onSubmit={submit}>
            <label htmlFor="mcp-token-name" className="text-xs text-muted">Token name</label>
            <div className="flex flex-wrap gap-2">
              <input
                id="mcp-token-name"
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder="Key name"
                aria-label="Key name"
                className="min-w-0 flex-1 rounded-lg border border-edge bg-surface-2 px-3 py-2 text-sm text-paper outline-none placeholder:text-faint focus:border-signal/60"
              />
              <button
                type="submit"
                disabled={create.isPending || name.trim() === ""}
                className="rounded-lg bg-signal px-4 py-2 text-sm font-medium text-ink disabled:cursor-not-allowed disabled:opacity-45"
              >
                {create.isPending ? "Creating…" : "Create token"}
              </button>
            </div>
          </form>

          {create.isError && <p role="alert" className="mt-3 text-sm text-danger">{create.error.message}</p>}

          {create.data && showCreatedToken && (
            <div role="status" aria-label="New personal access token, shown once" className="mt-4 rounded-lg border border-signal/40 bg-signal/8 p-4">
              <p className="text-sm font-medium text-paper">Copy this token now.</p>
              <p className="mt-1 text-xs leading-5 text-muted">Forma stores only its hash. This complete value will not appear in the list again.</p>
              <div className="mt-3 flex flex-wrap items-start gap-2">
                <code className="min-w-0 flex-1 break-all rounded-lg bg-ink px-3 py-2 font-mono text-xs text-paper">{create.data.token}</code>
                <CopyTextButton value={create.data.token} label="Copy newly created token" />
                <button
                  type="button"
                  aria-label={"Dismiss the full token for " + create.data.key.name + " after saving it"}
                  onClick={() => {
                    setShowCreatedToken(false);
                    create.reset();
                  }}
                  className="rounded-lg border border-edge px-3 py-2 text-xs text-muted hover:text-paper"
                >
                  I’ve saved it
                </button>
              </div>
              <McpTokenFollowUp host={selectedHost} onFollowTokenSteps={followSelectedTokenSteps} />
            </div>
          )}

          <div className="mcp-token-list mt-5 space-y-2">
            {keys.isLoading && <p className="text-sm text-muted">Loading tokens…</p>}
            {keys.isError && <p role="alert" className="text-sm text-danger">{keys.error.message}</p>}
            {revoke.isError && <p role="alert" className="text-sm text-danger">{revoke.error.message}</p>}
            {keys.data?.length === 0 && <p className="text-sm text-muted">No personal access tokens yet.</p>}
            {keys.data?.map((key) => (
              <article key={key.id} aria-label={key.name + ", created " + dateLabel(key.created_at)} className="rounded-lg border border-edge bg-surface-2 px-4 py-3">
                <div className="flex items-start justify-between gap-4">
                  <div className="min-w-0">
                    <p className="truncate text-sm text-paper">{key.name}</p>
                    <p className="mt-1 font-mono text-[11px] text-faint">
                      {key.token_prefix}… · created {dateLabel(key.created_at)}
                      {key.last_used_at ? " · used " + dateLabel(key.last_used_at) : ""}
                    </p>
                  </div>
                  {key.revoked_at ? (
                    <span className="shrink-0 text-xs text-faint">Revoked</span>
                  ) : (
                    <InlineConfirmAction
                      action="Revoke"
                      subject={key.name}
                      specific={key.name + ", created " + dateLabel(key.created_at)}
                      detail="Tools using this token will lose access to your Forma workspace. This cannot be undone; create a new token to reconnect."
                      pending={revoke.isPending}
                      onConfirm={() => revoke.mutate(key.id)}
                    />
                  )}
                </div>
              </article>
            ))}
          </div>
          <p className="mcp-token-safety">If a token leaks, revoke it here and create a new one.</p>
        </div>
      </section>
    </main>
  );
}

function McpHostPicker({
  mcpUrl,
  selectedHostId,
  onSelectHost,
  tokenOpen,
  onTokenOpenChange,
  tokenDetailsRef,
  tokenSummaryRef,
}: {
  mcpUrl: string;
  selectedHostId: string;
  onSelectHost: (hostId: string) => void;
  tokenOpen: boolean;
  onTokenOpenChange: (open: boolean) => void;
  tokenDetailsRef: RefObject<HTMLDetailsElement | null>;
  tokenSummaryRef: RefObject<HTMLElement | null>;
}) {
  const tabRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const selectedIndex = MCP_HOSTS_IN_TAB_ORDER.findIndex((host) => host.id === selectedHostId);
  const selectedHost = MCP_HOSTS_IN_TAB_ORDER[selectedIndex] ?? MCP_HOSTS[0];

  function selectByKeyboard(index: number) {
    const wrappedIndex = (index + MCP_HOSTS_IN_TAB_ORDER.length) % MCP_HOSTS_IN_TAB_ORDER.length;
    const host = MCP_HOSTS_IN_TAB_ORDER[wrappedIndex];
    onSelectHost(host.id);
    tabRefs.current[wrappedIndex]?.focus();
  }

  function onTabKeyDown(event: ReactKeyboardEvent<HTMLButtonElement>, index: number) {
    if (event.key === "ArrowRight" || event.key === "ArrowDown") {
      event.preventDefault();
      selectByKeyboard(index + 1);
    } else if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
      event.preventDefault();
      selectByKeyboard(index - 1);
    } else if (event.key === "Home") {
      event.preventDefault();
      selectByKeyboard(0);
    } else if (event.key === "End") {
      event.preventDefault();
      selectByKeyboard(MCP_HOSTS_IN_TAB_ORDER.length - 1);
    }
  }

  function renderHostTab(host: McpHost) {
    const index = MCP_HOSTS_IN_TAB_ORDER.findIndex((entry) => entry.id === host.id);
    const selected = host.id === selectedHost.id;
    const tabId = `mcp-host-tab-${host.id}`;

    return (
      <button
        key={host.id}
        ref={(element) => { tabRefs.current[index] = element; }}
        type="button"
        role="tab"
        id={tabId}
        aria-selected={selected}
        aria-controls="mcp-host-panel"
        tabIndex={selected ? 0 : -1}
        onClick={() => onSelectHost(host.id)}
        onKeyDown={(event) => onTabKeyDown(event, index)}
        className="mcp-host-tab"
      >
        <span className="mcp-host-tab-content">
          <McpHostIcon name={host.icon} />
          <span className="mcp-host-tab-name">{host.name}</span>
        </span>
      </button>
    );
  }

  return (
    <div className="mcp-host-layout">
      <div role="tablist" aria-labelledby="pick-app-heading" aria-orientation="vertical" className="mcp-host-tabs">
        {MCP_HOST_GROUPS.map((group) => {
          const hosts = MCP_HOSTS.filter((host) => host.group === group.id);
          return (
            <div key={group.id} className="mcp-host-group" role="presentation">
              <h3 className="mcp-host-group-heading">{group.heading}</h3>
              <div className="mcp-host-group-tabs">
                {hosts.map(renderHostTab)}
              </div>
            </div>
          );
        })}
        <div className="mcp-host-other" role="presentation">
          {renderHostTab(MCP_OTHER_HOST)}
        </div>
      </div>
      <article
        id="mcp-host-panel"
        role="tabpanel"
        aria-labelledby={`mcp-host-tab-${selectedHost.id}`}
        tabIndex={0}
        className="mcp-host-panel"
      >
        <header className="mcp-panel-heading">
          <span className="mcp-panel-icon"><McpHostIcon name={selectedHost.icon} /></span>
          <h3>{selectedHost.name}</h3>
        </header>
        {selectedHost.requirement && <p className="mcp-host-requirement">{selectedHost.requirement}</p>}
        <ol className="mcp-steps">
          {selectedHost.steps.map((step, index) => (
            <McpStep key={`${selectedHost.id}-step-${index}`} step={step} index={index} mcpUrl={mcpUrl} />
          ))}
        </ol>
        {selectedHost.officialGuide && (
          <a
            href={selectedHost.officialGuide}
            target="_blank"
            rel="noopener noreferrer"
            aria-label={`Official ${selectedHost.name} setup guide, opens in a new tab`}
            className="mcp-guide"
          >
            Official guide ↗
          </a>
        )}
        <p className="mcp-next-line">
          After connecting, ask your AI app to use Forma. Designs and edits appear in your workspace{selectedHost.id === "chatgpt" ? "; on Pro, you can view designs only" : ""}.
        </p>
        {selectedHost.token && (
          <TokenInstructions
            host={selectedHost}
            mcpUrl={mcpUrl}
            tokenOpen={tokenOpen}
            onTokenOpenChange={onTokenOpenChange}
            detailsRef={tokenDetailsRef}
            summaryRef={tokenSummaryRef}
          />
        )}
      </article>
    </div>
  );
}

function TokenInstructions({
  host,
  mcpUrl,
  tokenOpen,
  onTokenOpenChange,
  detailsRef,
  summaryRef,
}: {
  host: McpHost;
  mcpUrl: string;
  tokenOpen: boolean;
  onTokenOpenChange: (open: boolean) => void;
  detailsRef: RefObject<HTMLDetailsElement | null>;
  summaryRef: RefObject<HTMLElement | null>;
}) {
  const tokenPath = host.token;
  if (!tokenPath) return null;

  return (
    <details
      id="mcp-token-details"
      ref={detailsRef}
      open={tokenOpen}
      onToggle={(event) => onTokenOpenChange(event.currentTarget.open)}
      className="mcp-token-details"
    >
      <summary ref={summaryRef}>
        Use a token instead
      </summary>
      <div className="mcp-token-content">
        <p className="mcp-token-intro">
          Browser sign-in is the main path when your app offers it. If you need a token, <a href="#token-form">create a personal access token below</a>.
        </p>
        {tokenPath.warning && <p className="mcp-token-warning">{tokenPath.warning}</p>}
        <ol className="mcp-steps mcp-token-steps">
          {tokenPath.steps.map((step, index) => (
            <McpStep key={`${host.id}-token-step-${index}`} step={step} index={index} mcpUrl={mcpUrl} />
          ))}
        </ol>
      </div>
    </details>
  );
}

function McpStep({ step, index, mcpUrl }: { step: McpSetupStep; index: number; mcpUrl: string }) {
  return (
    <li className="mcp-step">
      <span className="mcp-step-number" aria-hidden="true">{String(index + 1).padStart(2, "0")}</span>
      <div className="mcp-step-content">
        <p className="mcp-step-copy">{renderMcpStepText(step)}</p>
        {step.code && <CodeSample sample={step.code} mcpUrl={mcpUrl} />}
      </div>
    </li>
  );
}

function renderMcpStepText(step: McpSetupStep) {
  const tokenFormPhrase = "Create a personal access token";
  const matches = [
    ...(step.inlineCode ?? []).map((phrase) => ({ phrase, kind: "code" as const })),
    ...(step.linksToTokenForm ? [{ phrase: tokenFormPhrase, kind: "token-link" as const }] : []),
  ];
  const nodes: ReactNode[] = [];
  let cursor = 0;
  let partIndex = 0;

  while (cursor < step.text.length) {
    let nextMatch: { phrase: string; kind: "code" | "token-link"; index: number } | undefined;
    for (const match of matches) {
      const index = step.text.indexOf(match.phrase, cursor);
      if (index < 0) continue;
      if (!nextMatch || index < nextMatch.index || (index === nextMatch.index && match.phrase.length > nextMatch.phrase.length)) {
        nextMatch = { ...match, index };
      }
    }

    if (!nextMatch) {
      nodes.push(step.text.slice(cursor));
      break;
    }
    if (nextMatch.index > cursor) nodes.push(step.text.slice(cursor, nextMatch.index));
    nodes.push(nextMatch.kind === "code"
      ? <code key={`inline-code-${partIndex}`}>{nextMatch.phrase}</code>
      : <a key={`token-link-${partIndex}`} href="#token-form">{nextMatch.phrase}</a>);
    partIndex += 1;
    cursor = nextMatch.index + nextMatch.phrase.length;
  }

  return <>{nodes}</>;
}

function McpTokenGuidance({ host, onFollowTokenSteps }: { host: McpHost; onFollowTokenSteps: (event: ReactMouseEvent<HTMLAnchorElement>) => void }) {
  const noTokenText = `${host.name} signs in in your browser and does not need a token.`;

  return (
    <ol className="mcp-token-guidance">
      <li><span>Name the token and create it.</span></li>
      <li><span>Copy it now. It is shown only once.</span></li>
      <li>
        <span>
          {host.token ? (
            <a href="#mcp-token-details" onClick={onFollowTokenSteps}>Follow {host.name}'s token steps above</a>
          ) : noTokenText}
        </span>
      </li>
    </ol>
  );
}

function McpTokenFollowUp({ host, onFollowTokenSteps }: { host: McpHost; onFollowTokenSteps: (event: ReactMouseEvent<HTMLAnchorElement>) => void }) {
  const noTokenText = `${host.name} signs in in your browser and does not need a token.`;

  return (
    <p className="mcp-token-follow-up">
      {host.token ? (
        <>Next: <a href="#mcp-token-details" onClick={onFollowTokenSteps}>follow {host.name}'s token steps ↑</a></>
      ) : noTokenText}
    </p>
  );
}

function CodeSample({ sample, mcpUrl }: { sample: McpCodeSample; mcpUrl: string }) {
  const value = sample.content(mcpUrl);

  return (
    <div className="mcp-code-sample">
      <div className="mcp-code-heading">
        <p>{sample.label}</p>
        <CopySnippetButton value={value} label={sample.copyName} />
      </div>
      <pre aria-label={sample.accessibleName} className="mcp-code"><code>{value}</code></pre>
    </div>
  );
}

function CopySnippetButton({ value, label }: { value: string; label: string }) {
  const [state, setState] = useState<"idle" | "copied" | "failed">("idle");

  async function copy() {
    try {
      if (!navigator.clipboard?.writeText) throw new Error("clipboard unavailable");
      await navigator.clipboard.writeText(value);
      setState("copied");
    } catch {
      setState("failed");
    }
  }

  return (
    <span className="mcp-copy-control">
      <button type="button" onClick={() => void copy()} aria-label={label}>
        {state === "copied" ? "Copied" : state === "failed" ? "Copy failed" : "Copy"}
      </button>
      {state === "failed" && <span role="status">Clipboard unavailable. Select the text and copy it manually.</span>}
    </span>
  );
}

function ConnectionHeader({ title, page }: { title: string; page: "chat" | "mcp" }) {
  return (
    <header className="mb-8">
      <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.12em] text-signal">Forma</div>
      <h1 className="text-3xl font-medium tracking-tight text-paper">{title}</h1>
      <nav aria-label="Agent setup pages" className="mt-4 flex flex-wrap items-center gap-x-6 gap-y-2 border-b border-edge pb-3 text-sm">
        <a href="/agents" aria-current={page === "chat" ? "page" : undefined} className={page === "chat" ? "text-paper underline decoration-signal underline-offset-[14px]" : "text-muted hover:text-paper"}>
          Chat in the editor
        </a>
        <a href="/agents/mcp" aria-current={page === "mcp" ? "page" : undefined} className={page === "mcp" ? "text-paper underline decoration-signal underline-offset-[14px]" : "text-muted hover:text-paper"}>
          AI apps (MCP)
        </a>
        <a href="/" className="ml-auto text-xs text-faint underline underline-offset-4 hover:text-muted">← Projects</a>
      </nav>
    </header>
  );
}

function InlineConfirmAction({
  action,
  subject,
  specific,
  detail,
  pending,
  onConfirm,
}: {
  action: "Disconnect" | "Revoke";
  subject: string;
  specific: string;
  detail: string;
  pending: boolean;
  onConfirm: () => void;
}) {
  const [confirming, setConfirming] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const keep = useRef<HTMLButtonElement>(null);
  const wasConfirming = useRef(false);

  useEffect(() => {
    if (confirming) {
      wasConfirming.current = true;
      keep.current?.focus();
    } else if (wasConfirming.current) {
      wasConfirming.current = false;
      trigger.current?.focus();
    }
  }, [confirming]);

  useEffect(() => {
    if (!confirming) return;
    const escapeKeeps = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !pending) {
        event.preventDefault();
        setConfirming(false);
      }
    };
    window.addEventListener("keydown", escapeKeeps);
    return () => window.removeEventListener("keydown", escapeKeeps);
  }, [confirming, pending]);

  return (
    <div className="shrink-0">
      <button
        ref={trigger}
        type="button"
        disabled={pending || confirming}
        aria-label={action + " " + subject}
        aria-expanded={confirming}
        onClick={() => setConfirming(true)}
        className="rounded-lg border border-edge px-3 py-1.5 text-xs text-muted hover:border-danger/50 hover:text-danger disabled:opacity-45"
      >
        {pending ? action + "ing…" : action}
      </button>
      {confirming && (
        <div className="mt-3 grid gap-3 border-t border-edge pt-3">
          <p className="max-w-2xl text-xs leading-5 text-muted">
            <strong className="font-medium text-paper">{action} {subject}?</strong> {detail}
          </p>
          <div className="flex gap-2">
            <button
              type="button"
              disabled={pending}
              aria-label={"Confirm " + action.toLowerCase() + " " + specific}
              onClick={onConfirm}
              className="rounded-lg border border-danger/50 px-3 py-1.5 text-xs text-danger disabled:opacity-45"
            >
              {pending ? action + "ing…" : action}
            </button>
            <button
              ref={keep}
              type="button"
              disabled={pending}
              aria-label={"Keep " + specific}
              onClick={() => setConfirming(false)}
              className="rounded-lg border border-edge px-3 py-1.5 text-xs text-muted hover:text-paper disabled:opacity-45"
            >
              Keep
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

/**
 * The provider's grants, when there is a provider.
 *
 * `available` is false on a local Forma, where nothing can have been authorized
 * because there is nobody to authorize it — and the section is not drawn at all
 * rather than drawn empty, which would look like a connection that failed.
 */
function useGrants() {
  const [list, setList] = useState<Grant[] | null>(null);
  const [available, setAvailable] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState<string | null>(null);

  const load = () => {
    listGrants()
      .then(setList)
      .catch((cause: unknown) =>
        setError(cause instanceof Error ? cause.message : "connections could not be read"),
      );
  };

  useEffect(() => {
    let live = true;
    void (async () => {
      // The server decides whether this deployment has identity, not the
      // bundle's configuration: a build can carry provider values while the
      // server serving it runs in local mode.
      const hosted = (await getIdentity().catch(() => ({ required: false }))).required;
      if (!live || !hosted || !supabase) return;
      setAvailable(true);
      load();
    })();
    return () => {
      live = false;
    };
  }, []);

  async function disconnect(clientId: string) {
    setPending(clientId);
    setError(null);
    try {
      await revokeGrant(clientId);
      load();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "the connection could not be ended");
    } finally {
      setPending(null);
    }
  }

  return { list, available, error, pending, disconnect };
}

/**
 * Whether this Forma has a Pontis to ask about: hosted identity, signed in, with
 * an account.
 *
 * The same condition as `useUiChannel`'s gate in `App.tsx` (`access.at ===
 * "ready"`), asked the same way — the cheapest authenticated read doubles as
 * the account question. A local Forma fails the first step, and then the page
 * shows nothing Pontis-related at all rather than a panel for a sidecar that
 * cannot be there (plan §7.3, scenario S11).
 */
function usePontisEnabled(): boolean {
  const [enabled, setEnabled] = useState(false);

  useEffect(() => {
    let live = true;
    void (async () => {
      try {
        const hosted = (await getIdentity()).required;
        if (!hosted || !supabase || !(await currentSession())) return;
        await listProjects();
        if (live) setEnabled(true);
      } catch {
        // No server, no account or no session: the page works without the panel.
      }
    })();
    return () => {
      live = false;
    };
  }, []);

  return enabled;
}

/**
 * The URL, and a way to take it that is not "select carefully and hope".
 *
 * Clipboard access can be refused or absent, so the failure is shown rather
 * than swallowed — a copy button that silently does nothing is worse than no
 * button, because the person believes they have the URL.
 */
function CopyableUrl({ url }: { url: string }) {
  const [state, setState] = useState<"idle" | "copied" | "failed">("idle");

  async function copy() {
    try {
      if (!navigator.clipboard?.writeText) throw new Error("clipboard unavailable");
      await navigator.clipboard.writeText(url);
      setState("copied");
    } catch {
      setState("failed");
    }
  }

  return (
    <div className="mcp-url-row mt-4 flex items-center gap-2">
      <code className="mcp-url min-w-0 flex-1 break-all font-mono text-xs text-paper">
        {url}
      </code>
      <button
        type="button"
        onClick={() => void copy()}
        aria-label="Copy your MCP URL"
        className="shrink-0 rounded-lg border border-edge px-3 py-2 text-xs text-muted hover:border-edge-strong hover:text-paper"
      >
        {state === "copied" ? "Copied" : state === "failed" ? "Copy failed" : "Copy"}
      </button>
      {state === "failed" && <span role="status" className="text-xs text-muted">Clipboard unavailable.</span>}
    </div>
  );
}

function CopyTextButton({ value, label }: { value: string; label: string }) {
  const [state, setState] = useState<"idle" | "copied" | "failed">("idle");

  async function copy() {
    try {
      if (!navigator.clipboard?.writeText) throw new Error("clipboard unavailable");
      await navigator.clipboard.writeText(value);
      setState("copied");
    } catch {
      setState("failed");
    }
  }

  return (
    <span className="flex flex-col items-start gap-1">
      <button type="button" onClick={() => void copy()} aria-label={label} className="rounded-lg border border-edge px-3 py-2 text-xs text-muted hover:border-edge-strong hover:text-paper">
        {state === "copied" ? "Copied" : state === "failed" ? "Copy failed" : "Copy"}
      </button>
      {state === "failed" && <span role="status" className="text-[11px] text-muted">Clipboard unavailable. Select the token and copy it manually.</span>}
    </span>
  );
}

function dateLabel(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "date unknown";
  return new Intl.DateTimeFormat("en-GB", { day: "numeric", month: "short", timeZone: "UTC" }).format(date);
}
