<script lang="ts">
  import { untrack } from 'svelte'
  import Field from '../../components/page/Field.svelte'
  import { decimal, toAtomic, assetKey, type Asset } from './model'
  import { w } from './text'
  let { value = $bindable('0'), label, asset }: { value: string; label: string; asset: Asset } = $props()
  let shown = $state(untrack(() => decimal(value, asset.decimals ?? 0))),
    last = untrack(() => value),
    precision = untrack(() => asset.decimals),
    identity = untrack(() => assetKey(asset))
  let invalid = $state(false)
  $effect(() => {
    if (identity !== assetKey(asset)) {
      identity = assetKey(asset)
      last = ''
      value = ''
      shown = ''
      invalid = true
      precision = asset.decimals
      return
    }
    if (value !== last || precision !== asset.decimals) {
      shown = decimal(value, asset.decimals ?? 0)
      invalid = false
      last = value
      precision = asset.decimals
    }
  })
  function update(text: string) {
    shown = text
    try {
      last = toAtomic(text.trim(), asset.decimals ?? 0)
      invalid = false
    } catch {
      last = ''
      invalid = true
    }
    value = last
  }
</script>

<Field label={`${label} · ${asset.symbol || asset.asset}${asset.decimals == null ? ` (${w('atomic')})` : ''}`}>
  <input value={shown} oninput={(e) => update(e.currentTarget.value)} inputmode="decimal" dir="ltr" aria-invalid={invalid} />
  {#if invalid}<span class="text-danger text-[12px]" role="alert">{w('invalidAmount')}</span>{/if}
</Field>
