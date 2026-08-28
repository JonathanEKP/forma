import { useState } from "react";

import { Editor } from "./editor/Editor";
import { Projects } from "./Projects";

/**
 * Two views, no router.
 *
 * V1 has exactly one navigation step — pick a design, edit it — and a router
 * would be more machinery than that earns. It becomes worth adding when a URL
 * has to be shareable, which is a Phase 11 concern.
 */
export function App() {
  const [designId, setDesignId] = useState<string | null>(null);

  return designId === null ? (
    <Projects onOpen={setDesignId} />
  ) : (
    <Editor designId={designId} onBack={() => setDesignId(null)} />
  );
}
