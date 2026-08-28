/**
 * The Brand Kit panel (plan section 77, Phase 6, and section 13).
 *
 * This is where a project gets a brand, which is the step everything else
 * assumes: `$brand.*` tokens resolve against it, the validator judges the safe
 * margin and the minimum font size by its rules, and the agent is told to read
 * it before writing a word.
 *
 * Fonts are chosen from the catalog the renderer reports rather than typed. A
 * family Forma cannot draw is a FONT_NOT_FOUND at render time, and a text field
 * would invite exactly that.
 */

import { useEffect, useState } from "react";

import { ApiError, type BrandKit, type BrandKitDraft } from "../api";
import { useBrand, usePutBrand, useStatus } from "../queries";

/** The tokens, in the plan's own spelling. `text_inverse` is `$brand.textInverse`. */
const COLORS = [
  ["primary", "$brand.primary"],
  ["secondary", "$brand.secondary"],
  ["accent", "$brand.accent"],
  ["background", "$brand.background"],
  ["surface", "$brand.surface"],
  ["text", "$brand.text"],
  ["text_inverse", "$brand.textInverse"],
  ["muted", "$brand.muted"],
] as const;

/**
 * A starting point for a project that has none.
 *
 * Deliberately neutral rather than Forma's own palette: this is the *user's*
 * brand, and shipping our orange as their default would put it in their work.
 */
function blank(fonts: string[]): BrandKitDraft {
  const font = fonts[0] ?? "";
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
    typography: { display: font, body: font, mono: font },
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
  const brand = useBrand(projectId);
  const save = usePutBrand(projectId);

  const fonts = status.data?.fonts ?? [];
  const [draft, setDraft] = useState<BrandKitDraft | null>(null);

  // The server's copy wins until the user starts editing, so opening the panel
  // on a project that already has a kit shows that kit rather than a blank one.
  useEffect(() => {
    if (brand.data && draft === null) setDraft(toDraft(brand.data));
  }, [brand.data, draft]);

  if (brand.isLoading || status.isLoading) {
    return <p className="px-1 py-2 text-xs text-faint">Loading…</p>;
  }

  if (draft === null) {
    return (
      <div className="px-1">
        <p className="mb-3 text-[11px] leading-snug text-muted">
          This project has no Brand Kit. Until it has one, <code>$brand.*</code>{" "}
          tokens cannot resolve and a design that uses them will not render.
        </p>
        <button
          type="button"
          onClick={() => setDraft(blank(fonts))}
          className="w-full rounded-lg bg-signal px-2.5 py-2 text-xs font-semibold text-ink"
        >
          Create a Brand Kit
        </button>
      </div>
    );
  }

  const set = (patch: Partial<BrandKitDraft>) => setDraft({ ...draft, ...patch });
  const dirty = brand.data === undefined || brand.data === null
    ? true
    : JSON.stringify(toDraft(brand.data)) !== JSON.stringify(draft);

  return (
    <div className="px-1">
      {save.error instanceof ApiError && (
        <div className="mb-2.5 rounded-lg border border-signal/45 bg-signal/6 px-2.5 py-2 text-[11px]">
          <code className="text-[10px] text-signal">{save.error.code}</code>{" "}
          <span className="text-muted">{save.error.message}</span>
        </div>
      )}

      <Field label="Name">
        <input
          value={draft.name}
          onChange={(event) => set({ name: event.target.value })}
          className="w-full bg-transparent text-xs text-paper outline-none"
        />
      </Field>

      <div className="mb-1.5 mt-3.5 text-[10px] text-faint">COLOURS</div>
      {COLORS.map(([key, token]) => (
        <Colour
          key={key}
          token={token}
          value={draft.colors[key]}
          onChange={(value) => set({ colors: { ...draft.colors, [key]: value } })}
        />
      ))}

      <div className="mb-1.5 mt-3.5 text-[10px] text-faint">TYPE</div>
      {(["display", "body", "mono"] as const).map((slot) => (
        <Field key={slot} label={`$brand.${slot}`}>
          <select
            value={draft.typography[slot]}
            onChange={(event) =>
              set({ typography: { ...draft.typography, [slot]: event.target.value } })
            }
            className="w-full bg-transparent text-xs text-paper outline-none"
          >
            {/* Only what the renderer can actually draw. */}
            {fonts.map((font) => (
              <option key={font} value={font} className="bg-surface-2">
                {font}
              </option>
            ))}
          </select>
        </Field>
      ))}

      <div className="mb-1.5 mt-3.5 text-[10px] text-faint">RULES</div>
      <Field label="Safe margin">
        <NumberField
          value={draft.rules.safe_margin}
          onChange={(safe_margin) => set({ rules: { ...draft.rules, safe_margin } })}
        />
      </Field>
      <Field label="Minimum font size">
        <NumberField
          value={draft.rules.minimum_font_size}
          onChange={(minimum_font_size) =>
            set({ rules: { ...draft.rules, minimum_font_size } })
          }
        />
      </Field>

      <div className="mb-1.5 mt-3.5 text-[10px] text-faint">VOICE</div>
      <textarea
        value={draft.voice}
        rows={3}
        placeholder="Direct, technical and brief. No hype."
        onChange={(event) => set({ voice: event.target.value })}
        className="w-full rounded-lg border border-edge bg-surface-2 px-2.5 py-2 text-[11px]
                   leading-snug text-paper outline-none placeholder:text-faint
                   focus:border-edge-strong"
      />
      {/* Section 51.4: the agent is told this outranks its editorial defaults. */}
      <p className="mt-1 text-[10px] leading-snug text-faint">
        An instruction to the agent. It takes precedence over its own defaults.
      </p>

      <button
        type="button"
        disabled={save.isPending || !dirty}
        onClick={() => save.mutate(draft)}
        className="mt-3.5 w-full rounded-lg bg-signal px-2.5 py-2 text-xs font-semibold
                   text-ink disabled:opacity-40"
      >
        {save.isPending ? "Saving…" : dirty ? "Save Brand Kit" : "Saved"}
      </button>
    </div>
  );
}

function Colour({
  token,
  value,
  onChange,
}: {
  token: string;
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <div className="mb-1 flex items-center gap-2 rounded-lg border border-edge bg-surface-2 px-2 py-1.5">
      <input
        type="color"
        // A colour input only speaks #rrggbb, so an alpha the agent set is not
        // shown here; the text field beside it keeps whatever was stored.
        value={value.length === 7 ? value : value.slice(0, 7)}
        onChange={(event) => onChange(event.target.value.toUpperCase())}
        className="h-5 w-5 shrink-0 cursor-pointer rounded border-0 bg-transparent p-0"
        aria-label={token}
      />
      <code className="flex-1 truncate text-[10px] text-faint">{token}</code>
      <input
        value={value}
        onChange={(event) => onChange(event.target.value.toUpperCase())}
        spellCheck={false}
        className="w-[70px] bg-transparent text-right font-mono text-[10px] text-muted outline-none"
      />
    </div>
  );
}

function NumberField({ value, onChange }: { value: number; onChange: (value: number) => void }) {
  return (
    <input
      type="number"
      value={value}
      min={0}
      onChange={(event) => {
        const parsed = Number.parseFloat(event.target.value);
        // An empty field would otherwise send NaN, which the server rejects
        // with a code the user did not cause.
        if (!globalThis.isNaN(parsed)) onChange(parsed);
      }}
      className="w-full bg-transparent text-right text-xs text-paper outline-none"
    />
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="mb-1 flex items-center gap-2 rounded-lg border border-edge bg-surface-2 px-2.5 py-1.5">
      <span className="shrink-0 text-[10px] text-faint">{label}</span>
      <div className="min-w-0 flex-1 text-right">{children}</div>
    </div>
  );
}
