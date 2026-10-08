<script lang="ts">
  import { api } from "../lib/api";
  import { app, errorMessage, openSettings, toast } from "../lib/state.svelte";
  import { confirmAction } from "../lib/confirm.svelte";
  import { t } from "../lib/i18n.svelte";
  import { digestPng, shareCard } from "../lib/share";
  import type { PublishRecord, PublishTarget } from "../lib/types";
  import Icon from "../lib/Icon.svelte";
  import DropdownMenu, { type DropdownMenuItem } from "./DropdownMenu.svelte";

  let { id, title, html, disabled = false }: { id: string; title: string; html: string; disabled?: boolean } = $props();

  let records = $state<Partial<Record<PublishTarget, PublishRecord>>>({});
  let busy = $state<PublishTarget | null>(null);

  async function loadRecords(pageId: string) {
    try {
      const all = await api.publishedList();
      if (pageId === id) records = all[pageId] ?? {};
    } catch (e) {
      console.error(e);
    }
  }

  $effect(() => {
    void loadRecords(id);
  });

  async function ensureConnected(): Promise<string | null> {
    const gh = await api.githubSettings();
    if (gh.hasToken) return gh.login ?? "";
    toast(t("share.connectFirst"), "info", 8000, {
      label: t("remix.openSettings"),
      run: async () => {
        await openSettings("ai");
      },
    });
    return null;
  }

  /** Where the site is served: a `<login>.github.io` repository is the user's own site. */
  function siteAddress(login: string, repo: string): string {
    const origin = `https://${login.toLowerCase()}.github.io`;
    return repo.toLowerCase() === `${login.toLowerCase()}.github.io` ? `${origin}/` : `${origin}/${repo}/`;
  }

  async function publish(target: PublishTarget) {
    if (busy) return;
    const login = await ensureConnected().catch((e) => (toast(errorMessage(e), "error"), null));
    if (login === null) return;
    if (!records[target]) {
      const gh = await api.githubSettings();
      const ok = await confirmAction(
        target === "gist"
          ? { title: t("share.gistConfirmTitle"), message: t("share.gistConfirm"), confirmLabel: t("share.gistAction") }
          : {
              title: t("share.siteConfirmTitle"),
              message: t("share.siteConfirm", { url: siteAddress(login, gh.repo) }),
              confirmLabel: t("share.siteAction"),
            },
      );
      if (!ok) return;
    }
    busy = target;
    try {
      const record = (await api.publishPage(id, target)) as PublishRecord;
      records = { ...records, [target]: record };
      await navigator.clipboard?.writeText(record.url).catch(() => {});
      toast(target === "gist" ? t("share.gistDone") : t("share.siteDone"), "success", 10000, {
        label: t("common.open"),
        run: () => void api.openExternal(record.url),
      });
    } catch (e) {
      toast(`${t("share.failed")}: ${errorMessage(e)}`, "error");
    } finally {
      busy = null;
    }
  }

  async function unpublish(target: PublishTarget) {
    const ok = await confirmAction({
      title: target === "gist" ? t("share.deleteGistTitle") : t("share.removeSiteTitle"),
      message: target === "gist" ? t("share.deleteGistMessage") : t("share.removeSiteMessage"),
      confirmLabel: target === "gist" ? t("share.deleteGist") : t("share.removeSite"),
      danger: true,
    });
    if (!ok) return;
    busy = target;
    try {
      await api.publishPage(id, target, true);
      const next = { ...records };
      delete next[target];
      records = next;
      toast(t("share.unpublished"), "success");
    } catch (e) {
      toast(`${t("share.failed")}: ${errorMessage(e)}`, "error");
    } finally {
      busy = null;
    }
  }

  async function copyCard() {
    const url = records.site?.url ?? records.gist?.url ?? null;
    const digest = app.previews[id];
    const image = digest ? await digestPng(digest) : null;
    const card = shareCard({ title, url, html, image });
    try {
      await api.copyRich(card.html, card.text);
      toast(url ? t("share.cardCopied") : t("share.cardCopiedNoLink"), "success", url ? 4000 : 8000);
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  }

  async function copyLink(url: string) {
    try {
      await navigator.clipboard.writeText(url);
      toast(t("share.linkCopied"), "success");
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  }

  const items = $derived.by((): DropdownMenuItem[] => {
    const list: DropdownMenuItem[] = [
      { label: t("share.copyCard"), icon: "share", onclick: () => void copyCard() },
      { divider: true },
      { header: true, label: t("share.gistHeader") },
    ];
    const gist = records.gist;
    list.push({
      label: busy === "gist" ? t("share.working") : gist ? t("share.gistUpdate") : t("share.gistAction"),
      disabled: !!busy,
      onclick: () => void publish("gist"),
    });
    if (gist) {
      list.push(
        { label: t("share.open"), icon: "external-link", onclick: () => void api.openExternal(gist.url) },
        { label: t("share.copyLink"), onclick: () => void copyLink(gist.url) },
        { label: t("share.deleteGist"), danger: true, disabled: !!busy, onclick: () => void unpublish("gist") },
      );
    }
    list.push({ divider: true }, { header: true, label: t("share.siteHeader") });
    const site = records.site;
    list.push({
      label: busy === "site" ? t("share.working") : site ? t("share.siteUpdate") : t("share.siteAction"),
      disabled: !!busy,
      onclick: () => void publish("site"),
    });
    if (site) {
      list.push(
        { label: t("share.open"), icon: "external-link", onclick: () => void api.openExternal(site.url) },
        { label: t("share.copyLink"), onclick: () => void copyLink(site.url) },
        { label: t("share.removeSite"), danger: true, disabled: !!busy, onclick: () => void unpublish("site") },
      );
    }
    return list;
  });
</script>

<DropdownMenu {items} align="right" ariaLabel={t("share.label")}>
  {#snippet trigger({ open, toggle })}
    <button
      class="btn btn-sm details-btn"
      class:active={open || !!records.gist || !!records.site}
      aria-expanded={open}
      aria-haspopup="menu"
      onclick={toggle}
      {disabled}
      title={t("share.hint")}
    >
      {#if busy}<span class="spinner small"></span>{:else}<Icon name="share" size={14} />{/if}
      {t("share.label")}
    </button>
  {/snippet}
</DropdownMenu>

<style>
  .details-btn.active {
    background: var(--sunken);
    border-color: var(--border-hover);
    color: var(--accent-strong);
  }
  .small {
    width: 12px;
    height: 12px;
  }
</style>
