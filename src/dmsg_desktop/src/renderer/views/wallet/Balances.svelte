<script lang="ts">
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { when } from '../../rpc'
  import Collapsible from '../../components/Collapsible.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import { amountText, assetChain, assetIdentity, assetKey, type Asset, type Wallet } from './model'
  import { w } from './text'
  let {
    wallet,
    balances,
    metadata,
    busy,
    error,
    onRefresh,
    onReceive,
    onSend
  }: {
    wallet: Wallet
    balances: any
    metadata: Asset[]
    busy: boolean
    error: string
    onRefresh: () => void
    onReceive: (address: number, chain?: number) => void
    onSend: (address: number, asset: Asset) => void
  } = $props()
  const canSend = $derived(wallet.me?.status === 'Active' && !['Viewer', 'Recovery'].includes(wallet.me.role) && !wallet.frozen)
</script>

<section data-testid="wallet-balances" class="grid gap-3" aria-busy={busy}>
  <div class="flex flex-wrap items-center justify-between gap-2">
    <h3 class="text-[15px] font-semibold">{w('holdings')}</h3>
    <button class={buttonClass('ghost', 'sm')} disabled={busy} aria-label={`${t('refresh')} ${w('holdings')}`} onclick={onRefresh}
      >{t('refresh')}</button
    >
  </div>
  <p class="text-[12px] text-muted" role="status"
    >{balances ? `${w('readAt')}: ${when(balances.read_at)}` : w(busy ? 'loading' : 'noBalance')}{balances && busy
      ? ` · ${w('cachedBalance')}`
      : ''}</p
  >
  {#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
  <div class="divide-line divide-y">
    {#each wallet.addresses as address (address.id)}
      {@const amounts =
        balances?.addresses?.find((r: any) => r.address === address.id)?.balances ??
        metadata.filter((a) => assetChain(a) === address.chain).map((a) => ({ metadata: a }))}
      {#each amounts as balance, i (i)}
        {@const asset = balance.metadata ?? balance}
        <div class="flex flex-wrap items-center gap-3 py-3" data-testid="wallet-balance-row">
          <div class="min-w-0 flex-1"
            ><p class="text-[13px] break-all">{assetIdentity(asset)}</p><p class="text-muted text-[12px]"
              >{address.label || address.chain} · #{address.id}</p
            ></div
          >
          <div class="text-end">
            {#if balance.balance?.Err}<p class="text-danger text-[12px]" role="status">{balance.balance.Err}</p>
            {:else}<b class="text-[20px] tabular-nums"
                >{balance.balance?.Ok != null ? amountText(String(balance.balance.Ok), asset, w('atomic')) : '—'}</b
              >{/if}
          </div>
          <button
            class={buttonClass('secondary', 'sm')}
            disabled={!address.verified}
            onclick={() => onReceive(address.id, asset.chain_id)}
            aria-label={`${w('receive')} ${assetIdentity(asset)}`}>{w('receive')}</button
          >
          {#if canSend && wallet.policy.spendable_assets.some((a: Asset) => assetKey(a) === assetKey(asset))}<button
              class={buttonClass('ghost', 'sm')}
              disabled={!address.verified}
              onclick={() => onSend(address.id, asset)}
              aria-label={`${w('proposePayment')} ${assetIdentity(asset)}`}>{w('send')}</button
            >{/if}
        </div>
      {/each}
    {/each}
  </div>
  {#if balances}<Collapsible
      >{#snippet title()}<span class="text-[12px]">{w('source')}</span>{/snippet}
      {#each balances.addresses ?? [] as row}{#each row.balances ?? [] as b}<p class="text-muted text-[12px] break-all"
            >{assetIdentity(b.metadata ?? b)} · {b.source ?? '—'}</p
          >{/each}{/each}
    </Collapsible>{/if}
</section>
