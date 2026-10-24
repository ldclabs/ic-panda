<script lang="ts">
  import { untrack } from 'svelte'
  import Field from '../../components/page/Field.svelte'
  import Select from '../../components/Select.svelte'
  import { locale } from '../../i18n/index.svelte'
  import { durationText } from './model'
  import { w } from './text'
  let { value = $bindable(0), label }: { value: number; label: string } = $props()
  const scales = [86400000, 3600000, 60000, 1000]
  let scale = $state(untrack(() => String(scales.find((s) => value >= s && value % s === 0) ?? 60000)))
</script>

<div class="grid grid-cols-[minmax(0,1fr)_8rem] items-end gap-2">
  <Field {label}
    ><input
      type="number"
      min="0"
      step="any"
      value={Number.isFinite(value) ? value / Number(scale) : ''}
      oninput={(e) => (value = e.currentTarget.value === '' ? NaN : Number(e.currentTarget.value) * Number(scale))}
      aria-invalid={!Number.isSafeInteger(value) || value < 0}
    /></Field
  >
  <Select
    bind:value={scale}
    label={`${label} · ${w('unit')}`}
    options={scales.map((s) => ({ value: String(s), label: w(s === 86400000 ? 'unitDays' : s === 3600000 ? 'unitHours' : s === 60000 ? 'unitMinutes' : 'unitSeconds') }))}
  />
</div>
