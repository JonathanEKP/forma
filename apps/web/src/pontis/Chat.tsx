/**
 * The agent chat (`pontis/docs/13` § Agent Chat), docked beside the canvas.
 *
 * Closed by default and a column when open, so it never floats over the work it
 * is about. What it shows is the five kinds of line `docs/13` § Human visibility
 * asks to be told apart, each with its own label and shape — agent output, the
 * application running a capability, an approval request, the application's
 * result, and a navigation of the person's own view.
 */
import { useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";

import type { Line, NoticeCopy, ProviderView, ViewChip } from "./chat";
import { ApprovalCard } from "./ChatApproval";
import { FormattedText } from "./formatted-text";
import { recallDraft, rememberDraft } from "./session-store";
import type { Chat as ChatController, Connection, Runtime } from "./useChat";

const MAX_MESSAGE_BODY_BYTES = 64 * 1024;
// The sidecar adds its bounded view line to the message before sending it to the Runtime.
const VIEW_LINE_HEADROOM_BYTES = 1024;
const SHOW_LIMIT_AT = 0.9;

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
  const [draft, setDraft] = useState(recallDraft);
  const log = useRef<HTMLOListElement>(null);
  const composer = useRef<HTMLTextAreaElement>(null);
  const sessionInfo = useRef<HTMLDivElement>(null);
  const sessionInfoButton = useRef<HTMLButtonElement>(null);
  const followsLatest = useRef(true);
  const [showJump, setShowJump] = useState(false);
  const [sessionInfoOpen, setSessionInfoOpen] = useState(false);
  const [runOnOpen, setRunOnOpen] = useState(chat.needsComputerChoice);
  const { state } = chat;

  useEffect(() => {
    if (chat.needsComputerChoice) setRunOnOpen(true);
  }, [chat.needsComputerChoice]);

  useEffect(() => {
    if (!sessionInfoOpen) return;
    const closeOnEscape = (event: globalThis.KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      setSessionInfoOpen(false);
      sessionInfoButton.current?.focus();
    };
    const closeOutside = (event: PointerEvent) => {
      const popover = sessionInfo.current;
      if (popover && event.target instanceof Node && !popover.contains(event.target)) setSessionInfoOpen(false);
    };
    document.addEventListener("keydown", closeOnEscape);
    document.addEventListener("pointerdown", closeOutside);
    return () => {
      document.removeEventListener("keydown", closeOnEscape);
      document.removeEventListener("pointerdown", closeOutside);
    };
  }, [sessionInfoOpen]);

  // Follow new entries only while the person is already at the end of the transcript.
  useEffect(() => {
    const transcript = log.current;
    if (!transcript || !followsLatest.current) {
      setShowJump(true);
      return;
    }
    transcript.scrollTop = transcript.scrollHeight;
    setShowJump(false);
  }, [state.lines.length, state.working, state.streaming?.text, chat.dead]);

  const onTranscriptScroll = () => {
    const transcript = log.current;
    if (!transcript) return;
    const atLatest = transcript.scrollHeight - transcript.scrollTop - transcript.clientHeight < 32;
    followsLatest.current = atLatest;
    setShowJump(!atLatest);
  };
  const jumpToLatest = () => {
    const transcript = log.current;
    if (!transcript) return;
    followsLatest.current = true;
    transcript.scrollTop = transcript.scrollHeight;
    setShowJump(false);
  };
  const startNewSession = async () => {
    await chat.newSession();
    composer.current?.focus();
  };

  if (!chat.enabled) return null;

  // A live session already has its device; the block only matters for starting one. A dead one
  // is not carried on at all: the person starts another, from the button, on purpose.
  const cannot = chat.dead !== null || chat.needsComputerChoice || chat.blocked !== null;
  const body = JSON.stringify({
    type: "session.input",
    payload: { sessionId: state.session?.id ?? `ses_${"0".repeat(32)}`, text: draft.trim() },
  });
  const encodedBytes = new TextEncoder().encode(body).byteLength;
  const sizeWithHeadroom = encodedBytes + VIEW_LINE_HEADROOM_BYTES;
  const overLimit = sizeWithHeadroom > MAX_MESSAGE_BODY_BYTES;
  const nearLimit = sizeWithHeadroom >= MAX_MESSAGE_BODY_BYTES * SHOW_LIMIT_AT;

  const submit = () => {
    const text = draft.trim();
    if (text.length === 0 || cannot || state.working || overLimit) return;
    setDraft("");
    void chat.send(text, () => rememberDraft(""));
  };
  const changeDraft = (text: string) => {
    setDraft(text);
    rememberDraft(text);
  };
  const onKey = (event: ReactKeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      submit();
    }
  };

  return (
    <aside aria-label="Agent chat" className="flex w-[340px] min-w-0 shrink-0 flex-col border-l border-edge bg-surface">
      <header className="flex items-center gap-2 border-b border-edge px-4 py-3">
        <h2 className="text-[13px] font-medium">Agent</h2>
        <div className="flex-1" />
        <div ref={sessionInfo} className="relative">
          <button ref={sessionInfoButton} type="button" aria-label="Session information" aria-expanded={sessionInfoOpen} onClick={() => setSessionInfoOpen((open) => !open)} className="text-xs text-muted hover:text-paper">
            Info
          </button>
          {sessionInfoOpen && (
            <section aria-label="Session information" className="absolute right-0 top-full z-10 mt-2 w-56 rounded-lg border border-edge bg-surface-2 p-3 text-[11px] text-muted shadow-xl">
              <p><span className="text-faint">Computer</span><br />{sessionComputer(chat, state.deviceId)}</p>
              <p className="mt-2"><span className="text-faint">Started</span><br />{state.startedAt ? new Date(state.startedAt).toLocaleString() : "Not started"}</p>
              <p className="mt-2 text-paper">This window&apos;s session</p>
            </section>
          )}
        </div>
        <button type="button" onClick={() => void startNewSession()} className="text-xs text-muted hover:text-paper">
          New session
        </button>
        <button type="button" onClick={onClose} className="text-xs text-muted hover:text-paper">
          Close
        </button>
      </header>

      <Status connection={chat.connection} live={chat.live} runtime={chat.runtime} />

      <ol ref={log} onScroll={onTranscriptScroll} role="log" aria-live="polite" tabIndex={0} className="min-h-0 min-w-0 flex-1 space-y-2.5 overflow-y-auto overflow-x-hidden px-4 py-3 [overflow-wrap:anywhere]">
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
              unconfirmed={line.kind === "approval" && chat.unconfirmed.has(line.approval.approvalId)}
              onDecide={(decision) => line.kind === "approval" && void chat.decide(line.approval.approvalId, decision)}
              onNoticeAction={(action) => action === "New session" && void startNewSession()}
              deviceName={chat.runtime.at === "online" ? chat.runtime.name : undefined}
            />
          </li>
        ))}
        {state.streaming && (
          // The reply as it arrives. Replaced, not extended, by the final text.
          <li data-kind="agent" data-streaming="true">
            <Row line={{ kind: "agent", id: "streaming", text: state.streaming.text }} answering={false} unconfirmed={false} onDecide={() => undefined} onNoticeAction={() => undefined} />
          </li>
        )}
        {state.working && !state.streaming && (
          <li data-kind="working" className="text-xs text-faint">
            The agent is working…
          </li>
        )}
        {chat.dead !== null && (
          <li data-kind="notice">
            <SessionNotice notice={chat.dead} onAction={(action) => action === "New session" && void startNewSession()} />
          </li>
        )}
      </ol>

      {showJump && (
        <button type="button" onClick={jumpToLatest} className="mx-auto my-1 rounded-lg border border-edge bg-surface-2 px-3 py-1.5 text-xs text-paper">
          Jump to latest
        </button>
      )}

      <footer className="border-t border-edge p-3">
        <textarea
          ref={composer}
          value={draft}
          onChange={(event) => changeDraft(event.target.value)}
          onKeyDown={onKey}
          rows={2}
          aria-label="Message the agent"
          placeholder={chat.dead !== null ? "Start a new session to carry on" : cannot ? (chat.blocked ?? "") : "Message the agent"}
          className="w-full resize-none rounded-lg border border-edge bg-surface-2 px-2.5 py-2 text-xs placeholder:text-faint focus:border-edge-strong focus:outline-none"
        />
        {nearLimit && (
          <p role={overLimit ? "alert" : "status"} className={`mt-1 text-[11px] ${overLimit ? "text-danger" : "text-muted"}`}>
            {overLimit ? "That message is too long (max 64 KB)." : `Message size ${Math.ceil(encodedBytes / 1024)} KB; maximum 64 KB.`}
          </p>
        )}
        <div className="mt-2 flex items-center gap-2">
          <button
            type="button"
            onClick={submit}
            aria-describedby="send-reason"
            disabled={draft.trim().length === 0 || cannot || state.working || overLimit}
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
          {chat.computers.length >= 2 && (
            <div className="relative ml-auto">
              <button type="button" aria-expanded={runOnOpen} aria-haspopup="true" onClick={() => setRunOnOpen((open) => !open)} className="rounded-lg border border-edge px-2.5 py-1.5 text-xs text-muted hover:text-paper">
                Run on: {chat.computers.find((computer) => computer.id === chat.selectedComputerId)?.name ?? "Choose a computer"}
              </button>
              {runOnOpen && (
                <section aria-label="Run on computer" className="absolute bottom-full right-0 z-10 mb-2 w-64 rounded-lg border border-edge bg-surface-2 p-2 shadow-xl">
                  {chat.computers.map((computer) => (
                    <button key={computer.id} type="button" aria-pressed={chat.selectedComputerId === computer.id} onClick={() => { void (async () => { await chat.chooseComputer(computer.id); setRunOnOpen(false); composer.current?.focus(); })(); }} className="flex w-full flex-col items-start rounded px-2 py-2 text-left text-xs hover:bg-surface">
                      <span className="text-paper">{computer.name}</span>
                      <span className="text-[10px] text-faint">{computer.presence} · Runtime {computer.runtimeVersion}</span>
                    </button>
                  ))}
                </section>
              )}
            </div>
          )}
        </div>
        <p id="send-reason" role="status" className="mt-1.5 text-[11px] text-faint">
          {state.working ? "The agent is working. Stop it to send now." : chat.dead?.next ?? chat.blocked ?? (draft.trim().length === 0 ? "Write a message to send." : overLimit ? "Shorten the message to send it." : "")}
        </p>
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
    case "unauthenticated":
      return <p>Your sign-in has expired. Sign in again.</p>;
    case "none":
      return (
        <p>
          No device is paired. Pair one from <a href="/agents" className="underline">the Agents page</a>.
        </p>
      );
    case "offline":
      return <p>{runtime.name} is offline. Start the Runtime on it to chat.</p>;
    case "revoked":
      return (
        <p>
          {runtime.name} was revoked. Pair a new device from <a href="/agents" className="underline">the Agents page</a>.
        </p>
      );
    case "ambiguous":
      return <p>{runtime.devices.length} computers are online. Choose one in Run on.</p>;
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

function Row({ line, answering, unconfirmed, onDecide, onNoticeAction, deviceName }: { line: Line; answering: boolean; unconfirmed: boolean; onDecide: (decision: "approveOnce" | "deny") => void; onNoticeAction: (action: NoticeCopy["action"]) => void; deviceName?: string }) {
  switch (line.kind) {
    case "person":
      return (
        <div className="ml-8 rounded-lg bg-surface-2 px-2.5 py-2 text-xs leading-5">
          <Label>You</Label>
          <p dir="auto" style={{ unicodeBidi: "plaintext" }} className="whitespace-pre-wrap">{line.text}</p>
          {line.chip && <Chip chip={line.chip} />}
        </div>
      );
    case "divider":
      return (
        <div role="separator" className="flex items-center gap-2 py-1 text-[10px] uppercase tracking-[0.1em] text-faint">
          <span className="h-px flex-1 bg-edge" />
          {line.text}
          <span className="h-px flex-1 bg-edge" />
        </div>
      );
    case "agent":
      return (
        <div className="text-xs leading-5">
          <Label>Agent</Label>
          <div dir="auto" style={{ unicodeBidi: "plaintext" }}>
            <FormattedText text={line.text} />
          </div>
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
      return <ApprovalCard approval={line.approval} status={line.status} answering={answering} unconfirmed={unconfirmed} onDecide={onDecide} deviceName={deviceName} />;
    case "approval-repeat":
      return (
        <p className="editor-approval-repeat" role="status">
          {line.text} <span>{line.hint}</span>
        </p>
      );
    case "navigation":
      return (
        <article aria-label={line.heading} className="grid min-w-0 gap-2 rounded-lg border border-edge bg-surface-2 px-3 py-2.5 text-xs [overflow-wrap:anywhere]">
          <h3 className="font-medium text-paper">{line.heading}</h3>
          {line.summaries.length > 0 && (
            <ul className="grid min-w-0 gap-1.5 text-muted">
              {line.summaries.map((summary, index) => <li key={index} className="min-w-0 [overflow-wrap:anywhere]">{summary}</li>)}
            </ul>
          )}
          <details className="min-w-0 text-[11px] text-faint">
            <summary className="cursor-pointer">Details</summary>
            <pre className="mt-2 max-w-full whitespace-pre-wrap [overflow-wrap:anywhere]">{line.details}</pre>
          </details>
        </article>
      );
    case "refusal":
      return (
        <article aria-label={line.heading} className="grid min-w-0 gap-1.5 rounded-lg bg-surface-2 px-3 py-2.5 text-xs text-muted [overflow-wrap:anywhere]">
          <h3 className="font-medium text-muted">{line.heading}</h3>
          <p className="min-w-0 [overflow-wrap:anywhere]">{line.summary}</p>
          <details className="min-w-0 text-[11px] text-faint">
            <summary className="cursor-pointer">Details</summary>
            <pre className="mt-2 max-w-full whitespace-pre-wrap [overflow-wrap:anywhere]">{line.details}</pre>
          </details>
        </article>
      );
    case "notice":
      return <SessionNotice notice={line.notice} onAction={onNoticeAction} />;
  }
}

/** Where the person was when they said it: what the agent was told, in their own words. */
function Chip({ chip }: { chip: ViewChip }) {
  const parts = [
    chip.project === undefined ? undefined : `Project “${chip.project}”`,
    chip.design,
    chip.page,
    chip.layer,
  ].filter((part): part is string => part !== undefined);
  return (
    <p
      aria-label={`Sent with: ${parts.join(", ")}`}
      className="mt-1.5 inline-block max-w-full truncate rounded border border-edge px-1.5 py-0.5 text-[10px] text-faint"
    >
      {parts.join(" · ")}
    </p>
  );
}

function SessionNotice({ notice, onAction }: { notice: NoticeCopy; onAction: (action: NoticeCopy["action"]) => void }) {
  const label = notice.tone === "stopped" ? "Session stopped" : notice.tone === "needs-you" ? "Session needs you" : "Session information";
  const toneText = notice.tone === "stopped" ? "Stopped" : notice.tone === "needs-you" ? "Needs you" : "Info";
  return (
    <section aria-label={label} data-tone={notice.tone} className={`grid min-w-0 gap-1.5 rounded-lg border px-3 py-2.5 text-xs [overflow-wrap:anywhere] ${notice.tone === "stopped" ? "border-edge bg-surface-2" : "border-edge bg-surface"}`}>
      <p className="text-[10px] font-semibold uppercase tracking-[0.1em] text-faint">{toneText}</p>
      <p className="font-medium text-paper">{notice.happened}</p>
      <p className="text-muted">{notice.next}</p>
      {notice.action === "New session" && <button type="button" onClick={() => onAction(notice.action)} className="mt-1 justify-self-start rounded-lg bg-signal px-2.5 py-1.5 text-xs font-semibold text-ink">New session</button>}
      {notice.action === "Sign in again" && <a href="/login" className="mt-1 justify-self-start text-xs text-paper underline">Sign in again</a>}
      {notice.action === "Open Agents page" && <a href="/agents" className="mt-1 justify-self-start text-xs text-paper underline">Open Agents page</a>}
    </section>
  );
}

function sessionComputer(chat: ChatController, deviceId: string | null): string {
  const selected = chat.computers.find((computer) => computer.id === deviceId);
  if (selected) return selected.name;
  if ("sessionDevice" in chat.runtime && chat.runtime.sessionDevice) return chat.runtime.sessionDevice.name;
  return deviceId === null ? "Not started" : "Offline";
}

const STATUS = { running: "· running", done: "· done", failed: "· failed", not_run: "· Not run" } as const;

function Label({ children }: { children: string }) {
  return <span className="mb-0.5 block text-[10px] font-semibold uppercase tracking-[0.1em] text-faint">{children}</span>;
}
