/**
 * The assets panel and the AssetRequest flow (plan sections 22 and 23).
 *
 * When an agent needs an image that does not exist, it records an AssetRequest
 * instead of stalling or inventing an asset. This is the other half of that: the
 * user sees what was asked for, uploads it, and the request resolves so the
 * agent can carry on.
 */

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

  // Only the pending ones are actionable; a resolved request is history.
  const pending = (requests.data ?? []).filter((r) => r.status === "pending");
  const busy = disabled || upload.isPending || resolve.isPending;
  const error = [upload.error, resolve.error, cancel.error].find(Boolean);

  return (
    <Panel title="Assets" onClose={onClose}>
      {error && <Failure error={error} />}

      {pending.length > 0 && (
        <section className="mb-4">
          <div className="mb-1.5 flex items-center gap-2">
            <span className="h-[7px] w-[7px] rounded-full bg-signal" />
            <span className="text-[10px] text-faint">
              REQUESTED ({pending.length})
            </span>
          </div>

          {pending.map((request) => (
            <Request
              key={request.id}
              request={request}
              busy={busy}
              onUpload={(file) => resolve.mutate({ requestId: request.id, file })}
              onCancel={() => cancel.mutate(request.id)}
            />
          ))}
        </section>
      )}

      <div className="mb-1.5 text-[10px] text-faint">
        LIBRARY ({assets.data?.length ?? 0})
      </div>

      <Drop
        busy={busy}
        label="Drop an image, or click to choose"
        onFile={(file) => upload.mutate(file)}
      />

      {assets.data?.map((asset) => (
        <button
          key={asset.id}
          type="button"
          disabled={busy}
          onClick={() => onPlace(asset)}
          title={`Place ${asset.original_filename ?? "image"} on this page`}
          className="mt-1.5 flex w-full items-center gap-2.5 rounded-lg border border-edge
                     bg-surface-2 p-1.5 text-left transition-colors
                     enabled:hover:border-edge-strong disabled:opacity-40"
        >
          <img
            src={assetContentUrl(asset.id)}
            alt=""
            className="h-9 w-9 shrink-0 rounded object-cover"
          />
          <span className="min-w-0 flex-1">
            <span className="block truncate text-[11px] text-muted">
              {asset.original_filename ?? "image"}
            </span>
            <span className="block text-[10px] text-faint tabular-nums">
              {asset.width}×{asset.height}
            </span>
          </span>
        </button>
      ))}

      {assets.data?.length === 0 && pending.length === 0 && (
        <p className="mt-2 text-[11px] leading-snug text-faint">
          Nothing here yet. Upload an image, or ask your agent for one — it will
          appear above as a request.
        </p>
      )}
    </Panel>
  );
}

/**
 * One outstanding request.
 *
 * The suggested prompt is copyable because Forma deliberately does not generate
 * images (plan section 2.2): the user takes it to whatever tool they prefer and
 * brings the result back.
 */
function Request({
  request,
  busy,
  onUpload,
  onCancel,
}: {
  request: AssetRequest;
  busy: boolean;
  onUpload: (file: File) => void;
  onCancel: () => void;
}) {
  const [copied, setCopied] = useState(false);

  return (
    <div className="mb-2 rounded-lg border border-signal/40 bg-signal/6 p-2.5">
      <div className="text-[11px] font-medium text-paper">{request.purpose}</div>
      <p className="mt-1 text-[11px] leading-snug text-muted">{request.description}</p>

      {request.requested_width && request.requested_height && (
        <div className="mt-1 text-[10px] text-faint tabular-nums">
          Suggested {request.requested_width}×{request.requested_height}
        </div>
      )}

      {request.suggested_prompt && (
        <div className="mt-2">
          <p className="rounded border border-edge bg-surface px-2 py-1.5 text-[10px]
                        leading-snug text-faint">
            {request.suggested_prompt}
          </p>
          <button
            type="button"
            className="mt-1 text-[10px] text-muted hover:text-paper"
            onClick={() => {
              void navigator.clipboard
                ?.writeText(request.suggested_prompt ?? "")
                .then(() => setCopied(true))
                // Clipboard access can be denied; saying nothing would look
                // like the button did nothing at all.
                .catch(() => setCopied(false));
            }}
          >
            {copied ? "Copied" : "Copy prompt"}
          </button>
        </div>
      )}

      <div className="mt-2">
        <Drop busy={busy} label="Drop the image here to resolve" onFile={onUpload} />
      </div>

      <button
        type="button"
        disabled={busy}
        onClick={onCancel}
        className="mt-1.5 text-[10px] text-faint hover:text-muted disabled:opacity-40"
      >
        Cancel request
      </button>
    </div>
  );
}

/** A drop target that is also a file picker, since dragging is not for everyone. */
function Drop({
  busy,
  label,
  onFile,
}: {
  busy: boolean;
  label: string;
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
        className={`w-full rounded-lg border border-dashed px-2.5 py-3 text-[10px]
                    transition-colors disabled:opacity-40 ${
                      over
                        ? "border-signal bg-signal/8 text-paper"
                        : "border-edge text-faint hover:border-edge-strong"
                    }`}
      >
        {busy ? "Uploading…" : label}
      </button>

      <input
        ref={input}
        type="file"
        accept="image/png,image/jpeg,image/webp,image/svg+xml"
        className="hidden"
        onChange={(event) => {
          take(event.target.files);
          // Reset, so choosing the same file twice still fires a change.
          event.target.value = "";
        }}
      />
    </>
  );
}

/** Upload failures carry a server code; showing it beats a generic message. */
function Failure({ error }: { error: Error }) {
  return (
    <div className="mb-2 rounded-lg border border-signal/45 bg-signal/6 px-2.5 py-2 text-[11px]">
      {error instanceof ApiError ? (
        <>
          <code className="text-[10px] text-signal">{error.code}</code>{" "}
          <span className="text-muted">{error.message}</span>
          {error.hint && <span className="mt-0.5 block text-faint">{error.hint}</span>}
        </>
      ) : (
        <span className="text-muted">{error.message}</span>
      )}
    </div>
  );
}
