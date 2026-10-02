<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/api";
  import { app, reloadPages, toast } from "../lib/state.svelte";
  import Icon from "../lib/Icon.svelte";
  import { t } from "../lib/i18n.svelte";

  let name = $state("Herbarium");
  let busy = $state<"create" | "open" | null>(null);
  let error = $state("");

  async function createNew() {
    if (busy) return;
    error = "";
    busy = "create";
    try {
      const parent = await open({
        directory: true,
        title: t("onboard.createDialog"),
      });
      if (typeof parent !== "string") return;
      const folderName = name.trim() || "Herbarium";
      app.config = await api.createVault(parent, folderName);
      await reloadPages();
      toast(t("toast.vaultCreated", { name: folderName }), "success");
    } catch (e) {
      error = String(e);
    } finally {
      busy = null;
    }
  }

  async function openExisting() {
    if (busy) return;
    error = "";
    busy = "open";
    try {
      const picked = await open({
        directory: true,
        title: t("onboard.openDialog"),
      });
      if (typeof picked !== "string") return;
      app.config = await api.setVault(picked);
      await reloadPages();
      toast(t("toast.vaultOpened"), "success");
    } catch (e) {
      error = String(e);
    } finally {
      busy = null;
    }
  }
</script>

<div class="onboard">
  <div class="hero">
    <span class="hero-mark"><Icon name="leaf" size={30} /></span>
    <h1>{t("onboard.welcome")}</h1>
    <p>
      {t("onboard.intro")}
    </p>
  </div>

  <div class="cards">
    <section class="card option">
      <span class="option-icon"><Icon name="file-plus" size={20} /></span>
      <h2>{t("onboard.createTitle")}</h2>
      <p>{t("onboard.createText")}</p>
      <div class="name-row">
        <input
          type="text"
          aria-label={t("onboard.nameLabel")}
          placeholder="Herbarium"
          maxlength={60}
          bind:value={name}
          disabled={busy !== null}
        />
        <button class="btn btn-primary" onclick={createNew} disabled={busy !== null}>
          {#if busy === "create"}
            <span class="spinner"></span>
          {:else}
            <Icon name="plus" size={14} />
          {/if}
          {busy === "create" ? t("onboard.creating") : t("onboard.create")}
        </button>
      </div>
    </section>

    <section class="card option">
      <span class="option-icon"><Icon name="folder-open" size={20} /></span>
      <h2>{t("onboard.openTitle")}</h2>
      <p>{t("onboard.openText")}</p>
      <button class="btn" onclick={openExisting} disabled={busy !== null}>
        {#if busy === "open"}
          <span class="spinner"></span>
        {:else}
          <Icon name="folder" size={14} />
        {/if}
        {busy === "open" ? t("onboard.opening") : t("onboard.choose")}
      </button>
    </section>
  </div>

  {#if error}
    <p class="error">{error}</p>
  {/if}
</div>

<style>
  .onboard {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 30px;
    height: 100%;
    padding: 32px;
    overflow-y: auto;
  }

  .hero {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    max-width: 480px;
    text-align: center;
  }

  .hero-mark {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 64px;
    height: 64px;
    margin-bottom: 4px;
    border-radius: var(--radius-lg);
    background: var(--accent-soft);
    color: var(--accent);
    box-shadow: var(--shadow-sm);
  }

  h1 {
    font-family: var(--font-display);
    font-size: 28px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .hero p {
    color: var(--muted);
  }

  .cards {
    display: flex;
    gap: 16px;
    flex-wrap: wrap;
    justify-content: center;
    width: 100%;
  }

  .option {
    display: flex;
    flex-direction: column;
    gap: 10px;
    width: 300px;
    max-width: 100%;
    padding: 20px;
  }

  .option-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 38px;
    height: 38px;
    border-radius: var(--radius-sm);
    background: var(--accent-soft);
    color: var(--accent);
  }

  h2 {
    font-size: 15px;
    font-weight: 600;
  }

  .option p {
    flex: 1;
    font-size: 13px;
    color: var(--muted);
  }

  .name-row {
    display: flex;
    gap: 8px;
  }

  .name-row input {
    flex: 1;
    min-width: 0;
  }

  .error {
    padding: 8px 14px;
    border-radius: var(--radius-sm);
    background: var(--danger-soft);
    color: var(--danger);
    font-size: 13px;
    max-width: 480px;
  }
</style>
