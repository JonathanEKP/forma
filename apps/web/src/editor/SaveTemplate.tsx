import { useState } from "react";

import { ApiError } from "../api";
import { useCreateTemplate } from "../queries";

export function SaveAsTemplateButton({
  projectId,
  designId,
  designName,
}: {
  projectId: string;
  designId: string;
  designName: string;
}) {
  const save = useCreateTemplate(projectId, designId);
  const [open, setOpen] = useState(false);
  const [name, setName] = useState(designName);
  const [saved, setSaved] = useState(false);

  return (
    <div className="relative min-w-0">
      <button
        type="button"
        onClick={() => {
          setSaved(false);
          setOpen((was) => !was);
        }}
        aria-expanded={open}
        aria-controls="save-template-form"
        className="editor-template-button min-h-8 whitespace-nowrap rounded-lg px-2.5 py-1.5 text-[11px] text-muted hover:text-paper"
      >
        Save as template
      </button>
      {open && (
        <form
          id="save-template-form"
          className="editor-template-form absolute right-0 top-[calc(100%+8px)] z-30 grid w-64 gap-2 rounded-xl border border-edge-strong bg-surface p-3 shadow-lg"
          onSubmit={(event) => {
            event.preventDefault();
            const cleanName = name.trim();
            if (!cleanName) return;
            save.mutate(cleanName, {
              onSuccess: () => {
                setOpen(false);
                setSaved(true);
              },
            });
          }}
        >
          <label htmlFor="template-name" className="text-[11px] text-muted">Template name</label>
          <input
            id="template-name"
            required
            maxLength={80}
            value={name}
            onChange={(event) => setName(event.target.value)}
            className="w-full rounded-lg border border-edge bg-surface-2 px-2.5 py-2 text-xs text-paper outline-none focus:border-signal"
            autoFocus
          />
          {save.error instanceof ApiError && (
            <p role="alert" className="text-[11px] text-muted">
              <code className="text-signal">{save.error.code}</code> {save.error.message}
            </p>
          )}
          <div className="flex justify-end gap-2">
            <button
              type="button"
              onClick={() => setOpen(false)}
              className="rounded-lg px-2.5 py-1.5 text-[11px] text-muted hover:text-paper"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={save.isPending || !name.trim()}
              className="rounded-lg bg-signal px-2.5 py-1.5 text-[11px] font-semibold text-ink disabled:opacity-40"
            >
              {save.isPending ? "Saving…" : "Save template"}
            </button>
          </div>
        </form>
      )}
      {saved && <span role="status" className="absolute right-0 top-full mt-1 whitespace-nowrap text-[10px] text-muted">Template saved.</span>}
    </div>
  );
}
