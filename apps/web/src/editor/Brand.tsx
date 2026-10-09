/** The project Brand Kit form, shared by the editor and projects home. */

import { useEffect, useRef, useState } from "react";

import { ApiError, type BrandKit, type BrandKitDraft } from "../api";
import { useAssets, useBrand, useProject, usePutBrand, useStatus } from "../queries";

const COLORS = [
  ["primary", "Primary"],
  ["secondary", "Secondary"],
  ["accent", "Accent"],
  ["background", "Background"],
  ["surface", "Surface"],
  ["text", "Text"],
  ["text_inverse", "Text on dark"],
  ["muted", "Muted"],
] as const;

function blank(fonts: string[]): BrandKitDraft {
  const pick = (preferred: string) => fonts.find((font) => font === preferred) ?? fonts[0] ?? "";
  const font = pick("Geist");
  return {
    name: "Default",
    colors: {
      primary: "#2563EB",
      secondary: "#F59E0B",
      accent: "#7C3AED",
      background: "#0B0B0C",
      surface: "#17171A",
      text: "#FFFFFF",
      text_inverse: "#0B0B0C",
      muted: "#A1A1AA",
    },
    typography: { display: font, body: font, mono: pick("Geist Mono") },
    logo_asset_id: null,
    voice: "",
    rules: { safe_margin: 80, minimum_font_size: 24 },
  };
}

function toDraft(brand: BrandKit): BrandKitDraft {
  return {
    name: brand.name,
    colors: brand.colors,
    typography: brand.typography,
    logo_asset_id: brand.logo_asset_id ?? null,
    voice: brand.voice,
    rules: brand.rules,
  };
}

export function Brand({ projectId }: { projectId: string }) {
  const status = useStatus();
  const project = useProject(projectId);
  const assets = useAssets(projectId);
  const brand = useBrand(projectId);
  const save = usePutBrand(projectId);
  const [draft, setDraft] = useState<BrandKitDraft | null>(null);
  const draftProject = useRef(projectId);

  useEffect(() => {
    if (draftProject.current === projectId) return;
    draftProject.current = projectId;
    setDraft(null);
  }, [projectId]);

  // A saved kit is the starting form state. An empty project stays on its
  // explanation until the person explicitly chooses to set one up.
  useEffect(() => {
    if (brand.isSuccess && brand.data && draft === null) setDraft(toDraft(brand.data));
  }, [brand.data, brand.isSuccess, draft]);

  const fonts = status.data?.fonts ?? [];
  if (brand.isLoading || status.isLoading || project.isLoading) {
    return <p className="px-1 py-2 text-xs text-faint">Loading…</p>;
  }

  if (draft === null) {
    return (
      <div className="space-y-3 px-1 py-2">
        <h2 className="text-sm font-medium text-paper">No Brand Kit yet</h2>
        <p className="text-xs leading-relaxed text-muted">
          A Brand Kit keeps this project’s colours, fonts and voice together.
        </p>
        <p className="text-xs leading-relaxed text-muted">
          Designs that use brand colours or fonts need a kit before they can render.
        </p>
        <button
          type="button"
          onClick={() => setDraft(blank(fonts))}
          className="w-full rounded-lg bg-signal px-2.5 py-2 text-xs font-semibold text-ink"
        >
          Set up Brand Kit
        </button>
      </div>
    );
  }

  const update = (patch: Partial<BrandKitDraft>) => setDraft({ ...draft, ...patch });
  const dirty = brand.data === undefined || brand.data === null
    ? true
    : JSON.stringify(toDraft(brand.data)) !== JSON.stringify(draft);

  return (
    <form
      className="space-y-6 px-1 pb-2"
      onSubmit={(event) => {
        event.preventDefault();
        if (!dirty || !event.currentTarget.reportValidity()) return;
        save.mutate(draft);
      }}
    >
      {save.error instanceof ApiError && (
        <div role="alert" className="rounded-lg border border-signal/45 bg-signal/6 px-2.5 py-2 text-[11px]">
          <code className="text-[10px] text-signal">{save.error.code}</code>{" "}
          <span className="text-muted">{save.error.message}</span>
          {save.error.hint && <span className="mt-1 block text-faint">{save.error.hint}</span>}
        </div>
      )}

      <section className="space-y-2 border-b border-edge pb-5">
        <label className="block text-xs text-muted" htmlFor="brand-kit-name">Kit name</label>
        <input
          id="brand-kit-name"
          name="name"
          required
          maxLength={80}
          value={draft.name}
          onChange={(event) => update({ name: event.target.value })}
          className="w-full rounded-lg border border-edge bg-surface-2 px-2.5 py-2 text-xs text-paper outline-none focus:border-signal"
        />
        <p className="text-[11px] text-faint">
          Shared by every design in {project.data?.name ?? "this project"}.
        </p>
      </section>

      <section className="space-y-3 border-b border-edge pb-5" aria-labelledby="brand-colours-heading">
        <h3 id="brand-colours-heading" className="text-xs font-medium text-paper">Colours</h3>
        <div className="grid grid-cols-2 gap-x-3 gap-y-3">
          {COLORS.map(([key, label]) => (
            <div key={key} className="grid min-w-0 grid-cols-[32px_minmax(0,1fr)] items-end gap-x-2 gap-y-1.5">
              <label htmlFor={`brand-color-${key}`} className="col-span-2 text-[11px] text-muted">{label}</label>
              <input
                id={`brand-color-${key}`}
                type="color"
                value={draft.colors[key].length === 7 ? draft.colors[key] : draft.colors[key].slice(0, 7)}
                onChange={(event) => update({ colors: { ...draft.colors, [key]: event.target.value.toUpperCase() } })}
                aria-label={`Choose ${label.toLowerCase()} colour`}
                className="h-9 w-8 cursor-pointer rounded border border-edge-strong bg-transparent p-1"
              />
              <input
                type="text"
                value={draft.colors[key]}
                onChange={(event) => update({ colors: { ...draft.colors, [key]: event.target.value.toUpperCase() } })}
                aria-label={`${label} hex colour`}
                pattern="#[0-9a-fA-F]{6}"
                maxLength={7}
                required
                spellCheck={false}
                className="min-w-0 rounded-lg border border-edge bg-surface-2 px-1.5 py-2 text-right font-mono text-[10px] text-paper outline-none focus:border-signal"
              />
            </div>
          ))}
        </div>
      </section>

      <section className="space-y-3 border-b border-edge pb-5" aria-labelledby="brand-type-heading">
        <h3 id="brand-type-heading" className="text-xs font-medium text-paper">Type</h3>
        {(["display", "body", "mono"] as const).map((slot) => (
          <label key={slot} className="block space-y-1.5 text-[11px] text-muted">
            <span>{slot === "display" ? "Display font" : slot === "body" ? "Body font" : "Mono font"}</span>
            <select
              value={draft.typography[slot]}
              onChange={(event) => update({ typography: { ...draft.typography, [slot]: event.target.value } })}
              className="w-full rounded-lg border border-edge bg-surface-2 px-2.5 py-2 text-xs text-paper outline-none focus:border-signal"
            >
              {fonts.map((font) => <option key={font} value={font}>{font}</option>)}
            </select>
          </label>
        ))}
        <p className="text-[11px] text-faint">Choose from fonts installed in Forma.</p>
      </section>

      <section className="space-y-2 border-b border-edge pb-5">
        <label htmlFor="brand-logo" className="block text-xs text-muted">Logo</label>
        <select
          id="brand-logo"
          value={draft.logo_asset_id ?? ""}
          onChange={(event) => update({ logo_asset_id: event.target.value || null })}
          className="w-full rounded-lg border border-edge bg-surface-2 px-2.5 py-2 text-xs text-paper outline-none focus:border-signal"
        >
          <option value="">No logo</option>
          {(assets.data ?? []).map((asset) => (
            <option key={asset.id} value={asset.id}>{asset.original_filename ?? "image"}</option>
          ))}
        </select>
        <p className="text-[11px] text-faint">Optional · choose an image from this project.</p>
      </section>

      <section className="space-y-3 border-b border-edge pb-5" aria-labelledby="brand-rules-heading">
        <h3 id="brand-rules-heading" className="text-xs font-medium text-paper">Rules</h3>
        <div className="grid grid-cols-2 gap-3">
          <NumberField
            label="Safe margin · px"
            value={draft.rules.safe_margin}
            max={540}
            onChange={(safe_margin) => update({ rules: { ...draft.rules, safe_margin } })}
          />
          <NumberField
            label="Minimum font size · px"
            value={draft.rules.minimum_font_size}
            min={1}
            max={200}
            onChange={(minimum_font_size) => update({ rules: { ...draft.rules, minimum_font_size } })}
          />
        </div>
      </section>

      <section className="space-y-2">
        <label htmlFor="brand-voice" className="block text-xs text-muted">Voice</label>
        <textarea
          id="brand-voice"
          value={draft.voice}
          maxLength={600}
          rows={4}
          onChange={(event) => update({ voice: event.target.value })}
          className="w-full resize-y rounded-lg border border-edge bg-surface-2 px-2.5 py-2 text-xs leading-relaxed text-paper outline-none focus:border-signal"
        />
        <p className="text-[11px] text-faint">The agent reads this before writing copy.</p>
      </section>

      <section className="sticky bottom-0 space-y-2 border-t border-edge bg-ink py-4">
        <p className="text-[11px] leading-relaxed text-muted">
          Saving changes how every design in this project looks, right away.
        </p>
        <p role="status" className="min-h-4 text-[11px] text-faint">
          {save.isPending ? "Saving changes…" : dirty ? "Unsaved changes" : "All changes saved"}
        </p>
        <button
          type="submit"
          disabled={save.isPending || !dirty}
          className={`w-full rounded-lg px-2.5 py-2 text-xs font-semibold disabled:cursor-default disabled:opacity-40 ${
            dirty ? "bg-signal text-ink" : "bg-transparent text-muted"
          }`}
        >
          {save.isPending ? "Saving…" : "Save Brand Kit"}
        </button>
      </section>
    </form>
  );
}

function NumberField({
  label,
  value,
  min = 0,
  max,
  onChange,
}: {
  label: string;
  value: number;
  min?: number;
  max: number;
  onChange: (value: number) => void;
}) {
  return (
    <label className="block min-w-0 space-y-1.5 text-[11px] text-muted">
      <span>{label}</span>
      <input
        type="number"
        value={value}
        min={min}
        max={max}
        required
        onChange={(event) => {
          const parsed = Number.parseFloat(event.target.value);
          if (Number.isFinite(parsed)) onChange(parsed);
        }}
        className="w-full min-w-0 rounded-lg border border-edge bg-surface-2 px-2 py-2 text-xs text-paper outline-none focus:border-signal"
      />
    </label>
  );
}
