/**
 * Boundary F, from the sidecar's side: the fixed operations forma-server serves
 * at `/api/v1/pontis`, plus the one resolve call that precedes them
 * (Pontis plan §4, §10(b) D1–D2).
 *
 * There is no generic request method here. Each operation is a method with its
 * own path, so a capability cannot reach an endpoint nobody wrote a binding for,
 * and the model never chooses a URL, a credential, a user or a context.
 *
 * Every call carries the sidecar's service credential and, except resolve, the
 * represented user and application context the session was bound to. Forma
 * revalidates the user's membership in that context on every request (ADR-024),
 * so nothing here caches an authorization decision.
 */
import { CapabilityError, isCapabilityErrorCode } from '@pontis/core';
import type { AgentManual } from '@forma/bindings/AgentManual';
import type { AssetRequest } from '@forma/bindings/AssetRequest';
import type { DesignRecord } from '@forma/bindings/DesignRecord';
import type { DesignSummary } from '@forma/bindings/DesignSummary';
import type { PontisApplyRequest } from '@forma/bindings/PontisApplyRequest';
import type { PontisApplyResult } from '@forma/bindings/PontisApplyResult';
import type { PontisAssetRequestBody } from '@forma/bindings/PontisAssetRequestBody';
import type { PontisBrand } from '@forma/bindings/PontisBrand';
import type { Project } from '@forma/bindings/Project';
import type { SubjectResolution } from '@forma/bindings/SubjectResolution';
import type { ValidationReport } from '@forma/bindings/ValidationReport';

import type { Log } from './log.ts';

/** forma-server's header names (`apps/server/src/auth.rs`, `routes/pontis.rs`). */
export const REPRESENTED_USER_HEADER = 'x-forma-represented-user';
export const APPLICATION_CONTEXT_HEADER = 'x-forma-application-context';
export const SUBJECT_TOKEN_HEADER = 'x-forma-subject-token';
export const INVOCATION_HEADER = 'x-forma-invocation-id';
export const TIMEOUT_HEADER = 'x-forma-timeout-ms';

/** Who a Boundary F call acts for, as the session bound them. */
export interface Represented {
  readonly userId: string;
  readonly applicationContextId: string;
}

/** The largest error envelope read back. Forma's are a few hundred bytes. */
const MAX_ERROR_BYTES = 64 * 1024;

/** A resolve answer is three identifiers. */
const MAX_RESOLUTION_BYTES = 4 * 1024;

export class FormaBoundary {
  readonly #origin: string;
  readonly #credential: string;
  readonly #log: Log;

  constructor(origin: string, credential: string, log: Log) {
    this.#origin = origin.replace(/\/+$/u, '');
    this.#credential = credential;
    this.#log = log;
  }

  /**
   * Who a Forma session token belongs to and the account they may enter, or
   * `undefined` when Forma does not accept the token.
   *
   * The token is Forma's to verify (§10(b) D2). It travels in its own header
   * because `Authorization` carries the sidecar's credential, and the call is
   * never marked `x-forma-entry`.
   */
  async resolveSubject(token: string, signal?: AbortSignal): Promise<SubjectResolution | undefined> {
    const response = await this.#send('service.resolve', '/api/v1/service/resolve', {
      method: 'GET',
      headers: { [SUBJECT_TOKEN_HEADER]: token },
      ...(signal ? { signal } : {}),
    });
    if (response.status === 401 || response.status === 403) {
      await response.body?.cancel();
      return undefined;
    }
    return await this.#answer<SubjectResolution>(response, MAX_RESOLUTION_BYTES);
  }

  async listProjects(who: Represented, maxBytes: number, signal: AbortSignal): Promise<Project[]> {
    const response = await this.#send('project.list', '/api/v1/pontis/projects', {
      method: 'GET',
      headers: representing(who),
      signal,
    });
    return await this.#answer<Project[]>(response, maxBytes);
  }

  /** A project's designs, newest change first, with no documents: what turns a name into an id. */
  async listDesigns(who: Represented, projectId: string, maxBytes: number, signal: AbortSignal): Promise<DesignSummary[]> {
    const response = await this.#send('design.list', `/api/v1/pontis/projects/${encodeURIComponent(projectId)}/designs`, {
      method: 'GET',
      headers: representing(who),
      signal,
    });
    return await this.#answer<DesignSummary[]>(response, maxBytes);
  }

  async getDesign(who: Represented, designId: string, maxBytes: number, signal: AbortSignal): Promise<DesignRecord> {
    const response = await this.#send('design.get', `/api/v1/pontis/designs/${encodeURIComponent(designId)}`, {
      method: 'GET',
      headers: representing(who),
      signal,
    });
    return await this.#answer<DesignRecord>(response, maxBytes);
  }

  /** The manual for this door and its revision (plan §6.1). */
  async guide(who: Represented, maxBytes: number, signal: AbortSignal): Promise<AgentManual> {
    const response = await this.#send('forma.guide', '/api/v1/pontis/guide', {
      method: 'GET',
      headers: representing(who),
      signal,
    });
    return await this.#answer<AgentManual>(response, maxBytes);
  }

  /** A project's Brand Kit with the font catalog the renderer can draw. */
  async getBrand(who: Represented, projectId: string, maxBytes: number, signal: AbortSignal): Promise<PontisBrand> {
    const response = await this.#send('brand.get', `/api/v1/pontis/projects/${encodeURIComponent(projectId)}/brand`, {
      method: 'GET',
      headers: representing(who),
      signal,
    });
    return await this.#answer<PontisBrand>(response, maxBytes);
  }

  /** A write, ledgered like a batch: `invocationId` is its idempotency key. */
  async createAssetRequest(
    who: Represented,
    projectId: string,
    request: PontisAssetRequestBody,
    invocationId: string,
    maxBytes: number,
    signal: AbortSignal,
  ): Promise<AssetRequest> {
    const response = await this.#send(
      'assetRequest.create',
      `/api/v1/pontis/projects/${encodeURIComponent(projectId)}/asset-requests`,
      {
        method: 'POST',
        headers: { ...representing(who), [INVOCATION_HEADER]: invocationId, 'content-type': 'application/json' },
        body: JSON.stringify(request),
        signal,
      },
      invocationId,
    );
    return await this.#answer<AssetRequest>(response, maxBytes);
  }

  /**
   * The cancellable operation. Aborting `signal` drops the request, and Forma
   * stops validating at its next checkpoint (`cancel_on_drop`); `timeoutMs` is
   * the deadline Forma enforces itself, so it answers `timeout` rather than
   * running past what the caller will wait for.
   */
  async validate(
    who: Represented,
    designId: string,
    timeoutMs: number,
    maxBytes: number,
    signal: AbortSignal,
  ): Promise<ValidationReport> {
    const response = await this.#send('design.validate', `/api/v1/pontis/designs/${encodeURIComponent(designId)}/validate`, {
      method: 'POST',
      headers: { ...representing(who), [TIMEOUT_HEADER]: String(timeoutMs) },
      signal,
    });
    return await this.#answer<ValidationReport>(response, maxBytes);
  }

  /** The write. `invocationId` is Forma's idempotency key (§10(b) D6). */
  async applyOperations(
    who: Represented,
    designId: string,
    request: PontisApplyRequest,
    invocationId: string,
    maxBytes: number,
    signal: AbortSignal,
  ): Promise<PontisApplyResult> {
    const response = await this.#send(
      'design.applyOperations',
      `/api/v1/pontis/designs/${encodeURIComponent(designId)}/operations`,
      {
        method: 'POST',
        headers: { ...representing(who), [INVOCATION_HEADER]: invocationId, 'content-type': 'application/json' },
        body: JSON.stringify(request),
        signal,
      },
      invocationId,
    );
    return await this.#answer<PontisApplyResult>(response, maxBytes);
  }

  async #send(
    operation: string,
    path: string,
    init: { method: string; headers: Record<string, string>; body?: string; signal?: AbortSignal },
    invocationId?: string,
  ): Promise<Response> {
    const started = performance.now();
    const fields = { operation, ...(invocationId ? { invocation: invocationId } : {}) };
    // Logged when sent as well as when answered: a call Forma is still
    // working on — or one abandoned before it answered — is otherwise invisible.
    this.#log('boundary_f_request', fields);
    try {
      const response = await fetch(`${this.#origin}${path}`, {
        ...init,
        headers: { ...init.headers, authorization: `Bearer ${this.#credential}` },
        redirect: 'error',
      });
      this.#log('boundary_f', { ...fields, status: response.status, ms: Math.round(performance.now() - started) });
      return response;
    } catch {
      // Never the error's own text: it can quote the request.
      if (init.signal?.aborted) {
        this.#log('boundary_f', { ...fields, outcome: 'aborted', ms: Math.round(performance.now() - started) });
        throw new CapabilityError('timeout', 'the operation was abandoned before Forma answered');
      }
      this.#log('boundary_f', { ...fields, outcome: 'unreachable' });
      throw new CapabilityError('unavailable', 'Forma could not be reached');
    }
  }

  /** The body of a success, or the typed refusal Forma sent instead. */
  async #answer<T>(response: Response, maxBytes: number): Promise<T> {
    if (response.ok) return (await readBoundedJson(response, maxBytes)) as T;
    throw refusal(response.status, await readBoundedJson(response, MAX_ERROR_BYTES).catch(() => undefined));
  }
}

function representing(who: Represented): Record<string, string> {
  return {
    [REPRESENTED_USER_HEADER]: who.userId,
    [APPLICATION_CONTEXT_HEADER]: who.applicationContextId,
  };
}

/**
 * Forma's §32 envelope as a Pontis registry error.
 *
 * Forma already says where a failure sits in the registry — `capability_error`,
 * decided by `routes/pontis.rs::classify` — so this trusts that field and maps
 * nothing by Forma code. Re-deriving it here would be a second classification
 * to drift from the first. The Forma code and its hint travel in `details`,
 * because `VERSION_CONFLICT` (rebase) and `DESIGN_NOT_FOUND` (give up) are
 * different instructions that the registry's single `conflict`/`not_found`
 * would blur.
 *
 * A refusal from the authentication arm runs before any route and carries no
 * `capability_error`; the status decides it then.
 */
export function refusal(status: number, body: unknown): CapabilityError {
  const envelope = formaError(body);
  const code =
    envelope?.capability_error !== undefined && isCapabilityErrorCode(envelope.capability_error)
      ? envelope.capability_error
      : byStatus(status);
  if (code === 'internal_error' || code === 'unavailable') {
    // Forma hides the detail of its own faults; so does this.
    return new CapabilityError(code, code === 'unavailable' ? 'Forma is unavailable' : 'Forma could not complete the operation');
  }
  const details = envelope
    ? {
        forma_code: envelope.code,
        ...(envelope.hint ? { hint: envelope.hint } : {}),
        ...(envelope.details !== undefined ? { details: envelope.details } : {}),
      }
    : undefined;
  return new CapabilityError(code, envelope?.message ?? `Forma refused the operation (${status})`, details);
}

function byStatus(status: number): CapabilityError['code'] {
  if (status === 401 || status === 403) return 'authorization_denied';
  if (status === 404) return 'not_found';
  if (status === 409) return 'conflict';
  if (status === 429) return 'rate_limited';
  if (status === 503) return 'unavailable';
  if (status >= 400 && status < 500) return 'validation_error';
  return 'internal_error';
}

interface FormaErrorBody {
  readonly code: string;
  readonly message?: string;
  readonly hint?: string;
  readonly details?: unknown;
  readonly capability_error?: string;
}

function formaError(body: unknown): FormaErrorBody | undefined {
  if (!isRecord(body) || !isRecord(body.error)) return undefined;
  const { code, message, hint, details, capability_error } = body.error;
  if (typeof code !== 'string') return undefined;
  return {
    code,
    ...(typeof message === 'string' ? { message } : {}),
    ...(typeof hint === 'string' ? { hint } : {}),
    ...(details !== undefined ? { details } : {}),
    ...(typeof capability_error === 'string' ? { capability_error } : {}),
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/**
 * The one place a Boundary F body becomes a value: read under a byte bound,
 * parsed as `unknown`. A body over the bound is refused, never truncated, which
 * is also what Pontis does with a result over `maxResultBytes`.
 */
export async function readBoundedJson(response: Response, maxBytes: number): Promise<unknown> {
  const declared = Number(response.headers.get('content-length'));
  if (Number.isFinite(declared) && declared > maxBytes) {
    await response.body?.cancel();
    throw new CapabilityError('result_too_large', `Forma's answer is over the ${maxBytes}-byte bound`);
  }
  if (!response.body) throw new CapabilityError('internal_error', 'Forma sent no body');
  const chunks: Uint8Array[] = [];
  let total = 0;
  // Leaving the loop early cancels the stream, so an oversized body is never
  // read to its end.
  for await (const chunk of response.body as AsyncIterable<unknown>) {
    if (!(chunk instanceof Uint8Array)) throw new CapabilityError('internal_error', 'Forma sent an unreadable body');
    total += chunk.byteLength;
    if (total > maxBytes) {
      throw new CapabilityError('result_too_large', `Forma's answer is over the ${maxBytes}-byte bound`);
    }
    chunks.push(chunk);
  }
  try {
    const parsed: unknown = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(Buffer.concat(chunks)));
    return parsed;
  } catch {
    throw new CapabilityError('internal_error', 'Forma sent a body that is not JSON');
  }
}
