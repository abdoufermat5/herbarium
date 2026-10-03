<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import {
    EditorView,
    keymap,
    lineNumbers,
    highlightActiveLineGutter,
    highlightSpecialChars,
    drawSelection,
    dropCursor,
    rectangularSelection,
    crosshairCursor,
    highlightActiveLine,
  } from "@codemirror/view";
  import { EditorState, Compartment, Transaction, Prec } from "@codemirror/state";
  import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
  import { search, searchKeymap, highlightSelectionMatches, closeSearchPanel } from "@codemirror/search";
  import {
    syntaxHighlighting,
    defaultHighlightStyle,
    HighlightStyle,
    bracketMatching,
    foldGutter,
    foldKeymap,
    indentOnInput,
    indentUnit,
  } from "@codemirror/language";
  import { autocompletion, completionKeymap, closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete";
  import { html } from "@codemirror/lang-html";
  import { tags as t } from "@lezer/highlight";

  /**
   * Clamps selection offsets to the valid document range [0, docLength].
   */
  export function clampSelection(
    selection: { anchor: number; head: number },
    docLength: number,
  ): { anchor: number; head: number } {
    return {
      anchor: Math.max(0, Math.min(selection.anchor, docLength)),
      head: Math.max(0, Math.min(selection.head, docLength)),
    };
  }

  /**
   * Creates dynamic font and typography theme for CodeMirror 6.
   */
  export function getDynamicTheme(
    fontSize: number | string,
    lineHeight: number | string,
    fontFamily: string,
  ) {
    const size = typeof fontSize === "number" ? `${fontSize}px` : fontSize;
    const height = typeof lineHeight === "number" ? `${lineHeight}` : `${lineHeight}`;
    return EditorView.theme({
      "&": {
        fontSize: size,
        lineHeight: height,
        fontFamily: fontFamily,
      },
      ".cm-scroller": {
        fontFamily: fontFamily,
        lineHeight: height,
      },
      ".cm-content": {
        fontFamily: fontFamily,
      },
    });
  }

  /**
   * Warm, ink-tinted CSS variable syntax styling adapting to light/dark themes.
   */
  export const htmlHighlightStyle = HighlightStyle.define([
    { tag: t.tagName, color: "var(--danger, #9f2f2d)", fontWeight: "500" },
    { tag: t.angleBracket, color: "var(--muted, #6b6a68)" },
    { tag: t.attributeName, color: "var(--warn, #956400)" },
    { tag: t.attributeValue, color: "var(--leaf, #346538)" },
    { tag: [t.string, t.character], color: "var(--leaf, #346538)" },
    { tag: t.comment, color: "var(--muted, #6b6a68)", fontStyle: "italic" },
    { tag: [t.documentMeta, t.processingInstruction], color: "var(--muted, #6b6a68)" },
    { tag: t.operator, color: "var(--text-soft, #50555a)" },
    { tag: t.keyword, color: "var(--danger, #9f2f2d)" },
    { tag: t.propertyName, color: "var(--warn, #956400)" },
    { tag: [t.definition(t.name), t.variableName], color: "var(--text, #2f3437)" },
    { tag: t.number, color: "var(--info, #1f6c9f)" },
    { tag: t.bool, color: "var(--danger, #9f2f2d)" },
    { tag: t.escape, color: "var(--warn, #956400)" },
    { tag: t.invalid, color: "var(--danger, #9f2f2d)", textDecoration: "underline" },
  ]);

  /**
   * CodeMirror base theme integrating with Herbarium's design system & CSS variables.
   */
  export const editorTheme = EditorView.theme({
    "&": {
      height: "100%",
      width: "100%",
      backgroundColor: "var(--surface)",
      color: "var(--text)",
      fontFamily: "var(--ed-font, var(--mono))",
      fontSize: "var(--ed-size, 12.5px)",
      lineHeight: "var(--ed-lh, 1.6)",
      tabSize: "var(--ed-tab, 2)",
    },
    "&.cm-focused": {
      outline: "none",
    },
    ".cm-scroller": {
      height: "100%",
      overflow: "auto",
      fontFamily: "inherit",
      lineHeight: "inherit",
      overflowWrap: "anywhere",
    },
    ".cm-content": {
      padding: "14px 0",
      caretColor: "var(--accent-strong, var(--text))",
    },
    ".cm-line": {
      padding: "0 16px 0 12px",
    },
    ".cm-cursor, .cm-dropCursor": {
      borderLeftColor: "var(--accent-strong, var(--text))",
      borderLeftWidth: "1.5px",
    },
    "&.cm-focused .cm-cursor": {
      borderLeftColor: "var(--accent-strong, var(--text))",
    },
    ".cm-selectionBackground, &.cm-focused .cm-selectionBackground, ::selection": {
      backgroundColor: "var(--selection) !important",
    },
    ".cm-selectionMatch": {
      backgroundColor: "var(--warn-soft, rgba(149, 100, 0, 0.15))",
      outline: "1px solid var(--warn-border, rgba(149, 100, 0, 0.25))",
    },
    ".cm-gutters": {
      backgroundColor: "var(--raised)",
      color: "var(--muted)",
      borderRight: "1px solid var(--border)",
      borderTop: "none",
      borderBottom: "none",
      borderLeft: "none",
      paddingRight: "4px",
      fontFamily: "var(--mono)",
      fontSize: "0.9em",
      userSelect: "none",
    },
    ".cm-gutterElement": {
      color: "var(--muted)",
    },
    ".cm-activeLineGutter": {
      backgroundColor: "var(--sunken, var(--raised))",
      color: "var(--text)",
    },
    ".cm-lineNumbers .cm-gutterElement": {
      minWidth: "32px",
      textAlign: "right",
      paddingRight: "8px",
      paddingLeft: "8px",
    },
    ".cm-foldGutter .cm-gutterElement": {
      color: "var(--muted)",
      cursor: "pointer",
      padding: "0 4px",
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
    },
    ".cm-foldGutter .cm-gutterElement:hover": {
      color: "var(--text)",
    },
    ".cm-foldPlaceholder": {
      backgroundColor: "var(--raised)",
      border: "1px solid var(--border)",
      color: "var(--muted)",
      borderRadius: "var(--radius-xs, 4px)",
      padding: "0 6px",
      margin: "0 4px",
      fontSize: "0.85em",
    },
    ".cm-activeLine": {
      backgroundColor: "var(--raised, rgba(0, 0, 0, 0.02))",
    },
    ".cm-matchingBracket": {
      backgroundColor: "var(--sunken)",
      outline: "1px solid var(--border-strong)",
      borderRadius: "2px",
      color: "var(--text)",
    },
    ".cm-nonmatchingBracket": {
      backgroundColor: "var(--danger-soft)",
      outline: "1px solid var(--danger-border)",
      color: "var(--danger)",
    },
    ".cm-panels": {
      backgroundColor: "var(--raised)",
      borderTop: "1px solid var(--border)",
      color: "var(--text)",
      zIndex: "5",
    },
    ".cm-panels.cm-panels-top": {
      borderTop: "none",
      borderBottom: "1px solid var(--border)",
    },
    ".cm-panel.cm-search": {
      padding: "6px 12px",
      display: "flex",
      flexWrap: "wrap",
      alignItems: "center",
      gap: "6px 10px",
      fontSize: "var(--fs-xs, 12px)",
      fontFamily: "var(--font)",
    },
    ".cm-panel.cm-search input": {
      backgroundColor: "var(--surface)",
      color: "var(--text)",
      border: "1px solid var(--border-input, var(--border))",
      borderRadius: "var(--radius-sm, 6px)",
      padding: "4px 8px",
      fontSize: "var(--fs-xs, 12px)",
      fontFamily: "var(--mono)",
      outline: "none",
    },
    ".cm-panel.cm-search input:focus": {
      borderColor: "var(--accent-strong, var(--text))",
    },
    ".cm-panel.cm-search button": {
      backgroundColor: "var(--surface)",
      color: "var(--text)",
      border: "1px solid var(--border)",
      borderRadius: "var(--radius-sm, 6px)",
      padding: "3px 8px",
      fontSize: "var(--fs-xs, 12px)",
      cursor: "pointer",
      transition: "background 120ms ease, border-color 120ms ease",
    },
    ".cm-panel.cm-search button:hover": {
      backgroundColor: "var(--sunken)",
      borderColor: "var(--border-strong)",
    },
    ".cm-panel.cm-search label": {
      display: "inline-flex",
      alignItems: "center",
      gap: "4px",
      cursor: "pointer",
      color: "var(--text-soft)",
    },
    ".cm-searchMatch": {
      backgroundColor: "var(--warn-soft, #fbf3db)",
      outline: "1px solid var(--warn-border, #efe0b0)",
    },
    ".cm-searchMatch.cm-searchMatch-selected": {
      backgroundColor: "var(--info-soft, #e1f3fe)",
      outline: "1px solid var(--info, #1f6c9f)",
    },
    ".cm-tooltip": {
      backgroundColor: "var(--surface)",
      border: "1px solid var(--border)",
      borderRadius: "var(--radius-sm, 6px)",
      color: "var(--text)",
      boxShadow: "var(--shadow, 0 2px 8px rgba(0, 0, 0, 0.08))",
      fontSize: "var(--fs-xs, 12px)",
      fontFamily: "var(--mono)",
    },
    ".cm-tooltip-autocomplete": {
      "& > ul": {
        maxHeight: "180px",
        fontFamily: "var(--mono)",
      },
      "& > ul > li": {
        padding: "4px 8px",
        borderRadius: "var(--radius-xs, 4px)",
      },
      "& > ul > li[aria-selected]": {
        backgroundColor: "var(--selection)",
        color: "var(--text)",
      },
    },
    ".cm-completionLabel": {
      color: "var(--text)",
    },
    ".cm-completionDetail": {
      color: "var(--muted)",
      fontStyle: "italic",
      marginLeft: "6px",
    },
    ".cm-completionMatchedText": {
      textDecoration: "none",
      fontWeight: "bold",
      color: "var(--accent-strong, var(--accent))",
    },
  });

  // Props definition with Svelte 5 runes
  let {
    value = $bindable(""),
    onsave,
    wrap = false,
    fontSize = 12.5,
    lineHeight = 1.6,
    tabSize = 2,
    tabIndents = true,
    spellcheck = false,
    fontFamily = "var(--mono)",
    ariaLabel = "HTML source code",
    readonly = false,
    class: className = "",
    ...restProps
  }: {
    value?: string;
    onsave?: () => void;
    wrap?: boolean;
    fontSize?: number | string;
    lineHeight?: number | string;
    tabSize?: number;
    tabIndents?: boolean;
    spellcheck?: boolean;
    fontFamily?: string;
    ariaLabel?: string;
    readonly?: boolean;
    class?: string;
    [key: string]: unknown;
  } = $props();

  let editorContainer: HTMLDivElement | undefined = $state();
  let view: EditorView | undefined = $state();

  /**
   * Exported focus method to allow parents to focus the editor.
   */
  export function focus() {
    view?.focus();
  }

  /**
   * Expose editor instance if needed by caller.
   */
  export function getView(): EditorView | undefined {
    return view;
  }

  // Compartments for live props reconfiguration
  const wrapCompartment = new Compartment();
  const tabSizeCompartment = new Compartment();
  const indentUnitCompartment = new Compartment();
  const tabIndentsCompartment = new Compartment();
  const readonlyCompartment = new Compartment();
  const editableCompartment = new Compartment();
  const attributesCompartment = new Compartment();
  const styleThemeCompartment = new Compartment();

  onMount(() => {
    if (!editorContainer) return;

    const customKeymap = Prec.highest(
      keymap.of([
        {
          key: "Mod-s",
          run: () => {
            if (onsave) {
              onsave();
              return true;
            }
            return false;
          },
          preventDefault: true,
        },
        {
          key: "Escape",
          run: (v) => {
            // Close search panel if open.
            // If closed, return false so the Escape keydown event propagates outside the editor.
            if (closeSearchPanel(v)) {
              return true;
            }
            return false;
          },
        },
      ]),
    );

    const state = EditorState.create({
      doc: value ?? "",
      extensions: [
        // Line numbers and active line gutter
        lineNumbers(),
        highlightActiveLineGutter(),
        // Code folding
        foldGutter(),
        // Selection & active line
        highlightSpecialChars(),
        drawSelection(),
        dropCursor(),
        rectangularSelection(),
        crosshairCursor(),
        highlightActiveLine(),
        // HTML language support (includes autoCloseTags)
        html({ autoCloseTags: true }),
        // Syntax highlighting
        syntaxHighlighting(htmlHighlightStyle),
        syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
        // Brackets
        bracketMatching(),
        closeBrackets(),
        indentOnInput(),
        // History (undo / redo)
        history(),
        // Autocomplete
        autocompletion(),
        // Search & find/replace
        search({ top: true }),
        highlightSelectionMatches(),
        // Keymaps
        customKeymap,
        keymap.of(closeBracketsKeymap),
        keymap.of(defaultKeymap),
        keymap.of(searchKeymap),
        keymap.of(historyKeymap),
        keymap.of(foldKeymap),
        keymap.of(completionKeymap),
        // Base theme
        editorTheme,
        // Live Compartments with initial values
        wrapCompartment.of(wrap ? EditorView.lineWrapping : []),
        tabSizeCompartment.of(EditorState.tabSize.of(tabSize)),
        indentUnitCompartment.of(indentUnit.of(" ".repeat(Math.max(1, tabSize)))),
        tabIndentsCompartment.of(tabIndents ? keymap.of([indentWithTab]) : []),
        readonlyCompartment.of(EditorState.readOnly.of(readonly)),
        editableCompartment.of(EditorView.editable.of(!readonly)),
        attributesCompartment.of(
          EditorView.contentAttributes.of({
            "aria-label": ariaLabel || "HTML source code",
            spellcheck: spellcheck ? "true" : "false",
            autocapitalize: "off",
            autocorrect: "off",
          }),
        ),
        styleThemeCompartment.of(getDynamicTheme(fontSize, lineHeight, fontFamily)),
        // Document update listener: syncs CodeMirror changes to bindable `value`
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            const next = update.state.doc.toString();
            if (next !== value) {
              value = next;
            }
          }
        }),
      ],
    });

    view = new EditorView({
      state,
      parent: editorContainer,
    });

    return () => {
      if (view) {
        view.destroy();
        view = undefined;
      }
    };
  });

  onDestroy(() => {
    if (view) {
      view.destroy();
      view = undefined;
    }
  });

  // External value change: update document without adding an undo step to history, preserving selection
  $effect(() => {
    const currentVal = value ?? "";
    if (view) {
      const currentDoc = view.state.doc.toString();
      if (currentVal !== currentDoc) {
        const curSel = view.state.selection.main;
        const sel = clampSelection(curSel, currentVal.length);

        view.dispatch({
          changes: { from: 0, to: view.state.doc.length, insert: currentVal },
          selection: sel,
          annotations: [Transaction.addToHistory.of(false)],
        });
      }
    }
  });

  // Live prop reconfiguration: wrap
  $effect(() => {
    if (!view) return;
    view.dispatch({
      effects: wrapCompartment.reconfigure(wrap ? EditorView.lineWrapping : []),
    });
  });

  // Live prop reconfiguration: tabSize
  $effect(() => {
    if (!view) return;
    view.dispatch({
      effects: [
        tabSizeCompartment.reconfigure(EditorState.tabSize.of(tabSize)),
        indentUnitCompartment.reconfigure(indentUnit.of(" ".repeat(Math.max(1, tabSize)))),
      ],
    });
  });

  // Live prop reconfiguration: tabIndents
  $effect(() => {
    if (!view) return;
    view.dispatch({
      effects: tabIndentsCompartment.reconfigure(tabIndents ? keymap.of([indentWithTab]) : []),
    });
  });

  // Live prop reconfiguration: readonly
  $effect(() => {
    if (!view) return;
    view.dispatch({
      effects: [
        readonlyCompartment.reconfigure(EditorState.readOnly.of(readonly)),
        editableCompartment.reconfigure(EditorView.editable.of(!readonly)),
      ],
    });
  });

  // Live prop reconfiguration: spellcheck & ariaLabel
  $effect(() => {
    if (!view) return;
    view.dispatch({
      effects: attributesCompartment.reconfigure(
        EditorView.contentAttributes.of({
          "aria-label": ariaLabel || "HTML source code",
          spellcheck: spellcheck ? "true" : "false",
          autocapitalize: "off",
          autocorrect: "off",
        }),
      ),
    });
  });

  // Live prop reconfiguration: fontSize, lineHeight, fontFamily
  $effect(() => {
    if (!view) return;
    view.dispatch({
      effects: styleThemeCompartment.reconfigure(getDynamicTheme(fontSize, lineHeight, fontFamily)),
    });
  });
</script>

<div
  class="html-editor {className}"
  bind:this={editorContainer}
  style:--ed-font={fontFamily}
  style:--ed-size={typeof fontSize === "number" ? `${fontSize}px` : fontSize}
  style:--ed-lh={typeof lineHeight === "number" ? `${lineHeight}` : lineHeight}
  style:--ed-tab={tabSize}
  {...restProps}
></div>

<style>
  .html-editor {
    flex: 1;
    min-height: 0;
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    position: relative;
    background: var(--surface);
    color: var(--text);
    font-family: var(--ed-font, var(--mono));
    font-size: var(--ed-size, 12.5px);
    line-height: var(--ed-lh, 1.6);
    tab-size: var(--ed-tab, 2);
    overflow: hidden;
  }

  .html-editor :global(.cm-editor) {
    height: 100%;
    width: 100%;
    flex: 1;
    min-height: 0;
    outline: none;
    background: var(--surface);
    color: var(--text);
    font-family: inherit;
    font-size: inherit;
    line-height: inherit;
    tab-size: inherit;
  }

  .html-editor :global(.cm-scroller) {
    height: 100%;
    width: 100%;
    overflow: auto;
    font-family: inherit;
    font-size: inherit;
    line-height: inherit;
    tab-size: inherit;
    overflow-wrap: anywhere;
  }

  .html-editor :global(.cm-content) {
    padding: 14px 0;
  }

  .html-editor :global(.cm-line) {
    padding: 0 16px 0 12px;
  }
</style>
