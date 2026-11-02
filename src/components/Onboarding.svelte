<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { api } from "../lib/api";
  import { app, reloadPages, toast } from "../lib/state.svelte";
  import Icon from "../lib/Icon.svelte";
  import Illustration from "../lib/Illustration.svelte";
  import { t } from "../lib/i18n.svelte";

  /** Welcome, then choose/create, then opening. */
  let step = $state<0 | 1 | 2>(0);
  let name = $state("Herbarium");
  /** Start a new vault with the sample pages and their reading path. */
  let withSamples = $state(true);
  let busy = $state<"create" | "open" | null>(null);
  let error = $state("");
  let pendingName = $state("");
  let nameEl: HTMLInputElement | undefined = $state();

  // The step entering gets the focus, so Enter and Tab always continue naturally.
  $effect(() => {
    if (step === 1) queueMicrotask(() => nameEl?.focus());
  });

  function begin() {
    step = 1;
    error = "";
  }

  function back() {
    if (busy) return;
    step = 0;
    error = "";
  }

  /** Enter moves from the welcome card to the vault choices. */
  function onKeydown(e: KeyboardEvent) {
    if (step !== 0 || e.key !== "Enter") return;
    if ((e.target as HTMLElement | null)?.closest("button, input, select, textarea, a")) return;
    e.preventDefault();
    begin();
  }

  async function createNew() {
    if (busy) return;
    error = "";
    let parent: string | string[] | null;
    try {
      parent = await open({
        directory: true,
        title: t("onboard.createDialog"),
      });
    } catch (e) {
      error = String(e);
      return;
    }
    if (typeof parent !== "string") return;
    const folderName = name.trim() || "Herbarium";
    pendingName = folderName;
    busy = "create";
    step = 2;
    try {
      app.config = await api.createVault(parent, folderName);
      if (withSamples) {
        try {
          await api.addSamples();
        } catch (e) {
          console.error(e);
        }
      }
      await reloadPages();
      toast(t("toast.vaultCreated", { name: folderName }), "success");
    } catch (e) {
      error = String(e);
      step = 1;
    } finally {
      busy = null;
    }
  }

  async function openExisting() {
    if (busy) return;
    error = "";
    let picked: string | string[] | null;
    try {
      picked = await open({
        directory: true,
        title: t("onboard.openDialog"),
      });
    } catch (e) {
      error = String(e);
      return;
    }
    if (typeof picked !== "string") return;
    pendingName = picked;
    busy = "open";
    step = 2;
    try {
      app.config = await api.setVault(picked);
      await reloadPages();
      toast(t("toast.vaultOpened"), "success");
    } catch (e) {
      error = String(e);
      step = 1;
    } finally {
      busy = null;
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="onboard">
  <div class="stage">
    {#key step}
      <div
        class="step"
        class:welcome={step === 0}
        in:fly={{ x: 16, duration: 240, easing: cubicOut }}
        out:fly={{ x: -16, duration: 240, easing: cubicOut }}
      >
        {#if step === 0}
          <Illustration name="pressed-flower" size={140} animate />
          <h1 class="display">{t("onboard.welcome")}</h1>
          <p class="lede">{t("onboard.intro")}</p>
          <button class="btn btn-primary continue" onclick={begin}>
            {t("onboard.continue")}
            <kbd class="kbd">↵</kbd>
          </button>
        {:else if step === 1}
          <h1 class="display">{t("onboard.vaultStepTitle")}</h1>
          <p class="lede">{t("onboard.vaultStepHint")}</p>

          <div class="cards">
            <section class="card option">
              <span class="option-icon"><Icon name="file-plus" size={18} /></span>
              <h2>{t("onboard.createTitle")}</h2>
              <p>{t("onboard.createText")}</p>
              <div class="name-row">
                <input
                  bind:this={nameEl}
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
              <label class="samples">
                <input type="checkbox" bind:checked={withSamples} disabled={busy !== null} />
                {t("onboard.samples")}
              </label>
            </section>

            <section class="card option">
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
            <p class="error" role="alert">{error}</p>
          {/if}

          <button class="btn btn-ghost back" onclick={back} disabled={busy !== null}>
            {t("common.back")}
          </button>
        {:else}
          <Illustration name="watering-can" size={110} animate />
          <h1 class="display">{t("onboard.openingTitle")}</h1>
          <p class="lede">
            {busy === "create"
              ? t("onboard.openingCreate", { name: pendingName })
              : t("onboard.openingOpen")}
          </p>
          <p class="opening-status">
            <span class="spinner" aria-hidden="true"></span>
            {busy === "create" ? t("onboard.creating") : t("onboard.opening")}
          </p>
        {/if}
      </div>
    {/key}
  </div>

  <div class="dots" role="group" aria-label={t("onboard.stepsLabel")}>
    {#each [0, 1, 2] as i (i)}
      <span class="dot" class:active={step === i} class:past={step > i} aria-hidden="true"></span>
    {/each}
    <span class="sr-only">{t("onboard.stepOf", { current: step + 1, total: 3 })}</span>
  </div>
</div>

<style>
  .onboard {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 36px;
    height: 100%;
    padding: 56px 32px;
    overflow-y: auto;
    background-color: var(--bg);
    background-image: radial-gradient(
      ellipse 55% 45% at 50% 0%,
      var(--glow),
      transparent 70%
    );
  }

  /* Both steps share one cell: the outgoing one can leave while the new one enters. */
  .stage {
    display: grid;
    width: 100%;
    justify-items: center;
  }
  .stage > .step {
    grid-area: 1 / 1;
  }

  .step {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 18px;
    width: 100%;
    max-width: 720px;
    text-align: center;
  }
  .step.welcome {
    max-width: 540px;
  }

  h1 {
    font-size: calc(var(--fs-4xl) * var(--display-size-adjust));
    letter-spacing: var(--display-tracking);
  }
  .welcome h1 {
    font-size: calc(var(--fs-5xl) * var(--display-size-adjust));
    letter-spacing: var(--display-tracking);
  }

  .lede {
    font-size: var(--fs-md);
    color: var(--muted);
    max-width: 480px;
  }

  .continue {
    margin-top: 6px;
  }
  .continue .kbd {
    margin-left: 2px;
  }

  .cards {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 320px));
    gap: 16px;
    justify-content: center;
    width: 100%;
    margin-top: 8px;
  }

  .option {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 32px;
    text-align: left;
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
    font-variation-settings: var(--font-display-settings);
    font-size-adjust: none;
    font-size: calc(var(--fs-xl) * var(--display-size-adjust));
    font-weight: var(--display-weight);
    letter-spacing: var(--display-tracking);
    line-height: var(--display-leading);
  }

  .option p {
    flex: 1;
    font-size: var(--fs-sm);
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
    font-size: var(--fs-sm);
    max-width: 480px;
    overflow-wrap: anywhere;
  }

  .back {
    margin-top: 2px;
  }

  .opening-status {
    display: flex;
    align-items: center;
    gap: 8px;
    font-family: var(--mono);
    font-variation-settings: var(--font-mono-settings);
    font-size: var(--fs-sm);
    color: var(--muted);
    margin-top: 6px;
  }

  .dots {
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .dot {
    width: 5px;
    height: 5px;
    border-radius: 999px;
    background: var(--border-hover);
    transform: scaleX(1);
    transition:
      transform var(--t-med) var(--ease-out),
      background-color var(--t-fast) var(--ease-out);
  }
  .dot.past {
    background: var(--muted);
  }
  .dot.active {
    background: var(--text);
    transform: scaleX(2.4);
  }

  @media (max-width: 720px) {
    .cards {
      grid-template-columns: minmax(0, 420px);
    }
    h1,
    .welcome h1 {
      font-size: var(--fs-3xl);
    }
  }
  .samples {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--fs-sm);
    color: var(--text-soft);
    cursor: pointer;
  }
</style>
