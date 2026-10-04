/**
 * The editor following the agent's writes (D-7; F-chat-2, F-30-3).
 *
 * The agent's batch is real in Forma the moment it completes, and the editor
 * holds the design as of the last time *it* read or wrote it. Without this the
 * person is told the agent changed their design by a conflict banner on their
 * next edit — the one way of finding out that costs them an edit.
 *
 * The cue is the chat's own history: a completed `design.applyOperations` names
 * the design and the version it made. Delivery is idempotent on purpose. The
 * history replays after a reload and arrives twice (the page and the live
 * stream), so a write whose version the cache already holds is dropped, and
 * only a version newer than what the window knows is news.
 */
import type { QueryClient } from "@tanstack/react-query";

import type { DesignRecord } from "../api";
import { keys } from "../queries";
import { useEditor } from "../store";
import { layerId } from "../types";
import type { AgentWrite } from "./chat";

export interface FollowReach {
  readonly queries: QueryClient;
  /** The design the window is showing, or `null` on the project list. */
  readonly openDesignId: () => string | null;
}

export async function followAgentWrite({ queries, openDesignId }: FollowReach, write: AgentWrite): Promise<void> {
  const known = queries.getQueryData<DesignRecord>(keys.design(write.designId));
  if (known && known.version >= write.version) return;

  const open = openDesignId() === write.designId;
  // A drag that began on the old geometry would commit a frame the agent has since moved.
  if (open) useEditor.getState().setDraft(null);

  const projectId = write.projectId ?? known?.project_id ?? null;
  if (projectId !== null) void queries.invalidateQueries({ queryKey: keys.designs(projectId) });
  void queries.invalidateQueries({ queryKey: keys.history(write.designId) });
  // Refetches the open design (it is the one active query) and leaves any other to be read when
  // it is next opened, instead of serving the version from before.
  await queries.invalidateQueries({ queryKey: keys.design(write.designId) });
  if (!open) return;

  const fresh = queries.getQueryData<DesignRecord>(keys.design(write.designId));
  // The read failed or is behind: the next edit's conflict banner is the fallback, not a wrong number here.
  if (!fresh || fresh.version < write.version) return;

  const editor = useEditor.getState();
  const selected = editor.selectedLayerId;
  // A layer the agent removed cannot stay selected; one it only changed stays where the person put it.
  if (selected !== null && !fresh.document.pages.some((page) => page.layers.some((layer) => layerId(layer) === selected))) {
    editor.selectLayer(null);
  }
  editor.announce(`The agent updated the design (v${fresh.version})`);
}
