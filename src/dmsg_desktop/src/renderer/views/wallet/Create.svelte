<script lang="ts">
  import { walletError as errorText } from './errors'
  import { onMount } from 'svelte'
  import Avatar from '../../components/Avatar.svelte'
  import Icon from '../../components/Icon.svelte'
  import Field from '../../components/page/Field.svelte'
  import Id from '../../components/page/Id.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import Select from '../../components/Select.svelte'
  import Tooltip from '../../components/Tooltip.svelte'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { bridge, call, units } from '../../rpc'
  import { activationTotal, MAX_DRAFT_MEMBERS, memberInputs, memberProblem, thresholdLimits, type MemberDraft } from './new-wallet'

  import { utf8Length } from './model'
  import { w } from './text'
  import { WALLET_CREATION_PAUSED } from '../../../shared/wallet-availability'
  let { status, onRequest }: { status: any; onRequest: (request: any, owner: string) => void } = $props()
  let preview = $state<any>(null),
    readiness = $state<any>(null),
    quoteLoading = $state(false),
    busy = $state(false),
    error = $state('')
  let alive = true
  let name = $state(''),
    nextDraft = 0,
    submitted = $state<any>(null)
  const blankDraft = (): MemberDraft => ({ key: nextDraft++, id: '', role: 'approver', label: '', touched: false })
  let drafts = $state<MemberDraft[]>([blankDraft()]),
    approvals = $state(1),
    adminApprovals = $state(1)
  const creator = $derived(String(status?.account ?? ''))
  const limits = $derived(thresholdLimits(drafts))
  const memberCount = $derived(1 + drafts.filter((row) => row.id.trim()).length)
  const problems = $derived(drafts.map((row) => memberProblem(row, drafts, creator)))
  const invalid = $derived(problems.some((p) => p !== null) || drafts.some((d) => utf8Length(d.label) > 64))

  const remaining = $derived(preview?.remaining ?? 0)
  const total = $derived(preview ? activationTotal(preview.price_e8s, preview.ledger_fee_e8s) : '0')
  const canCreate = $derived(
    !!name.trim() &&
      utf8Length(name.trim()) <= 64 &&
      !invalid &&
      readiness?.blockers?.length === 0 &&
      BigInt(readiness?.balance?.balance ?? '0') >= BigInt(total)
  )
  const roleOptions = $derived(
    ['admin', 'approver', 'proposer', 'viewer', 'recovery'].map((role) => ({
      value: role,
      label: w(role[0].toUpperCase() + role.slice(1))
    }))
  )
  $effect(() => {
    if (approvals > limits.approvers) approvals = limits.approvers
    if (adminApprovals > limits.admins) adminApprovals = limits.admins
  })
  async function getQuote() {
    if (WALLET_CREATION_PAUSED) return
    quoteLoading = true
    error = ''
    try {
      const [q, r] = await Promise.all([call('wallet/quote_preview'), call('wallet/readiness', { funding: true })])
      preview = q
      readiness = r
    } catch (e) {
      error = errorText(e)
    } finally {
      quoteLoading = false
    }
  }
  function addMember() {
    if (drafts.length < MAX_DRAFT_MEMBERS) drafts = [...drafts, blankDraft()]
  }
  function removeMember(key: number) {
    drafts = drafts.filter((r) => r.key !== key)
    if (!drafts.length) drafts = [blankDraft()]
  }
  function cancelCreate() {
    name = ''
    drafts = [blankDraft()]
    approvals = 1
    adminApprovals = 1
    preview = null
  }
  async function create() {
    if (WALLET_CREATION_PAUSED) return
    if (submitted) {
      await bridge(() => window.dmsg.openApproval(submitted.id))
      return
    }
    if (!canCreate) return
    busy = true
    error = ''
    const owner = creator
    try {
      readiness = await call('wallet/readiness', { funding: true })
      if (readiness.blockers.length) return
      const quote: any = await call('wallet/quote')
      preview = quote
      if (!quote.remaining) {
        return
      }
      if (BigInt(readiness.balance.balance) < BigInt(activationTotal(quote.price_e8s, quote.ledger_fee_e8s))) {
        error = w('INSUFFICIENT_NATIVE_BALANCE')
        return
      }
      const request = await call('wallet/create', {
        quote: quote.quote_id,
        name: name.trim(),
        members: memberInputs(drafts),
        approvals,
        admin_approvals: adminApprovals
      })
      if (!alive || creator !== owner) return
      submitted = request
      onRequest(request, owner)
      await bridge(() => window.dmsg.openApproval(request.id))
    } catch (e) {
      error = errorText(e)
    } finally {
      busy = false
    }
  }
  onMount(() => {
    void getQuote()
    return () => {
      alive = false
    }
  })
</script>

{#if WALLET_CREATION_PAUSED}
  <Notice testid="wallet-creation-paused"><b>{w('creationPaused')}</b> · {w('creationPausedHint')}</Notice>
{:else}
{#if error}<Notice tone="danger">{error}<button class={buttonClass('ghost', 'xs')} onclick={getQuote}>{w('retry')}</button></Notice>{/if}
{#if readiness}
  <div class="border-line bg-surface rounded-xl border p-4 text-[12px]">
    <p>{w('payer')}: <span class="font-mono break-all">{readiness.payer?.principal ?? readiness.payer?.payer ?? '—'}</span></p>
    <p>{t('balance')}: {readiness.balance?.balance ? units(String(readiness.balance.balance), 8) + ' ICP' : '—'}</p>
    {#if preview && BigInt(readiness.balance?.balance ?? '0') < BigInt(total)}<p class="text-danger mt-2"
        >{w('INSUFFICIENT_NATIVE_BALANCE')}</p
      >{/if}
    {#each readiness.blockers ?? [] as blocker}<p class="text-danger mt-2">{w(blocker)}</p>{/each}
    {#if readiness.blockers?.length}<a href="#/settings/devices" class="text-forest underline">{t('settings')}</a>{/if}
  </div>
{/if}
<Section title={t('twNew')} description={t('twNewHint')} icon="add" testid="wallet-new">
  {#if !preview}
    <div class="border-line bg-surface-2 flex flex-wrap items-center gap-3 rounded-xl border px-4 py-3.5">
      <span class="border-line bg-surface text-forest grid size-10 shrink-0 place-items-center rounded-xl border"
        ><Icon name="coins" /></span
      >
      <p class="text-muted min-w-0 flex-1 text-[12.5px] leading-relaxed">{t('twQuoteHint')}</p>
      <button class={buttonClass('secondary')} disabled={busy || quoteLoading} onclick={getQuote} data-testid="wallet-quote">
        <Icon name={quoteLoading ? 'pending' : 'coins'} class={quoteLoading ? 'animate-spin motion-reduce:animate-none' : ''} />
        {t('twQuote')}
      </button>
    </div>
  {:else}
    <div class="border-line bg-surface-2 @container rounded-xl border p-3.5" data-testid="wallet-quote-line">
      <div class="flex flex-wrap items-start gap-3">
        <span class="border-line bg-surface text-forest mt-0.5 grid size-10 shrink-0 place-items-center rounded-xl border"
          ><Icon name="coins" /></span
        >
        <dl class="grid min-w-0 flex-1 grid-cols-2 gap-x-6 gap-y-2 @min-[520px]:grid-cols-3">
          <div class="min-w-0">
            <dt class="text-faint text-[11px]">{t('twActivation')}</dt>
            <dd class="text-[19px] leading-snug font-semibold tabular-nums"
              >{units(preview.price_e8s, 8)}{'\u00a0'}<span class="text-muted text-[12px] font-normal">ICP</span></dd
            >
          </div>
          <div class="min-w-0">
            <dt class="text-faint text-[11px]">{t('twFee')}</dt>
            <dd class="text-[13.5px] font-medium tabular-nums"
              >{units(preview.ledger_fee_e8s, 8)}{'\u00a0'}<span class="text-muted text-[12px] font-normal">ICP</span></dd
            >
          </div>
          <div class="min-w-0">
            <dt class="text-faint text-[11px]">{t('twTotal')}</dt>
            <dd class="text-[13.5px] font-semibold tabular-nums"
              >{units(total, 8)}{'\u00a0'}<span class="text-muted text-[12px] font-normal">ICP</span></dd
            >
          </div>
        </dl>
        <div class="flex items-center gap-1">
          <Pill tone={remaining > 0 ? 'forest' : 'warn'}>{t('twQuotaLeft', { left: preview.remaining, plan: preview.plan })}</Pill>
          <Tooltip content={t('refresh')}>
            {#snippet children(props)}
              <button
                {...props}
                class={buttonClass('ghost', 'sm', 'size-7 px-0')}
                aria-label={t('refresh')}
                disabled={busy || quoteLoading}
                onclick={getQuote}
                data-testid="wallet-quote"
              >
                <Icon name="sync-now" class={quoteLoading ? 'animate-spin motion-reduce:animate-none' : ''} />
              </button>
            {/snippet}
          </Tooltip>
        </div>
      </div>
      {#if remaining === 0}
        <p class="border-line text-warn mt-3 flex flex-wrap items-center gap-1.5 border-t pt-2.5 text-[12px]">
          <Icon name="warning" class="size-3.5" />
          <span>{t('twQuoteNone')}</span>
          <a class="text-forest font-medium underline underline-offset-2" href="#/plan">{t('openPlan')}</a>
        </p>
      {/if}
    </div>

    {#if submitted}<button class={buttonClass('primary', 'sm', 'mt-4')} onclick={() => bridge(() => window.dmsg.openApproval(submitted.id))}
        >{w('openApproval')}</button
      >{/if}
    {#if remaining > 0 && !submitted}
      <div class="mt-4 grid gap-5" data-testid="wallet-new-form">
        <Field label={t('twName')}>
          <input
            aria-invalid={utf8Length(name.trim()) > 64}
            bind:value={name}
            placeholder={t('twNameHint')}
            maxlength="64"
            autocomplete="off"
            data-testid="wallet-name"
          />
        </Field>
        {#if utf8Length(name.trim()) > 64}<p class="text-danger text-[12px]">{w('nameBytes')}</p>{/if}

        <div class="grid gap-2">
          <div class="flex flex-wrap items-center justify-between gap-2">
            <div class="flex items-baseline gap-2">
              <span class="text-faint text-[12px]">{t('members')}</span>
              <span class="text-faint text-[11px] tabular-nums">{memberCount}</span>
            </div>
            <button
              type="button"
              class={buttonClass('secondary', 'xs')}
              disabled={busy || drafts.length >= MAX_DRAFT_MEMBERS}
              onclick={addMember}
              data-testid="wallet-member-add"
            >
              <Icon name="login" />{t('twAddMember')}
            </button>
          </div>
          <div class="border-line @container overflow-hidden rounded-xl border" data-testid="wallet-members">
            <div class="border-line bg-pale/40 flex flex-wrap items-center gap-2 border-b px-3 py-2.5" data-testid="wallet-creator">
              {#if creator}<Avatar id={creator} size="sm" />{/if}
              <div class="min-w-0 flex-1">
                <div class="flex flex-wrap items-center gap-1.5">
                  <b class="text-[12.5px] font-medium">{t('you')}</b>
                  <Pill tone="forest">{t('twRoleAdmin')}</Pill>
                </div>
                {#if creator}<div class="mt-0.5"><Id id={creator} /></div>{/if}
              </div>
            </div>
            <div
              class="border-line bg-surface-2 text-faint hidden gap-2 border-b px-2.5 py-1.5 text-[11px] font-medium @min-[560px]:grid @min-[560px]:grid-cols-[minmax(0,1fr)_10rem_10rem_1.75rem]"
            >
              <span>{t('twMemberId')}</span>
              <span>{t('role')}</span>
              <span>{t('label')}</span>
              <span></span>
            </div>
            {#each drafts as row, i (row.key)}
              {@const problem = problems[i]}
              <div
                class="border-line grid gap-2 border-b p-2.5 last:border-0 @min-[560px]:grid-cols-[minmax(0,1fr)_10rem_10rem_1.75rem] @min-[560px]:items-start"
                data-testid="wallet-member"
              >
                <input
                  bind:value={row.id}
                  class="min-w-0 font-mono text-[12.5px] placeholder:font-sans"
                  placeholder={t('twMemberId')}
                  spellcheck="false"
                  autocomplete="off"
                  aria-label={t('twMemberId')}
                  aria-invalid={problem ? 'true' : undefined}
                  onblur={() => (row.touched = true)}
                  data-testid="wallet-member-id"
                />
                <Select bind:value={row.role} options={roleOptions} label={t('role')} testid="wallet-member-role" class="w-full" />
                <input
                  bind:value={row.label}
                  class="min-w-0"
                  placeholder={t('label')}
                  maxlength="64"
                  aria-label={t('label')}
                  data-testid="wallet-member-label"
                />
                <button
                  type="button"
                  class={buttonClass('ghost', 'sm', 'size-7 justify-self-start px-0')}
                  aria-label={t('twRemoveMember')}
                  disabled={busy}
                  onclick={() => removeMember(row.key)}
                  data-testid="wallet-member-remove"><Icon name="close" /></button
                >
                {#if problem && (row.touched || problem !== 'twMemberInvalid')}
                  <p class="text-danger text-[11.5px] @min-[560px]:col-span-4" data-testid="wallet-member-error">{t(problem)}</p>
                {/if}
              </div>
            {/each}
          </div>
          <p class="text-faint text-[11.5px] leading-normal">{t('twMembersHint')}</p>
        </div>

        <div class="@container grid gap-2">
          <span class="text-faint text-[12px]">{t('twPolicySection')}</span>
          <div class="grid gap-2 @min-[560px]:grid-cols-2">
            <div class="border-line bg-surface-2 flex items-start gap-3 rounded-xl border p-3.5">
              <div class="min-w-0 flex-1">
                <p class="text-[12.5px] font-medium">{t('twApprovals')}</p>
                <p class="text-faint mt-0.5 text-[11.5px] leading-normal">{t('twApprovalsHint', { max: limits.approvers })}</p>
              </div>
              <div class="border-edge-soft bg-surface flex shrink-0 items-center rounded-lg border p-0.5">
                <button
                  type="button"
                  class={buttonClass('ghost', 'sm', 'size-7 px-0')}
                  aria-label={`${t('twDecrease')} ${t('twApprovals')}`}
                  disabled={busy || approvals <= 1}
                  onclick={() => (approvals -= 1)}
                  data-testid="wallet-approvals-dec"><span aria-hidden="true" class="text-[15px] leading-none">−</span></button
                >
                <span class="w-8 text-center text-[13px] font-medium tabular-nums" data-testid="wallet-approvals">{approvals}</span>
                <button
                  type="button"
                  class={buttonClass('ghost', 'sm', 'size-7 px-0')}
                  aria-label={`${t('twIncrease')} ${t('twApprovals')}`}
                  disabled={busy || approvals >= limits.approvers}
                  onclick={() => (approvals += 1)}
                  data-testid="wallet-approvals-inc"><span aria-hidden="true" class="text-[15px] leading-none">+</span></button
                >
              </div>
            </div>
            <div class="border-line bg-surface-2 flex items-start gap-3 rounded-xl border p-3.5">
              <div class="min-w-0 flex-1">
                <p class="text-[12.5px] font-medium">{t('twAdminApprovals')}</p>
                <p class="text-faint mt-0.5 text-[11.5px] leading-normal">{t('twAdminApprovalsHint', { max: limits.admins })}</p>
              </div>
              <div class="border-edge-soft bg-surface flex shrink-0 items-center rounded-lg border p-0.5">
                <button
                  type="button"
                  class={buttonClass('ghost', 'sm', 'size-7 px-0')}
                  aria-label={`${t('twDecrease')} ${t('twAdminApprovals')}`}
                  disabled={busy || adminApprovals <= 1}
                  onclick={() => (adminApprovals -= 1)}
                  data-testid="wallet-admin-approvals-dec"><span aria-hidden="true" class="text-[15px] leading-none">−</span></button
                >
                <span class="w-8 text-center text-[13px] font-medium tabular-nums" data-testid="wallet-admin-approvals"
                  >{adminApprovals}</span
                >
                <button
                  type="button"
                  class={buttonClass('ghost', 'sm', 'size-7 px-0')}
                  aria-label={`${t('twIncrease')} ${t('twAdminApprovals')}`}
                  disabled={busy || adminApprovals >= limits.admins}
                  onclick={() => (adminApprovals += 1)}
                  data-testid="wallet-admin-approvals-inc"><span aria-hidden="true" class="text-[15px] leading-none">+</span></button
                >
              </div>
            </div>
          </div>
        </div>

        <div class="border-line flex flex-wrap items-center gap-2 border-t pt-4">
          <p class="text-faint min-w-0 flex-1 text-[11.5px] leading-normal">
            {t('twCreateSummary', { total: units(total, 8), approvals, admin: adminApprovals })}
          </p>
          <button type="button" class={buttonClass('ghost')} disabled={busy} onclick={cancelCreate} data-testid="wallet-create-cancel"
            >{t('cancel')}</button
          >
          <button
            type="button"
            class={buttonClass('primary')}
            disabled={busy || quoteLoading || !canCreate}
            onclick={create}
            data-testid="wallet-create"><Icon name="coins" />{t('twCreate')}</button
          >
        </div>
      </div>
    {/if}
  {/if}
</Section>
{/if}
