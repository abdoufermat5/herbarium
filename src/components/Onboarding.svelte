<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/api";
  import { app, reloadPages, toast } from "../lib/state.svelte";
  import Icon from "../lib/Icon.svelte";
  import { reveal } from "../lib/reveal";
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
  <div class="hero" use:reveal>
    <span class="hero-mark"><Icon name="leaf" size={22} /></span>
    <h1 class="display">{t("onboard.welcome")}</h1>
    <p>
      {t("onboard.intro")}
    </p>
  </div>

  <div class="cards">
    <section class="card option" use:reveal>
      <span class="option-icon"><Icon name="file-plus" size={18} /></span>
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
            <Icon name="plus" size={13} />
          {/if}
          {busy === "create" ? t("onboard.creating") : t("onboard.create")}
        </button>
      </div>
    </section>

    <section class="card option" use:reveal>
      <span class="option-icon"><Icon name="folder-open" size={18} /></span>
      <h2>{t("onboard.openTitle")}</h2>
      <p>{t("onboard.openText")}</p>
      <div class="name-row">
        <button class="btn" onclick={openExisting} disabled={busy !== null}>
          {#if busy === "open"}
            <span class="spinner"></span>
          {:else}
            <Icon name="folder" size={13} />
          {/if}
          {busy === "open" ? t("onboard.opening") : t("onboard.choose")}
        </button>
      </div>
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
    gap: 56px;
    height: 100%;
    padding: 64px 32px;
    overflow-y: auto;
    background-color: var(--bg);
    background-image: radial-gradient(
      ellipse 55% 45% at 50% 0%,
      var(--glow),
      transparent 70%
    );
  }

  .hero {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    max-width: 560px;
    text-align: center;
  }

  .hero-mark {
    display: grid;
    place-items: center;
    width: 48px;
    height: 48px;
    margin-bottom: 8px;
    border-radius: var(--radius);
    background: var(--leaf-soft);
    color: var(--leaf);
  }

  h1 {
    font-size: 52px;
    letter-spacing: -0.035em;
  }

  .hero p {
    font-size: 15px;
    color: var(--muted);
    max-width: 460px;
  }

  .cards {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 320px));
    gap: 16px;
    justify-content: center;
    width: 100%;
  }

  .option {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 32px;
  }

  .option-icon {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    margin-bottom: 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--raised);
    color: var(--text);
  }

  h2 {
    font-family: var(--font-display);
    font-size: 22px;
    font-weight: 500;
    letter-spacing: -0.02em;
    line-height: 1.2;
  }

  .option p {
    flex: 1;
    font-size: 13px;
    color: var(--muted);
  }

  .name-row {
    display: flex;
    gap: 8px;
    margin-top: 14px;
  }

  .name-row input {
    flex: 1;
    min-width: 0;
  }

  .error {
    padding: 10px 14px;
    border-radius: var(--radius-sm);
    background: var(--danger-soft);
    color: var(--danger);
    font-size: 13px;
    max-width: 480px;
  }

  @media (max-width: 720px) {
    .cards {
      grid-template-columns: minmax(0, 420px);
    }
  }
</style>
