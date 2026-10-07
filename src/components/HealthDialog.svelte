<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { app, errorMessage, openPage } from "../lib/state.svelte";
  import { t } from "../lib/i18n.svelte";
  import type { HealthIssue } from "../lib/types";
  import Icon from "../lib/Icon.svelte";

  /** Every page that may not work, and why. */
  let pages = $state<Array<{ id: string; title: string; folder: string | null; issues: HealthIssue[] }> | null>(null);
  let error = $state<string | null>(null);
  let closeBtn: HTMLButtonElement | undefined = $state();

  function close() {
    app.healthOpen = false;
  }

  async function open(id: string) {
    close();
    await openPage(id);
  }

  function summary(issues: HealthIssue[]): string {
    return issues
      .filter((i) => i.level !== "info")
      .map((i) => t(`health.short.${i.kind}`))
      .join(" · ");
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      close();
    }
  }

  onMount(async () => {
    closeBtn?.focus();
    try {
      pages = (await api.vaultHealth()).pages;
    } catch (e) {
      error = errorMessage(e);
    }
  });
</script>

<svelte:window onkeydown={onKey} />

<div class="overlay" role="presentation" onmousedown={(e) => e.target === e.currentTarget && close()}>
  <div class="dialog card" role="dialog" aria-modal="true" aria-labelledby="health-title">
    <header class="head">
      <h2 id="health-title">{t("health.dialogTitle")}</h2>
      <button bind:this={closeBtn} class="btn btn-ghost btn-icon" aria-label={t("common.close")} onclick={close}>
        <Icon name="x" size={15} />
      </button>
    </header>
    {#if error}
      <p class="err">{error}</p>
    {:else if pages === null}
      <div class="state"><span class="spinner"></span>{t("health.checking")}</div>
    {:else if pages.length === 0}
      <p class="ok"><Icon name="circle-check" size={16} />{t("health.allGood")}</p>
    {:else}
      <p class="muted">{t("health.found", { count: pages.length })}</p>
      <ul class="list">
        {#each pages as p (p.id)}
          <li>
            <button onclick={() => void open(p.id)}>
              <span class="title ellipsis">{p.title}</span>
              <span class="why">{summary(p.issues)}{#if p.folder} · {p.folder}{/if}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
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
    padding: 12vh 24px 24px;
    background: var(--scrim);
  }
  .dialog {
    width: 560px;
    max-width: 100%;
    max-height: 76vh;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 22px 24px;
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  h2 {
    margin: 0;
    font-size: var(--fs-lg);
    font-weight: 500;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .list li + li {
    border-top: 1px solid var(--border);
  }
  .list button {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    padding: 8px 12px;
    border: 0;
    background: none;
    color: var(--text);
    text-align: left;
    cursor: pointer;
  }
  .list button:hover {
    background: var(--sunken);
  }
  .title {
    font-size: var(--fs-sm);
    font-weight: 500;
  }
  .why {
    font-size: var(--fs-xs);
    color: var(--warn);
  }
  .ok {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--leaf);
    margin: 0;
  }
  .muted {
    margin: 0;
    color: var(--muted);
    font-size: var(--fs-sm);
  }
  .err {
    color: var(--danger);
  }
  .state {
    display: flex;
    gap: 10px;
    align-items: center;
    color: var(--muted);
  }
</style>
