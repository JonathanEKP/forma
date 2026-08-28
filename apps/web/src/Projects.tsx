/**
 * The project view: pick a project, then a design (plan section 77, Phase 6).
 */

import { useState } from "react";

import { Brand } from "./editor/Brand";
import { useBrand, useCreateDesign, useCreateProject, useDesigns, useProjects, useStatus } from "./queries";

export function Projects({ onOpen }: { onOpen: (designId: string) => void }) {
  const status = useStatus();
  const projects = useProjects();
  const [projectId, setProjectId] = useState<string | null>(null);
  const designs = useDesigns(projectId);

  const createProject = useCreateProject();
  const createDesign = useCreateDesign(projectId);
  const brand = useBrand(projectId);
  const [showBrand, setShowBrand] = useState(false);

  if (status.isError) {
    return (
      <Centered>
        <p className="mb-1 font-medium text-paper">forma-server is not reachable.</p>
        <p className="text-muted">Start it and reload; the editor talks to nothing else.</p>
      </Centered>
    );
  }

  return (
    <div className="mx-auto w-full max-w-4xl px-6 py-12">
      <header className="mb-10">
        <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.12em] text-signal">
          Forma
        </div>
        <h1 className="text-4xl font-medium tracking-tight">Give visual ideas a form.</h1>
        {status.data && (
          <p className="mt-3 text-xs text-faint">
            {status.data.name} {status.data.version} · schema v
            {status.data.schema_version} · fonts: {status.data.fonts.join(", ")}
          </p>
        )}
      </header>

      <Section
        title="Projects"
        action={
          <NameForm
            placeholder="New project"
            pending={createProject.isPending}
            onSubmit={(name) => createProject.mutate(name)}
          />
        }
      >
        {projects.data?.length === 0 && <Empty>No projects yet. Create one above.</Empty>}
        <div className="grid gap-1.5 sm:grid-cols-2">
          {projects.data?.map((project) => (
            <button
              key={project.id}
              type="button"
              onClick={() => setProjectId(project.id)}
              className={`rounded-xl border px-3.5 py-3 text-left transition-colors ${
                project.id === projectId
                  ? "border-signal/45 bg-signal/8"
                  : "border-edge bg-surface hover:border-edge-strong"
              }`}
            >
              <div className="text-sm">{project.name}</div>
              <div className="mt-0.5 text-[11px] text-faint">
                {project.created_at.slice(0, 10)}
              </div>
            </button>
          ))}
        </div>
      </Section>

      {/* Plan section 79 configures the Brand Kit before any design exists, so
          it has to be reachable here and not only inside the editor. */}
      {projectId && (
        <Section
          title="Brand Kit"
          action={
            <button
              type="button"
              onClick={() => setShowBrand((open) => !open)}
              className="rounded-lg border border-edge bg-surface-2 px-2.5 py-1.5 text-xs
                         text-muted hover:text-paper"
            >
              {showBrand ? "Close" : brand.data ? "Edit" : "Set up"}
            </button>
          }
        >
          {!showBrand &&
            (brand.data ? (
              <div className="flex items-center gap-2.5 rounded-xl border border-edge bg-surface px-3.5 py-3">
                <div className="flex gap-1">
                  {[
                    brand.data.colors.primary,
                    brand.data.colors.secondary,
                    brand.data.colors.accent,
                    brand.data.colors.text,
                  ].map((colour) => (
                    <span
                      key={colour}
                      className="h-4 w-4 rounded border border-edge"
                      style={{ background: colour }}
                    />
                  ))}
                </div>
                <span className="text-sm">{brand.data.name}</span>
                <span className="text-[11px] text-faint">
                  {brand.data.typography.display}
                </span>
              </div>
            ) : (
              <Empty>
                No Brand Kit yet. Until this project has one, $brand.* tokens cannot
                resolve and a design that uses them will not render.
              </Empty>
            ))}

          {showBrand && (
            <div className="max-w-sm rounded-xl border border-edge bg-surface p-3">
              <Brand projectId={projectId} />
            </div>
          )}
        </Section>
      )}

      {projectId && (
        <Section
          title="Designs"
          action={
            <NameForm
              placeholder="New design"
              pending={createDesign.isPending}
              onSubmit={(name) =>
                createDesign.mutate(name, { onSuccess: (record) => onOpen(record.id) })
              }
            />
          }
        >
          {designs.data?.length === 0 && (
            <Empty>Nothing here yet. A new design starts with one blank page.</Empty>
          )}
          <div className="grid gap-1.5 sm:grid-cols-2">
            {designs.data?.map((design) => (
              <button
                key={design.id}
                type="button"
                onClick={() => onOpen(design.id)}
                className="rounded-xl border border-edge bg-surface px-3.5 py-3 text-left
                           transition-colors hover:border-edge-strong"
              >
                <div className="text-sm">{design.name}</div>
                <div className="mt-0.5 text-[11px] text-faint tabular-nums">
                  v{design.version} · {design.page_count} page
                  {design.page_count === 1 ? "" : "s"} · {Math.round(design.format.width)}×
                  {Math.round(design.format.height)}
                </div>
              </button>
            ))}
          </div>
        </Section>
      )}
    </div>
  );
}

function Section({
  title,
  action,
  children,
}: {
  title: string;
  action: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section className="mb-10">
      <div className="mb-3 flex items-center justify-between gap-4">
        <h2 className="text-[11px] uppercase tracking-[0.11em] text-faint">{title}</h2>
        {action}
      </div>
      {children}
    </section>
  );
}

function NameForm({
  placeholder,
  pending,
  onSubmit,
}: {
  placeholder: string;
  pending: boolean;
  onSubmit: (name: string) => void;
}) {
  const [name, setName] = useState("");

  return (
    <form
      className="flex gap-1.5"
      onSubmit={(event) => {
        event.preventDefault();
        const trimmed = name.trim();
        // The server rejects a blank name; not sending it is friendlier than
        // showing the rejection.
        if (trimmed === "") return;
        onSubmit(trimmed);
        setName("");
      }}
    >
      <input
        value={name}
        onChange={(event) => setName(event.target.value)}
        placeholder={placeholder}
        aria-label={placeholder}
        className="rounded-lg border border-edge bg-surface-2 px-2.5 py-1.5 text-xs
                   text-paper outline-none placeholder:text-faint focus:border-signal"
      />
      <button
        type="submit"
        disabled={pending || name.trim() === ""}
        className="rounded-lg bg-signal px-2.5 py-1.5 text-xs font-semibold text-ink
                   disabled:opacity-40"
      >
        Create
      </button>
    </form>
  );
}

function Empty({ children }: { children: React.ReactNode }) {
  return <p className="text-xs text-faint">{children}</p>;
}

function Centered({ children }: { children: React.ReactNode }) {
  return (
    <div className="grid h-full place-items-center p-10 text-center text-sm">
      <div>{children}</div>
    </div>
  );
}
