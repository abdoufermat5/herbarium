<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../../lib/api";
  import { errorMessage, toast } from "../../lib/state.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { GithubSettings } from "../../lib/types";
  import Icon from "../../lib/Icon.svelte";
  import SettingsSection from "./SettingsSection.svelte";
  import SettingRow from "./SettingRow.svelte";

  const TOKEN_URL = "https://github.com/settings/tokens/new?scopes=gist,public_repo&description=Herbarium";

  let settings = $state<GithubSettings | null>(null);
  let tokenDraft = $state("");
  let repoDraft = $state("herbarium-pages");
  let saving = $state(false);

  async function save(token?: string) {
    if (!settings || saving) return;
    saving = true;
    try {
      settings = await api.setGithub(repoDraft.trim() || settings.repo, token);
      repoDraft = settings.repo;
      if (token !== undefined) {
        tokenDraft = "";
        toast(token ? t("publish.connectedAs", { login: settings.login ?? "?" }) : t("publish.disconnected"), "success");
      }
    } catch (e) {
      toast(`${t("publish.saveFailed")}: ${errorMessage(e)}`, "error");
      repoDraft = settings?.repo ?? "herbarium-pages";
    } finally {
      saving = false;
    }
  }

  function commitRepo() {
    if (settings && repoDraft.trim() && repoDraft.trim() !== settings.repo) void save();
  }

  onMount(async () => {
    try {
      settings = await api.githubSettings();
      repoDraft = settings.repo;
    } catch (e) {
      console.error(e);
    }
  });
</script>

<SettingsSection id="settings-publish" title={t("publish.title")} note={t("publish.note")}>
  <SettingRow
    title={t("publish.account")}
    hint={settings?.hasToken ? t("publish.accountIsSet", { login: settings.login ?? "?" }) : t("publish.accountHint")}
  >
    <form class="inline" onsubmit={(e) => (e.preventDefault(), tokenDraft.trim() && void save(tokenDraft))}>
      {#if settings?.hasToken}
        <span class="who"><Icon name="circle-check" size={13} />{settings.login}</span>
        <button class="btn btn-sm btn-ghost" type="button" onclick={() => void save("")} disabled={saving}>
          {t("publish.disconnect")}
        </button>
      {:else}
        <input
          type="password"
          bind:value={tokenDraft}
          placeholder="ghp_…"
          aria-label={t("publish.token")}
          autocomplete="off"
          spellcheck="false"
          disabled={!settings || saving}
        />
        <button class="btn btn-sm" type="submit" disabled={!tokenDraft.trim() || saving}>
          {saving ? t("publish.checking") : t("publish.connect")}
        </button>
        <button class="btn btn-sm btn-ghost" type="button" onclick={() => void api.openExternal(TOKEN_URL)}>
          <Icon name="external-link" size={12} />{t("publish.createToken")}
        </button>
      {/if}
    </form>
  </SettingRow>
  <SettingRow
    title={t("publish.repo")}
    hint={settings?.login
      ? t("publish.repoHintFor", { url: `https://${settings.login.toLowerCase()}.github.io/${settings.repo}/` })
      : t("publish.repoHint")}
  >
    <input
      class="repo"
      type="text"
      bind:value={repoDraft}
      onchange={commitRepo}
      aria-label={t("publish.repo")}
      spellcheck="false"
      disabled={!settings || saving}
    />
  </SettingRow>
</SettingsSection>

<style>
  .inline {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
  }
  .inline input,
  .repo {
    width: 220px;
    font-family: var(--mono);
    font-size: var(--fs-sm);
  }
  .inline .btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .who {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--leaf);
    font-size: var(--fs-sm);
  }
</style>
