<script lang="ts">
  import { app, saveReviewSettings, toast } from "../../lib/state.svelte";
  import { prefs, setPref } from "../../lib/prefs.svelte";
  import { t } from "../../lib/i18n.svelte";
  import { fmtDuration, plural } from "../../lib/format";
  import type { ReviewSettings, ReviewStrategy } from "../../lib/types";
  import Select, { type SelectOption } from "../Select.svelte";
  import SettingsSection from "./SettingsSection.svelte";
  import SettingRow from "./SettingRow.svelte";
  import Switch from "./Switch.svelte";
  import PresetsEditor from "./PresetsEditor.svelte";
  import DurationField from "./DurationField.svelte";

  const QUEUE_SIZES = [5, 10, 20, 50];

  let saving = $state(false);

  /** Save a change; the stored settings stay as they were if the backend rejects it. */
  async function update(patch: Partial<ReviewSettings>): Promise<boolean> {
    if (saving) return false;
    saving = true;
    try {
      await saveReviewSettings({ ...app.review, ...patch });
      return true;
    } catch (e) {
      toast(t("review.cfg.saveFailed", { error: String(e) }), "error");
      return false;
    } finally {
      saving = false;
    }
  }

  /** Exclusion lists are edited as comma-separated text. */
  async function commitList(e: Event, key: "excludeFolders" | "excludeTags") {
    const input = e.currentTarget as HTMLInputElement;
    const list = input.value
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
    const ok = await update({ [key]: list });
    input.value = app.review[key].join(", ");
    if (!ok) return;
  }

  async function commitMultiplier(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const ok = input.value.trim() !== "" && (await update({ multiplier: Number(input.value) }));
    if (!ok) input.value = String(app.review.multiplier);
  }

  /** The retention field is edited as a percent; settings store 0..1. */
  function retentionPercent(): number {
    return Math.round(app.review.desiredRetention * 100);
  }

  async function commitRetention(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const pct = Number(input.value);
    const ok = input.value.trim() !== "" && (await update({ desiredRetention: pct / 100 }));
    if (!ok) input.value = String(retentionPercent());
  }

  /** `current` stays selectable even when it is not one of the usual choices. */
  function withCurrent(values: number[], current: number | null): number[] {
    return current && !values.includes(current) ? [...values, current].sort((a, b) => a - b) : values;
  }

  const strategyOptions = $derived<SelectOption<ReviewStrategy>[]>([
    { value: "fsrs", label: t("review.cfg.strategyFsrs") },
    { value: "ladder", label: t("review.cfg.strategyLadder") },
    { value: "multiply", label: t("review.cfg.strategyMultiply") },
    { value: "same", label: t("review.cfg.strategySame") },
  ]);
  // 0 means "off" in both selects below.
  const importOptions = $derived<SelectOption<number>[]>([
    { value: 0, label: t("review.cfg.off") },
    ...withCurrent(app.review.presets, app.review.importReviewMinutes).map((m) => ({
      value: m,
      label: fmtDuration(m),
    })),
  ]);
  const queueOptions = $derived<SelectOption<number>[]>([
    { value: 0, label: t("review.cfg.noLimit") },
    ...withCurrent(QUEUE_SIZES, app.review.queueLimit).map((n) => ({
      value: n,
      label: plural(n, "page"),
    })),
  ]);
</script>

<SettingsSection id="settings-review" title={t("review.cfg.title")} note={t("review.cfg.storedInVault")}>
  <SettingRow stacked title={t("review.cfg.intervals")} hint={t("review.cfg.intervalsHint")}>
    <PresetsEditor disabled={saving} onchange={(presets) => update({ presets })} />
  </SettingRow>

  <SettingRow title={t("review.cfg.strategy")} hint={t("review.cfg.strategyHint")}>
    <Select
      fill
      size="md"
      align="right"
      value={app.review.strategy}
      ariaLabel={t("review.cfg.strategy")}
      disabled={saving}
      options={strategyOptions}
      onchange={(strategy) => void update({ strategy })}
    />
  </SettingRow>

  {#if app.review.strategy === "fsrs"}
    <SettingRow title={t("review.cfg.retention")} hint={t("review.cfg.retentionHint")}>
      <input
        type="number"
        min="70"
        max="97"
        step="1"
        value={retentionPercent()}
        aria-label={t("review.cfg.retention")}
        disabled={saving}
        onchange={commitRetention}
      />
    </SettingRow>
  {/if}

  {#if app.review.strategy === "multiply"}
    <SettingRow title={t("review.cfg.multiplier")} hint={t("review.cfg.multiplierHint")}>
      <input
        type="number"
        min="1.1"
        max="10"
        step="0.1"
        value={app.review.multiplier}
        aria-label={t("review.cfg.multiplier")}
        disabled={saving}
        onchange={commitMultiplier}
      />
    </SettingRow>
  {/if}

  {#if app.review.strategy !== "same"}
    <SettingRow title={t("review.cfg.maxInterval")} hint={t("review.cfg.maxIntervalHint")}>
      <DurationField
        value={app.review.maxIntervalMinutes}
        label={t("review.cfg.maxInterval")}
        disabled={saving}
        oncommit={(maxIntervalMinutes) => update({ maxIntervalMinutes })}
      />
    </SettingRow>
  {/if}

  <SettingRow title={t("review.cfg.importSchedule")} hint={t("review.cfg.importScheduleHint")}>
    <Select
      fill
      size="md"
      align="right"
      value={app.review.importReviewMinutes ?? 0}
      ariaLabel={t("review.cfg.importSchedule")}
      disabled={saving}
      options={importOptions}
      onchange={(m) => void update({ importReviewMinutes: m || null })}
    />
  </SettingRow>

  <SettingRow title={t("review.cfg.notify")} hint={t("review.cfg.notifyHint")}>
    <Switch
      checked={prefs.reviewNotify}
      label={t("review.cfg.notify")}
      onchange={(v) => setPref("reviewNotify", v)}
    />
  </SettingRow>

  <SettingRow title={t("review.cfg.queueLimit")} hint={t("review.cfg.queueLimitHint")}>
    <Select
      fill
      size="md"
      align="right"
      value={app.review.queueLimit ?? 0}
      ariaLabel={t("review.cfg.queueLimit")}
      disabled={saving}
      options={queueOptions}
      onchange={(n) => void update({ queueLimit: n || null })}
    />
  </SettingRow>

  <SettingRow title={t("review.cfg.excludeFolders")} hint={t("review.cfg.excludeFoldersHint")}>
    <input
      type="text"
      class="list-input"
      value={app.review.excludeFolders.join(", ")}
      placeholder={t("review.cfg.excludeFoldersPlaceholder")}
      aria-label={t("review.cfg.excludeFolders")}
      disabled={saving}
      onchange={(e) => void commitList(e, "excludeFolders")}
    />
  </SettingRow>

  <SettingRow title={t("review.cfg.excludeTags")} hint={t("review.cfg.excludeTagsHint")}>
    <input
      type="text"
      class="list-input"
      value={app.review.excludeTags.join(", ")}
      placeholder={t("review.cfg.excludeTagsPlaceholder")}
      aria-label={t("review.cfg.excludeTags")}
      disabled={saving}
      onchange={(e) => void commitList(e, "excludeTags")}
    />
  </SettingRow>
</SettingsSection>

<style>
  .list-input {
    width: 220px;
  }
</style>
