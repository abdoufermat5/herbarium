<script lang="ts">
  import { onDestroy } from "svelte";
  import { MergeView } from "@codemirror/merge";
  import { EditorState } from "@codemirror/state";
  import { EditorView } from "@codemirror/view";
  import { syntaxHighlighting, HighlightStyle } from "@codemirror/language";
  import { html } from "@codemirror/lang-html";
  import { tags as lezerTags } from "@lezer/highlight";

  /** Read-only side-by-side diff of two HTML documents. */
  let { before, after, label }: { before: string; after: string; label: string } = $props();

  let mergeHost: HTMLElement | undefined = $state();
  let mergeView: MergeView | undefined;

  // The diff reuses the editor's CSS variables and HTML token colors, trimmed
  // to a read-only view (no gutters, folds or autocomplete).
  const highlight = HighlightStyle.define([
    { tag: lezerTags.tagName, color: "var(--danger, #9f2f2d)", fontWeight: "500" },
    { tag: lezerTags.angleBracket, color: "var(--muted, #6b6a68)" },
    { tag: lezerTags.attributeName, color: "var(--warn, #956400)" },
    { tag: [lezerTags.attributeValue, lezerTags.string], color: "var(--leaf, #346538)" },
    { tag: lezerTags.comment, color: "var(--muted, #6b6a68)", fontStyle: "italic" },
    { tag: lezerTags.keyword, color: "var(--danger, #9f2f2d)" },
    { tag: lezerTags.number, color: "var(--info, #1f6c9f)" },
    { tag: lezerTags.bool, color: "var(--danger, #9f2f2d)" },
  ]);

  const diffTheme = EditorView.theme({
    "&": {
      height: "100%",
      backgroundColor: "var(--surface)",
      color: "var(--text)",
      fontSize: "12.5px",
    },
    ".cm-scroller": {
      fontFamily: "var(--mono)",
      lineHeight: "1.6",
      overflow: "auto",
    },
    ".cm-content": { padding: "8px 0" },
    ".cm-gutters": {
      backgroundColor: "var(--raised)",
      color: "var(--muted)",
      border: "none",
    },
    // @codemirror/merge paints this bar with a hard-coded light gradient.
    ".cm-collapsedLines": {
      background: "var(--raised)",
      color: "var(--muted)",
    },
  });

  function side(doc: string) {
    return {
      doc,
      extensions: [
        html(),
        syntaxHighlighting(highlight),
        diffTheme,
        EditorState.readOnly.of(true),
        EditorView.editable.of(false),
        EditorView.lineWrapping,
      ],
    };
  }

  onDestroy(() => mergeView?.destroy());

  // Rebuild the diff whenever either side changes.
  $effect(() => {
    const host = mergeHost;
    const a = before;
    const b = after;
    if (!host) return;
    mergeView = new MergeView({
      a: side(a),
      b: side(b),
      parent: host,
      highlightChanges: true,
      gutter: true,
      collapseUnchanged: { margin: 3, minSize: 6 },
    });
    return () => {
      mergeView?.destroy();
      mergeView = undefined;
    };
  });
</script>

<div class="merge" bind:this={mergeHost} aria-label={label}></div>

<style>
  .merge {
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
  .merge :global(.cm-mergeView) {
    height: 100%;
  }
  .merge :global(.cm-mergeViewEditors) {
    height: 100%;
  }
</style>
