import { listen } from "@tauri-apps/api/event";
import { api } from "./api";
import { t } from "./i18n.svelte";
import { app, toast } from "./state.svelte";

/** Emitted by the backend (`navigation.rs`) with a blocked http(s) URL. */
const BLOCKED_EVENT = "navigation-blocked";
const PROMPT_MS = 12000;
const MAX_URL_CHARS = 200;

let promptId: number | null = null;

/** The page id of a `herbarium-app://open/<id>` link, or null. */
export function pageLinkId(href: string): string | null {
  const m = /^herbarium-app:\/\/open\/([^?#/]+)/i.exec(href);
  if (!m) return null;
  try {
    return decodeURIComponent(m[1]);
  } catch {
    return null;
  }
}

/**
 * Offer to open a web link a page tried to follow in the system browser.
 * One prompt at a time: while it is shown, further attempts are ignored, so a
 * page cannot swap the URL under the user's cursor.
 */
export function offerWebLink(url: string) {
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
}

/**
 * Pages cannot navigate away from the app; the web links they try to follow
 * are offered here, and open in the system browser only when the user clicks.
 */
export function initLinkPrompts() {
  void listen<string>(BLOCKED_EVENT, ({ payload: url }) => {
    // Only the page open in the reader offers links; the hidden frame that
    // draws library previews runs pages too, and must never prompt.
    if (!app.readId) return;
    offerWebLink(url);
  });
}
