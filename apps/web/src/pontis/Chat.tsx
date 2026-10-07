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
  const [renderJump, setRenderJump] = useState(false);
  const [hasNewEntries, setHasNewEntries] = useState(false);
  const [transcriptScrolling, setTranscriptScrolling] = useState(false);
  const scrollIdleTimer = useRef<number | null>(null);
  const [sessionInfoOpen, setSessionInfoOpen] = useState(false);
  const [runOnOpen, setRunOnOpen] = useState(chat.needsComputerChoice);
  const { state } = chat;
  const contextChip = chat.currentContext();

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
    if (!transcript) return;
    if (!followsLatest.current) {
      setShowJump(true);
      setHasNewEntries(true);
      return;
    }
    transcript.scrollTop = transcript.scrollHeight;
    setShowJump(false);
    setHasNewEntries(false);
  }, [state.lines.length, state.working, state.streaming?.text, chat.dead]);

  // Approval cards can gain their named layer chips after the design query resolves.
  // That changes transcript height without changing the chat state dependencies above.
  // MutationObserver runs after DOM updates and works in background tabs too.
  useEffect(() => {
    const transcript = log.current;
    if (!transcript) return;
    const followContentChange = () => {
      if (!followsLatest.current) {
        setShowJump(true);
        return;
      }
      transcript.scrollTop = transcript.scrollHeight;
      setShowJump(false);
    };
    const observer = new MutationObserver(followContentChange);
    observer.observe(transcript, { childList: true, subtree: true, characterData: true });
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (showJump) {
      setRenderJump(true);
      return;
    }
    if (!renderJump) return;
    const prefersReducedMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
    if (prefersReducedMotion || document.visibilityState === "hidden") {
      setRenderJump(false);
      return;
    }
    const timer = window.setTimeout(() => setRenderJump(false), 180);
    return () => window.clearTimeout(timer);
  }, [renderJump, showJump]);

  useEffect(() => () => {
    if (scrollIdleTimer.current !== null) window.clearTimeout(scrollIdleTimer.current);
  }, []);

  const onTranscriptScroll = () => {
    const transcript = log.current;
    if (!transcript) return;
    setTranscriptScrolling(true);
    if (scrollIdleTimer.current !== null) window.clearTimeout(scrollIdleTimer.current);
    scrollIdleTimer.current = window.setTimeout(() => {
      setTranscriptScrolling(false);
      scrollIdleTimer.current = null;
    }, 600);
    const atLatest = transcript.scrollHeight - transcript.scrollTop - transcript.clientHeight < 32;
    followsLatest.current = atLatest;
    setShowJump(!atLatest);
    if (atLatest) setHasNewEntries(false);
  };
  const jumpToLatest = () => {
    const transcript = log.current;
    if (!transcript) return;
    followsLatest.current = true;
    transcript.scrollTop = transcript.scrollHeight;
    setShowJump(false);
    setHasNewEntries(false);
  };
  const startNewSession = async () => {
    await chat.newSession();
    composer.current?.focus();
  };

  if (!chat.enabled) return null;

  // A live session already has its device; the block only matters for starting one. A dead one
  // is not carried on at all: the person starts another, from the button, on purpose.
  const cannot = chat.dead !== null || chat.needsComputerChoice || chat.blocked !== null;
  const signInNotice = knownSignInNotice(chat.runtime);
  const body = JSON.stringify({
    type: "session.input",
    payload: { sessionId: state.session?.id ?? `ses_${"0".repeat(32)}`, text: draft.trim() },
  });
  const encodedBytes = new TextEncoder().encode(body).byteLength;
  const sizeWithHeadroom = encodedBytes + VIEW_LINE_HEADROOM_BYTES;
  const overLimit = sizeWithHeadroom > MAX_MESSAGE_BODY_BYTES;
  const nearLimit = sizeWithHeadroom >= MAX_MESSAGE_BODY_BYTES * SHOW_LIMIT_AT;
  const composerState = state.working
    ? "The agent is working. Stop it to send now."
    : chat.dead?.next ?? chat.blocked ?? (overLimit ? "Shorten the message to send it." : "Ready");

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
      <header className="grid gap-2 border-b border-edge px-4 py-3">
        <div className="flex items-center gap-2">
          <h2 className="text-[13px] font-medium">Agent</h2>
          <div className="flex-1" />
          <div ref={sessionInfo} className="relative">
            <button
              ref={sessionInfoButton}
              type="button"
              aria-label="Session info"
              aria-expanded={sessionInfoOpen}
              aria-controls="agent-session-info"
              title="Session info"
              onClick={() => setSessionInfoOpen((open) => !open)}
              className="grid h-8 w-8 place-items-center rounded text-muted hover:bg-surface-2 hover:text-paper"
            >
              <svg aria-hidden="true" viewBox="0 0 16 16" className="h-4 w-4 fill-none stroke-current" strokeWidth="1.25">
                <circle cx="8" cy="8" r="6" />
                <path d="M8 7v4M8 5h.01" />
              </svg>
            </button>
          {sessionInfoOpen && (
              <section id="agent-session-info" role="region" aria-label="Session info" className="absolute right-0 top-full z-10 mt-2 grid w-64 gap-3 rounded-lg border border-edge bg-surface-2 p-3 text-[11px] text-muted shadow-xl">
                <p><span className="text-faint">Computer</span><br />{sessionComputer(chat, state.deviceId)}</p>
                <p><span className="text-faint">Started</span><br />{state.startedAt ? new Date(state.startedAt).toLocaleString() : "Not started"}</p>
                <p className="font-mono text-[10px] text-paper">This window&apos;s session</p>
                <div className="border-t border-edge pt-3"><Device runtime={chat.runtime} /></div>
              </section>
            )}
          </div>
          <button type="button" aria-label="New session" title="New session" onClick={() => void startNewSession()} className="grid h-8 w-8 place-items-center rounded text-muted hover:bg-surface-2 hover:text-paper">
            <svg aria-hidden="true" viewBox="0 0 16 16" className="h-4 w-4 fill-none stroke-current" strokeWidth="1.25">
              <path d="M8 3v10M3 8h10" />
            </svg>
          </button>
          <button type="button" aria-label="Close" title="Close chat" onClick={onClose} className="grid h-8 w-8 place-items-center rounded text-muted hover:bg-surface-2 hover:text-paper">
            <svg aria-hidden="true" viewBox="0 0 16 16" className="h-4 w-4 fill-none stroke-current" strokeWidth="1.25">
              <path d="m4 4 8 8M12 4l-8 8" />
            </svg>
          </button>
        </div>
        <Status connection={chat.connection} runtime={chat.runtime} computer={sessionComputer(chat, state.deviceId)} />
      </header>

      <div className="chat-transcript-shell">
        <ol ref={log} onScroll={onTranscriptScroll} role="log" aria-live="polite" tabIndex={0} className={`chat-transcript min-h-0 min-w-0 flex-1 space-y-2.5 overflow-y-auto overflow-x-hidden px-4 py-3 [overflow-wrap:anywhere] ${transcriptScrolling ? "is-scrolling" : ""}`}>
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
          {signInNotice && chat.dead === null && (
            <li data-kind="notice">
              <SessionNotice notice={signInNotice} onAction={() => undefined} />
            </li>
          )}
          {chat.dead !== null && (
            <li data-kind="notice">
              <SessionNotice notice={chat.dead} onAction={(action) => action === "New session" && void startNewSession()} />
            </li>
          )}
        </ol>

        {renderJump && (
          <button
            type="button"
            onClick={jumpToLatest}
            aria-label="Jump to latest"
            aria-hidden={!showJump}
            disabled={!showJump}
            tabIndex={showJump ? 0 : -1}
            className={`chat-jump-latest ${showJump ? "is-visible" : ""}`}
          >
            <span aria-hidden="true">↓</span>
            {hasNewEntries && <span data-new-entry-indicator aria-hidden="true" className="chat-new-entry-dot" />}
          </button>
        )}
      </div>

      <footer className="border-t border-edge p-3">
        {contextChip && (
          <div role="status" aria-label="Current message context" className="mb-2 flex min-w-0 items-center gap-2 px-1 py-1 text-xs text-muted">
            <ContextGlyph />
            <span className="min-w-0 truncate">{viewChipText(contextChip)}</span>
          </div>
        )}
        <div className="rounded-lg border border-edge bg-surface-2 p-3">
          <label htmlFor="agent-message" className="block text-xs text-muted">Message the agent</label>
          <textarea
            id="agent-message"
            ref={composer}
            value={draft}
            onChange={(event) => changeDraft(event.target.value)}
            onKeyDown={onKey}
            rows={2}
            aria-label="Message the agent"
            aria-describedby="composer-hint"
            placeholder="Ask for a change…"
            className="w-full resize-y border-0 bg-transparent px-0 py-2 text-xs placeholder:text-faint focus:outline-none"
          />
          {nearLimit && (
            <p role={overLimit ? "alert" : "status"} className={`mt-1 text-[11px] ${overLimit ? "text-danger" : "text-muted"}`}>
              {overLimit ? "That message is too long (max 64 KB)." : `Message size ${Math.ceil(encodedBytes / 1024)} KB; maximum 64 KB.`}
            </p>
          )}
          <div className="mt-1 flex items-center justify-between gap-2">
            <p id="send-reason" role="status" className="min-w-0 text-[11px] text-faint">{composerState}</p>
            <div className="flex shrink-0 items-center gap-2">
              {state.working && (
                <button
                  type="button"
                  onClick={() => void chat.stop()}
                  className="rounded-lg border border-edge px-2.5 py-1.5 text-xs text-muted hover:text-paper"
                >
                  Stop
                </button>
              )}
              {(chat.computers.length >= 2 || (chat.dead !== null && chat.computers.length > 0)) && (
                <div className="relative">
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
              <button
                type="button"
                onClick={submit}
                aria-label="Send"
                aria-describedby="send-reason"
                disabled={draft.trim().length === 0 || cannot || state.working || overLimit}
                className="rounded-lg bg-signal px-2.5 py-1.5 text-xs font-semibold text-ink disabled:opacity-40"
              >
                Send ↑
              </button>
            </div>
          </div>
        </div>
        <p id="composer-hint" className="mt-2 text-[11px] text-faint">Only this window&apos;s view is shared.</p>
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

/** The quiet connection line and per-window session label beneath the title. */
function Status({ connection, runtime, computer }: { connection: Connection; runtime: Runtime; computer: string }) {
  const link = CONNECTION[connection];
  const status = runtime.at === "offline"
    ? "Offline"
    : runtime.at === "revoked"
      ? "Revoked"
      : runtime.at === "ambiguous"
        ? "Choose a computer"
        : runtime.at === "none"
          ? "No computer paired"
          : runtime.at === "unauthenticated"
            ? "Sign-in expired"
            : connection === "live" ? "Connected" : link.text;
  return (
    <section aria-label="Status" className="grid gap-1 text-[11px] text-muted">
      <div className="flex min-w-0 items-center gap-2">
        <span aria-hidden="true" className={`h-[4px] w-[4px] shrink-0 rounded-full ${link.dot}`} />
        <span className="shrink-0">{status}</span>
        {computer !== "Not started" && <span className="min-w-0 truncate text-right">{computer}</span>}
      </div>
      <p className="font-mono text-[10px] text-faint">This window&apos;s session</p>
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
  const parts = viewChipParts(chip);
  return (
    <p
      aria-label={`Sent with: ${parts.join(", ")}`}
      className="mt-1.5 inline-block max-w-full truncate rounded border border-edge px-1.5 py-0.5 text-[10px] text-faint"
    >
      {parts.join(" · ")}
    </p>
  );
}

function viewChipParts(chip: ViewChip): string[] {
  return [
    chip.project === undefined ? undefined : `Project “${chip.project}”`,
    chip.design,
    chip.page,
    chip.layer,
  ].filter((part): part is string => part !== undefined);
}

function viewChipText(chip: ViewChip): string {
  if (chip.project !== undefined) return `Project “${chip.project}”`;
  return [
    chip.design === undefined ? undefined : `“${chip.design}”`,
    chip.pageNumber === undefined ? undefined : `page ${chip.pageNumber}`,
    chip.layer === undefined ? undefined : `“${chip.layer}” selected`,
  ].filter((part): part is string => part !== undefined).join(" · ");
}

function ContextGlyph() {
  return (
    <svg aria-hidden="true" viewBox="0 0 16 16" className="h-4 w-4 shrink-0 fill-none stroke-current" strokeWidth="1.25">
      <path d="m2 5 6-3 6 3-6 3-6-3Zm0 3 6 3 6-3M2 11l6 3 6-3" />
    </svg>
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
  if (chat.runtime.at === "online" || chat.runtime.at === "offline" || chat.runtime.at === "revoked") return chat.runtime.name;
  if (chat.runtime.at === "ambiguous") return "Choose a computer";
  return deviceId === null ? "Not started" : "Offline";
}

function knownSignInNotice(runtime: Runtime): NoticeCopy | null {
  if (runtime.at === "unauthenticated") {
    return {
      tone: "needs-you",
      happened: "Your sign-in has expired.",
      next: "Sign in again, then start a new session.",
      action: "Sign in again",
    };
  }
  if (runtime.at === "online" && runtime.provider?.unusableBecause === "not signed in on this device") {
    return {
      tone: "needs-you",
      happened: `${runtime.provider.id} is not signed in on this device.`,
      next: "Choose a computer with an available provider.",
      action: "Open Agents page",
    };
  }
  return null;
}

const STATUS = { running: "· running", done: "· done", failed: "· failed", not_run: "· Not run" } as const;

function Label({ children }: { children: string }) {
  return <span className="mb-0.5 block text-[10px] font-semibold uppercase tracking-[0.1em] text-faint">{children}</span>;
}
