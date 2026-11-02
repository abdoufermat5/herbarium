<script lang="ts">
  // Plant a reading path (or tend one): pick pages on the left, name the path
  // and order its pages in the bed, and watch the tree it will grow on the
  // right. The tree is seeded by the real id the backend will use, so the
  // preview is the tree that will actually grow.
  import { onMount, tick, untrack } from "svelte";
  import { flip } from "svelte/animate";
  import {
    app,
    createPath,
    inFolder,
    openPathView,
    refreshPathProgress,
    savePath,
  } from "../lib/state.svelte";
  import { pathSlug } from "../lib/grove";
  import { confirmAction, confirmState } from "../lib/confirm.svelte";
  import { t } from "../lib/i18n.svelte";
  import { modKey } from "../lib/format";
  import { reducedMotion } from "../lib/motion";
  import Icon from "../lib/Icon.svelte";
  import Illustration from "../lib/Illustration.svelte";
  import DropdownMenu, { type DropdownMenuItem } from "./DropdownMenu.svelte";
  import PathTree from "./PathTree.svelte";
  import VitalityLeaf from "./VitalityLeaf.svelte";
  import type { PageMeta } from "../lib/types";

  const MAX_ROWS = 200;
  const MAX_NAME = 100;

  const request = untrack(() => app.plant);
  const tendId = request?.pathId ?? null;
  const tendPath = tendId ? (app.paths.find((p) => p.id === tendId) ?? null) : null;
  const initialPages = untrack(() => (tendPath ? [...tendPath.pages] : [...(request?.pages ?? [])]));
  const initialName = untrack(() => tendPath?.name ?? "");
  const initialDescription = untrack(() => tendPath?.description ?? "");

  let name = $state(initialName);
  let description = $state(initialDescription);
  let chosen = $state<string[]>(initialPages);
  let pickedOrder = $state<string[]>([...initialPages]);
  let search = $state("");
  let active = $state(-1);
  let hovered = $state<number | null>(null);
  let sortMode = $state<"picked" | "title" | "oldest" | null>(null);
  let saving = $state(false);
  let closing = $state(false);

  const reduced = reducedMotion();
  const collator = new Intl.Collator(undefined, { sensitivity: "base", numeric: true });
  const norm = (s: string) => s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
  const titleOf = (p: PageMeta) => p.title || t("common.untitled");

  const byId = $derived(new Map(app.library.map((p) => [p.id, p])));
  const chosenIndex = $derived(new Map(chosen.map((id, i) => [id, i])));
  const count = $derived(chosen.length);

  // One normalized haystack per page, built once per library change: filtering
  // thousands of pages stays a single `includes` per row in the derived below.
  const entries = $derived(
    app.library.map((meta) => ({
      meta,
      hay: norm(`${meta.title}\n${meta.folder ?? ""}\n${meta.tags.join("\n")}`),
    })),
  );
  const needle = $derived(norm(search.trim()));
  const matches = $derived(needle ? entries.filter((e) => e.hay.includes(needle)) : entries);
  const shown = $derived(matches.slice(0, MAX_ROWS));
  const hidden = $derived(matches.length - shown.length);

  const bedRows = $derived(
    chosen.map((id, i) => {
      const meta = byId.get(id) ?? null;
      return { id, i, meta, title: meta ? titleOf(meta) : id };
    }),
  );

  // The suggested name: the folder the picked pages share, else their most
  // common tag, else the neutral fallback.
  const suggestion = $derived.by(() => {
    const metas: PageMeta[] = [];
    for (const id of chosen) {
      const meta = byId.get(id);
      if (meta) metas.push(meta);
    }
    if (metas.length === 0) return t("plant.nameFallback");
    const folder = metas[0].folder;
    if (folder && metas.every((m) => m.folder === folder)) {
      return folder.split("/").filter(Boolean).pop() ?? folder;
    }
    const counts = new Map<string, number>();
    for (const meta of metas) for (const tag of meta.tags) counts.set(tag, (counts.get(tag) ?? 0) + 1);
    let best: string | null = null;
    for (const [tag, n] of counts) {
      if (best === null || n > counts.get(best)! || (n === counts.get(best)! && collator.compare(tag, best) < 0)) {
        best = tag;
      }
    }
    return best ?? t("plant.nameFallback");
  });

  const effectiveName = $derived(name.trim() || suggestion);
  const nameTooLong = $derived([...name.trim()].length > MAX_NAME);

  // The preview seed: the real id the backend would give this name (or the
  // existing id when tending). Debounced so typing replants the tree at most
  // every 200 ms instead of on every keystroke.
  const takenIds = $derived(app.paths.filter((p) => p.id !== tendId).map((p) => p.id));
  const seed = $derived(tendId ?? pathSlug(effectiveName, takenIds));

  let treeSeed = $state(untrack(() => seed));
  let leavingSeed = $state<string | null>(null);
  let treeKey = $state(0);
  let seedTimer: ReturnType<typeof setTimeout> | undefined;
  let leaveTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    const next = seed;
    clearTimeout(seedTimer);
    if (untrack(() => treeSeed) === next) return;
    seedTimer = setTimeout(() => {
      const old = untrack(() => treeSeed);
      if (old === next) return;
      treeSeed = next;
      if (reduced) return;
      leavingSeed = old;
      treeKey += 1;
      clearTimeout(leaveTimer);
      leaveTimer = setTimeout(() => (leavingSeed = null), 130);
    }, 200);
  });

  /* ------------------------------------------------------------- picking */

  function addMany(ids: string[]) {
    if (ids.length === 0) return;
    const next = [...chosen];
    const picked = [...pickedOrder];
    const seen = new Set(chosen);
    for (const id of ids) {
      if (seen.has(id)) continue;
      seen.add(id);
      next.push(id);
      picked.push(id);
    }
    if (next.length !== chosen.length) {
      chosen = next;
      pickedOrder = picked;
    }
  }

  function toggle(id: string) {
    if (chosenIndex.has(id)) remove(id);
    else addMany([id]);
  }

  function remove(id: string) {
    if (!chosenIndex.has(id)) return;
    chosen = chosen.filter((x) => x !== id);
    pickedOrder = pickedOrder.filter((x) => x !== id);
  }

  function removeAt(i: number) {
    const id = chosen[i];
    if (!id) return;
    remove(id);
    void tick().then(() => {
      if (chosen.length === 0) searchEl?.focus();
      else bedEls[Math.min(i, chosen.length - 1)]?.focus();
    });
  }

  function byTitle(a: PageMeta, b: PageMeta) {
    return collator.compare(titleOf(a), titleOf(b));
  }

  function addFolder(folder: string) {
    addMany(
      app.library
        .filter((p) => inFolder(p.folder, folder))
        .sort(byTitle)
        .map((p) => p.id),
    );
  }

  function addTag(tag: string) {
    addMany(
      app.library
        .filter((p) => p.tags.includes(tag))
        .sort(byTitle)
        .map((p) => p.id),
    );
  }

  function addRecent() {
    const here = new Set(chosen);
    addMany(
      [...app.library]
        .filter((p) => !here.has(p.id))
        .sort((a, b) => b.createdAt - a.createdAt)
        .slice(0, 5)
        .map((p) => p.id),
    );
  }

  const folderCounts = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const page of app.library) {
      let folder = page.folder;
      while (folder) {
        counts.set(folder, (counts.get(folder) ?? 0) + 1);
        const cut = folder.lastIndexOf("/");
        folder = cut < 0 ? null : folder.slice(0, cut);
      }
    }
    return counts;
  });

  const folderItems = $derived.by<DropdownMenuItem[]>(() =>
    app.folders.length === 0
      ? [{ id: "no-folders", label: t("plant.noFolders"), header: true }]
      : app.folders.map((folder) => {
          const n = folderCounts.get(folder) ?? 0;
          return {
            id: `folder:${folder}`,
            label: `${folder} · ${n}`,
            icon: "folder",
            disabled: n === 0,
            onclick: () => addFolder(folder),
          };
        }),
  );

  const tagItems = $derived.by<DropdownMenuItem[]>(() =>
    app.tags.length === 0
      ? [{ id: "no-tags", label: t("plant.noTags"), header: true }]
      : app.tags.map(({ tag, count: n }) => ({
          id: `tag:${tag}`,
          label: `${tag} · ${n}`,
          icon: "hash",
          onclick: () => addTag(tag),
        })),
  );

  function reorder(mode: "picked" | "title" | "oldest") {
    sortMode = mode;
    if (mode === "picked") {
      const here = new Set(chosen);
      const order = pickedOrder.filter((id) => here.has(id));
      for (const id of chosen) if (!order.includes(id)) order.push(id);
      chosen = order;
    } else if (mode === "title") {
      chosen = [...chosen].sort((a, b) => {
        const ma = byId.get(a);
        const mb = byId.get(b);
        return collator.compare(ma ? titleOf(ma) : a, mb ? titleOf(mb) : b);
      });
    } else {
      chosen = [...chosen].sort(
        (a, b) => (byId.get(a)?.createdAt ?? 0) - (byId.get(b)?.createdAt ?? 0),
      );
    }
  }

  /* ------------------------------------------------------ keyboard & drag */

  let searchEl = $state<HTMLInputElement | null>(null);
  let dialogEl = $state<HTMLElement | null>(null);
  let rowEls: Array<HTMLButtonElement | null> = [];
  let bedEls: Array<HTMLElement | null> = [];

  function onSearchKey(e: KeyboardEvent) {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    if (shown.length === 0) return;
    e.preventDefault();
    e.stopPropagation();
    const step = e.key === "ArrowDown" ? 1 : -1;
    let next = active < 0 ? (step > 0 ? 0 : shown.length - 1) : active + step;
    next = Math.max(0, Math.min(shown.length - 1, next));
    active = next;
    rowEls[next]?.focus();
  }

  function onRowKey(e: KeyboardEvent, i: number) {
    if (e.altKey) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      e.stopPropagation();
      const step = e.key === "ArrowDown" ? 1 : -1;
      const next = Math.max(0, Math.min(shown.length - 1, i + step));
      active = next;
      rowEls[next]?.focus();
    } else if (e.key === "Home" || e.key === "End") {
      e.preventDefault();
      const next = e.key === "Home" ? 0 : shown.length - 1;
      active = next;
      rowEls[next]?.focus();
    }
  }

  function onNameKey(e: KeyboardEvent) {
    if (e.key === "Tab" && !e.shiftKey && !name.trim()) {
      e.preventDefault();
      name = suggestion;
    }
  }

  function onBedKey(e: KeyboardEvent, i: number) {
    if (e.altKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
      e.preventDefault();
      e.stopPropagation();
      const j = i + (e.key === "ArrowUp" ? -1 : 1);
      if (j < 0 || j >= chosen.length) return;
      const next = [...chosen];
      [next[i], next[j]] = [next[j], next[i]];
      chosen = next;
      sortMode = null;
      void tick().then(() => bedEls[j]?.focus());
      return;
    }
    if (e.key === "Delete" || e.key === "Backspace") {
      e.preventDefault();
      e.stopPropagation();
      removeAt(i);
    }
  }

  interface Drag {
    id: string;
    from: number;
    to: number;
    startY: number;
    dy: number;
    rowH: number;
  }
  let drag = $state<Drag | null>(null);

  function startDrag(e: PointerEvent, i: number) {
    if (e.button !== 0) return;
    const el = bedEls[i];
    if (!el) return;
    e.preventDefault();
    const rect = el.getBoundingClientRect();
    const following = bedEls[i + 1]?.getBoundingClientRect();
    const rowH = Math.max(following ? following.top - rect.top : rect.height + 4, 24);
    drag = { id: chosen[i], from: i, to: i, startY: e.clientY, dy: 0, rowH };
  }

  function onPointerMove(e: PointerEvent) {
    if (!drag) return;
    const min = -drag.from * drag.rowH;
    const max = (chosen.length - 1 - drag.from) * drag.rowH;
    const dy = Math.min(Math.max(e.clientY - drag.startY, min), max);
    drag = { ...drag, dy, to: drag.from + Math.round(dy / drag.rowH) };
  }

  function onPointerUp() {
    const d = drag;
    if (!d) return;
    drag = null;
    if (d.to === d.from) return;
    const next = [...chosen];
    const [moved] = next.splice(d.from, 1);
    next.splice(d.to, 0, moved);
    chosen = next;
    sortMode = null;
  }

  function shiftFor(i: number): string | undefined {
    const d = drag;
    if (!d) return undefined;
    if (i === d.from) return `translateY(${d.dy}px)`;
    if (d.from < d.to && i > d.from && i <= d.to) return `translateY(${-d.rowH}px)`;
    if (d.to < d.from && i >= d.to && i < d.from) return `translateY(${d.rowH}px)`;
    return undefined;
  }

  /* --------------------------------------------------------- modal shell */

  const dirty = $derived(
    name !== initialName ||
      description !== initialDescription ||
      chosen.length !== initialPages.length ||
      chosen.some((id, i) => id !== initialPages[i]),
  );

  const disabledReason = $derived(
    count === 0 ? t("plant.tipEmpty") : nameTooLong ? t("plant.tipLong") : null,
  );

  async function plant() {
    if (saving || disabledReason) return;
    saving = true;
    const finalName = effectiveName.trim();
    const desc = description.trim();
    try {
      if (tendId) {
        const updated = await savePath(tendId, { name: finalName, pages: chosen, description: desc });
        if (!updated) return;
        app.plant = null;
      } else {
        const created = await createPath(finalName, chosen, desc);
        if (!created) return;
        app.justPlanted = created.id;
        app.plant = null;
        void openPathView(created.id);
      }
      void refreshPathProgress();
    } finally {
      saving = false;
    }
  }

  async function requestClose() {
    if (closing) return;
    if (!dirty) {
      app.plant = null;
      return;
    }
    closing = true;
    const ok = await confirmAction({
      title: t("plant.discardTitle"),
      message: t("plant.discardMessage"),
      confirmLabel: t("plant.discard"),
      danger: true,
    });
    closing = false;
    if (ok) app.plant = null;
  }

  function focusables(): HTMLElement[] {
    if (!dialogEl) return [];
    return Array.from(
      dialogEl.querySelectorAll<HTMLElement>(
        'a[href], button:not(:disabled), input:not(:disabled), textarea:not(:disabled), select:not(:disabled), [tabindex]:not([tabindex="-1"])',
      ),
    ).filter((el) => el.getClientRects().length > 0 && !el.closest("[inert]"));
  }

  function trapTab(e: KeyboardEvent) {
    const list = focusables();
    if (list.length === 0) return;
    const first = list[0];
    const last = list[list.length - 1];
    const current = document.activeElement as HTMLElement | null;
    if (e.shiftKey) {
      if (current === first || !dialogEl?.contains(current)) {
        e.preventDefault();
        last.focus();
      }
    } else if (current === last || !dialogEl?.contains(current)) {
      e.preventDefault();
      first.focus();
    }
  }

  // Escape reaches the dialog before the app's global shortcuts (capture).
  // An open DropdownMenu owns Escape and closes itself first.
  function onWindowKey(e: KeyboardEvent) {
    if (confirmState.pending || app.paletteOpen) return;
    if (e.key === "Escape") {
      if (dialogEl?.querySelector(".dropdown-menu")) return;
      e.preventDefault();
      e.stopPropagation();
      void requestClose();
      return;
    }
    if (e.key === "/" && !e.metaKey && !e.ctrlKey && !e.altKey) {
      const el = document.activeElement as HTMLElement | null;
      if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable)) return;
      e.preventDefault();
      e.stopPropagation();
      searchEl?.focus();
      searchEl?.select();
      return;
    }
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      e.preventDefault();
      e.stopPropagation();
      void plant();
    }
  }

  function onDialogKey(e: KeyboardEvent) {
    if (e.key === "Tab") {
      trapTab(e);
      return;
    }
    if (e.key === "Escape") {
      // Keep the app's global Escape out even when a menu just handled it.
      e.stopPropagation();
      if (e.defaultPrevented) return;
      e.preventDefault();
      void requestClose();
    }
  }

  let prevFocus: HTMLElement | null = null;

  onMount(() => {
    prevFocus = document.activeElement as HTMLElement | null;
    searchEl?.focus();
    window.addEventListener("keydown", onWindowKey, true);
    return () => {
      window.removeEventListener("keydown", onWindowKey, true);
      clearTimeout(seedTimer);
      clearTimeout(leaveTimer);
      if (prevFocus?.isConnected) prevFocus.focus();
    };
  });
</script>

<svelte:window
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerUp}
  onblur={() => (drag = null)}
/>

<div
  class="overlay"
  role="presentation"
  onmousedown={(e) => e.target === e.currentTarget && void requestClose()}
>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="plant-title"
    tabindex="-1"
    bind:this={dialogEl}
    onkeydown={onDialogKey}
  >
    <header class="head">
      <span class="eyebrow">{t("plant.eyebrow")}</span>
      <h2 id="plant-title" class="display">{tendId ? t("plant.tendTitle") : t("plant.title")}</h2>
    </header>

    <div class="body">
      <section class="col pick" aria-label={t("plant.pick")}>
        <h3 class="eyebrow col-head">{t("plant.pick")}</h3>
        <div class="search">
          <Icon name="search" size={14} />
          <input
            bind:this={searchEl}
            bind:value={search}
            oninput={() => (active = -1)}
            onkeydown={onSearchKey}
            type="search"
            placeholder={t("plant.searchPlaceholder")}
            aria-label={t("plant.searchLabel")}
            autocomplete="off"
            spellcheck="false"
          />
          <kbd class="kbd" aria-hidden="true">/</kbd>
        </div>

        <div class="quick">
          <DropdownMenu items={folderItems} ariaLabel={t("plant.folderMenu")}>
            {#snippet trigger({ open, toggle })}
              <button
                class="chip quick-chip"
                type="button"
                aria-haspopup="menu"
                aria-expanded={open}
                title={t("plant.folderMenu")}
                onclick={toggle}
              >
                {t("plant.folder")}
              </button>
            {/snippet}
          </DropdownMenu>
          <DropdownMenu items={tagItems} ariaLabel={t("plant.tagMenu")}>
            {#snippet trigger({ open, toggle })}
              <button
                class="chip quick-chip"
                type="button"
                aria-haspopup="menu"
                aria-expanded={open}
                title={t("plant.tagMenu")}
                onclick={toggle}
              >
                {t("plant.tag")}
              </button>
            {/snippet}
          </DropdownMenu>
          <button
            class="chip quick-chip"
            type="button"
            title={t("plant.recentHint")}
            onclick={addRecent}
          >
            {t("plant.recent")}
          </button>
        </div>

        {#if shown.length > 0}
          <ul class="results">
            {#each shown as entry, i (entry.meta.id)}
              {@const order = chosenIndex.get(entry.meta.id)}
              <li>
                <button
                  type="button"
                  class="pick-row"
                  class:chosen={order !== undefined}
                  class:active={i === active}
                  aria-pressed={order !== undefined}
                  tabindex={i === active || (active < 0 && i === 0) ? 0 : -1}
                  bind:this={rowEls[i]}
                  onclick={() => {
                    active = i;
                    toggle(entry.meta.id);
                  }}
                  onkeydown={(e) => onRowKey(e, i)}
                  onmouseenter={() => (active = i)}
                >
                  <span class="tick" aria-hidden="true">
                    {#if order !== undefined}
                      <span class="order mono">{order + 1}</span>
                    {/if}
                  </span>
                  <span class="row-title ellipsis">{titleOf(entry.meta)}</span>
                  {#if entry.meta.folder}
                    <span class="row-folder mono ellipsis">{entry.meta.folder}</span>
                  {/if}
                  <span class="row-leaf"><VitalityLeaf meta={entry.meta} size={14} /></span>
                </button>
              </li>
            {/each}
          </ul>
          {#if hidden > 0}
            <p class="more-quiet">{t("plant.more", { count: hidden })}</p>
          {/if}
        {:else}
          <div class="pick-empty">
            <Illustration name="magnifier-leaf" size={72} />
            <p class="pick-empty-title">{t("plant.noMatches")}</p>
            <p class="pick-empty-hint">
              {app.library.length === 0 ? t("plant.noPages") : t("plant.noMatchesHint")}
            </p>
          </div>
        {/if}
      </section>

      <section class="col bed" aria-label={t("plant.bed")}>
        <h3 class="eyebrow col-head">{t("plant.bed")}</h3>
        <input
          class="name-input display"
          class:overlong={nameTooLong}
          bind:value={name}
          onkeydown={onNameKey}
          placeholder={suggestion}
          aria-label={t("plant.nameLabel")}
          autocomplete="off"
          spellcheck="false"
        />
        <input
          class="desc-input"
          bind:value={description}
          placeholder={t("plant.descriptionPlaceholder")}
          aria-label={t("plant.descriptionLabel")}
          autocomplete="off"
          spellcheck="false"
        />

        <div class="bed-tools">
          <div class="seg" role="group" aria-label={t("plant.order")}>
            <button
              type="button"
              class="seg-btn order-btn"
              class:active={sortMode === "picked"}
              onclick={() => reorder("picked")}
            >
              {t("plant.orderPicked")}
            </button>
            <button
              type="button"
              class="seg-btn order-btn"
              class:active={sortMode === "title"}
              onclick={() => reorder("title")}
            >
              {t("plant.orderTitle")}
            </button>
            <button
              type="button"
              class="seg-btn order-btn"
              class:active={sortMode === "oldest"}
              onclick={() => reorder("oldest")}
            >
              {t("plant.orderOldest")}
            </button>
          </div>
        </div>

        {#if bedRows.length === 0}
          <div class="bed-empty">
            <Illustration name="seedling" size={80} />
            <p>{t("plant.bedEmpty")}</p>
          </div>
        {:else}
          <ol class="bed-list" aria-label={t("plant.order")}>
            {#each bedRows as r (r.id)}
              <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
              <li
                class="bed-row"
                class:dragging={drag?.id === r.id}
                animate:flip={{ duration: reduced ? 0 : 180 }}
                style:transform={shiftFor(r.i)}
                tabindex="0"
                onkeydown={(e) => onBedKey(e, r.i)}
                onmouseenter={() => (hovered = r.i)}
                onmouseleave={() => (hovered = null)}
                onfocusin={() => (hovered = r.i)}
                onfocusout={(e) => {
                  if (!e.currentTarget.contains(e.relatedTarget as Node | null)) hovered = null;
                }}
                bind:this={bedEls[r.i]}
              >
                <button
                  type="button"
                  class="grip"
                  tabindex="-1"
                  aria-label={t("plant.grip")}
                  onpointerdown={(e) => startDrag(e, r.i)}
                >
                  <Icon name="arrow-up-down" size={13} />
                </button>
                <span class="bed-num mono">{r.i + 1}</span>
                <span class="bed-title ellipsis">{r.title}</span>
                <button
                  type="button"
                  class="bed-remove"
                  aria-label={t("plant.removePage", { title: r.title })}
                  title={t("plant.removePage", { title: r.title })}
                  onclick={() => removeAt(r.i)}
                >
                  <Icon name="x" size={12} />
                </button>
              </li>
            {/each}
          </ol>
        {/if}
      </section>

      <section class="col tree-col" aria-label={t("plant.tree")}>
        <h3 class="eyebrow col-head">{t("plant.tree")}</h3>
        <div class="tree-stage">
          {#if leavingSeed}
            <div class="tree-layer leave" aria-hidden="true">
              <PathTree
                seed={leavingSeed}
                read={count}
                total={count}
                size={260}
                detail="full"
                animate={false}
                highlight={hovered}
              />
            </div>
          {/if}
          {#key treeKey}
            <div class="tree-layer" class:enter={treeKey > 0}>
              <PathTree
                seed={treeSeed}
                read={count}
                total={count}
                size={260}
                detail="full"
                animate={false}
                highlight={hovered}
                label={effectiveName}
              />
            </div>
          {/key}
        </div>
        <p class="tree-caption">{t("plant.treeCaption")}</p>
        <p class="tree-meta mono">{t("plant.treeMeta", { count })}</p>
      </section>
    </div>

    <footer class="foot">
      <span class="esc-hint"><kbd class="kbd">Esc</kbd> <span>{t("plant.esc")}</span></span>
      <span class="foot-gap"></span>
      <kbd class="kbd foot-mod">{modKey("↵")}</kbd>
      <span class="tip" title={disabledReason ?? undefined}>
        <button class="btn btn-primary" disabled={!!disabledReason || saving} onclick={() => void plant()}>
          {tendId ? t("plant.save") : t("plant.plant", { count })}
        </button>
      </span>
    </footer>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: var(--z-modal);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
    background: var(--scrim);
    animation: plant-scrim var(--t-fast) var(--ease-out);
  }
  .dialog {
    width: min(980px, 92vw);
    height: min(680px, 88vh);
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    overflow: hidden;
    outline: none;
    animation: plant-rise var(--t-med) var(--ease-out);
  }
  @keyframes plant-scrim {
    from {
      opacity: 0;
    }
  }
  @keyframes plant-rise {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
  }

  .head {
    padding: 16px 20px 12px;
    border-bottom: 1px solid var(--border);
  }
  .head h2 {
    margin: 2px 0 0;
    font-size: var(--fs-xl);
  }

  .body {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1.05fr) 300px;
  }
  .col {
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: 14px 16px 16px;
  }
  .pick,
  .bed {
    border-right: 1px solid var(--border);
  }
  .col-head {
    margin: 0 0 10px;
  }

  /* ---------------------------------------------------------------- pick */
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 9px;
    border: 1px solid var(--border-input);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--muted);
    transition:
      border-color var(--t-fast) var(--ease-out),
      box-shadow var(--t-fast) var(--ease-out);
  }
  .search:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-ring);
  }
  .search input {
    flex: 1;
    min-width: 0;
    padding: 2px 0;
    border: 0;
    background: transparent;
    outline: none;
    box-shadow: none;
    font-size: var(--fs-sm);
    color: var(--text);
    appearance: none;
  }
  .search input:focus {
    box-shadow: none;
  }
  .search input::-webkit-search-cancel-button {
    display: none;
  }

  .quick {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin: 10px 0;
  }
  .quick-chip {
    background: var(--sunken);
    color: var(--text-soft);
    text-transform: none;
    letter-spacing: 0;
    font-size: var(--fs-xs);
    padding: 2px 9px;
    border: 1px solid transparent;
    transition:
      background var(--t-fast) var(--ease-out),
      border-color var(--t-fast) var(--ease-out);
  }
  .quick-chip:hover {
    background: var(--raised);
    border-color: var(--border-strong);
  }

  .results {
    list-style: none;
    margin: 0;
    padding: 0;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  .pick-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 7px 6px;
    border-radius: var(--radius-xs);
    text-align: left;
    font-size: var(--fs-sm);
    color: var(--text);
    transition: background 120ms var(--ease-out);
  }
  .pick-row:hover,
  .pick-row.active {
    background: var(--sunken);
  }
  .pick-row:active {
    scale: 1;
  }
  .tick {
    flex: none;
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    box-sizing: border-box;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-xs);
    background: var(--surface);
    color: var(--on-accent);
    transition:
      background 120ms var(--ease-out),
      border-color 120ms var(--ease-out);
  }
  .chosen .tick {
    background: var(--accent-fill);
    border-color: var(--accent-fill);
  }
  .order {
    font-family: var(--mono);
    font-variation-settings: var(--font-mono-settings);
    font-size: var(--fs-2xs);
    line-height: 1;
  }
  .row-title {
    flex: 1;
    min-width: 0;
  }
  .row-folder {
    flex: none;
    max-width: 34%;
    font-family: var(--mono);
    font-variation-settings: var(--font-mono-settings);
    font-size: var(--fs-2xs);
    color: var(--muted);
  }
  .row-leaf {
    flex: none;
    display: inline-flex;
  }
  .more-quiet {
    margin: 8px 0 0;
    font-size: var(--fs-xs);
    color: var(--muted);
  }
  .pick-empty {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 16px;
    text-align: center;
  }
  .pick-empty-title {
    margin: 4px 0 0;
    font-size: var(--fs-sm);
    color: var(--text-soft);
  }
  .pick-empty-hint {
    margin: 0;
    max-width: 32ch;
    font-size: var(--fs-xs);
    color: var(--muted);
  }

  /* ----------------------------------------------------------------- bed */
  .name-input {
    width: 100%;
    padding: 4px 2px 8px;
    border: 0;
    border-bottom: 1px solid var(--border-strong);
    border-radius: 0;
    background: transparent;
    font-family: var(--font-display);
    font-variation-settings: var(--font-display-settings);
    font-size-adjust: none;
    font-size: calc(var(--fs-2xl) * var(--display-size-adjust));
    font-weight: var(--display-weight);
    letter-spacing: var(--display-tracking);
    color: var(--text);
    outline: none;
    transition: border-color var(--t-fast) var(--ease-out);
  }
  .name-input:hover {
    border-color: var(--border-hover);
  }
  .name-input:focus {
    border-color: var(--accent);
    box-shadow: none;
  }
  .name-input.overlong {
    border-color: var(--danger);
  }
  .desc-input {
    margin-top: 8px;
    width: 100%;
    padding: 4px 2px;
    border: 0;
    border-radius: 0;
    background: transparent;
    font-size: var(--fs-sm);
    color: var(--text-soft);
    outline: none;
  }
  .desc-input:focus {
    box-shadow: none;
    border-color: transparent;
  }
  .bed-tools {
    display: flex;
    justify-content: flex-start;
    margin: 10px 0;
  }
  .order-btn {
    width: auto;
    padding: 0 8px;
    font-size: var(--fs-xs);
    white-space: nowrap;
  }

  .bed-empty {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 16px;
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius);
    text-align: center;
  }
  .bed-empty p {
    margin: 0;
    max-width: 30ch;
    font-size: var(--fs-xs);
    color: var(--muted);
  }

  .bed-list {
    list-style: none;
    margin: 0;
    padding: 0;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  .bed-row {
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 6px;
    margin-bottom: 2px;
    border: 1px solid transparent;
    border-radius: var(--radius-xs);
    background: var(--surface);
    font-size: var(--fs-sm);
    color: var(--text);
    cursor: default;
    outline: none;
  }
  .bed-row:hover,
  .bed-row:focus-visible {
    background: var(--raised);
    border-color: var(--border);
  }
  .bed-row.dragging {
    z-index: 1;
    background: var(--surface);
    border-color: var(--border-strong);
    box-shadow: var(--shadow);
  }
  .grip {
    flex: none;
    display: grid;
    place-items: center;
    width: 20px;
    height: 22px;
    color: var(--muted);
    cursor: grab;
    touch-action: none;
    border-radius: var(--radius-xs);
  }
  .grip:hover {
    color: var(--text-soft);
  }
  .grip:active {
    cursor: grabbing;
    scale: 1;
  }
  .bed-num {
    flex: none;
    min-width: 14px;
    font-family: var(--mono);
    font-variation-settings: var(--font-mono-settings);
    font-size: var(--fs-2xs);
    color: var(--muted);
    text-align: right;
  }
  .bed-title {
    flex: 1;
    min-width: 0;
  }
  .bed-remove {
    flex: none;
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: var(--radius-xs);
    color: var(--muted);
  }
  .bed-remove:hover {
    background: var(--danger-soft);
    color: var(--danger);
  }

  /* ---------------------------------------------------------------- tree */
  .tree-col {
    align-items: center;
    justify-content: flex-start;
    text-align: center;
    background: var(--surface);
  }
  .tree-col .col-head {
    align-self: flex-start;
  }
  .tree-stage {
    display: grid;
    place-items: end center;
    width: 260px;
    max-width: 100%;
    margin: auto 0 0;
    color: var(--text-soft);
  }
  .tree-layer {
    grid-area: 1 / 1;
    width: 100%;
    transform-origin: 50% 85%;
  }
  .tree-layer :global(svg.tree) {
    width: 100%;
    height: auto;
    display: block;
  }
  .tree-layer.enter {
    animation: tree-in 300ms var(--ease-spring) both;
  }
  .tree-layer.leave {
    animation: tree-out 120ms var(--ease-out) both;
  }
  @keyframes tree-in {
    from {
      opacity: 0;
      transform: scale(0.96);
    }
  }
  @keyframes tree-out {
    to {
      opacity: 0;
    }
  }
  .tree-caption {
    margin: 12px 0 0;
    font-family: var(--font-display);
    font-variation-settings: var(--font-display-settings);
    font-size-adjust: none;
    font-size: calc(var(--fs-base) * var(--display-size-adjust));
    font-weight: var(--display-weight);
    letter-spacing: var(--display-tracking);
    line-height: var(--display-leading);
    color: var(--text-soft);
  }
  .tree-meta {
    margin: 4px 0 0;
    font-family: var(--mono);
    font-variation-settings: var(--font-mono-settings);
    font-size: var(--fs-2xs);
    color: var(--muted);
  }

  /* -------------------------------------------------------------- footer */
  .foot {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 16px;
    border-top: 1px solid var(--border);
  }
  .esc-hint {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-xs);
    color: var(--muted);
  }
  .foot-gap {
    flex: 1;
  }
  .foot-mod {
    color: var(--muted);
  }
  .tip {
    display: inline-flex;
  }

  /* Narrow window: one column, body scrolls, the tree shrinks. */
  @media (max-width: 899px) {
    .body {
      display: block;
      overflow-y: auto;
      overscroll-behavior: contain;
    }
    .pick,
    .bed {
      border-right: 0;
      border-bottom: 1px solid var(--border);
    }
    .results {
      flex: none;
      max-height: 220px;
    }
    .pick-empty {
      min-height: 170px;
      flex: none;
    }
    .bed-list {
      flex: none;
      max-height: 200px;
    }
    .bed-empty {
      flex: none;
      min-height: 150px;
    }
    .tree-stage {
      width: 170px;
      margin: 12px 0 0;
    }
    .tree-col {
      padding-bottom: 18px;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .overlay,
    .dialog,
    .tree-layer.enter,
    .tree-layer.leave {
      animation: none;
    }
  }
</style>
