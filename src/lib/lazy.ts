// Components loaded on first use. CodeMirror (the source editor and the
// diffs) is most of the interface's code, and the heavy views and dialogs
// are opened in a minority of sessions; Today, Library and Read stay eager.
import type HtmlEditor from "../components/HtmlEditor.svelte";
import type HtmlDiff from "../components/HtmlDiff.svelte";
import type GraphView from "../components/GraphView.svelte";
import type SettingsView from "../components/SettingsView.svelte";
import type PathView from "../components/PathView.svelte";
import type TrashView from "../components/TrashView.svelte";
import type AiPanel from "../components/ai/AiPanel.svelte";
import type ImportDialog from "../components/ImportDialog.svelte";
import type AiImportDialog from "../components/AiImportDialog.svelte";
import type HealthDialog from "../components/HealthDialog.svelte";
import type PlantPathDialog from "../components/PlantPathDialog.svelte";

function once<T>(load: () => Promise<T>): () => Promise<T> {
  let promise: Promise<T> | null = null;
  return () => {
    promise ??= load().catch((e) => {
      promise = null; // let a later attempt retry
      throw e;
    });
    return promise;
  };
}

export const loadHtmlEditor = once(
  (): Promise<typeof HtmlEditor> => import("../components/HtmlEditor.svelte").then((m) => m.default),
);
export const loadHtmlDiff = once(
  (): Promise<typeof HtmlDiff> => import("../components/HtmlDiff.svelte").then((m) => m.default),
);

export const loadGraphView = once(
  (): Promise<typeof GraphView> => import("../components/GraphView.svelte").then((m) => m.default),
);
export const loadSettingsView = once(
  (): Promise<typeof SettingsView> => import("../components/SettingsView.svelte").then((m) => m.default),
);
export const loadPathView = once(
  (): Promise<typeof PathView> => import("../components/PathView.svelte").then((m) => m.default),
);
export const loadTrashView = once(
  (): Promise<typeof TrashView> => import("../components/TrashView.svelte").then((m) => m.default),
);
export const loadAiPanel = once(
  (): Promise<typeof AiPanel> => import("../components/ai/AiPanel.svelte").then((m) => m.default),
);
export const loadImportDialog = once(
  (): Promise<typeof ImportDialog> => import("../components/ImportDialog.svelte").then((m) => m.default),
);
export const loadAiImportDialog = once(
  (): Promise<typeof AiImportDialog> => import("../components/AiImportDialog.svelte").then((m) => m.default),
);
export const loadHealthDialog = once(
  (): Promise<typeof HealthDialog> => import("../components/HealthDialog.svelte").then((m) => m.default),
);
export const loadPlantPathDialog = once(
  (): Promise<typeof PlantPathDialog> => import("../components/PlantPathDialog.svelte").then((m) => m.default),
);
