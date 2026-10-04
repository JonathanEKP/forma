/**
 * The agent chat (`pontis/docs/13` § Agent Chat), docked beside the canvas.
 *
 * Closed by default and a column when open, so it never floats over the work it
 * is about. What it shows is the five kinds of line `docs/13` § Human visibility
 * asks to be told apart, each with its own label and shape — agent output, the
 * application running a capability, an approval request, the application's
 * result, and a navigation of the person's own view.
 */
import { useEffect, useRef, useState, type KeyboardEvent } from "react";

import type { Line, ProviderView } from "./chat";
import { ApprovalCard } from "./ChatApproval";
import type { Chat as ChatController, Connection, Runtime } from "./useChat";

/** The header button. Nothing at all where there is no sidecar to talk to. */
export function ChatToggle({ chat, open, onToggle }: { chat: ChatController; open: boolean; onToggle: () => void }) {
  if (!chat.enabled) return null;
  return (
    <button
      type="button"
      onClick={onToggle}
      aria-expanded={open}
      className={`relative rounded-lg border px-2.5 py-1.5 text-xs ${
        open ? "border-signal bg-signal font-semibold text-ink" : "border-edge bg-surface-2 text-muted hover:text-paper"
      }`}
    >
      Agent
      {chat.pending > 0 && (
        // An agent waiting on consent is the one thing worth seeing with the panel shut.
        <span
          aria-label={`${chat.pending} waiting for your approval`}
          className="absolute -right-1.5 -top-1.5 grid h-4 min-w-4 place-items-center rounded-full bg-paper px-1 text-[10px] font-semibold text-ink"
        >
          {chat.pending}
        </span>
      )}
    </button>
  );
}

export function ChatPanel({ chat, onClose }: { chat: ChatController; onClose: () => void }) {
  const [draft, setDraft] = useState("");
  const log = useRef<HTMLOListElement>(null);
  const { state } = chat;

  // New lines are at the bottom, and the person is reading there.
  useEffect(() => {
    if (log.current) log.current.scrollTop = log.current.scrollHeight;
  }, [state.lines.length, state.working, state.streaming?.text]);

  if (!chat.enabled) return null;

  // A live session already has its device; the block only matters for starting one.
  const cannot = chat.blocked !== null && state.session?.status !== "live";

  const submit = () => {
    const text = draft.trim();
    if (text.length === 0 || cannot) return;
    setDraft("");
    void chat.send(text);
  };
  const onKey = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      submit();
    }
  };

  return (
    <aside aria-label="Agent chat" className="flex w-[340px] shrink-0 flex-col border-l border-edge bg-surface">
      <header className="flex items-center gap-2 border-b border-edge px-4 py-3">
        <h2 className="text-[13px] font-medium">Agent</h2>
        <div className="flex-1" />
        <button type="button" onClick={onClose} className="text-xs text-muted hover:text-paper">
          Close
        </button>
      </header>

      <Status connection={chat.connection} live={chat.live} runtime={chat.runtime} />

      <ol ref={log} role="log" aria-live="polite" className="min-h-0 flex-1 space-y-2.5 overflow-y-auto px-4 py-3">
        {state.lines.length === 0 && (
          <li className="text-xs leading-5 text-faint">
            Ask the agent to work on this design. It acts through the same operations you do, and it asks before changing anything
            that matters.
          </li>
        )}
        {state.lines.map((line) => (
          <li key={line.id} data-kind={line.kind}>
            <Row
              line={line}
              answering={line.kind === "approval" && chat.answering.has(line.approval.approvalId)}
              onDecide={(decision) => line.kind === "approval" && void chat.decide(line.approval.approvalId, decision)}
            />
          </li>
        ))}
        {state.streaming && (
          // The reply as it arrives. Replaced, not extended, by the final text.
          <li data-kind="agent" data-streaming="true">
            <Row line={{ kind: "agent", id: "streaming", text: state.streaming.text }} answering={false} onDecide={() => undefined} />
          </li>
        )}
        {state.working && !state.streaming && (
          <li data-kind="working" className="text-xs text-faint">
            The agent is working…
          </li>
        )}
      </ol>

      <footer className="border-t border-edge p-3">
        <textarea
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={onKey}
          rows={2}
          aria-label="Message the agent"
          placeholder={cannot ? (chat.blocked ?? "") : "Message the agent"}
          className="w-full resize-none rounded-lg border border-edge bg-surface-2 px-2.5 py-2 text-xs placeholder:text-faint focus:border-edge-strong focus:outline-none"
        />
        <div className="mt-2 flex items-center gap-2">
          <button
            type="button"
            onClick={submit}
            disabled={draft.trim().length === 0 || cannot}
            className="rounded-lg bg-signal px-2.5 py-1.5 text-xs font-semibold text-ink disabled:opacity-40"
          >
            Send
          </button>
          {state.working && (
            <button
              type="button"
              onClick={() => void chat.stop()}
              className="rounded-lg border border-edge bg-surface-2 px-2.5 py-1.5 text-xs text-muted hover:text-paper"
            >
              Stop
            </button>
          )}
        </div>
      </footer>
    </aside>
  );
}

const CONNECTION: Record<Connection, { dot: string; text: string }> = {
  connecting: { dot: "bg-faint", text: "Connecting…" },
  live: { dot: "bg-paper", text: "Connected" },
  offline: { dot: "bg-faint", text: "Can't reach the agent service. Retrying." },
  gap: { dot: "bg-faint", text: "The history has a hole Forma can't read past. Some of it may be missing." },
};

/**
 * Runtime, device, provider and how it is signed in: the first thing a person
 * wants to know before trusting a conversation to it.
 */
function Status({ connection, live, runtime }: { connection: Connection; live: boolean; runtime: Runtime }) {
  const link = CONNECTION[connection];
  return (
    <section aria-label="Status" className="space-y-1.5 border-b border-edge px-4 py-3 text-[11px] text-muted">
      <div className="flex items-center gap-2">
        <span className={`h-[7px] w-[7px] rounded-full ${link.dot}`} />
        <span>{connection === "live" && live ? "Connected · live" : link.text}</span>
      </div>
      <Device runtime={runtime} />
    </section>
  );
}

function Device({ runtime }: { runtime: Runtime }) {
  switch (runtime.at) {
    case "unknown":
      return null;
    case "unreachable":
      return null;
    case "none":
      return (
        <p>
          No device is paired. Pair one from <a href="/agents" className="underline">the Agents page</a>.
        </p>
      );
    case "offline":
      return <p>{runtime.name} is offline. Start the Runtime on it to chat.</p>;
    case "ambiguous":
      return <p>More than one of your devices is online, so Forma can&apos;t tell which one to use.</p>;
    case "online":
      return (
        <>
          <p>
            <span className="text-paper">{runtime.name}</span> · online · Runtime {runtime.runtimeVersion}
          </p>
          {runtime.provider ? <Provider provider={runtime.provider} /> : <p>It reports no agent provider.</p>}
        </>
      );
  }
}

function Provider({ provider }: { provider: ProviderView }) {
  return (
    <div className="space-y-1">
      <p>
        <span className="text-paper">{provider.id}</span> · {provider.authMode}
        {provider.authMode === "api_key" && " — usage is billed to that API key"}
      </p>
      {!provider.usable && <p className="text-paper">Unavailable: {provider.unusableBecause}.</p>}
      {provider.signInNotVerified && (
        <p>Sign-in not verified: this provider can&apos;t report it. If it isn&apos;t signed in, the first message will say so.</p>
      )}
      {/* The registry tokens, verbatim and apart: ADR-019 keeps `provider_managed`
          from being read as runtime-enforced, and the four classifications from
          running together. */}
      <p className="flex flex-wrap gap-1.5">
        {provider.classification && (
          <span className="rounded border border-edge-strong px-1.5 py-0.5 font-mono text-[10px]">{provider.classification}</span>
        )}
        {provider.sandbox && (
          <span className="rounded border border-edge-strong px-1.5 py-0.5 font-mono text-[10px]">sandbox: {provider.sandbox}</span>
        )}
      </p>
    </div>
  );
}

function Row({ line, answering, onDecide }: { line: Line; answering: boolean; onDecide: (decision: "approveOnce" | "deny") => void }) {
  switch (line.kind) {
    case "person":
      return (
        <div className="ml-8 rounded-lg bg-surface-2 px-2.5 py-2 text-xs leading-5">
          <Label>You</Label>
          <p className="whitespace-pre-wrap">{line.text}</p>
        </div>
      );
    case "agent":
      return (
        <div className="text-xs leading-5">
          <Label>Agent</Label>
          <p className="whitespace-pre-wrap">{line.text}</p>
        </div>
      );
    case "capability":
      return (
        <div className="rounded border border-edge px-2.5 py-1.5 text-[11px] text-muted">
          <Label>Running in Forma</Label>
          <code className="text-paper">{line.capabilityId}</code> <span>{line.summary}</span>
          <span className="ml-1 text-faint">{STATUS[line.status]}</span>
        </div>
      );
    case "result":
      return (
        <div className={`ml-3 border-l-2 pl-2.5 text-[11px] ${line.ok ? "border-edge-strong text-muted" : "border-paper text-paper"}`}>
          <Label>Result</Label>
          <code>{line.capabilityId}</code> · {line.text}
        </div>
      );
    case "approval":
      return <ApprovalCard approval={line.approval} status={line.status} answering={answering} onDecide={onDecide} />;
    case "navigation":
      return (
        <div className="rounded border border-dashed border-edge-strong px-2.5 py-1.5 text-[11px] text-muted">
          <Label>Moved your view</Label>
          <span className="break-words">{line.text}</span>
          {line.status === "failed" && <span className="ml-1 text-paper">· didn&apos;t land</span>}
        </div>
      );
    case "notice":
      return <p className={`text-center text-[11px] ${line.tone === "error" ? "text-paper" : "text-faint"}`}>{line.text}</p>;
  }
}

const STATUS = { running: "· running", done: "· done", failed: "· failed" } as const;

function Label({ children }: { children: string }) {
  return <span className="mb-0.5 block text-[10px] font-semibold uppercase tracking-[0.1em] text-faint">{children}</span>;
}
