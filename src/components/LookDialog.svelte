<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { app, errorMessage, toast } from "../lib/state.svelte";
  import { ICON_CHOICES, LABEL_COLORS } from "../lib/appearance";
  import { t } from "../lib/i18n.svelte";
  import type { LabelColor } from "../lib/types";
  import Icon from "../lib/Icon.svelte";
  import { confirmState } from "../lib/confirm.svelte";
  import { folderPickerState } from "../lib/folder-picker.svelte";

  /** Edit a folder's icon and colour, or a tag's colour. */
  const target = app.lookEdit!;
  const current = target.kind === "folder" ? app.appearance.folders[target.key] : { color: app.appearance.tags[target.key] };
  let icon = $state(target.kind === "folder" ? (current?.icon ?? "") : "");
  let color = $state<LabelColor | null>(current?.color ?? null);
  let saving = $state(false);
  let first: HTMLButtonElement | undefined = $state();

  function close() {
    app.lookEdit = null;
  }

  async function save() {
    saving = true;
    try {
      app.appearance =
        target.kind === "folder"
          ? await api.setFolderLook(target.key, icon.trim() || null, color)
          : await api.setTagColor(target.key, color);
      close();
    } catch (e) {
      toast(`${t("look.failed")}: ${errorMessage(e)}`, "error");
    } finally {
      saving = false;
    }
  }

  // Capture phase: the window-level shortcuts run first otherwise, and their
  // Escape would also close the reader or leave Settings.
  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape" && !app.paletteOpen && !confirmState.pending && !folderPickerState.pending) {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  }

  onMount(() => first?.focus());
</script>

<svelte:window onkeydowncapture={onKey} />

<div class="overlay" role="presentation" onmousedown={(e) => e.target === e.currentTarget && close()}>
  <div class="dialog card" role="dialog" aria-modal="true" aria-labelledby="look-title">
    <h2 id="look-title">
      {target.kind === "folder" ? t("look.folderTitle", { name: target.key }) : t("look.tagTitle", { name: target.key })}
    </h2>

    {#if target.kind === "folder"}
      <div class="field">
        <span class="label">{t("look.icon")}</span>
        <div class="icons">
          <button bind:this={first} class="icon-choice" class:on={!icon} onclick={() => (icon = "")} title={t("look.noIcon")}>
            <Icon name="folder" size={14} />
          </button>
          {#each ICON_CHOICES as choice (choice)}
            <button class="icon-choice" class:on={icon === choice} onclick={() => (icon = choice)}>{choice}</button>
          {/each}
        </div>
        <input type="text" bind:value={icon} maxlength="8" placeholder={t("look.iconPlaceholder")} aria-label={t("look.icon")} />
      </div>
    {/if}

    <div class="field">
      <span class="label">{t("look.color")}</span>
      <div class="swatches">
        <button class="swatch none" class:on={!color} onclick={() => (color = null)} title={t("look.noColor")} aria-label={t("look.noColor")}></button>
        {#each LABEL_COLORS as c (c)}
          <button
            class="swatch"
            class:on={color === c}
            data-color={c}
            onclick={() => (color = c)}
            title={t(`look.color.${c}`)}
            aria-label={t(`look.color.${c}`)}
          ></button>
        {/each}
      </div>
      <span class="preview">
        <span class="chip" data-color={color ?? undefined}>{icon ? `${icon} ` : ""}{target.key}</span>
      </span>
    </div>

    <footer class="foot">
      <button class="btn" onclick={close}>{t("common.cancel")}</button>
      <button class="btn btn-primary" onclick={save} disabled={saving}>{t("look.save")}</button>
    </footer>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: var(--z-modal);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding: 14vh 24px 24px;
    background: var(--scrim);
  }
  .dialog {
    width: 400px;
    max-width: 100%;
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 22px 24px;
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
  }
  h2 {
    margin: 0;
    font-size: var(--fs-lg);
    font-weight: 500;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .label {
    font-size: var(--fs-xs);
    color: var(--muted);
  }
  .icons {
    display: grid;
    grid-template-columns: repeat(9, 1fr);
    gap: 4px;
  }
  .icon-choice {
    display: grid;
    place-items: center;
    height: 30px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: var(--sunken);
    color: var(--muted);
    font-size: 16px;
    cursor: pointer;
  }
  .icon-choice.on {
    border-color: var(--leaf);
    background: var(--surface);
  }
  .swatches {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .swatch {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    border: 2px solid var(--surface);
    outline: 1px solid var(--border);
    background: var(--chip-ink);
    cursor: pointer;
  }
  .swatch.none {
    background: linear-gradient(135deg, var(--sunken) 45%, var(--border) 45% 55%, var(--sunken) 55%);
  }
  .swatch.on {
    outline: 2px solid var(--text);
  }
  .preview {
    font-size: var(--fs-sm);
  }
  .foot {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
