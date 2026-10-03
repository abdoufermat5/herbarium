// External text editors: the list detected on this system, and which one the
// "open in editor" button uses given the user's preferences.

import { api } from "./api";
import { prefs } from "./prefs.svelte";
import type { EditorInfo } from "./types";

export const editors = $state<{ list: EditorInfo[]; loaded: boolean }>({ list: [], loaded: false });

let inflight: Promise<void> | null = null;

/** Detect installed editors once (or again with `force`). */
export function loadEditors(force = false): Promise<void> {
  if (editors.loaded && !force) return Promise.resolve();
  inflight ??= api
    .listEditors()
    .then((list) => {
      editors.list = list;
      editors.loaded = true;
    })
    .catch((e) => console.error(e))
    .finally(() => {
      inflight = null;
    });
  return inflight;
}

export interface EditorChoice {
  /** Detected editor id, when not using a custom command. */
  id: string | null;
  custom: string | null;
  /** Name to show on the button. */
  name: string;
}

/** The editor "open in editor" will use, or null when there is none. */
export function currentEditor(): EditorChoice | null {
  const custom = prefs.externalEditorCommand.trim();
  if (custom) {
    const program = custom.split(/\s+/)[0].replace(/^["']|["']$/g, "").split("/").pop() ?? custom;
    return { id: null, custom, name: program };
  }
  // A saved editor that has since been uninstalled falls back to the first detected one.
  const picked = editors.list.find((e) => e.id === prefs.externalEditor) ?? editors.list[0];
  return picked ? { id: picked.id, custom: null, name: picked.name } : null;
}
