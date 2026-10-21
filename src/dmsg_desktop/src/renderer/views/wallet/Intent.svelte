<script lang="ts">
  import { w } from './text'
  import {
    amountText,
    assetIdentity,
    assetInfo,
    feeAsset,
    intentFields,
    knownIntent,
    leaves,
    previousIntent,
    type Asset,
    type Wallet
  } from './model'
  let { intent, metadata = [], before, compact = false }: { intent: any; metadata?: Asset[]; before?: Wallet; compact?: boolean } = $props()
  const old = $derived(previousIntent(intent, before))
  const asset = $derived(intent.asset ? assetInfo(intent.asset, metadata) : undefined)
  const primitive = (value: string) => (['true', 'false'].includes(value) ? w(value) : value)
</script>

{#if !knownIntent(intent)}
  <p class="text-danger" role="alert">{w('unsupported')}</p>
{:else}
  <div class="grid gap-2" data-testid="wallet-intent">
    <h3 class="text-[14px] font-semibold">{w(intent.kind)}</h3>
    {#if asset}
      <p class="text-muted text-[12px] break-all" dir="ltr">{assetIdentity(asset)}</p>
      {#each intent.outputs ?? [] as output, i (i)}
        <div class="border-line bg-surface rounded-lg border px-3 py-2">
          <b class="block text-[15px] tabular-nums">{amountText(output.amount, asset, w('atomic'))}</b>
          <span class="block font-mono text-[12px] break-all select-all" dir="ltr">{output.to}</span>
          {#if !compact}<small class="text-faint">{output.amount} {w('atomic')}</small>{/if}
        </div>
      {/each}
    {/if}
    {#if compact && !asset}
      <p class="text-muted text-[12px]"
        >{leaves(intent)
          .filter((r) => r.path[0] !== 'kind')
          .slice(0, 3)
          .map((r) => `${r.path.map(w).join(' · ')}: ${primitive(r.value)}`)
          .join(' / ')}</p
      >
    {/if}
    {#if !compact}
      <dl class="grid grid-cols-[minmax(7rem,1fr)_minmax(0,2fr)] gap-x-4 gap-y-2 text-[12.5px]">
        {#each intentFields(intent).filter((key) => !['asset', 'outputs'].includes(key)) as key (key)}
          {#each leaves(intent[key], [key]) as row, i (`${key}-${i}`)}
            {@const prior = leaves(old[key], [key]).find((r) => r.path.join('.') === row.path.join('.'))?.value}
            <dt class="text-faint min-w-0 break-words">{row.path.map((p) => (/^\d+$/.test(p) ? `#${Number(p) + 1}` : w(p === 'cycles' ? 'attached_cycles' : p === 'refuel' ? 'refuel_rule' : p))).join(' · ')}</dt
            >
            <dd class="text-ink-2 min-w-0 [overflow-wrap:anywhere] whitespace-pre-wrap">
              {#if key === 'max_fee' && asset}
                {amountText(String(intent.max_fee), feeAsset(asset), w('atomic'))}
                <small class="text-faint block">{intent.max_fee} {w('atomic')}</small>
              {:else}
                {#if old[key] !== undefined && prior !== row.value}<del class="text-danger me-2" aria-label={w('before')}
                    >{primitive(prior ?? '—')}</del
                  >{/if}
                <span class={old[key] !== undefined && prior !== row.value ? 'text-forest font-medium' : ''}>{primitive(row.value)}</span>
              {/if}
            </dd>
          {/each}
        {/each}
      </dl>
    {/if}
  </div>
{/if}
