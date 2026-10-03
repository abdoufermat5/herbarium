<script lang="ts">
  import { t } from "../../lib/i18n.svelte";
  import { DURATION_UNITS, UNIT_MINUTES, splitDuration, type DurationUnit } from "../../lib/format";
  import Select, { type SelectOption } from "../Select.svelte";

  // An amount plus a unit (minutes, hours, days).
  //  - Controlled: pass `value` (minutes) and `oncommit`; the field follows
  //    `value` and snaps back to it when `oncommit` resolves to false.
  //  - Free-standing: omit `value` and `bind:amount` / `bind:unit`.
  let {
    value,
    amount = $bindable(""),
    unit = $bindable("day"),
    label,
    disabled = false,
    oncommit,
  }: {
    value?: number;
    amount?: string;
    unit?: DurationUnit;
    label: string;
    disabled?: boolean;
    oncommit?: (minutes: number) => boolean | Promise<boolean>;
  } = $props();

  const unitOptions = $derived<SelectOption<DurationUnit>[]>(
    DURATION_UNITS.map((u) => ({ value: u, label: t(`duration.unit.${u}`) })),
  );

  function sync() {
    if (value === undefined) return;
    const split = splitDuration(value);
    amount = String(split.n);
    unit = split.unit;
  }

  $effect(() => {
    void value;
    sync();
  });

  async function commit() {
    if (!oncommit) return;
    const n = Number(amount);
    if (amount.trim() === "" || !Number.isInteger(n) || n < 1) return sync();
    if (!(await oncommit(n * UNIT_MINUTES[unit]))) sync();
  }

  function pickUnit(next: DurationUnit) {
    unit = next;
    void commit();
  }
</script>

<div class="duration">
  <input
    type="number"
    min="1"
    step="1"
    inputmode="numeric"
    value={amount}
    oninput={(e) => (amount = e.currentTarget.value)}
    onchange={commit}
    aria-label={label}
    {disabled}
  />
  <Select
    fill
    size="md"
    value={unit}
    options={unitOptions}
    ariaLabel={t("review.cfg.unit")}
    {disabled}
    onchange={pickUnit}
  />
</div>

<style>
  .duration {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 118px);
    gap: 8px;
    width: 100%;
  }
  input {
    height: 36px;
    min-width: 0;
  }
</style>
