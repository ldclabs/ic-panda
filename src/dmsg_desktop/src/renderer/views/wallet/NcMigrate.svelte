<script lang="ts">
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import Segmented from '../../components/Segmented.svelte'
  import Fact from '../../components/page/Fact.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { call, units, when } from '../../rpc'
  import { walletError as errorText } from './errors'
  import type { Wallet } from './model'
  import { w } from './text'

  /**
   * Moving a native Taproot address to this device. Its L1 commits to the
   * old device's key, so this device registers its own key, gets a Taproot
   * address with the old delay and recovery, adds it to the address book
   * when the tier needs that, and sweeps the old address there without
   * change: through L1 while the old device cosigns, or through L2 once
   * outputs are deep enough. Each step is a proposal the team approves; the
   * dialog proposes the next one and shows what waits.
   */
  let {
    open = $bindable(false),
    wallet,
    from,
    onChanged,
    onShowProposal
  }: { open?: boolean; wallet: Wallet; from: number; onChanged: () => void; onShowProposal: (id: number) => void } = $props()

  let path = $state('l1'),
    view = $state<any>(null),
    busy = $state(false),
    error = $state('')
  let read = 0

  const LABELS: Record<string, string> = {
    root: 'ncStepRoot',
    register: 'ncStepRegister',
    address: 'ncStepAddress',
    book: 'ncStepBook',
    cooldown: 'ncStepCooldown',
    wait: 'ncStepWait',
    sweep: 'ncStepSweep',
    archive: 'ncStepArchive',
    done: 'ncStepDone'
  }
  const checklist = $derived([
    { key: 'register', done: view?.cosigner != null },
    { key: 'address', done: !!view?.to },
    { key: 'sweep', done: !!view?.archived || !!view?.sweep?.empty },
    { key: 'archive', done: !!view?.archived }
  ])
  const proposes = $derived(['register', 'address', 'book', 'sweep', 'archive'].includes(view?.next) && !(view?.next === 'sweep' && view?.sweep?.dust))
  const btc = (sats: string) => `${units(sats, 8)} BTC`
  const spendable = (u: any) => u.spendable_at != null && u.spendable_at <= view.sweep.tip + 1

  /** What this path would do now, read again on every change: an older read never shows. */
  async function load(chosen: string) {
    const turn = ++read
    view = null
    error = ''
    try {
      const value = await call('wallet/nc_migrate_plan', { wallet: wallet.wallet, from, path: chosen })
      if (turn === read) view = value
    } catch (e) {
      if (turn === read) error = errorText(e)
    }
  }

  async function next() {
    if (busy || !proposes) return
    busy = true
    error = ''
    const turn = ++read
    try {
      const value = await call('wallet/nc_migrate_start', { wallet: wallet.wallet, from, path })
      if (turn === read) view = value
      onChanged()
    } catch (e) {
      if (turn === read) error = errorText(e)
    } finally {
      busy = false
    }
  }

  $effect(() => {
    if (open) void load(path)
  })
</script>

<Dialog bind:open title={w('ncTitle')} description={w('ncHint')} icon="transfer" dismissible={!busy} wide testid="wallet-nc-migrate">
  <div class="grid gap-3">
    <Segmented
      bind:value={path}
      label={w('ncPath')}
      options={[
        { value: 'l1', label: w('ncL1') },
        { value: 'l2', label: w('ncL2') }
      ]}
      disabled={busy}
      testid="wallet-nc-path"
      class="justify-self-start"
    />
    <Notice tone={path === 'l1' ? 'warn' : 'info'}
      >{path === 'l1' ? w('ncL1Hint') : t('wallet.ncL2Hint', { blocks: view?.from?.chain_key_after ?? '…' })}</Notice
    >
    {#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
    {#if !view && !error}<p class="text-muted text-[12px]" role="status">{w('loading')}</p>{/if}
    {#if view}
      <dl>
        <Fact label={w('ncFrom')}
          ><b>{view.from.label || `#${view.from.id}`}</b><bdi dir="ltr" class="font-mono text-[12px] break-all">{view.from.text}</bdi></Fact
        >
        <Fact label={w('ncTo')}
          >{#if view.to}<b>{view.to.label || `#${view.to.id}`}</b><bdi dir="ltr" class="font-mono text-[12px] break-all">{view.to.text}</bdi
            >{:else}—{/if}</Fact
        >
      </dl>
      <ol class="grid gap-1" data-testid="wallet-nc-steps">
        {#each checklist as step (step.key)}<li class="flex items-center gap-2 text-[13px]"
            ><Icon name={step.done ? 'check' : 'pending'} class={cn('size-4', step.done ? 'text-ok' : 'text-faint')} />{w(LABELS[step.key])}</li
          >{/each}
      </ol>
      <p class="text-[13px]" data-testid="wallet-nc-next">{w('ncNextStep')}: <b>{w(LABELS[view.next] ?? view.next)}</b></p>
      {#if view.pending}<Notice
          >{t('wallet.ncPending', { id: view.pending.proposal, status: view.pending.status })}{#snippet action()}<button
              class={buttonClass('secondary', 'xs')}
              onclick={() => onShowProposal(view.pending.proposal)}>{w('details')}</button
            >{/snippet}</Notice
        >{/if}
      {#if view.next === 'cooldown'}<Notice>{w('cooldown')}: {when(view.cooldown_until)}</Notice>{/if}
      {#if view.next === 'root'}<Notice tone="warn">{w('ncNoRoot')}</Notice>{/if}
      {#if view.sweep}
        <div class="grid gap-2" data-testid="wallet-nc-sweep">
          <p class="text-faint text-[12px]">{t('wallet.ncTip', { height: view.sweep.tip })} · {view.sweep.fee_rate} sat/vB</p>
          <ul class="divide-line border-line divide-y rounded-xl border">
            {#each view.sweep.utxos as u (`${u.txid}:${u.vout}`)}<li class="flex flex-wrap items-center gap-2 px-3 py-2 text-[12px]"
                ><bdi dir="ltr" class="min-w-0 flex-1 truncate font-mono" title={`${u.txid}:${u.vout}`}>{u.txid.slice(0, 16)}…:{u.vout}</bdi
                ><b class="tabular-nums">{btc(u.value)}</b>{#if path === 'l1' || spendable(u)}<Pill tone="ok">{w('ncSpendableNow')}</Pill
                  >{:else if u.spendable_at == null}<Pill>{w('ncUnconfirmed')}</Pill>{:else}<Pill tone="warn"
                    >{t('wallet.ncSpendableAt', { height: u.spendable_at })}</Pill
                  >{/if}</li
              >{:else}<li class="text-faint px-3 py-2 text-[12px]">{w('ncEmpty')}</li>{/each}
          </ul>
          {#if !view.sweep.empty}<dl>
              <Fact label={w('ncInputs')}>{view.sweep.inputs} · {btc(view.sweep.total)}</Fact>
              {#if view.sweep.fee != null}<Fact label={w('ncFee')}>{btc(view.sweep.fee)}</Fact>{/if}
              {#if view.sweep.amount != null}<Fact label={w('ncAmount')}>{btc(view.sweep.amount)}</Fact>{/if}
            </dl>{/if}
          {#if view.sweep.dust}<Notice tone="warn">{w('ncDust')}</Notice>{/if}
        </div>
      {/if}
      {#if view.next === 'done'}<Notice tone="ok" icon="check">{w('ncStepDone')}</Notice>{/if}
    {/if}
  </div>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('secondary')} disabled={busy} onclick={() => (open = false)}>{t('close')}</button>
    <button class={buttonClass('primary')} disabled={busy || !proposes} aria-busy={busy} onclick={next} data-testid="wallet-nc-next-step"
      >{view?.next === 'archive' ? w('ncArchive') : w('ncNext')}</button
    >
  {/snippet}
</Dialog>
