<script lang="ts">
  import { onMount } from 'svelte'
  import Dialog from '../../components/Dialog.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Field from '../../components/page/Field.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import { buttonClass } from '../../components/ui'
  import { copyText } from '../../components/clipboard'
  import { call, bridge, onChange, when } from '../../rpc'
  import { t } from '../../i18n/index.svelte'
  import { walletError as errorText } from './errors'
  import Intent from './Intent.svelte'
  import { explorerUrl, actions, approvalCount, knownIntent, leaves, type Asset, type Wallet, type Proposal } from './model'
  import { w } from './text'
  let {
    open = $bindable(false),
    wallet,
    id,
    metadata,
    now,
    onUpdated
  }: { open?: boolean; wallet: Wallet; id: number; metadata: Asset[]; now: number; onUpdated: () => void } = $props()
  let p = $state<Proposal | null>(null),
    busy = $state(false),
    error = $state(''),
    notice = $state(''),
    packet = $state(''),
    collection = $state<any>(null),
    replace = $state(false)
  let alive = true,
    sequence = 0
  const available = $derived(p ? actions(p, wallet, now) : null)
  const address = $derived(p ? wallet.addresses.find((a) => a.id === (p!.intent.from ?? p!.intent.address)) : null)
  const plan = $derived(p?.plans ?? 0)
  async function load() {
    const seq = ++sequence
    try {
      const result: Proposal = await call('wallet/proposal', { wallet: wallet.wallet, proposal: id })
      if (!alive || seq !== sequence) return
      p = result
      if (address?.cosign && ['Ready', 'Delayed', 'Signed', 'Broadcast'].includes(result.status)) {
        try {
          const c = await call('wallet/cosign_status', { wallet: wallet.wallet, proposal: id, plan: result.plans })
          if (alive && seq === sequence) collection = c
        } catch {
          if (alive && seq === sequence) collection = null
        }
      }
    } catch (e) {
      if (alive && seq === sequence) error = errorText(e)
    }
  }
  async function run(method: string, extra: Record<string, unknown> = {}) {
    busy = true
    error = ''
    notice = ''
    try {
      const result: any = await call(method, { wallet: wallet.wallet, proposal: id, ...extra })
      if (result.kind === 'wallet' && result.digest) await bridge(() => window.dmsg.openApproval(result.id))
      if (result.pending) notice = w('pending')
      if (result.broadcast_error) error = result.broadcast_error
      if (result.notification_error) notice = w('notificationFailed')
      await load()
      onUpdated()
    } catch (e) {
      error = errorText(e)
    } finally {
      busy = false
      replace = false
    }
  }
  onMount(() => {
    void load()
    const off = onChange(['wallet/changed', 'approval/changed'], load)
    return () => {
      alive = false
      sequence++
      off()
    }
  })
</script>

<Dialog bind:open title={`${w('proposals')} #${id}`} description={wallet.name} wide icon="approvals" testid="wallet-proposal-detail">
  {#if error}<Notice tone="danger">{error}</Notice>{/if}
  {#if notice}<Notice>{notice}</Notice>{/if}
  {#if p && available}
    <div class="grid gap-4">
      <div class="flex flex-wrap items-center gap-2"
        ><Pill>{w(available.state)}</Pill><span class="text-[12px]"
          >{approvalCount(p, wallet.members)} / {p.requirement.approvals} · {w(p.requirement.admin ? 'Admin' : 'Approver')}</span
        >{#if available.voted}<Pill>{w('voted')}</Pill>{/if}</div
      >
      <Intent intent={p.intent} {metadata} before={wallet} />
      <dl class="border-line grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2 border-t pt-4 text-[12px]">
        <dt>{w('Proposer')}</dt><dd class="font-mono break-all">{p.proposer}</dd>
        {#if p.via_agent}<dt>Agent ID</dt><dd class="font-mono break-all">{p.via_agent}</dd>{/if}
        <dt>{t('created')}</dt><dd>{when(p.created_at)}</dd><dt>{t('expires')}</dt><dd>{when(p.expires_at)}</dd>
        {#if p.ready_at}<dt>{t('twReadyAt')}</dt><dd>{when(p.ready_at)}</dd>{/if}
        <dt>{t('digest')}</dt><dd class="font-mono break-all">{p.digest}</dd>
      </dl>
      <section
        ><h3 class="mb-2 text-[13px] font-semibold">{t('twApprovals')}</h3>{#each p.votes as v, i (i)}<p class="mb-1 text-[12px] break-all"
            >{wallet.members.find((m) => m.id === v.member)?.label || v.member} · {v.decision === 'Approve'
              ? t('approve')
              : v.decision === 'Reject'
                ? t('reject')
                : t('twVeto')} · {when(v.at)}</p
          >{:else}<p class="text-faint text-[12px]">{w('none')}</p>{/each}</section
      >
      {#if p.failure}<Notice tone="danger">{p.failure}</Notice>{/if}
      {#if p.outcome}<section class="border-line rounded-lg border p-3"
          ><h3 class="text-[13px] font-semibold">{w('details')}</h3>{#each leaves(p.outcome) as row, i (i)}<p class="text-[12px] break-all"
              >{row.path.map(w).join(' · ')}: {row.value}</p
            >{/each}<button class={buttonClass('ghost', 'xs')} onclick={() => copyText(JSON.stringify(p!.outcome, null, 2))}
            >{t('copy')}</button
          ></section
        >{/if}
      {#each p.plan_details ?? [] as detail, i (i)}
        <section class="border-line rounded-lg border p-3"
          ><h3 class="text-[13px] font-semibold">{w('plan')} #{detail.seq} · {detail.chain}</h3><p class="text-[12px]"
            >{w('fee')}: {detail.fee} {w('atomic')} · {t('expires')}: {when(detail.expires_at)}</p
          ><p class="font-mono text-[11px] break-all">{detail.from}</p></section
        >
      {/each}
      {#each p.signed as signed (signed.plan)}
        {@const link = p.intent.asset ? explorerUrl(p.intent.asset, signed.txid, wallet.environment) : null}
        {#if link}<button class={buttonClass('ghost', 'xs', 'justify-self-start')} onclick={() => bridge(() => window.dmsg.openUrl(link))}
            >{link}</button
          >{/if}<p class="flex gap-2 font-mono text-[11px] break-all"
          >#{signed.plan} · {signed.txid}<button
            class={buttonClass('ghost', 'xs')}
            aria-label={t('copy')}
            onclick={() => copyText(signed.txid)}>{t('copy')}</button
          ></p
        >{/each}
      {#each p.broadcasts ?? [] as b, i (i)}<p class="text-[12px] break-all">{when(b.at)} · {w(b.outcome)} · {b.member} · {b.note}</p
        >{/each}
      {#if address?.cosign && ['Ready', 'Delayed', 'Signed', 'Broadcast'].includes(available.state)}
        <section class="border-line bg-surface-2 grid gap-3 rounded-xl border p-4" data-testid="wallet-cosign-collection">
          <h3 class="text-[13px] font-semibold">{w('cosigners')} · {w('plan')} #{plan}</h3><p class="text-muted text-[12px]"
            >{w('collectHint')}</p
          >
          {#if collection}<p>{w('collected')}: {collection.collected} / {collection.required}</p><button
              class={buttonClass('secondary', 'sm', 'justify-self-start')}
              onclick={() => copyText(collection.packet)}>{w('copyPacket')}</button
            >{/if}
          <Field label={w('packet')}
            ><textarea bind:value={packet} rows="3" spellcheck="false" class="font-mono text-[11px]" dir="ltr"></textarea></Field
          >
          <div class="flex flex-wrap gap-2">
            <button
              class={buttonClass('secondary', 'sm')}
              disabled={busy || !packet.trim()}
              onclick={() => run('wallet/cosign_import', { plan, packet: packet.trim() })}>{w('importPacket')}</button
            >
            <button
              class={buttonClass('secondary', 'sm')}
              disabled={busy || wallet.me?.role === 'Viewer' || (address.chain !== 'Icp' && !plan)}
              onclick={() => run('wallet/cosign', { plan })}>{w('cosign')}</button
            >
            <button
              class={buttonClass('primary', 'sm')}
              disabled={busy || !collection?.ready || !available.execute}
              onclick={() => run('wallet/cosign_execute', { plan })}>{w('executeCollected')}</button
            >
          </div>
        </section>
      {/if}
      {#if replace}<Notice tone="warn"
          >{w('resignHint')}<button class={buttonClass('danger', 'sm', 'mt-2')} disabled={busy} onclick={() => run('wallet/execute')}
            >{w('continue')}</button
          ></Notice
        >{/if}
    </div>
  {:else}<p class="text-muted">{w('loading')}</p>{/if}
  {#snippet footer()}
    {#if p && available}
      {#if available.approve && knownIntent(p.intent)}<button
          class={buttonClass('primary', 'sm')}
          disabled={busy}
          onclick={() => run('wallet/approve')}>{t('approve')}</button
        >{/if}
      {#if available.reject}<button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => run('wallet/reject')}
          >{t('reject')}</button
        >{/if}
      {#if available.veto}<button class={buttonClass('danger', 'sm')} disabled={busy} onclick={() => run('wallet/veto')}
          >{t('twVeto')}</button
        >{/if}
      {#if available.execute}<button
          class={buttonClass('primary', 'sm')}
          disabled={busy}
          onclick={() => run(p?.status === 'Executing' ? 'wallet/reconcile' : 'wallet/execute')}
          >{p.status === 'Executing' ? w('reconcile') : t('twExecute')}</button
        >{/if}
      {#if available.rebroadcast}<button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => run('wallet/broadcast')}
          >{w('rebroadcast')}</button
        ><button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => run('wallet/settle')}>{t('twSettle')}</button>{/if}
      {#if available.resign}<button class={buttonClass('ghost', 'sm')} disabled={busy} onclick={() => (replace = true)}
          >{w('resign')}</button
        >{/if}
      <span class="flex-1"></span>
      {#if available.cancel}<button class={buttonClass('ghost', 'sm')} disabled={busy} onclick={() => run('wallet/cancel')}
          >{t('cancel')}</button
        >{/if}
    {/if}
  {/snippet}
</Dialog>
