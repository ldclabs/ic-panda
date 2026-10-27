<script lang="ts">
  import TextField from '../components/TextField.svelte'

  import { RadioGroup } from 'bits-ui'
  import { onDestroy, onMount } from 'svelte'
  import Checkbox from '../components/Checkbox.svelte'
  import Collapsible from '../components/Collapsible.svelte'
  import Icon from '../components/Icon.svelte'
  import Fact from '../components/page/Fact.svelte'
  import Id from '../components/page/Id.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Pill from '../components/page/Pill.svelte'
  import Section from '../components/page/Section.svelte'
  import Radio from '../components/Radio.svelte'
  import { buttonClass, cn } from '../components/ui'
  import { t, type Label } from '../i18n/index.svelte'
  import { bridge, call, errorText, onChange, single, when } from '../rpc'

  /**
   * Legacy migration: a one-time offer opens the old site in the system
   * browser, the old site saves an encrypted file sealed to this device,
   * and the import keeps the checked archive as account content.
   */
  let { status, go }: { status: any; go: (path: string) => void } = $props()
  const ready = $derived(status?.state === 'unlocked')
  const guidance = $derived<Label>(
    !status ? 'loading' : status.state === 'logged_out' ? 'legacyLoginHint'
      : status.state === 'locked' ? 'legacyUnlockHint'
      : status.state === 'no_account' || status.setup_pending ? 'legacySetupHint'
      : status.state === 'pairing_required' ? 'legacyDeviceHint' : 'legacyRootHint'
  )
  let offer = $state<any>(null)
  let pairs = $state<any[]>([])
  let archives = $state<any[]>([])
  let selected = $state('')
  let principal = $state('')
  let confirmed = $state(false)
  let report = $state<any>(null)
  let limit = $state(100)
  let notice = $state('')
  let error = $state('')
  let busy = $state(false)
  let alive = true
  onDestroy(() => { alive = false })

  const pairing = $derived(pairs.find((p) => p.nonce === selected))

  async function attempt(work: () => Promise<void>) {
    if (!ready || busy || !alive) return
    busy = true
    error = ''
    notice = ''
    try {
      await work()
    } catch (e) {
      if (alive) error = errorText(e)
    } finally {
      if (alive) busy = false
    }
  }

  /** The offers and kept archives; the daemon's `legacy/changed` reads them again after each change. */
  const reload = single(async () => {
    if (!ready || !alive) return
    const values = await Promise.all([call('legacy/pairs'), call('legacy/list')])
    if (alive) [pairs, archives] = values
  })

  const pair = () =>
    attempt(async () => {
      const result = await call('legacy/pair', { origin: 'https://dmsg.net' })
      if (!alive) return
      offer = result
      await bridge(() => window.dmsg.openLegacy(result.link))
      if (!alive) return
      selected = offer.nonce
      confirmed = false
    })

  const importFile = () =>
    attempt(async () => {
      if (!pairing || !confirmed || !principal.trim()) return
      const result = await bridge(() => window.dmsg.legacyImport(pairing.nonce, pairing.fingerprint, principal.trim()))
      if (!result || !alive) return
      report = result
      limit = 100
      notice = t('legacyKept')
    })

  const open = (id: string) =>
    attempt(async () => {
      limit = 100
      const result = await call('legacy/report', { id, limit })
      if (alive) report = result
    })

  const sync = () => attempt(async () => {
    await call('sync/run')
    await reload()
    if (alive && report) { const result = await call('legacy/report', { id: report.id, limit }); if (alive) report = result }
  })

  const more = () =>
    attempt(async () => {
      limit += 100
      const result = await call('legacy/report', { id: report.id, limit })
      if (alive) report = result
    })

  const frozen = () =>
    attempt(async () => {
      const result = await bridge(() => window.dmsg.legacyFrozen(report.id))
      if (alive && result) { const next = await call('legacy/report', { id: report.id, limit }); if (alive) report = next }
    })

  const saveFile = (file: any) =>
    attempt(async () => {
      await bridge(() => window.dmsg.legacyFile(report.id, file.source, file.name))
    })

  const cancel = (nonce: string) =>
    attempt(async () => {
      await call('legacy/cancel', { nonce })
      if (selected === nonce) selected = ''
    })

  const pretty = (value: unknown) => JSON.stringify(value, null, 2)

  onMount(() => {
    void attempt(reload)
    return onChange(['legacy/changed', 'sync/changed'], () => reload().catch(() => {}))
  })
</script>

{#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
{#if notice}<Notice tone="ok">{notice}</Notice>{/if}
{#if !ready}
  <Notice testid="legacy-not-ready">
    {t(guidance)}
    {#snippet action()}
      <button class={buttonClass('secondary', 'sm')} onclick={() => go('/settings/session')} data-testid="legacy-setup">{t('sessionAccounts')}</button>
    {/snippet}
  </Notice>
{/if}

<Section title={t('legacyTitle')} description={t('legacyHint')} icon="legacy" testid="legacy">
  <div class="mb-3 flex flex-wrap gap-2">
    <button class={buttonClass('primary')} disabled={busy || !ready} onclick={pair} data-testid="legacy-pair"><Icon name="external" />{t('legacyPair')}</button>
  </div>
  {#if ready && pairs.length}
    <h3 class="mb-1.5 text-[12px] font-medium text-faint">{t('legacyOffers')}</h3>
    <RadioGroup.Root bind:value={selected} disabled={busy} onValueChange={() => { confirmed = false; principal = '' }} aria-label={t('legacyOffers')}>
      <ul class="divide-y divide-line">
        {#each pairs as p (p.nonce)}
          <li class={cn('flex flex-wrap items-center gap-2 px-3.5 py-2.5', selected === p.nonce && 'bg-pale/60')}>
            <Radio value={p.nonce} testid="legacy-pick-{p.nonce}" class="min-w-0 flex-1">
              <span class="flex flex-wrap items-center gap-1.5 text-[12.5px]">
                <bdi dir="ltr" class="font-mono">{p.origin}</bdi>
                <span class="font-mono text-faint">{p.fingerprint.slice(0, 16)}</span>
                <span class="text-faint">· {t('expires')} {when(p.expires_at)}</span>
                {#if p.started}<Pill tone="warn">{t('legacyStarted')}</Pill>{/if}
              </span>
            </Radio>
            <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => window.dmsg.openLegacy(p.link)}>{t('open')}</button>
            <button class={buttonClass('ghost', 'xs')} disabled={busy} onclick={() => cancel(p.nonce)}>{t('cancel')}</button>
          </li>
        {/each}
      </ul>
    </RadioGroup.Root>
  {/if}
  {#if ready && pairing}
    <div class="mt-3 grid gap-3 rounded-xl border border-forest/30 bg-pale/40 p-4">
      <dl class="divide-y divide-line" data-testid="legacy-offer">
        <Fact label={t('legacyLink')}><bdi dir="ltr" class="font-mono text-[11.5px] break-all">{pairing.link}</bdi></Fact>
        <Fact label={t('device')}><span class="font-mono text-[12px] break-all">{pairing.target}</span></Fact>
        <Fact label={t('fingerprint')}><span class="font-mono text-[12px] font-semibold break-all text-forest">{pairing.fingerprint}</span></Fact>
      </dl>
      <TextField label={t('legacyPrincipal')} bind:value={principal} oninput={() => (confirmed = false)} autocomplete="off" mono disabled={busy} data-testid="legacy-principal" />
      <Checkbox bind:checked={confirmed} disabled={busy}>{t('legacyConfirm')}</Checkbox>
      <button class={buttonClass('primary', 'md', 'justify-self-start')} disabled={busy || !confirmed || !principal.trim()} onclick={importFile}>
        <Icon name="verified" />{t('legacyImport')}
      </button>
    </div>
  {/if}
</Section>

{#if ready}
<Section title={t('legacyArchives')} icon="archive" flush>
  <ul class="divide-y divide-line" data-testid="legacy-archives">
    {#each archives as a (a.id)}
      <li class="flex flex-wrap items-center gap-2 px-5 py-3">
        <Pill>{a.mode}</Pill>
        <Id id={a.principal} full class="max-w-[50%]" />
        <span class="min-w-0 flex-1 text-[12px] text-faint">{a.objects} {t('legacyObjects')} · {a.gaps} {t('legacyGaps')}</span>
        <Pill tone={a.synced ? 'ok' : 'neutral'}>{t(a.synced ? 'synced' : 'legacyWaitingSync')}</Pill>
        <a class={buttonClass('secondary', 'xs')} href={`#/channels?legacy=${encodeURIComponent(a.id)}`} data-testid="legacy-open-channels">{t('legacyHistory')}</a>
        <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => open(a.id)}>{t('legacyRead')}</button>
      </li>
    {:else}
      <li class="px-5 py-3 text-[13px] text-faint">{t('legacyNone')}</li>
    {/each}
  </ul>
</Section>

{#if report}
  <Section title={report.stage === 'partial' ? t('legacyPartial') : t('legacyVerified')} description={t('legacyClaims')} icon={report.stage === 'partial' ? 'warning' : 'verified'} testid="legacy-report">
    {#snippet actions()}
      <a class={buttonClass('secondary', 'sm')} href="#/settings/profile">{t('legacyUseProfile')}</a>
      <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={sync}>{t('sync')}</button>
    {/snippet}
    <Notice tone="ok">{t('legacyPartialHint')}</Notice>
    <div class="grid gap-4">
      <dl>
        <Fact label={t('legacyPrincipal')}><Id id={report.principal} full /></Fact>
        <Fact label={t('displayName')}>{report.profile.name || '-'}</Fact>
        {#if report.profile.username}
          <Fact label={t('legacyUsername')} testid="legacy-username">
            <span class="flex flex-wrap items-center gap-2">
              <b class="font-semibold">@{report.profile.username}</b>
              <a class={buttonClass('secondary', 'xs')} href={`#/plan?names=${encodeURIComponent(report.profile.username)}`} data-testid="legacy-claim-name">{t('claimOldName')}</a>
            </span>
          </Fact>
        {/if}
        <Fact label={t('bio')}>{report.profile.bio || '-'}</Fact>
      </dl>
      {#if report.gaps.length}
        <Notice tone="warn">
          <p class="font-medium">{t('legacyGapList')}</p>
          <ul class="mt-1 grid gap-0.5 font-mono text-[11.5px]">
            {#each report.gaps.slice(0, 30) as gap, i (i)}<li>{gap.code} · {gap.source}: {gap.detail}</li>{/each}
          </ul>
        </Notice>
      {/if}
      {#if report.files.length}
        <div>
          <h3 class="mb-1.5 text-[12px] font-medium text-faint">{t('legacyFiles')}</h3>
          <ul class="divide-y divide-line">
            {#each report.files as file (file.source)}
              <li class="flex items-center gap-2 px-3.5 py-2">
                <Icon name="file" class="shrink-0 text-faint" />
                <span class="min-w-0 flex-1 truncate text-[13px]">{file.name} <span class="font-mono text-[11px] text-faint">{file.source}</span></span>
                <button class={buttonClass('secondary', 'xs')} disabled={busy || !file.verified} onclick={() => saveFile(file)}>
                  {file.verified ? t('legacySave') : t('legacyUnverified')}
                </button>
              </li>
            {/each}
          </ul>
        </div>
      {/if}
      {#if report.resources.length || report.entitlements.length}
        <div class="grid gap-1.5">
          <h3 class="text-[12px] font-medium text-faint">{t('legacyResources')}</h3>
          <p class="text-[12px] text-muted">{t('legacyResourcesHint')}</p>
          {#each report.resources as r (r.source)}
            <p class="font-mono text-[11.5px] text-muted">{r.name} · {t('legacyResource', { paid: r.paid, gas: r.gas })}</p>
          {/each}
          {#each report.entitlements as e (e.source)}
            <Collapsible>
              {#snippet title()}<span class="font-mono text-[12px]">{e.source}</span>{/snippet}
              <pre class="overflow-x-auto rounded-lg bg-surface-2 p-2.5 font-mono text-[11px]">{pretty(e.value)}</pre>
            </Collapsible>
          {/each}
        </div>
      {/if}
      <div class="flex flex-wrap items-center gap-2 rounded-xl border border-line px-3.5 py-2.5">
        <Icon name={report.frozen.matched ? 'verified' : 'warning'} class={report.frozen.matched ? 'text-ok' : 'text-warn'} />
        <span class="min-w-0 flex-1 text-[12.5px]">
          <b class="font-medium">{t('legacyFrozen')}</b> · {report.frozen.matched ? t('legacyFrozenMatched') : `${report.frozen.total_changes} ${t('legacyFrozenChanges')}`}
        </span>
        <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={frozen}>{t('legacyFrozenImport')}</button>
      </div>
      <div>
        <h3 class="mb-1.5 text-[12px] font-medium text-faint">{t('legacyMessages')} · {report.total_messages}</h3>
        <ul class="divide-y divide-line" data-testid="legacy-messages">
          {#each report.messages as m (m.source)}
            <li class="px-3.5 py-2.5">
              <p class="font-mono text-[11px] text-faint">{m.author} · {m.time}{m.system ? ` · ${t('legacySystem')}` : ''}</p>
              <p class={cn('text-[13px] whitespace-pre-wrap [overflow-wrap:anywhere]', m.deleted && 'text-faint italic')}>
                {m.deleted ? t('legacyDeleted') : m.text}
              </p>
              {#if m.file}<p class="mt-0.5 inline-flex items-center gap-1 text-[12px] text-faint"><Icon name="attach" class="size-3.5" />{m.file.name}</p>{/if}
            </li>
          {/each}
        </ul>
        {#if report.total_messages > report.messages.length}
          <button class={buttonClass('ghost', 'sm', 'mt-2')} disabled={busy} onclick={more}>{t('more')}</button>
        {/if}
      </div>
    </div>
  </Section>
{/if}
{/if}
