<script lang="ts">
  import { t } from "../../lib/i18n.svelte";
  import Icon from "../../lib/Icon.svelte";

  /** A command or path to paste somewhere else, with a Copy button. */
  let { text, label }: { text: string; label?: string } = $props();

  let copied = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function copy() {
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
      clearTimeout(timer);
      timer = setTimeout(() => (copied = false), 1800);
    } catch (e) {
      console.error(e);
    }
  }
</script>

<div class="cmd">
  <code aria-label={label}>{text}</code>
  <button class="btn btn-xs" type="button" onclick={copy} aria-live="polite">
    <Icon name={copied ? "check" : "files"} size={12} />{copied ? t("setup.copied") : t("setup.copy")}
  </button>
</div>

<style>
  .cmd {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    max-width: 100%;
    padding: 4px 4px 4px 10px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    background: var(--bg);
  }
  code {
    flex: 1;
    min-width: 0;
    white-space: normal;
    overflow-wrap: anywhere;
    font-family: var(--mono);
    font-size: var(--fs-sm);
    color: var(--text);
  }
  .btn {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
</style>
