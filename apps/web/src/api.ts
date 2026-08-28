/**
 * Thin client for forma-server.
 *
 * Response shapes are generated from Rust (plan section 30) and never
 * redeclared here: CI fails if the checked-in bindings drift.
 */

export type { Status } from "@forma/bindings/Status";

export interface ApiErrorBody {
  code: string;
  message: string;
  details?: unknown;
}

export class ApiError extends Error {
  readonly code: string;

  constructor(body: ApiErrorBody) {
    super(body.message);
    this.code = body.code;
  }
}

/**
 * The one place where untyped data enters the app.
 *
 * `Response.json()` is typed as `any` by the DOM lib, which would let `any`
 * spread silently through everything downstream. It is funnelled through
 * `unknown` here and asserted exactly once, against a shape generated from
 * Rust. Nothing outside this function ever handles an unasserted payload.
 */
async function readJson(response: Response): Promise<unknown> {
  return (await response.json()) as unknown;
}

async function request<T>(path: string): Promise<T> {
  const response = await fetch(`/api/v1${path}`);

  if (!response.ok) {
    // Plan section 32: the error shape is stable, so it is parsed, not guessed.
    const payload = (await readJson(response).catch(() => null)) as {
      error?: ApiErrorBody;
    } | null;
    throw new ApiError(
      payload?.error ?? { code: "SERVER_ERROR", message: `HTTP ${response.status}` },
    );
  }

  return (await readJson(response)) as T;
}

import type { Status } from "@forma/bindings/Status";

export const getStatus = () => request<Status>("/status");
