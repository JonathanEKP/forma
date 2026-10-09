/** Project images, including requests an agent is waiting on. */

import { useRef, useState } from "react";

import { ApiError, assetContentUrl, type Asset, type AssetRequest } from "../api";
import {
  useAssetRequests,
  useAssets,
  useCancelAssetRequest,
  useResolveAssetRequest,
  useUploadAsset,
} from "../queries";
import { Panel } from "./Panels";

const ACCEPT = "image/png,image/jpeg,image/webp,image/gif,image/svg+xml,.png,.jpg,.jpeg,.webp,.gif,.svg";
const FORMATS = "PNG, JPEG, WebP, GIF or SVG";

export function Assets({
  projectId,
  onPlace,
  disabled,
  onClose,
}: {
  projectId: string;
  onPlace: (asset: Asset) => void;
  disabled: boolean;
  onClose?: () => void;
}) {
  const assets = useAssets(projectId);
  const requests = useAssetRequests(projectId);
  const upload = useUploadAsset(projectId);
  const resolve = useResolveAssetRequest(projectId);
  const cancel = useCancelAssetRequest(projectId);
  const pending = (requests.data ?? []).filter((request) => request.status === "pending");
  const busy = disabled || upload.isPending || resolve.isPending || cancel.isPending;
  const error = [upload.error, resolve.error, cancel.error].find(Boolean);

  return (
    <Panel title="Images" onClose={onClose}>
      <div className="space-y-6 px-1 pb-4">
        {error && <Failure error={error} />}

        {pending.length > 0 && (
          <section className="space-y-3 border-b border-edge pb-5">
            <div className="flex items-center gap-2">
              <span className="h-[7px] w-[7px] shrink-0 rounded-full bg-signal" aria-hidden="true" />
              <h3 className="text-xs font-medium text-paper">Requested <span className="text-muted">{pending.length}</span></h3>
            </div>
            <p className="text-xs leading-relaxed text-muted">
              Upload what the agent needs. It will continue on its own.
            </p>
            <div className="space-y-5">
              {pending.map((request) => (
                <Request
                  key={request.id}
                  request={request}
                  busy={busy}
                  uploading={resolve.isPending && resolve.variables?.requestId === request.id}
                  onUpload={(file) => resolve.mutate({ requestId: request.id, file })}
                  onCancel={() => cancel.mutate(request.id)}
                />
              ))}
            </div>
          </section>
        )}

        <section className="space-y-3">
          <h3 className="text-xs font-medium text-paper">Library <span className="text-muted">{assets.data?.length ?? 0}</span></h3>
          <Drop
            busy={disabled || upload.isPending}
            label="Drop an image, or click to choose"
            uploading={upload.isPending}
            onFile={(file) => upload.mutate(file)}
          />
          {upload.isPending && <Uploading />}

          {(assets.data ?? []).length === 0 && pending.length === 0 && !assets.isLoading && (
            <p className="text-xs leading-relaxed text-muted">
              Nothing here yet. Upload an image, or ask your agent for one; it will appear here as a request.
            </p>
          )}

          {(assets.data ?? []).length > 0 && (
            <div className="grid grid-cols-2 gap-3">
              {(assets.data ?? []).map((asset) => (
                <button
                  key={asset.id}
                  type="button"
                  disabled={busy}
                  onClick={() => onPlace(asset)}
                  aria-label={`Place ${asset.original_filename ?? "image"} on this page`}
                  className="group min-w-0 text-left disabled:opacity-40"
                >
                  <span className="mb-2 flex aspect-[4/3] min-w-0 items-center justify-center overflow-hidden rounded-lg bg-surface-2 p-2">
                    <img
                      src={assetContentUrl(asset.id)}
                      alt=""
                      loading="lazy"
                      className="block max-h-full max-w-full object-contain"
                    />
                  </span>
                  <span className="block truncate text-[11px] text-muted group-hover:text-paper">
                    {asset.original_filename ?? "image"}
                  </span>
                  <span className="block font-mono text-[10px] text-faint">
                    {asset.width} × {asset.height}
                  </span>
                </button>
              ))}
            </div>
          )}

          {assets.isLoading && <p className="text-xs text-faint">Loading images…</p>}
          <p className="text-[11px] leading-relaxed text-faint">Choose an image to place it on the current page.</p>
        </section>
      </div>
    </Panel>
  );
}

function Request({
  request,
  busy,
  uploading,
  onUpload,
  onCancel,
}: {
  request: AssetRequest;
  busy: boolean;
  uploading: boolean;
  onUpload: (file: File) => void;
  onCancel: () => void;
}) {
  const [copied, setCopied] = useState(false);
  const [confirmCancel, setConfirmCancel] = useState(false);

  return (
    <article className="space-y-2.5 border-b border-edge pb-5 last:border-0 last:pb-0">
      <div className="flex min-w-0 items-center gap-2">
        <span className="h-[7px] w-[7px] shrink-0 rounded-full bg-signal" aria-hidden="true" />
        <h4 className="min-w-0 flex-1 truncate text-xs font-medium text-paper">{request.purpose}</h4>
        <span className="shrink-0 text-[11px] text-muted">Pending</span>
      </div>
      <p className="text-[11px] leading-relaxed text-muted">{request.description}</p>
      {request.requested_width && request.requested_height && (
        <p className="font-mono text-[10px] text-faint">
          Suggested {request.requested_width} × {request.requested_height}
        </p>
      )}

      {request.suggested_prompt && (
        <details className="text-[11px]">
          <summary className="py-1 text-muted">Suggested prompt</summary>
          <div className="space-y-2 pt-2">
            <p className="whitespace-pre-wrap leading-relaxed text-muted">{request.suggested_prompt}</p>
            <button
              type="button"
              className="text-[11px] text-muted hover:text-paper"
              onClick={() => {
                void navigator.clipboard?.writeText(request.suggested_prompt ?? "")
                  .then(() => setCopied(true))
                  .catch(() => setCopied(false));
              }}
            >
              {copied ? "Copied" : "Copy prompt"}
            </button>
            <p className="text-[10px] leading-relaxed text-faint">
              Forma does not generate images. Use this prompt in any tool you like.
            </p>
          </div>
        </details>
      )}

      {uploading ? (
        <Uploading />
      ) : (
        <Drop busy={busy} label="Drop the image here" onFile={onUpload} />
      )}

      {confirmCancel ? (
        <div className="space-y-2" role="group" aria-label={`Cancel ${request.purpose} request`}>
          <p className="text-[11px] text-muted">Cancel this image request?</p>
          <div className="flex gap-2">
            <button
              type="button"
              disabled={busy}
              onClick={onCancel}
              className="rounded-lg px-2 py-1.5 text-[11px] text-paper hover:bg-surface-2 disabled:opacity-40"
            >
              Confirm cancellation
            </button>
            <button
              type="button"
              onClick={() => setConfirmCancel(false)}
              className="rounded-lg px-2 py-1.5 text-[11px] text-muted hover:text-paper"
            >
              Keep request
            </button>
          </div>
        </div>
      ) : (
        <button
          type="button"
          disabled={busy}
          onClick={() => setConfirmCancel(true)}
          className="min-h-8 px-0 text-left text-[11px] text-muted hover:text-paper disabled:opacity-40"
        >
          Cancel request
        </button>
      )}
    </article>
  );
}

function Drop({
  busy,
  label,
  uploading = false,
  onFile,
}: {
  busy: boolean;
  label: string;
  uploading?: boolean;
  onFile: (file: File) => void;
}) {
  const input = useRef<HTMLInputElement>(null);
  const [over, setOver] = useState(false);
  const take = (files: FileList | null) => {
    const file = files?.[0];
    if (file) onFile(file);
  };

  return (
    <>
      <button
        type="button"
        disabled={busy}
        aria-label={uploading ? "Uploading image" : label}
        aria-busy={uploading}
        data-drag-over={over ? "true" : "false"}
        onClick={() => input.current?.click()}
        onDragOver={(event) => {
          event.preventDefault();
          setOver(true);
        }}
        onDragLeave={() => setOver(false)}
        onDrop={(event) => {
          event.preventDefault();
          setOver(false);
          if (!busy) take(event.dataTransfer.files);
        }}
        className={`grid w-full min-h-[76px] place-content-center gap-1.5 rounded-lg border border-dashed px-3 py-3 text-center text-[11px] transition-colors disabled:opacity-40 ${
          over ? "border-paper bg-surface-2 text-paper" : "border-edge-strong text-muted hover:border-paper"
        }`}
      >
        <span>{uploading ? "Uploading…" : label}</span>
        <span className="text-[10px] text-faint">{FORMATS}</span>
      </button>
      <input
        ref={input}
        type="file"
        accept={ACCEPT}
        aria-label="Choose an image to upload"
        className="hidden"
        onChange={(event) => {
          take(event.target.files);
          event.target.value = "";
        }}
      />
    </>
  );
}

function Uploading() {
  return (
    <div role="status" className="grid gap-2 py-2 text-[11px] text-muted">
      <span>Uploading…</span>
      <progress aria-label="Uploading image" className="h-1 w-full accent-signal" />
    </div>
  );
}

function Failure({ error }: { error: Error }) {
  return (
    <div role="alert" className="rounded-lg border border-signal/45 bg-signal/6 px-2.5 py-2 text-[11px]">
      {error instanceof ApiError ? (
        <>
          <code className="text-[10px] text-signal">{error.code}</code>{" "}
          <span className="text-muted">{error.message}</span>
          {error.hint && <span className="mt-1 block text-faint">{error.hint}</span>}
        </>
      ) : <span className="text-muted">{error.message}</span>}
    </div>
  );
}
