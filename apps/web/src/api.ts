/**
 * Thin client for forma-server. Types here are hand-written only for Phase 0;
 * from Phase 1 on they come from the Rust bindings (plan section 30).
 */

export interface Status {
  name: string;
  version: string;
  schema_version: number;
  database: string;
  fonts: string[];
  projects: number;
}

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

async function request<T>(path: string): Promise<T> {
  const response = await fetch(`/api/v1${path}`);
  if (!response.ok) {
    // Plan section 32: the error shape is stable, so it is parsed, not guessed.
    const body = (await response.json().catch(() => null)) as { error?: ApiErrorBody } | null;
    throw new ApiError(
      body?.error ?? { code: "SERVER_ERROR", message: `HTTP ${response.status}` },
    );
  }
  return (await response.json()) as T;
}

export const getStatus = () => request<Status>("/status");
