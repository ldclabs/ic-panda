<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import Collapsible from '../../components/Collapsible.svelte'
  import Dialog from '../../components/Dialog.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Field from '../../components/page/Field.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Select from '../../components/Select.svelte'
  import { buttonClass } from '../../components/ui'
  import { copyText } from '../../components/clipboard'
  import { call, bridge, single, when } from '../../rpc'
  import { t, locale } from '../../i18n/index.svelte'
  import { walletError as errorText } from './errors'
  import Intent from './Intent.svelte'
  import {
    explorerUrl,
    ledgerLookupUrl,
    walletRoute,
    toAtomic,
    resultState,
    queueKind,
    durationText,
    intentFeeAsset,
    amountText,
    assetInfo,
    actions,
    approvalCount,
    knownIntent,
    leaves,
    cosignAddresses,
    type Asset,
    type Wallet,
    type Proposal
  } from './model'
  import { w } from './text'
  let {
    open = $bindable(false),
    wallet,
    id,
    metadata,
    now,
    onUpdated
  }: { open?: boolean; wallet: Wallet; id: number; metadata: Asset[]; now: number; onUpdated: () => void } = $props()
  const noteHintId = $props.id()
  let p = $state<Proposal | null>(null),
    busy = $state(false),
    error = $state(''),
    notice = $state(''),
    packet = $state(''),
    collection = $state<any>(null),
    replace = $state(false)
  let note = $state(''),
    noteLoaded = $state(false),
    noteError = $state('')
  async function saveNote() {
    try {
      await call('wallet/notes', { wallet: wallet.wallet, proposal: id, note })
      if (alive) { notice = w('saved'); onUpdated() }
    } catch (e) {
      if (alive) error = errorText(e)
    }
  }
  let nonceAction = $state(''),
    nonceFee = $state('0.001'),
    ledgerItem = $state(0),
    ledgerBlock = $state('')
  const ledgerLink = $derived(wallet.environment === 'production' ? ledgerLookupUrl(p?.ledger) : null)
  const signedNonce = $derived(p?.plan_details?.find((v) => v.seq === p?.signed[0]?.plan)?.nonce)
  let alive = true,
    sequence = 0
  let selectedPlan = $state<number | null>(null),
    collectionError = $state('')
  const plan = $derived(selectedPlan ?? p?.plans ?? 0)
  const available = $derived(p ? actions(p, wallet, now, plan) : null)
  const required = $derived(p ? cosignAddresses(p.intent, wallet) : [])
  const external = $derived(
    p
      ? ['transfer', 'batch'].includes(p.intent.kind)
        ? !['ICP', 'ICRC'].includes(p.intent.asset?.asset)
        : ['cancel_evm_nonce', 'init_sol_nonce'].includes(p.intent.kind)
      : false
  )
  const livePlans = $derived((p?.plan_details ?? []).filter((v) => v.expires_at > now && !p?.signed.some((s) => s.plan === v.seq)))
  const livePlan = $derived(!external || livePlans.some((v) => v.seq === plan))
  // The parent owns wallet events, focus and polling. Every completed
  // wallet refresh also refreshes this open detail, even if its head matches.
  $effect(() => {
    wallet
    untrack(() => void load())
  })
  const readCollection = single(async () => {
    if (!alive || !p) return
    const seq = sequence,
      chosen = plan,
      digest = p.digest
    const readAt = Date.now()
    collection = null
    collectionError = ''
    const usablePlan =
      !external || p.plan_details?.some((v) => v.seq === chosen && v.expires_at > readAt && !p?.signed.some((s) => s.plan === v.seq))
    if (!required.length || !usablePlan || !['Ready', 'Delayed', 'Signed', 'Broadcast'].includes(p.status) || wallet.frozen) return
    try {
      const result = await call('wallet/cosign_status', { wallet: wallet.wallet, proposal: id, plan: chosen })
      if (alive && seq === sequence && chosen === plan && digest === p?.digest) collection = result
    } catch (e) {
      if (alive && seq === sequence && chosen === plan) collectionError = errorText(e)
    }
  })
  const load = single(async () => {
    if (!alive) return
    const seq = ++sequence
    collection = null
    try {
      const result: Proposal = await call('wallet/proposal', { wallet: wallet.wallet, proposal: id })
      if (!alive || seq !== sequence) return
      p = result
      error = ''
      if (!noteLoaded) {
        try {
          const notes: any = await call('wallet/notes', { wallet: wallet.wallet })
          if (alive && seq === sequence) {
            note = notes[id] ?? ''
            noteLoaded = true
            noteError = ''
          }
        } catch (e) {
          if (alive) noteError = errorText(e)
        }
      }
      const readAt = Date.now()
      if (
        selectedPlan === null ||
        !result.plan_details?.some((v) => v.seq === selectedPlan && v.expires_at > readAt && !result.signed.some((s) => s.plan === v.seq))
      )
        selectedPlan = result.plans
      void readCollection()
    } catch (e) {
      if (alive && seq === sequence) error = errorText(e)
    }
  })
  async function run(method: string, extra: Record<string, unknown> = {}) {
    if (busy) return
    sequence++
    busy = true
    error = ''
    notice = ''
    try {
      const result: any = await call(method, { wallet: wallet.wallet, proposal: id, ...extra })
      if (!alive) return
      if (result.kind === 'wallet' && result.digest) await bridge(() => window.dmsg.openApproval(result.id))
      if (method === 'wallet/change') {
        location.hash = walletRoute(wallet.wallet, result.id)
        onUpdated()
        return
      }
      if (method === 'wallet/prepare') selectedPlan = result.plans
      if (result.pending) notice = w('pending')
      if (result.cosign_required) notice = w('cosignNoLocalKey')
      if (result.broadcast_error) error = result.broadcast_error
      if (result.notification_error) notice = w('notificationFailed')
      await load()
      onUpdated()
    } catch (e) {
      if (!alive) return
      error = errorText(e)
    } finally {
      busy = false
      replace = false
    }
  }
  async function proposeNonce() {
    if (!p) return
    try {
      await run('wallet/change', {
        change: {
          kind: nonceAction,
          address: p.intent.from,
          chain_id: p.intent.asset.chain_id,
          proposal: p.id,
          nonce: String(signedNonce),
          ...(nonceAction === 'cancel_nonce' ? { max_fee: toAtomic(nonceFee, 18) } : {})
        }
      })
    } catch (e) {
      error = errorText(e)
    }
  }
  onMount(() => {
    return () => {
      alive = false
      sequence++
    }
  })
</script>

<Dialog bind:open title={`${w('proposals')} #${id}`} description={wallet.name} wide icon="approvals" testid="wallet-proposal-detail">
  {#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
  {#if notice}<Notice>{notice}</Notice>{/if}
  {#if p && available}
    <div class="grid gap-4">
      <div class="flex flex-wrap items-center gap-2"
        ><Pill>{w(resultState(p, now))}</Pill><span class="text-[12px]"
          >{approvalCount(p, wallet.members)} / {p.requirement.approvals} · {w(p.requirement.admin ? 'Admin' : 'Approver')}</span
        >{#if available.voted}<Pill>{w('voted')}</Pill>{/if}</div
      >
      {#if resultState(p, now) === 'partialPayment'}<Notice tone="warn">{w('partialNext')}</Notice>
      {:else if resultState(p, now) === 'resultUnknown'}<Notice tone="warn">{w('unknownCallNext')}</Notice>
      {:else if resultState(p, now) === 'waitingResult' && ['ICP', 'ICRC'].includes(p.intent.asset?.asset)}<Notice tone="warn">{w('ledgerUnknown')}</Notice>{/if}
      {#if available.rebroadcast}<p class="text-muted text-[12px]">{w('confirmationNext')}</p>{/if}
      {#if resultState(p, now) === 'chainReported'}<p class="text-muted text-[12px]">{w('chainReportHint')}</p>{/if}
      {#if queueKind(p, wallet, now) !== 'history'}<p class="text-[13px] font-medium" role="status"
          >{w('nextStep')}: {w(queueKind(p, wallet, now))}{p.ready_at && p.ready_at > now
            ? ` · ${durationText(Math.ceil((p.ready_at - now) / 60000) * 60000, locale())}`
            : ''}</p
        >{/if}
      {#if available.state === 'Open'}<p class="text-muted text-[12px]"
          >{w('remainingVoters')}: {wallet.members
            .filter(
              (m) =>
                m.status === 'Active' &&
                (p?.requirement.admin ? m.role === 'Admin' : ['Admin', 'Approver'].includes(m.role)) &&
                !p?.votes.some((v) => v.member === m.id)
            )
            .map((m) => m.label || m.id)
            .join(', ')}</p
        >{/if}
      <Intent intent={p.intent} {metadata} before={wallet} />
      <Collapsible
        >{#snippet title()}<span class="text-[13px]">{w('technical')}</span>{/snippet}
        <dl class="border-line grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2 border-t pt-4 text-[12px]">
          <dt>{w('Proposer')}</dt><dd class="font-mono break-all">{p.proposer}</dd>
          {#if p.via_agent}<dt>Agent ID</dt><dd class="font-mono break-all">{p.via_agent}</dd>{/if}
          <dt>{t('created')}</dt><dd>{when(p.created_at)}</dd><dt>{t('expires')}</dt><dd>{when(p.expires_at)}</dd>
          {#if p.ready_at}<dt>{t('twReadyAt')}</dt><dd>{when(p.ready_at)}</dd>{/if}
          <dt>{t('digest')}</dt><dd class="font-mono break-all">{p.digest}</dd>
        </dl></Collapsible
      >
      <section
        ><h3 class="mb-2 text-[13px] font-semibold">{w('votes')}</h3>{#each p.votes as v, i (i)}<p class="mb-1 text-[12px] break-all"
            >{wallet.members.find((m) => m.id === v.member)?.label || v.member} · {v.decision === 'Approve'
              ? t('approve')
              : v.decision === 'Reject'
                ? t('reject')
                : t('twVeto')} · {when(v.at)}</p
          >{:else}<p class="text-faint text-[12px]">{w('none')}</p>{/each}</section
      >
      {#if p.failure}<Notice failure={p.failure} tone="danger">{p.failure}</Notice>{/if}
      {#if p.status === 'Executing' && ['ICP', 'ICRC'].includes(p.intent.asset?.asset)}
        <section class="border-line grid gap-3 rounded-lg border p-3" data-testid="wallet-ledger-reconcile">
          <Notice>{w('ledgerUnknown')}</Notice>
          <p class="text-[13px]">{w('ledgerLookupHint')}</p><p class="text-[12px] break-all">{p.ledger} · {p.source_account}</p>
          {#if ledgerLink}<button
              class={buttonClass('ghost', 'sm', 'justify-self-start')}
              onclick={() => bridge(() => window.dmsg.openUrl(ledgerLink))}>{w('ledgerLookup')}</button
            >{/if}
          <Field label={w('recipient')}
            ><Select
              value={String(ledgerItem)}
              onChange={(value) => (ledgerItem = Number(value))}
              options={(p.intent.outputs ?? []).map((o: any, i: number) => ({
                value: String(i),
                label: `${i + 1}. ${amountText(o.amount, assetInfo(p!.intent.asset, metadata), w('atomic'))} → ${o.to}`
              }))}
            /></Field
          >
          <Field label={w('ledgerBlock')}><input bind:value={ledgerBlock} inputmode="numeric" dir="ltr" /></Field>
          <button
            class={buttonClass('secondary', 'sm')}
            disabled={busy || !/^\d+$/.test(ledgerBlock) || !available.execute}
            onclick={() => run('wallet/reconcile_transfer', { item: ledgerItem, block: ledgerBlock })}>{w('reconcile')}</button
          >
        </section>
      {/if}
      {#if available.rebroadcast && p.intent.kind === 'transfer' && ['ETH', 'ERC20'].includes(p.intent.asset?.asset) && signedNonce != null}
        <Collapsible
          >{#snippet title()}<span class="text-[13px]">{w('advancedRecovery')}</span>{/snippet}
          <div class="flex flex-wrap gap-2">
            <button class={buttonClass('secondary', 'sm')} disabled={busy || wallet.frozen} onclick={() => (nonceAction = 'cancel_nonce')}
              >{w('cancel_evm_nonce')}</button
            >
            <button class={buttonClass('ghost', 'sm')} disabled={busy || wallet.frozen} onclick={() => (nonceAction = 'release_nonce')}
              >{w('release_evm_nonce')}</button
            >
          </div>
          {#if nonceAction}<section class="border-line grid gap-3 rounded-lg border p-3">
              <Notice tone="warn">{w('nonceReleaseHint')} · #{p.id} · nonce {signedNonce}</Notice>
              {#if nonceAction === 'cancel_nonce'}<Field label={`${w('fee')} · ETH`}
                  ><input bind:value={nonceFee} inputmode="decimal" dir="ltr" /></Field
                >{/if}
              <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={proposeNonce}>{w('submit')}</button>
            </section>{/if}</Collapsible
        >
      {/if}
      {#if p.outcome}<Collapsible
          >{#snippet title()}<span class="text-[13px]">{w('details')}</span>{/snippet}<section class="border-line rounded-lg border p-3"
            ><h3 class="text-[13px] font-semibold">{w('details')}</h3>{#each leaves(p.outcome) as row, i (i)}<p
                class="text-[12px] break-all">{row.path.map(w).join(' · ')}: {w(row.value)}</p
              >{/each}<button class={buttonClass('ghost', 'xs')} onclick={() => copyText(JSON.stringify(p!.outcome, null, 2))}
              >{t('copy')}</button
            ></section
          ></Collapsible
        >{/if}
      {#each p.plan_details ?? [] as detail, i (i)}
        <section class="border-line rounded-lg border p-3"
          ><h3 class="text-[13px] font-semibold">{w('plan')} #{detail.seq} · {detail.chain}</h3><p class="text-[12px]"
            >{w('fee')}: {amountText(detail.fee, intentFeeAsset(p.intent, metadata), w('atomic'))} · {t('expires')}: {when(
              detail.expires_at
            )} · {durationText(Math.max(0, Math.ceil((detail.expires_at - now) / 60000)) * 60000, locale())}</p
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
      {#if required.length && ['Ready', 'Delayed', 'Signed', 'Broadcast'].includes(available.state)}
        <section class="border-line bg-surface-2 grid gap-3 rounded-xl border p-4" data-testid="wallet-cosign-collection">
          <h3 class="text-[13px] font-semibold">{w('cosigners')} · {w('plan')} #{plan}</h3><p class="text-muted text-[12px]"
            >{w('collectHint')}</p
          >
          {#if external && livePlans.length > 1}<Field label={w('plan')}
              ><Select
                value={String(plan)}
                options={livePlans.map((v) => ({ value: String(v.seq), label: `#${v.seq} · ${when(v.expires_at)}` }))}
                onChange={(value) => {
                  selectedPlan = Number(value)
                  collection = null
                  void readCollection()
                }}
              /></Field
            >{/if}
          {#if !livePlan}<button
              class={buttonClass('secondary', 'sm', 'justify-self-start')}
              disabled={busy || wallet.frozen || (!available.execute && !available.resign)}
              onclick={() => run('wallet/prepare')}>{w(plan ? 'renewPlan' : 'preparePlan')}</button
            >
          {:else if collection}
            {#each collection.requirements ?? [{ address: required[0].id, collected: collection.collected, required: collection.required }] as r (r.address)}
              <p>#{r.address} · {w('collected')}: {r.collected} / {r.required}</p>
            {/each}

            {#if collection.can_cosign === false}<p class="text-muted text-[12px]">{w('cosignNoLocalKey')}</p>{/if}
          {/if}
          {#if collectionError}<Notice failure={collectionError} tone="danger"
              >{collectionError}<button class={buttonClass('ghost', 'xs')} onclick={load}>{w('retry')}</button></Notice
            >{/if}
          <Collapsible
            >{#snippet title()}<span class="text-[13px]">{w('packet')} · {w('technical')}</span>{/snippet}
            {#if collection}<button class={buttonClass('secondary', 'sm')} onclick={() => copyText(collection.packet)}
                >{w('copyPacket')}</button
              >{/if}
            <Field label={w('packet')}
              ><textarea bind:value={packet} rows="3" spellcheck="false" class="font-mono text-[11px]" dir="ltr"></textarea></Field
            >
            <div class="flex flex-wrap gap-2">
              <button
                class={buttonClass('secondary', 'sm')}
                disabled={busy || wallet.frozen || !livePlan || !packet.trim()}
                onclick={() => run('wallet/cosign_import', { plan, packet: packet.trim() })}>{w('importPacket')}</button
              ></div
            ></Collapsible
          ><div class="flex flex-wrap gap-2">
            <button
              class={buttonClass('secondary', 'sm')}
              disabled={busy ||
                wallet.frozen ||
                !livePlan ||
                wallet.me?.status !== 'Active' ||
                wallet.me?.role === 'Viewer' ||
                !collection?.can_cosign}
              onclick={() => run('wallet/cosign', { plan })}>{w('cosign')}</button
            >
            <button
              class={buttonClass('primary', 'sm')}
              disabled={busy || !livePlan || !collection?.ready || !available.executeCollected}
              onclick={() => run('wallet/cosign_execute', { plan })}>{w('executeCollected')}</button
            >
          </div>
        </section>
      {/if}
      <section class="border-line grid gap-2 border-t pt-4">
        {#if noteError}<Notice failure={noteError} tone="danger"
            >{noteError}<button class={buttonClass('ghost', 'sm')} onclick={load}>{w('retry')}</button></Notice
          >{/if}
        <Field label={w('privateNote')} hint={w('privateNoteHint')} hintId={noteHintId}><textarea aria-label={w('privateNote')} aria-describedby={noteHintId} bind:value={note} maxlength="500" rows="2"></textarea></Field>
        <button class={buttonClass('ghost', 'sm', 'justify-self-start')} disabled={!noteLoaded} onclick={saveNote}>{w('save')}</button>
      </section>
      {#if replace}<Notice tone="warn"
          >{w('resignHint')}<button
            class={buttonClass('danger', 'sm', 'mt-2')}
            disabled={busy}
            onclick={() => run(required.length ? 'wallet/prepare' : 'wallet/execute')}>{w('continue')}</button
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
      {#if available.execute && (!required.length || p.status === 'Executing')}<button
          class={buttonClass('primary', 'sm')}
          disabled={busy}
          onclick={() => run(p?.status === 'Executing' ? 'wallet/reconcile' : 'wallet/execute')}
          >{p.status === 'Executing' ? w('reconcile') : t('twExecute')}</button
        >{/if}
      {#if available.rebroadcast}<button class={buttonClass('primary', 'sm')} disabled={busy} onclick={() => run('wallet/settle')}>{t('twSettle')}</button>
        <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => run('wallet/broadcast')}
          >{w('rebroadcast')}</button
        >{/if}
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
