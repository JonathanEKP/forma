/** The projects home: a project is the frame around its brand, designs and images. */

import { useEffect, useRef, useState, type FormEvent, type ReactNode } from "react";

import type { FormatPreset } from "@forma/bindings/FormatPreset";

import { fetchPreviewPng, type AssetRequest, type DesignSummary, type Project, type Template } from "./api";
import { Brand } from "./editor/Brand";
import {
  useAssets,
  useAssetRequests,
  useBrand,
  useCreateDesignFromTemplate,
  useCreateDesignWithFormat,
  useCreateProject,
  useDeleteDesign,
  useDeleteProject,
  useDesigns,
  useProjects,
  useStatus,
  useTemplates,
} from "./queries";

const FORMAT_CHOICES: readonly { readonly preset: FormatPreset; readonly label: string }[] = [
  { preset: "square", label: "Square" },
  { preset: "instagram_portrait", label: "Portrait" },
  { preset: "story", label: "Story" },
  { preset: "landscape", label: "Landscape" },
  { preset: "custom", label: "Custom" },
];

type InitialTool = "layers" | "images" | "brand";

export function Projects({
  onOpen,
  onProjectChange,
  selectedProjectId = null,
  chat = null,
}: {
  onOpen: (designId: string, initialTool?: InitialTool) => void;
  onProjectChange?: (project: Pick<Project, "id" | "name"> | null) => void;
  selectedProjectId?: string | null;
  chat?: ReactNode;
}) {
  const status = useStatus();
  const projects = useProjects();
  const createProject = useCreateProject();
  const deleteProject = useDeleteProject();
  const [projectId, setProjectId] = useState<string | null>(selectedProjectId);
  const [showProjectForm, setShowProjectForm] = useState(false);
  const [projectName, setProjectName] = useState("");
  const [confirmProject, setConfirmProject] = useState(false);
  const [confirmProjectName, setConfirmProjectName] = useState("");
  const [showBrand, setShowBrand] = useState(false);
  const [showNewDesign, setShowNewDesign] = useState(false);
  const [templateToUse, setTemplateToUse] = useState("");
  const [confirmDesignId, setConfirmDesignId] = useState<string | null>(null);
  const projectNameRef = useRef<HTMLInputElement>(null);

  const projectList = projects.data ?? [];
  const selectedProject = projectList.find((item) => item.id === projectId) ?? null;
  const designs = useDesigns(selectedProject?.id ?? null);
  const brand = useBrand(selectedProject?.id ?? null);
  const templates = useTemplates(selectedProject?.id ?? null);
  const assets = useAssets(selectedProject?.id ?? null);
  const requests = useAssetRequests(selectedProject?.id ?? null);
  const deleteDesign = useDeleteDesign(selectedProject?.id ?? null);

  useEffect(() => {
    if (selectedProjectId !== null && selectedProjectId !== projectId) {
      setProjectId(selectedProjectId);
    }
  }, [projectId, selectedProjectId]);

  useEffect(() => {
    if (projectId !== null) return;
    const first = projectList[0];
    if (!first) return;
    setProjectId(first.id);
    onProjectChange?.({ id: first.id, name: first.name });
  }, [onProjectChange, projectId, projectList]);

  const selectProject = (nextId: string) => {
    const next = projectList.find((item) => item.id === nextId) ?? null;
    setProjectId(next?.id ?? null);
    setShowProjectForm(false);
    setConfirmProject(false);
    setConfirmDesignId(null);
    setShowBrand(false);
    setShowNewDesign(false);
    setTemplateToUse("");
    onProjectChange?.(next ? { id: next.id, name: next.name } : null);
  };

  const createNewProject = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const trimmed = projectName.trim();
    if (trimmed === "") return;
    createProject.mutate(trimmed, {
      onSuccess: (created) => {
        setProjectId(created.id);
        setProjectName("");
        setShowProjectForm(false);
        onProjectChange?.({ id: created.id, name: created.name });
      },
    });
  };

  const confirmProjectDelete = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!selectedProject || confirmProjectName !== selectedProject.name) return;
    deleteProject.mutate(selectedProject.id, {
      onSuccess: () => {
        setConfirmProject(false);
        setConfirmProjectName("");
        setProjectId(null);
        onProjectChange?.(null);
      },
    });
  };

  if (status.isError) {
    return (
      <div className="min-h-full bg-ink text-paper">
        <HomeHeader chat={chat} />
        <main className="mx-auto grid min-h-[65vh] max-w-4xl place-items-center px-6 py-12">
          <section className="w-full max-w-xl rounded-2xl border border-edge bg-surface p-8">
            <h1 className="text-2xl font-medium tracking-tight">Forma can&apos;t connect right now</h1>
            <p className="mt-3 text-sm leading-6 text-muted">
              Your workspace isn&apos;t available. Make sure the Forma server is running, then try again.
            </p>
            <button
              type="button"
              onClick={() => void status.refetch()}
              className="mt-6 rounded-lg bg-signal px-4 py-2.5 text-sm font-semibold text-ink"
            >
              Try again
            </button>
          </section>
        </main>
      </div>
    );
  }

  const pendingRequests = (requests.data ?? []).filter((request) => request.status === "pending");
  const projectContentsReady = !designs.isLoading && !assets.isLoading && !templates.isLoading;
  const projectContentsFailed = designs.isError || assets.isError || templates.isError;
  const designCount = designs.data?.length ?? 0;
  const imageCount = assets.data?.length ?? 0;
  const templateCount = templates.data?.length ?? 0;

  return (
    <div className="min-h-full bg-ink text-paper">
      <HomeHeader chat={chat} />

      <section className="mx-auto w-full max-w-[1440px] px-5 pb-8 pt-8 sm:px-8 sm:pt-10">
        <h1 className="max-w-3xl text-3xl font-medium tracking-tight sm:text-4xl">
          Give visual ideas a form.
        </h1>
        <p className="mt-2 max-w-2xl text-sm leading-6 text-muted">
          Your projects. Your designs. What you and your agents make.
        </p>
      </section>

      <div className="mx-auto grid w-full max-w-[1440px] grid-cols-1 border-t border-edge md:grid-cols-[220px_minmax(0,1fr)]">
        <aside aria-label="Projects" className="border-b border-edge px-5 py-5 md:border-b-0 md:border-r md:px-4">
          <div className="hidden items-center justify-between px-2 pb-3 md:flex">
            <h2 className="text-[11px] font-medium uppercase tracking-[0.12em] text-faint">Projects</h2>
            <span className="font-mono text-[10px] text-faint">{projectList.length}</span>
          </div>

          <div className="flex items-center gap-2 md:hidden">
            <label htmlFor="project-select" className="sr-only">Choose a project</label>
            <select
              id="project-select"
              value={projectId ?? ""}
              onChange={(event) => selectProject(event.target.value)}
              className="min-h-11 min-w-0 flex-1 rounded-lg border border-edge bg-surface px-3 text-sm text-paper outline-none focus:border-signal"
            >
              <option value="">Choose a project</option>
              {projectList.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}
            </select>
            <button
              type="button"
              onClick={() => { setShowProjectForm(true); setConfirmProject(false); }}
              aria-label="Create a new project"
              className="grid h-11 w-11 shrink-0 place-items-center rounded-lg border border-edge bg-surface text-lg text-paper hover:border-edge-strong"
            >
              <PlusIcon />
            </button>
          </div>

          <div className="hidden space-y-1 md:block">
            {projectList.map((item) => (
              <button
                key={item.id}
                type="button"
                aria-pressed={item.id === projectId}
                onClick={() => selectProject(item.id)}
                className={`block w-full rounded-lg border px-3 py-2.5 text-left transition-colors ${
                  item.id === projectId
                    ? "border-signal/40 bg-signal/8"
                    : "border-transparent hover:border-edge hover:bg-surface"
                }`}
              >
                <span className="block truncate text-sm font-medium">{item.name}</span>
                <span className="mt-1 block text-[11px] text-faint">Created {formatDate(item.created_at)}</span>
              </button>
            ))}
            {projectList.length === 0 && !projects.isLoading && (
              <p className="px-2 py-3 text-xs leading-5 text-faint">Your projects will appear here.</p>
            )}
          </div>

          <button
            type="button"
            onClick={() => { setShowProjectForm(true); setConfirmProject(false); }}
            className="mt-3 hidden min-h-10 w-full items-center gap-2 rounded-lg px-3 text-left text-xs text-muted transition-colors hover:bg-surface hover:text-paper md:flex"
          >
            <PlusIcon /> New project
          </button>
        </aside>

        <main className="min-w-0 px-5 py-6 sm:px-8 sm:py-8">
          {showProjectForm && (
            <form onSubmit={createNewProject} className="mb-6 rounded-xl border border-edge bg-surface p-4 sm:p-5">
              <h2 className="text-sm font-medium">New project</h2>
              <label htmlFor="project-name" className="mt-4 block text-xs text-muted">Project name</label>
              <div className="mt-1.5 flex flex-col gap-2 sm:flex-row">
                <input
                  ref={projectNameRef}
                  id="project-name"
                  autoFocus
                  value={projectName}
                  maxLength={80}
                  onChange={(event) => setProjectName(event.target.value)}
                  className="min-h-11 min-w-0 flex-1 rounded-lg border border-edge bg-surface-2 px-3 text-sm text-paper outline-none placeholder:text-faint focus:border-signal"
                  placeholder="For example, Spring campaign"
                />
                <button
                  type="submit"
                  disabled={createProject.isPending || projectName.trim() === ""}
                  className="min-h-11 rounded-lg bg-signal px-4 text-sm font-semibold text-ink disabled:cursor-default disabled:opacity-40"
                >
                  Create project
                </button>
                <button
                  type="button"
                  onClick={() => { setShowProjectForm(false); setProjectName(""); }}
                  className="min-h-11 rounded-lg border border-edge px-4 text-sm text-muted hover:text-paper"
                >
                  Cancel
                </button>
              </div>
              {createProject.error && <p role="alert" className="mt-3 text-xs text-signal">{createProject.error.message}</p>}
            </form>
          )}

          {!projectList.length && !projects.isLoading ? (
            <section className="mx-auto max-w-xl rounded-2xl border border-edge bg-surface p-6 sm:p-8">
              <p className="text-[11px] font-medium uppercase tracking-[0.12em] text-faint">First visit</p>
              <h2 className="mt-3 text-2xl font-medium tracking-tight">Start with a project.</h2>
              <p className="mt-3 text-sm leading-6 text-muted">
                A project keeps your designs, brand and images together. Give your first one a name.
              </p>
              {!showProjectForm && (
                <button
                  type="button"
                  onClick={() => setShowProjectForm(true)}
                  className="mt-6 rounded-lg bg-signal px-4 py-2.5 text-sm font-semibold text-ink"
                >
                  Create your first project
                </button>
              )}
              <p className="mt-5 text-xs text-faint">
                Already using an AI app? <a href="/agents/mcp" className="text-muted underline underline-offset-4 hover:text-paper">Connect it to Forma</a>.
              </p>
            </section>
          ) : selectedProject ? (
            <>
              <div className="mb-7 flex flex-wrap items-start justify-between gap-4">
                <div className="min-w-0">
                  <h2 className="break-words text-2xl font-medium tracking-tight">{selectedProject.name}</h2>
                  <p className="mt-1.5 text-xs text-faint">
                    {plural(designCount, "design")} <span aria-hidden="true">·</span> Created {formatDate(selectedProject.created_at)}
                  </p>
                </div>
                <button
                  type="button"
                  aria-label={`Delete project ${selectedProject.name}`}
                  title="Delete project"
                  onClick={() => { setConfirmProject(true); setConfirmProjectName(""); setShowProjectForm(false); }}
                  className="grid h-10 w-10 shrink-0 place-items-center rounded-lg border border-edge text-muted transition-colors hover:border-edge-strong hover:text-paper"
                >
                  <TrashIcon />
                </button>
              </div>

              {confirmProject && (
                <section className="mb-6 rounded-xl border border-signal/40 bg-surface p-4 sm:p-5" aria-label="Delete project confirmation">
                  <h3 className="text-sm font-medium">Delete {selectedProject.name}?</h3>
                  {projectContentsFailed ? (
                    <p role="alert" className="mt-2 text-xs leading-5 text-signal">
                      Forma couldn&apos;t load the project contents. Try again before deleting it.
                    </p>
                  ) : projectContentsReady ? (
                    <p className="mt-2 text-xs leading-5 text-muted">
                      Deleting this project also deletes {plural(designCount, "design")}, {plural(imageCount, "image")} and {plural(templateCount, "template")}. This can&apos;t be undone.
                    </p>
                  ) : (
                    <p className="mt-2 text-xs text-faint">Loading project contents…</p>
                  )}
                  <form onSubmit={confirmProjectDelete} className="mt-4">
                    <label htmlFor="confirm-project-name" className="block text-xs text-muted">
                      Type <strong className="font-medium text-paper">{selectedProject.name}</strong> to confirm
                    </label>
                    <input
                      id="confirm-project-name"
                      autoFocus
                      autoComplete="off"
                      value={confirmProjectName}
                      onChange={(event) => setConfirmProjectName(event.target.value)}
                      className="mt-1.5 min-h-11 w-full rounded-lg border border-edge bg-surface-2 px-3 text-sm text-paper outline-none focus:border-signal sm:max-w-md"
                    />
                    {deleteProject.error && <p role="alert" className="mt-2 text-xs text-signal">{deleteProject.error.message}</p>}
                    <div className="mt-3 flex flex-wrap gap-2">
                      <button
                        type="submit"
                        disabled={!projectContentsReady || projectContentsFailed || confirmProjectName !== selectedProject.name || deleteProject.isPending}
                        className="min-h-10 rounded-lg border border-signal/50 bg-signal/10 px-3.5 text-xs font-medium text-paper disabled:cursor-default disabled:opacity-40"
                      >
                        Delete project
                      </button>
                      <button
                        type="button"
                        onClick={() => { setConfirmProject(false); setConfirmProjectName(""); }}
                        className="min-h-10 rounded-lg border border-edge px-3.5 text-xs text-muted hover:text-paper"
                      >
                        Keep project
                      </button>
                    </div>
                  </form>
                </section>
              )}

              <section className="mb-7" aria-labelledby="brand-kit-heading">
                <div className="mb-2 flex items-center justify-between gap-3">
                  <h3 id="brand-kit-heading" className="text-[11px] font-medium uppercase tracking-[0.12em] text-faint">Brand Kit</h3>
                  <button
                    type="button"
                    onClick={() => setShowBrand((open) => !open)}
                    className="min-h-9 rounded-lg px-3 text-xs text-muted underline decoration-edge-strong underline-offset-4 hover:text-paper"
                  >
                    {showBrand ? "Close" : brand.data ? "Edit" : "Set up"}
                  </button>
                </div>

                {!showBrand && (brand.data ? (
                  <div className="flex min-h-[68px] items-center gap-3 rounded-xl border border-edge bg-surface px-3 py-3 sm:gap-4 sm:px-4">
                    <div className="flex shrink-0 gap-1" aria-label="Brand Kit color swatches">
                      {[
                        ["Primary", brand.data.colors.primary],
                        ["Secondary", brand.data.colors.secondary],
                        ["Accent", brand.data.colors.accent],
                        ["Text", brand.data.colors.text],
                      ].map(([label, colour]) => (
                        <span
                          key={`${label}:${colour}`}
                          title={`${label} ${colour}`}
                          role="img"
                          aria-label={`${label} color ${colour}`}
                          className="h-6 w-6 rounded-full border border-paper/25 shadow-[0_0_0_1px_rgba(0,0,0,0.45)]"
                          style={{ backgroundColor: colour }}
                        />
                      ))}
                    </div>
                    <div className="min-w-0 flex-1">
                      <strong className="block truncate text-sm font-medium">{brand.data.name}</strong>
                      <span className="mt-0.5 block truncate text-[11px] text-faint">Display font · {brand.data.typography.display}</span>
                    </div>
                  </div>
                ) : (
                  <div className="rounded-xl border border-edge bg-surface px-4 py-3">
                    <p className="text-sm font-medium">No Brand Kit yet.</p>
                    <p className="mt-1 max-w-2xl text-xs leading-5 text-muted">
                      A Brand Kit keeps this project&apos;s colors and fonts together. Designs that use those colors or fonts need a kit to display correctly.
                    </p>
                  </div>
                ))}

                {showBrand && (
                  <div className="max-w-2xl rounded-xl border border-edge bg-surface p-4 sm:p-5">
                    <Brand projectId={selectedProject.id} />
                  </div>
                )}
              </section>

              <RequestNotice requests={pendingRequests} onOpen={onOpen} />

              <section aria-labelledby="designs-heading">
                <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
                  <h3 id="designs-heading" className="text-[11px] font-medium uppercase tracking-[0.12em] text-faint">
                    Designs <span className="ml-1 font-mono normal-case tracking-normal text-muted">{designCount}</span>
                  </h3>
                  <button
                    type="button"
                    onClick={() => { setTemplateToUse(""); setShowNewDesign(true); setConfirmDesignId(null); }}
                    className="inline-flex min-h-10 items-center gap-2 rounded-lg border border-edge bg-surface px-3.5 text-xs text-paper transition-colors hover:border-edge-strong"
                  >
                    <PlusIcon /> New design
                  </button>
                </div>

                {showNewDesign && (
                  <NewDesignForm
                    templates={templates.data ?? []}
                    initialTemplateId={templateToUse}
                    onCancel={() => setShowNewDesign(false)}
                    onOpen={onOpen}
                    projectId={selectedProject.id}
                  />
                )}

                {designs.isError && (
                  <div className="mb-4 rounded-lg border border-edge bg-surface p-4 text-xs text-muted">
                    <p>Forma couldn&apos;t load these designs.</p>
                    <button type="button" onClick={() => void designs.refetch()} className="mt-2 underline underline-offset-4 hover:text-paper">Try again</button>
                  </div>
                )}

                {designs.isLoading ? (
                  <p className="py-6 text-xs text-faint">Loading designs…</p>
                ) : designCount > 0 ? (
                  <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-3">
                    {(designs.data ?? []).map((design) => (
                      <DesignCard
                        key={design.id}
                        design={design}
                        confirming={confirmDesignId === design.id}
                        deleting={deleteDesign.isPending && deleteDesign.variables === design.id}
                        error={deleteDesign.variables === design.id ? deleteDesign.error?.message : undefined}
                        onOpen={() => onOpen(design.id)}
                        onRequestDelete={() => { setConfirmDesignId(design.id); setShowNewDesign(false); }}
                        onKeep={() => setConfirmDesignId(null)}
                        onDelete={() => deleteDesign.mutate(design.id, {
                          onSuccess: () => setConfirmDesignId(null),
                        })}
                      />
                    ))}
                  </div>
                ) : (
                  <div className="rounded-xl border border-edge bg-surface px-5 py-6 sm:px-6">
                    <h4 className="text-lg font-medium tracking-tight">A place for your next idea.</h4>
                    <p className="mt-2 max-w-xl text-sm leading-6 text-muted">
                      Create a blank design, or connect an AI app and ask it to make something in this project.
                    </p>
                    {!showNewDesign && (
                      <button
                        type="button"
                        onClick={() => setShowNewDesign(true)}
                        className="mt-4 rounded-lg bg-signal px-3.5 py-2.5 text-xs font-semibold text-ink"
                      >
                        Create a design
                      </button>
                    )}
                    <a href="/agents/mcp" className="mt-4 block w-fit text-xs text-muted underline decoration-edge-strong underline-offset-4 hover:text-paper">
                      Connect an AI app
                    </a>
                  </div>
                )}
              </section>

              {(templates.data ?? []).length > 0 && (
                <section className="mt-9" aria-labelledby="templates-heading">
                  <h3 id="templates-heading" className="text-[11px] font-medium uppercase tracking-[0.12em] text-faint">Templates</h3>
                  <p className="mt-1 text-xs text-muted">Saved layouts you can start from.</p>
                  <div className="mt-3 divide-y divide-edge rounded-xl border border-edge bg-surface">
                    {(templates.data ?? []).map((item) => (
                      <div key={item.id} className="flex flex-wrap items-center justify-between gap-3 px-4 py-3">
                        <div className="min-w-0">
                          <strong className="block truncate text-sm font-medium">{item.name}</strong>
                          <span className="mt-0.5 block text-[11px] text-faint">
                            {item.document.format.width} × {item.document.format.height}
                          </span>
                        </div>
                        <button
                          type="button"
                          onClick={() => { setTemplateToUse(item.id); setShowNewDesign(true); }}
                          className="min-h-9 rounded-lg border border-edge px-3 text-xs text-muted hover:text-paper"
                        >
                          Use template
                        </button>
                      </div>
                    ))}
                  </div>
                </section>
              )}
            </>
          ) : projects.isLoading ? (
            <p className="py-8 text-xs text-faint">Loading projects…</p>
          ) : (
            <section className="mx-auto max-w-xl rounded-2xl border border-edge bg-surface p-6 sm:p-8">
              <h2 className="text-xl font-medium tracking-tight">Choose your workspace.</h2>
              <p className="mt-2 text-sm leading-6 text-muted">
                Select a project to see its Brand Kit, designs and saved templates.
              </p>
              <a href="/agents/mcp" className="mt-4 inline-block text-xs text-muted underline underline-offset-4 hover:text-paper">
                Find what your agent makes here
              </a>
            </section>
          )}
        </main>
      </div>

      <div className="mx-auto w-full max-w-[1440px] px-5 pb-5 sm:px-8">
        <details className="border-t border-edge py-3 text-xs text-faint">
          <summary className="w-fit cursor-pointer select-none hover:text-muted">About Forma</summary>
          <div className="mt-3 space-y-1 pb-2 font-mono text-[10px] leading-5 text-faint">
            {status.data ? (
              <p>{status.data.name} {status.data.version} · schema v{status.data.schema_version} · fonts: {status.data.fonts.join(", ")}</p>
            ) : <p>Forma local design workspace</p>}
            <p>Projects, designs and images stay in this Forma workspace.</p>
          </div>
        </details>
      </div>
    </div>
  );
}

function HomeHeader({ chat }: { chat: ReactNode }) {
  return (
    <header className="border-b border-edge bg-ink">
      <div className="mx-auto flex min-h-[68px] w-full max-w-[1440px] items-center justify-between gap-4 px-5 sm:px-8">
        <a href="/" aria-label="Forma home" className="flex min-w-0 items-center gap-2.5 text-paper">
          <FormaMark />
          <span className="text-sm font-medium tracking-wide">Forma</span>
        </a>
        <nav aria-label="Window actions" className="flex shrink-0 items-center gap-3 sm:gap-5">
          <a href="/agents/mcp" className="text-xs text-muted underline decoration-edge-strong underline-offset-4 hover:text-paper sm:text-sm">
            Connect an agent
          </a>
          {chat ?? (
            <button
              type="button"
              disabled
              aria-label="Open window chat"
              title="Window chat is available when an account is connected"
              className="grid h-10 w-10 place-items-center rounded-lg border border-edge text-muted opacity-55"
            >
              <ChatIcon />
            </button>
          )}
        </nav>
      </div>
    </header>
  );
}

function RequestNotice({ requests, onOpen }: { requests: AssetRequest[]; onOpen: (id: string, tool?: InitialTool) => void }) {
  if (requests.length === 0) return null;
  const label = `${requests.length} image ${requests.length === 1 ? "request is" : "requests are"} waiting`;
  const list = (
    <ul className="mt-2 space-y-1.5 text-xs leading-5 text-muted">
      {requests.map((request) => (
        <li key={request.id}>
          {request.design_id ? (
            <a
              href={`/?design=${encodeURIComponent(request.design_id)}&panel=images`}
              onClick={(event) => { event.preventDefault(); onOpen(request.design_id ?? "", "images"); }}
              className="underline decoration-edge-strong underline-offset-4 hover:text-paper"
            >
              {request.purpose}
            </a>
          ) : (
            <span>{request.purpose} · upload from any design&apos;s Images panel</span>
          )}
        </li>
      ))}
    </ul>
  );

  return requests.length > 3 ? (
    <details className="mb-6 rounded-xl border border-edge bg-surface/70 px-4 py-3">
      <summary className="cursor-pointer text-xs text-muted marker:text-faint">{label}</summary>
      {list}
    </details>
  ) : (
    <aside aria-label="Pending image requests" className="mb-6 rounded-xl border border-edge bg-surface/70 px-4 py-3">
      <p className="text-xs text-faint">{label}</p>
      {list}
    </aside>
  );
}

function NewDesignForm({
  projectId,
  templates,
  initialTemplateId,
  onCancel,
  onOpen,
}: {
  projectId: string;
  templates: Template[];
  initialTemplateId: string;
  onCancel: () => void;
  onOpen: (designId: string, initialTool?: InitialTool) => void;
}) {
  const createBlank = useCreateDesignWithFormat(projectId);
  const createFromTemplate = useCreateDesignFromTemplate(projectId);
  const [name, setName] = useState("");
  const [preset, setPreset] = useState<FormatPreset>("square");
  const [templateId, setTemplateId] = useState(initialTemplateId);
  const [width, setWidth] = useState("1080");
  const [height, setHeight] = useState("1080");
  const [error, setError] = useState<string | null>(null);
  const pending = createBlank.isPending || createFromTemplate.isPending;

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const cleanName = name.trim();
    if (cleanName === "") {
      setError("Enter a name for this design.");
      return;
    }

    const openCreated = (record: { id: string }) => onOpen(record.id);
    if (templateId !== "") {
      setError(null);
      createFromTemplate.mutate({ templateId, name: cleanName }, { onSuccess: openCreated });
      return;
    }

    let format: { preset: FormatPreset; width: number | null; height: number | null };
    if (preset === "custom") {
      const customWidth = Number(width);
      const customHeight = Number(height);
      if (
        !Number.isInteger(customWidth) || !Number.isInteger(customHeight) ||
        customWidth < 1 || customHeight < 1 || customWidth > 10000 || customHeight > 10000
      ) {
        setError("Use whole-number dimensions from 1 to 10,000 pixels.");
        return;
      }
      format = { preset, width: customWidth, height: customHeight };
    } else {
      format = { preset, width: null, height: null };
    }

    setError(null);
    createBlank.mutate({ name: cleanName, format }, { onSuccess: openCreated });
  };

  return (
    <form onSubmit={submit} className="mb-4 rounded-xl border border-edge bg-surface p-4 sm:p-5">
      <div className="flex items-start justify-between gap-3">
        <div>
          <h4 className="text-sm font-medium">New design</h4>
          <p className="mt-1 text-xs text-faint">Choose a name and a canvas to start with.</p>
        </div>
        <button type="button" onClick={onCancel} aria-label="Close new design form" className="grid h-9 w-9 shrink-0 place-items-center rounded-lg border border-edge text-muted hover:text-paper">
          <CloseIcon />
        </button>
      </div>

      <label htmlFor="design-name" className="mt-4 block text-xs text-muted">Design name</label>
      <input
        id="design-name"
        autoFocus
        required
        maxLength={80}
        value={name}
        onChange={(event) => setName(event.target.value)}
        placeholder="For example, Launch cover"
        className="mt-1.5 min-h-11 w-full rounded-lg border border-edge bg-surface-2 px-3 text-sm text-paper outline-none placeholder:text-faint focus:border-signal"
      />

      {templates.length > 0 && (
        <label className="mt-4 block text-xs text-muted">
          Start from a template
          <select
            aria-label="Start from a template"
            value={templateId}
            onChange={(event) => setTemplateId(event.target.value)}
            className="mt-1.5 block min-h-11 w-full rounded-lg border border-edge bg-surface-2 px-3 text-sm text-paper outline-none focus:border-signal"
          >
            <option value="">Blank design</option>
            {templates.map((item) => (
              <option key={item.id} value={item.id}>{item.name}</option>
            ))}
          </select>
        </label>
      )}

      {templateId === "" && (
        <fieldset className="mt-4">
          <legend className="text-xs text-muted">Format</legend>
          <div role="radiogroup" aria-label="Format" className="mt-2 grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-5">
            {FORMAT_CHOICES.map((choice) => (
              <button
                key={choice.preset}
                type="button"
                role="radio"
                aria-checked={preset === choice.preset}
                onClick={() => { setPreset(choice.preset); setError(null); }}
                className={`min-h-10 rounded-lg border px-2.5 text-xs transition-colors ${
                  preset === choice.preset
                    ? "border-signal/60 bg-signal/10 text-paper"
                    : "border-edge bg-surface-2 text-muted hover:text-paper"
                }`}
              >
                {choice.label}{choice.preset === "square" ? " · default" : ""}
              </button>
            ))}
          </div>
          {preset === "custom" && (
            <div className="mt-3 grid grid-cols-2 gap-3 sm:max-w-sm">
              <label className="text-xs text-muted">
                Custom width
                <input
                  type="number"
                  min={1}
                  max={10000}
                  step={1}
                  value={width}
                  onChange={(event) => setWidth(event.target.value)}
                  className="mt-1.5 min-h-11 w-full rounded-lg border border-edge bg-surface-2 px-3 text-sm text-paper outline-none focus:border-signal"
                />
              </label>
              <label className="text-xs text-muted">
                Custom height
                <input
                  type="number"
                  min={1}
                  max={10000}
                  step={1}
                  value={height}
                  onChange={(event) => setHeight(event.target.value)}
                  className="mt-1.5 min-h-11 w-full rounded-lg border border-edge bg-surface-2 px-3 text-sm text-paper outline-none focus:border-signal"
                />
              </label>
            </div>
          )}
        </fieldset>
      )}

      {(error || createBlank.error || createFromTemplate.error) && (
        <p role="alert" className="mt-3 text-xs text-signal">
          {error ?? createBlank.error?.message ?? createFromTemplate.error?.message}
        </p>
      )}

      <div className="mt-5 flex flex-wrap gap-2">
        <button
          type="submit"
          disabled={pending || name.trim() === ""}
          className="min-h-11 rounded-lg bg-signal px-4 text-sm font-semibold text-ink disabled:cursor-default disabled:opacity-40"
        >
          Create &amp; open
        </button>
        <button type="button" onClick={onCancel} className="min-h-11 rounded-lg border border-edge px-4 text-sm text-muted hover:text-paper">
          Cancel
        </button>
      </div>
    </form>
  );
}

function DesignCard({
  design,
  confirming,
  deleting,
  error,
  onOpen,
  onRequestDelete,
  onKeep,
  onDelete,
}: {
  design: DesignSummary;
  confirming: boolean;
  deleting: boolean;
  error?: string;
  onOpen: () => void;
  onRequestDelete: () => void;
  onKeep: () => void;
  onDelete: () => void;
}) {
  const [previewLoaded, setPreviewLoaded] = useState(false);
  const [previewFailed, setPreviewFailed] = useState(false);
  const [previewSrc, setPreviewSrc] = useState<string | null>(null);
  const previewRef = useRef<HTMLImageElement>(null);
  const pageId = design.first_page_id;
  const width = Math.round(design.format.width);
  const height = Math.round(design.format.height);

  useEffect(() => {
    setPreviewLoaded(false);
    setPreviewFailed(false);
    setPreviewSrc(null);
    if (!pageId || !previewRef.current) return;

    let cancelled = false;
    let requested = false;
    let objectUrl: string | null = null;
    const controller = new AbortController();
    const loadPreview = async () => {
      if (requested) return;
      requested = true;
      try {
        const blob = await fetchPreviewPng(design.id, pageId, design.version, 1, controller.signal);
        if (cancelled) return;
        objectUrl = URL.createObjectURL(blob);
        setPreviewSrc(objectUrl);
      } catch {
        if (!cancelled) setPreviewFailed(true);
      }
    };

    const image = previewRef.current;
    const checkVisibility = () => {
      const bounds = image.getBoundingClientRect();
      if (bounds.bottom >= -160 && bounds.top <= window.innerHeight + 160) void loadPreview();
    };
    let observer: IntersectionObserver | null = null;
    if (typeof IntersectionObserver !== "undefined") {
      observer = new IntersectionObserver((entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          observer?.disconnect();
          void loadPreview();
        }
      }, { rootMargin: "160px" });
    }
    if (observer) observer.observe(image);
    else void loadPreview();
    checkVisibility();
    document.addEventListener("scroll", checkVisibility, true);
    window.addEventListener("resize", checkVisibility);

    return () => {
      cancelled = true;
      controller.abort();
      observer?.disconnect();
      document.removeEventListener("scroll", checkVisibility, true);
      window.removeEventListener("resize", checkVisibility);
      if (objectUrl && typeof URL.revokeObjectURL === "function") URL.revokeObjectURL(objectUrl);
    };
  }, [design.id, design.version, pageId]);

  return (
    <article aria-label={`${design.name} design`} className={`overflow-hidden rounded-xl border bg-surface transition-colors ${confirming ? "border-signal/40" : "border-edge hover:border-edge-strong"}`}>
      <button
        type="button"
        onClick={onOpen}
        disabled={confirming}
        aria-label={`Open ${design.name}`}
        className="group relative block w-full overflow-hidden bg-surface text-left disabled:cursor-default"
      >
        <span className="relative flex h-[180px] w-full items-center justify-center overflow-hidden bg-surface p-3 sm:h-[216px] sm:p-4">
          {(!previewLoaded || previewFailed || !pageId) && (
            <span className="absolute inset-0 grid place-items-center text-xs text-faint">
              {previewFailed ? "Preview unavailable" : "Page 1 preview"}
            </span>
          )}
          {pageId && !previewFailed && (
            <img
              ref={previewRef}
              src={previewSrc ?? undefined}
              alt={`Page 1 of ${design.name}`}
              loading="lazy"
              onLoad={() => setPreviewLoaded(true)}
              onError={() => setPreviewFailed(true)}
              className={`h-full w-full object-contain transition-opacity duration-200 ${previewLoaded ? "opacity-100" : "opacity-0"} ${confirming ? "opacity-35 grayscale" : ""}`}
            />
          )}
        </span>
        {confirming && <span className="absolute inset-0 bg-ink/35" aria-hidden="true" />}
      </button>

      {confirming ? (
        <div className="space-y-3 p-3.5" aria-label="Delete design confirmation">
          <p className="text-xs leading-5 text-muted">Delete <strong className="font-medium text-paper">{design.name}</strong> and its {plural(design.page_count, "page")}? This can&apos;t be undone.</p>
          {error && <p role="alert" className="text-xs text-signal">{error}</p>}
          <div className="flex gap-2">
            <button
              type="button"
              onClick={onDelete}
              disabled={deleting}
              className="min-h-10 rounded-lg border border-signal/50 bg-signal/10 px-3 text-xs font-medium text-paper disabled:opacity-40"
            >
              Delete design
            </button>
            <button
              type="button"
              autoFocus
              onClick={onKeep}
              className="min-h-10 rounded-lg border border-edge px-3 text-xs text-muted hover:text-paper"
            >
              Keep design
            </button>
          </div>
        </div>
      ) : (
        <div className="p-3.5">
          <div className="flex items-start justify-between gap-2">
            <button type="button" onClick={onOpen} className="min-w-0 flex-1 text-left">
              <span className="block truncate text-sm font-medium text-paper">{design.name}</span>
              <span className="mt-1 block text-[11px] tabular-nums text-faint">
                v{design.version} · {plural(design.page_count, "page")} · {width} × {height}
              </span>
            </button>
            <button
              type="button"
              onClick={onRequestDelete}
              aria-label={`Delete ${design.name}`}
              title="Delete design"
              className="grid h-9 w-9 shrink-0 place-items-center rounded-lg border border-edge text-muted hover:border-edge-strong hover:text-paper"
            >
              <TrashIcon />
            </button>
          </div>
          <p className="mt-3 min-w-0 text-[11px] text-faint">
            <span className="block truncate">Edited by {editorLabel(design)} · <time className="whitespace-nowrap">{relativeTime(design.edited_at)}</time></span>
          </p>
          <details className="mt-2 border-t border-edge pt-2 text-[11px] text-faint">
            <summary className="w-fit cursor-pointer select-none hover:text-muted">Design details</summary>
            <dl className="mt-2 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 leading-5">
              <dt>Format</dt><dd>{formatLabel(design.format.preset)}</dd>
              <dt>Created</dt><dd>{formatDate(design.created_at)}</dd>
              <dt>Page 1</dt><dd>{pageId ?? "No page preview"}</dd>
            </dl>
          </details>
        </div>
      )}
    </article>
  );
}

function editorLabel(design: DesignSummary): string {
  if (design.actor_type === "human") return "You";
  if (design.actor_type === "system") return "Forma";
  const id = design.actor_id?.replace(/^agent:/u, "").toLowerCase();
  switch (id) {
    case "claude-code": return "Claude Code";
    case "codex-mcp-client":
    case "codex": return "Codex CLI";
    case "claude-ai": return "Claude";
    case "opencode": return "OpenCode";
    case "cursor": return "Cursor";
    case "chatgpt": return "ChatGPT";
    default: return "An agent";
  }
}

function relativeTime(value: string): string {
  const timestamp = Date.parse(value);
  if (!Number.isFinite(timestamp)) return "time unavailable";
  const age = Math.max(0, Date.now() - timestamp);
  if (age < 60_000) return "just now";
  if (age < 3_600_000) return `${Math.floor(age / 60_000)} min ago`;
  if (age < 86_400_000) return `${Math.floor(age / 3_600_000)} h ago`;
  if (age < 604_800_000) return `${Math.floor(age / 86_400_000)} d ago`;
  return formatDate(value);
}

function formatDate(value: string): string {
  const date = new Date(value);
  if (!Number.isFinite(date.getTime())) return value;
  return new Intl.DateTimeFormat(undefined, { year: "numeric", month: "short", day: "numeric" }).format(date);
}

function formatLabel(preset: FormatPreset): string {
  return FORMAT_CHOICES.find((choice) => choice.preset === preset)?.label ?? "Custom";
}

function plural(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}

function FormaMark() {
  return (
    <svg aria-hidden="true" viewBox="45.78 -710 520.82 710" className="h-7 w-6 fill-current">
      <path d="M123.77 0V-542.84q0-79.2 42.98-123.18Q209.73-710 300.43-710h114.67v112.15h-81.74q-31 0-45 15.5t-14 46.25V0Zm-77.98-423.83v-112.15h361.57v112.15ZM403.8 0v-153.2h162.8V0Z" />
    </svg>
  );
}

function PlusIcon() {
  return (
    <svg aria-hidden="true" viewBox="0 0 20 20" className="h-4 w-4 fill-none stroke-current" strokeWidth="1.5">
      <path d="M10 3.5v13M3.5 10h13" />
    </svg>
  );
}

function TrashIcon() {
  return (
    <svg aria-hidden="true" viewBox="0 0 20 20" className="h-4 w-4 fill-none stroke-current" strokeWidth="1.35" strokeLinecap="round" strokeLinejoin="round">
      <path d="M3.5 5.5h13M8 3.5h4m-6.5 2 .7 11h7.6l.7-11M8 8v6m4-6v6" />
    </svg>
  );
}

function ChatIcon() {
  return (
    <svg aria-hidden="true" viewBox="0 0 20 20" className="h-[18px] w-[18px] fill-none stroke-current" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round">
      <path d="M4 4.25h12a1.75 1.75 0 0 1 1.75 1.75v7A1.75 1.75 0 0 1 16 14.75H9l-4.75 2v-2.1A1.75 1.75 0 0 1 2.25 13V6A1.75 1.75 0 0 1 4 4.25Z" />
      <path d="M6.5 8h7m-7 3h4" />
    </svg>
  );
}

function CloseIcon() {
  return (
    <svg aria-hidden="true" viewBox="0 0 20 20" className="h-4 w-4 fill-none stroke-current" strokeWidth="1.5" strokeLinecap="round">
      <path d="m5 5 10 10M15 5 5 15" />
    </svg>
  );
}
