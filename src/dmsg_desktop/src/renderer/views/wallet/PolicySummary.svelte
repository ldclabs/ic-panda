<script lang="ts">
  import Collapsible from '../../components/Collapsible.svelte'
  import { locale } from '../../i18n/index.svelte'
  import { amountText, assetFromKey, assetIdentity, assetInfo, durationText, cyclesText, networkName, type Asset } from './model'
  import { w } from './text'
  let { policy, before, metadata = [] }: { policy: any; before?: any; metadata?: Asset[] } = $props()
  const asset = (a: any) => (typeof a === 'string' ? assetFromKey(a, metadata) : assetInfo(a, metadata))
  const money = (n: string, a: any) => amountText(n, asset(a), w('atomic'))
  const keys = [
    'approver_threshold',
    'admin_threshold',
    'spendable_assets',
    'tiers',
    'daily_limits',
    'address_book_cooldown_ms',
    'proposal_ttl_ms',
    'unfreeze_delay_ms',
    'hardware_cosigners',
    'public',
    'daily_signatures',
    'refuel',
    'call_targets',
    'evm_chains'
  ]
  function show(key: string, p: any): string {
    const value = p[key]
    if (key.endsWith('_ms')) return durationText(value, locale())
    if (key === 'call_targets') return value?.join('\n') || w('none')
    if (key === 'evm_chains') return value?.map((chain_id: number) => networkName({ asset: 'ETH', chain_id })).join('\n') || w('none')
    if (typeof value === 'boolean') return w(String(value))
    if (key === 'spendable_assets') return value.map((a: any) => assetIdentity(asset(a))).join('\n') || w('none')
    if (key === 'tiers')
      return (
        value
          .map(
            (r: any) =>
              `≤ ${money(r.up_to, r.asset)} · ${r.approvals} ${w('approvals')} · ${durationText(r.delay_ms, locale())}${r.book_only ? ` · ${w('book_only')}` : ''}`
          )
          .join('\n') || w('none')
      )
    if (key === 'daily_limits') return value.map((r: any) => `> ${money(r.amount, r.asset)} → ${w('allVoters')}`).join('\n') || w('none')
    if (key === 'refuel')
      return value
        ? `#${value.from} · ${money(value.icp_e8s, 'icp')} × ${value.max_per_day} / ${durationText(86400000, locale())} · < ${cyclesText(value.min_cycles)} T`
        : w('none')
    return String(value ?? '—')
  }
</script>

<dl class="grid grid-cols-[minmax(0,1fr)_minmax(0,2fr)] gap-x-4 gap-y-3 text-[13px]" data-testid="wallet-policy-summary">
  {#each keys as key (key)}
    {@const changed = before && JSON.stringify(before[key]) !== JSON.stringify(policy[key])}
    <dt class="text-muted">{w(key)}</dt>
    <dd class="min-w-0 whitespace-pre-line break-words">
      {#if changed}<del class="text-danger block" aria-label={w('before')}>{show(key, before)}</del>{/if}
      <span class:font-semibold={changed} class:text-forest={changed}>{show(key, policy)}</span>
    </dd>
  {/each}
</dl>
<p class="text-muted mt-3 text-[12px]">{w('policyLimitsHint')}</p>
{#if policy.refuel}<p class="text-muted mt-2 text-[12px]">{w('topupRuleHint')}</p>{/if}
<Collapsible class="mt-4">
  {#snippet title()}<span class="text-[12px]">{w('technical')}</span>{/snippet}
  <p class="text-[12px]">{w('version')}: {policy.version}</p>
</Collapsible>
