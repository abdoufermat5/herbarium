import { listen } from "@tauri-apps/api/event";
import { api } from "./api";
import { t } from "./i18n.svelte";
import { app, toast } from "./state.svelte";

/** Emitted by the backend (`navigation.rs`) with a blocked http(s) URL. */
const BLOCKED_EVENT = "navigation-blocked";
const PROMPT_MS = 12000;
const MAX_URL_CHARS = 200;

let promptId: number | null = null;

/**
 * Pages cannot navigate away from the app; the web links they try to follow
 * are offered here, and open in the system browser only when the user clicks.
 * One prompt at a time: while it is shown, further attempts are ignored, so a
 * page cannot swap the URL under the user's cursor.
 */
export function initLinkPrompts() {
  void listen<string>(BLOCKED_EVENT, ({ payload: url }) => {
    if (promptId !== null && app.toasts.some((x) => x.id === promptId)) return;
    const shown = url.length > MAX_URL_CHARS ? `${url.slice(0, MAX_URL_CHARS)}…` : url;
    promptId = toast(t("link.blocked", { url: shown }), "info", PROMPT_MS, {
      label: t("link.open"),
      run: () => {
        api.openExternal(url).catch((e) => {
          console.error(e);
          toast(t("link.failed"), "error");
        });
      },
    });
  });
}
