// Components loaded on first use: CodeMirror (the source editor and the
// diffs) is most of the interface's code, and most sessions never open it.
import type HtmlEditor from "../components/HtmlEditor.svelte";
import type HtmlDiff from "../components/HtmlDiff.svelte";

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
