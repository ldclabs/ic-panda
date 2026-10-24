<script lang="ts">
  import { w } from './text'
  import { locale } from '../../i18n/index.svelte'
  import Collapsible from '../../components/Collapsible.svelte'
  import PolicySummary from './PolicySummary.svelte'
  import CosignPolicy from './CosignPolicy.svelte'
  import {
    amountText,
    durationText,
    intentHeadline,
    intentFeeAsset,
    assetIdentity,
    assetInfo,
    feeAsset,
    intentFields,
    knownIntent,
    leaves,
    localizedValue,
    previousIntent,
    cosignAddresses,
    cosignerReferences,
    removedMemberKeys,
    type Asset,
    type Wallet
  } from './model'
  let { intent, metadata = [], before, compact = false }: { intent: any; metadata?: Asset[]; before?: Wallet; compact?: boolean } = $props()
  const old = $derived(previousIntent(intent, before))
  const asset = $derived(intent.asset ? assetInfo(intent.asset, metadata) : undefined)
  const requirements = $derived(cosignAddresses(intent, before))
  const removed = $derived(
    intent.kind === 'remove_cosigner'
      ? [intent.cosigner]
      : intent.kind === 'remove_member' && before
        ? removedMemberKeys(before, intent.member)
        : []
  )
  const affected = $derived(before ? cosignerReferences(before, removed) : [])
  const primitive = (value: string, path: string[]) => (localizedValue(path, value) ? w(value) : value)
</script>

{#if !knownIntent(intent)}
  <p class="text-danger" role="alert">{w('unsupported')}</p>
{:else}
  <div class="grid gap-2" data-testid="wallet-intent">
    <h3 class="text-[14px] font-semibold">{w(intent.kind)}</h3>
    {#if !compact && intent.kind === 'set_policy'}<PolicySummary policy={intent.policy} before={before?.policy} {metadata} />{/if}
    {#if asset}
      <p class="text-muted text-[12px] break-all" dir="ltr">{assetIdentity(asset)}</p>
      {#each intent.outputs ?? [] as output, i (i)}
        <div class="border-line bg-surface rounded-lg border px-3 py-2">
          <b class="block text-[15px] tabular-nums">{amountText(output.amount, asset, w('atomic'))}</b>
          <span class="block font-mono text-[12px] break-all select-all" dir="ltr">{output.to}</span>
        </div>
      {/each}
    {/if}
    {#if compact && !asset}
      {#if intent.kind === 'set_cosign'}
        <p class="text-muted text-[12px]"
          >#{intent.address} · {intent.cosign
            ? `${w('threshold')}: ${intent.cosign.threshold} / ${intent.cosign.cosigners.length}`
            : w('cosignRemove')}</p
        >
      {:else}
        <p class="text-muted text-[12px] break-all">{intentHeadline(intent, metadata).text}</p>
      {/if}
    {/if}
    {#if !compact}
      {#if intent.kind === 'set_cosign'}
        {#if !intent.cosign}<p class="text-warn font-medium">{w('cosignRemove')} · {w('cosignRemoveHint')}</p>{/if}
        <div class="grid gap-3 sm:grid-cols-2">
          {#if before}<CosignPolicy policy={old.cosign} wallet={before} address={intent.address} title={w('before')} />{/if}
          <CosignPolicy policy={intent.cosign} wallet={before} address={intent.address} title={w('after')} />
        </div>
      {/if}
      {#if affected.length}<p class="text-warn">{w('cosignBound')}</p>
        {#each affected as a (a.id)}<CosignPolicy policy={a.cosign} wallet={before} address={a.id} />{/each}
      {/if}
      {#if requirements.length}
        {#if intent.kind === 'canister_call'}<p class="text-warn">{w('cosignCallHint')}</p>{/if}
        {#each requirements as a (a.id)}<CosignPolicy policy={a.cosign} wallet={before} address={a.id} />{/each}
      {/if}
      <dl class="grid grid-cols-[minmax(7rem,1fr)_minmax(0,2fr)] gap-x-4 gap-y-2 text-[12.5px]">
        {#each intentFields(intent).filter((key) => !['asset', 'outputs', 'memo', 'curve', 'envelope', 'arg'].includes(key) && !['set_cosign', 'set_policy'].includes(intent.kind)) as key (key)}
          {#each leaves(intent[key], [key]) as row, i (`${key}-${i}`)}
            {@const prior = leaves(old[key], [key]).find((r) => r.path.join('.') === row.path.join('.'))?.value}
            <dt class="text-faint min-w-0 break-words"
              >{row.path
                .map((p) =>
                  /^\d+$/.test(p) ? `#${Number(p) + 1}` : w(p === 'cycles' ? 'attached_cycles' : p === 'refuel' ? 'refuel_rule' : p)
                )
                .join(' · ')}</dt
            >
            <dd class="text-ink-2 min-w-0 [overflow-wrap:anywhere] whitespace-pre-wrap">
              {#if key === 'max_fee'}
                {amountText(String(intent.max_fee), intentFeeAsset(intent, metadata), w('atomic'))}
              {:else if key === 'from' && before}
                {before.addresses.find((a) => a.id === intent.from)?.label} · #{intent.from}
                <p class="font-mono text-[12px] break-all" dir="ltr">{before.addresses.find((a) => a.id === intent.from)?.text}</p>
              {:else if key === 'icp_e8s'}{amountText(String(intent.icp_e8s), { asset: 'ICP', decimals: 8 }, w('atomic'))}
              {:else}
                {#if old[key] !== undefined && prior !== row.value}<del class="text-danger me-2" aria-label={w('before')}
                    >{primitive(prior ?? '—', row.path)}</del
                  >{/if}
                <span class={old[key] !== undefined && prior !== row.value ? 'text-forest font-medium' : ''}
                  >{primitive(row.value, row.path)}</span
                >
              {/if}
            </dd>
          {/each}
        {/each}
      </dl>
      {#if intent.kind === 'set_cosign'}<p class="text-muted text-[12px]">{w('cosignBoundary')}</p>{/if}
      {#if intent.kind !== 'set_policy'}<Collapsible>
          {#snippet title()}<span class="text-[12px]">{w('technical')}</span>{/snippet}
          {#each ['memo', 'curve', 'envelope', 'arg'].filter((key) => intent[key] != null) as key}<p class="text-[12px] break-all"
              >{w(key)}: {typeof intent[key] === 'object' ? JSON.stringify(intent[key]) : primitive(String(intent[key]), [key])}</p
            >{/each}
          {#each intent.outputs ?? [] as output}<p class="text-[12px] break-all">{output.amount} {w('atomic')} → {output.to}</p>{/each}
        </Collapsible>{/if}
    {/if}
  </div>
{/if}
