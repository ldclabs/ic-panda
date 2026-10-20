<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import Fact from '../../components/page/Fact.svelte'
  import Field from '../../components/page/Field.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import Segmented from '../../components/Segmented.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { bridge, call, code, onChange, single, when } from '../../rpc'
  import { startLogin } from '../../shell/login.svelte'
  import { monthText, purposeLabels, reasonLabels } from '../../approval'

  /**
   * Signing from this device: whether it can sign now and what stops it;
   * a text statement, a statement about a file or a file's digest (the file
   * picked in the main process's dialog, or a digest given in hex); an
   * application action; and the month's allowance.
   */
  let { open }: { open: (id: string) => unknown } = $props()

  type Profile = 'text' | 'file_statement' | 'digest'

  let profile = $state<Profile>('text')
  let text = $state('')
  let subject = $state('')
  let contentType = $state('')
  let location = $state('')
  let sha256 = $state('')
  let readiness = $state<any>(null)
  let usage = $state<any>(null)
  let usageError = $state('')
  let usageBusy = $state(false)
  let busy = $state(false)
  let error = $state('')

  const MAX_TEXT = 4096
  const bytes = $derived(new TextEncoder().encode(text).length)
  const hex = $derived(/^[0-9a-fA-F]{64}$/.test(sha256.trim()))
  const needsText = $derived(profile !== 'digest')
  const textOk = $derived(!needsText || (text.trim().length > 0 && bytes <= MAX_TEXT))

  const reloadReadiness = single(async () => {
    readiness = await call('attest/readiness').catch(() => null)
  })

  /** The month's allowance from the plan; the home also archives finished months here. */
  const loadUsage = single(async () => {
    usageBusy = true
    usageError = ''
    try {
      usage = await call('attest/usage')
    } catch (e) {
      usageError = code(e)
    } finally {
      usageBusy = false
    }
  })

  async function attempt(work: () => Promise<unknown>) {
    busy = true
    error = ''
    try {
      await work()
    } catch (e) {
      error = reasonLabels[code(e)] ? t(reasonLabels[code(e)]) : `${t('error')}: ${code(e)}`
    } finally {
      busy = false
    }
  }

  /** The statement's fields as the daemon reads them; empty ones are left out. */
  const fields = () => ({
    profile,
    ...(needsText ? { text } : {}),
    subject: subject.trim() || null,
    ...(profile !== 'text' ? { content_type: contentType.trim() || null, location: location.trim() || null } : {})
  })

  const opened = async (request: any) => {
    if (!request) return
    text = ''
    sha256 = ''
    await open(request.id)
  }

  const signText = () => attempt(async () => opened(await call('attest/request', { statement: fields() })))

  /** A local file goes to the daemon only from the main process's dialog. */
  const signFile = () => attempt(async () => opened(await bridge(() => window.dmsg.attestFile(fields()))))

  const signDigest = () => attempt(async () => opened(await call('attest/request', { statement: { ...fields(), sha256: sha256.trim().toLowerCase() } })))

  /** An action its application prepared, approved in the approval window like a page's `signAction`. */
  const signAction = () => attempt(async () => opened(await bridge(() => window.dmsg.attestAction())))

  const formal = (enable: boolean) => attempt(async () => opened(await call('device/formal_approve', { enable })))

  onMount(() => {
    void reloadReadiness()
    void loadUsage()
    return onChange(['approval/changed', 'session/changed'], reloadReadiness)
  })

  const purposes = (names?: string[]) => (names ?? []).map((p) => (purposeLabels[p] ? t(purposeLabels[p]) : p)).join(', ')
  const share = $derived(usage?.allowed ? Math.min(1, usage.charged / usage.allowed) : 0)
  const profiles = $derived([
    { value: 'text', label: t('profileText') },
    { value: 'file_statement', label: t('profileFileStatement') },
    { value: 'digest', label: t('profileDigest') }
  ])
</script>

<Section title={t('readiness')} description={t('readinessHint')} icon="verified" testid="sign-readiness">
  {#snippet actions()}
    {#if readiness}<Pill tone={readiness.ready ? 'ok' : 'warn'} testid="readiness-state">{readiness.ready ? t('readyToSign') : t('notReadyToSign')}</Pill>{/if}
  {/snippet}
  {#if readiness}
    <dl class="divide-y divide-line">
      <Fact label={t('login')}>
        {#if readiness.login}<Pill tone="ok">{t('loggedIn')}</Pill>{:else}<Pill tone="warn">{t('reasonAuthRequired')}</Pill>
          <button class={buttonClass('secondary', 'xs')} onclick={() => startLogin()}>{t('login')}</button>{/if}
      </Fact>
      <Fact label={t('formal')} testid="readiness-formal">
        {#if readiness.formal_approve === true}
          <Pill tone="ok">{t('formalOn')}</Pill>
          <button class={buttonClass('ghost', 'xs')} disabled={busy} onclick={() => formal(false)} data-testid="formal-off">{t('disable')}</button>
        {:else if readiness.formal_approve === false}
          <Pill tone="warn">{t('formalOff')}</Pill>
          <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => formal(true)} data-testid="formal-on">{t('enable')}</button>
        {:else}
          <span class="text-faint">-</span>
        {/if}
      </Fact>
      {#if readiness.policy}
        <Fact label={t('signingPolicy')}>
          <Pill tone={readiness.policy.frozen ? 'warn' : 'ok'}>{readiness.policy.frozen ? t('frozen') : t('policyOpen')}</Pill>
          <span class="text-[12.5px] text-muted">{purposes(readiness.policy.allowed_purposes)} · {t('perDay', { n: readiness.policy.daily_executions })}</span>
          <a class="text-[12.5px] text-forest hover:underline" href="#/settings/security">{t('openSecurity')}</a>
        </Fact>
      {/if}
    </dl>
  {:else}
    <p class="shimmer text-[13px]">{t('loading')}</p>
  {/if}
</Section>

<Section title={t('newSignature')} description={t('statementHint')} icon="signature">
  <div class="grid gap-3">
    <Segmented bind:value={() => profile, (next) => (profile = next as Profile)} label={t('newSignature')} options={profiles} testid="statement-profile" />
    {#if needsText}
      <Field label={t('statement')}>
        <textarea placeholder={t('statement')} bind:value={text} rows="4" data-testid="statement-text"></textarea>
        <span class={cn('justify-self-end text-[11.5px] tabular-nums', bytes > MAX_TEXT ? 'text-danger' : 'text-faint')} data-testid="statement-bytes">{bytes} / {MAX_TEXT}</span>
      </Field>
    {/if}
    <div class="grid gap-3 sm:grid-cols-2">
      <Field label={t('subject')}><input bind:value={subject} placeholder={t('subjectHint')} data-testid="statement-subject" /></Field>
      {#if profile !== 'text'}
        <Field label={t('contentType')}><input bind:value={contentType} placeholder="application/pdf" class="font-mono text-[12.5px]" /></Field>
        <Field label={t('location')}><input bind:value={location} placeholder="https://" class="font-mono text-[12.5px]" /></Field>
      {/if}
      {#if profile === 'digest'}
        <Field label="SHA-256"><input bind:value={sha256} placeholder={t('digestHint')} class="font-mono text-[12.5px]" data-testid="statement-sha256" /></Field>
      {/if}
    </div>
    {#if profile !== 'text'}<p class="text-[12px] text-faint">{t('signFileHint')}</p>{/if}
    {#if error}<Notice tone="danger" testid="sign-error">{error}</Notice>{/if}
    <div class="flex flex-wrap justify-end gap-2">
      {#if profile === 'text'}
        <button class={buttonClass('primary')} disabled={busy || !textOk} onclick={signText} data-testid="statement-submit">{t('submit')}</button>
      {:else}
        {#if profile === 'digest'}
          <button class={buttonClass('secondary')} disabled={busy || !hex} onclick={signDigest} data-testid="statement-digest">{t('signDigest')}</button>
        {/if}
        <button class={buttonClass('primary')} disabled={busy || !textOk} onclick={signFile} data-testid="statement-file"><Icon name="file" />{t('chooseFileToSign')}</button>
      {/if}
    </div>
  </div>
</Section>

<Section title={t('appAction')} description={t('appActionHint')} icon="api">
  <button class={buttonClass('secondary')} disabled={busy} data-testid="sign-action" onclick={signAction}><Icon name="upload" />{t('appActionPick')}</button>
</Section>

<Section title={t('usage')} description={t('usageHint')} icon="plan">
  {#snippet actions()}
    <button class={buttonClass('ghost', 'sm')} disabled={usageBusy} onclick={loadUsage}><Icon name="sync-now" />{t('refresh')}</button>
  {/snippet}
  {#if usage}
    <div class="grid gap-2" data-testid="usage">
      <div class="flex items-baseline justify-between gap-2">
        <span class="text-[22px] font-semibold tabular-nums">{usage.charged}<span class="text-[14px] font-normal text-faint"> / {usage.allowed}</span></span>
        <span class="text-[12px] text-muted">{usage.remaining} {t('remaining')} · {t('validUntil')} {when(usage.valid_until_ms)}</span>
      </div>
      <div class="h-2 overflow-hidden rounded-full bg-wash">
        <div class={cn('h-full rounded-full transition-[width] duration-500', share > 0.9 ? 'bg-warn' : 'bg-forest')} style:width="{share * 100}%"></div>
      </div>
      {#if !usage.allowed}<p class="text-[12.5px] text-warn">{t('noAllowance')} <a class="text-forest hover:underline" href="#/plan">{t('openPlan')}</a></p>{/if}
      {#if usage.history.length}
        <ul class="mt-2 grid gap-1 text-[12px]">
          {#each usage.history as month (month.month)}
            <li class="flex justify-between text-muted"><span>{monthText(month.month, locale())}</span><span class="tabular-nums">{month.charged} / {month.allowed}</span></li>
          {/each}
        </ul>
      {/if}
    </div>
  {:else if usageError}
    <Notice tone={usageError === 'AUTH_REQUIRED' ? 'info' : 'danger'} testid="usage-error">
      {reasonLabels[usageError] ? t(reasonLabels[usageError]) : `${t('error')}: ${usageError}`}
      {#snippet action()}
        {#if usageError === 'AUTH_REQUIRED'}
          <button class={buttonClass('primary', 'sm')} onclick={() => startLogin()}>{t('login')}</button>
        {:else}
          <button class={buttonClass('secondary', 'sm')} onclick={loadUsage}>{t('retry')}</button>
        {/if}
      {/snippet}
    </Notice>
  {:else}
    <p class="shimmer text-[13px]">{t('loading')}</p>
  {/if}
</Section>
