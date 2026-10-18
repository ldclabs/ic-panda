<script lang="ts">
  import Checkbox from '../../components/Checkbox.svelte'
  import { copyText } from '../../components/clipboard'
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import Fact from '../../components/page/Fact.svelte'
  import Field from '../../components/page/Field.svelte'
  import Id from '../../components/page/Id.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import Select from '../../components/Select.svelte'
  import Suggest from '../../components/Suggest.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { when } from '../../rpc'
  import { bySubject, covers, list, profileHint, statusTone } from './agents'

  /**
   * The account's principal: its controllers, each limited to what its
   * delegation names, the credentials granted to agents and what this
   * device signed for them. Every change is an approval request; a grant
   * or revocation goes to the newest current controller whose ceiling
   * covers it unless one is chosen.
   */
  let {
    principal,
    principalError,
    credentials,
    delegations,
    hosted,
    unlocked,
    busy,
    request,
    run,
    delegate = $bindable('')
  }: {
    principal: any
    principalError: string
    credentials: any[]
    delegations: any[]
    hosted: any
    unlocked: boolean
    busy: boolean
    request: (method: string, params?: Record<string, unknown>) => Promise<void>
    run: (method: string, params?: Record<string, unknown>) => Promise<void>
    delegate?: string
  } = $props()

  let kind = $state('person')
  let controllerName = $state('')
  let ceilingScopes = $state('')
  let ceilingAudiences = $state('')
  let scopes = $state('')
  let audiences = $state('')
  let days = $state(30)
  let relationship = $state('')
  let generation = $state('')

  /** A rotation in progress: the controller to retire once the new one is registered. */
  let rotation = $state<{ from: number; after: number } | null>(null)
  let renaming = $state<{ generation: number; name: string } | null>(null)
  let compromising = $state<{ generation: number; at: string } | null>(null)
  let rotating = $state<{ from: any; name: string; scopes: string; audiences: string } | null>(null)
  let renewing = $state<{ credential: any; scopes: Record<string, boolean>; audiences: Record<string, boolean>; days: number; relationship: string } | null>(null)
  let revoking = $state<{ credential: any; reason: string } | null>(null)

  const current = $derived((principal?.controllers ?? []).filter((c: any) => !c.retired_at))
  const labelOf = (did: string) => hosted.agents.find((a: any) => a.agent_id === did)?.label ?? did
  const generationOf = (did: string) => principal?.controllers.find((c: any) => c.agent_id === did)?.generation
  const ceilingText = (c: any) => (c.delegation === '*' ? t('everything') : `${c.delegation.scopes.join(', ')} → ${c.delegation.audiences.join(', ')}`)
  /** The registered successor of a rotation, once the principal shows it. */
  const successor = $derived(rotation ? current.find((c: any) => c.generation >= rotation!.after) : null)
  const rotatingFrom = $derived(rotation ? principal?.controllers.find((c: any) => c.generation === rotation!.from) : null)

  /** A time as a `datetime-local` value, in this computer's zone. */
  const localTime = (ms: number) => {
    const d = new Date(ms - new Date(ms).getTimezoneOffset() * 60_000)
    return d.toISOString().slice(0, 16)
  }

  const ceiling = (s: string, a: string) => (s.trim() || a.trim() ? { scopes: list(s), audiences: list(a) } : {})

  const register = () => request('agent/register', { name: controllerName.trim() || null, ...ceiling(ceilingScopes, ceilingAudiences) })

  const grant = () =>
    request('agent/grant', {
      subject: delegate.trim(),
      scopes: list(scopes),
      audiences: list(audiences),
      days,
      relationship: relationship.trim() || null,
      generation: generation ? Number(generation) : null
    })

  async function rotate() {
    if (!rotating) return
    const from = rotating.from.generation
    await request('agent/register', { name: rotating.name.trim() || null, ...ceiling(rotating.scopes, rotating.audiences) })
    rotation = { from, after: principal.next_generation }
    rotating = null
  }

  async function retireRotated() {
    if (!rotation) return
    await request('agent/retire', { generation: rotation.from })
    rotation = null
  }

  function renew(credential: any) {
    const all = (values: string[]) => Object.fromEntries(values.map((v) => [v, true]))
    renewing = { credential, scopes: all(credential.scopes), audiences: all(credential.audiences), days: 30, relationship: credential.relationship ?? '' }
  }

  async function submitRenewal() {
    if (!renewing) return
    const { credential } = renewing
    const picked = (values: Record<string, boolean>) => Object.keys(values).filter((v) => values[v])
    await request('agent/grant', {
      subject: credential.subject,
      scopes: picked(renewing.scopes),
      audiences: picked(renewing.audiences),
      days: renewing.days,
      relationship: renewing.relationship.trim() || null,
      replaces: credential.id
    })
    renewing = null
  }

  const statusLabel = (status: string) =>
    ({ active: t('credentialActive'), suspended: t('credentialSuspended'), expired: t('credentialExpired'), revoked: t('credentialRevoked') })[status] ?? status
  const stageLabel = (stage: string) =>
    ({ prepared: t('stagePrepared'), signed: t('stageSigned'), accepted: t('stageAccepted'), failed: t('stageFailed') })[stage] ?? stage

  const copyHint = (credential: any) => copyText(JSON.stringify(profileHint(principal, credential), null, 2))
</script>

<Section title={t('principal')} description={t('principalHint')} icon="plan">
  {#if principalError}<Notice tone="danger" class="mb-3" testid="principal-error">{principalError}</Notice>{/if}
  {#if principal}
    <dl class="divide-y divide-line">
      <Fact label="ID"><Id id={principal.principal_id} full testid="principal-id" /></Fact>
      <Fact label={t('type')}>{String(principal.type).toLowerCase()}</Fact>
      <Fact label={t('status')} testid="principal-published">
        <span class="flex flex-wrap items-center gap-1.5">
          {#if principal.published}
            <Pill tone="ok"><Icon name="check" />{t('published')}</Pill>
          {:else}
            <Pill tone="warn">{t('unpublished')}</Pill>
            {#if unlocked}<button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => run('agent/publish')}>{t('publish')}</button>{/if}
          {/if}
          <span class="text-[11.5px] text-faint tabular-nums">{t('principalVersion', { version: principal.version, published: principal.published_version })}</span>
          {#if unlocked && principal.published}
            <button class={buttonClass('ghost', 'xs')} disabled={busy} onclick={() => run('agent/refresh')} data-testid="principal-refresh"><Icon name="sync-now" />{t('refreshService')}</button>
          {/if}
        </span>
      </Fact>
    </dl>
  {:else if unlocked && !principalError}
    <div class="flex flex-wrap items-end gap-2">
      <Field label={t('type')}>
        <Select bind:value={kind} options={['person', 'organization', 'team', 'project', 'other'].map((k) => ({ value: k, label: k }))} />
      </Field>
      <button class={buttonClass('primary')} disabled={busy} data-testid="enable-principal" onclick={() => request('agent/enable', { type: kind })}>
        {t('enablePrincipal')}
      </button>
    </div>
  {:else if !unlocked}
    <p class="text-[13px] text-muted">{t('locked')}</p>
  {/if}
</Section>

{#if principal}
  <Section title={t('controllers')} description={t('controllersHint')} icon="key" flush testid="controllers">
    {#if rotation && rotatingFrom && !rotatingFrom.retired_at}
      <div class="border-t border-line px-5 py-3">
        <Notice tone={successor ? 'warn' : 'info'} testid="rotation">
          <p>{successor ? t('rotationRetire', { from: rotation.from, to: successor.generation }) : t('rotationPending', { from: rotation.from })}</p>
          {#snippet action()}
            {#if successor}<button class={buttonClass('primary', 'xs')} disabled={busy || !unlocked} onclick={retireRotated}>{t('retire')} #{rotation?.from}</button>{/if}
            <button class={buttonClass('ghost', 'xs')} onclick={() => (rotation = null)}>{t('dismiss')}</button>
          {/snippet}
        </Notice>
      </div>
    {/if}
    <ul class="divide-y divide-line border-t border-line">
      {#each principal.controllers as c (c.generation)}
        <li class="flex items-start gap-3 px-5 py-3">
          <span class="grid size-8 shrink-0 place-items-center rounded-lg bg-surface-2 font-mono text-[12px] text-muted">#{c.generation}</span>
          <div class="min-w-0 flex-1">
            <div class="flex flex-wrap items-center gap-1.5">
              <b class="text-[13px] font-medium">{c.name ?? t('controller')}</b>
              <Id id={c.agent_id} />
              {#if c.invalid_from}<Pill tone="danger">{t('compromisedFrom', { at: when(c.invalid_from) })}</Pill>{/if}
              {#if c.retired_at}<Pill>{t('retired')}</Pill>{/if}
            </div>
            <p class="mt-0.5 text-[12px] text-muted">{ceilingText(c)}</p>
            <p class="mt-0.5 text-[11.5px] text-faint">{t('validFrom')} {when(c.valid_from)}{c.retired_at ? ` · ${t('retired')} ${when(c.retired_at)}` : ''}</p>
          </div>
          {#if unlocked}
            <div class="flex shrink-0 flex-wrap justify-end gap-1.5">
              {#if !c.retired_at}
                <button class={buttonClass('ghost', 'sm')} disabled={busy} onclick={() => (renaming = { generation: c.generation, name: c.name ?? '' })}>{t('rename')}</button>
                <button
                  class={buttonClass('secondary', 'sm')}
                  disabled={busy}
                  data-testid="controller-rotate"
                  onclick={() =>
                    (rotating = {
                      from: c,
                      name: c.name ?? '',
                      scopes: c.delegation === '*' ? '' : c.delegation.scopes.join(', '),
                      audiences: c.delegation === '*' ? '' : c.delegation.audiences.join(', ')
                    })}>{t('rotateKey')}</button
                >
                <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => request('agent/retire', { generation: c.generation })}>{t('retire')}</button>
              {/if}
              <button
                class={buttonClass('ghost', 'sm', 'text-danger')}
                disabled={busy}
                data-testid="controller-compromise"
                onclick={() => (compromising = { generation: c.generation, at: localTime(Math.min(Date.now(), c.invalid_from ?? Date.now(), c.retired_at ?? Date.now())) })}
                >{t('markCompromised')}</button
              >
            </div>
          {/if}
        </li>
      {/each}
    </ul>
    {#if unlocked}
      <div class="grid gap-2 border-t border-line bg-surface-2/60 px-5 py-3.5 sm:grid-cols-[160px_1fr_1fr_auto] sm:items-end">
        <Field label={t('name')}><input bind:value={controllerName} /></Field>
        <Field label={t('scopes')}><input bind:value={ceilingScopes} placeholder="message.draft, …" /></Field>
        <Field label={t('audiences')}><input bind:value={ceilingAudiences} placeholder="https://…" /></Field>
        <button class={buttonClass('primary')} disabled={busy} onclick={register}>{t('registerController')}</button>
      </div>
    {/if}
  </Section>

  <Section title={t('credentials')} description={t('credentialsHint')} icon="epoch" flush>
    <div class="border-t border-line" data-testid="credentials">
      {#each bySubject(credentials) as [subject, group] (subject)}
        <div class="border-b border-line last:border-b-0">
          <div class="flex flex-wrap items-center gap-1.5 bg-surface-2/50 px-5 py-2">
            <Icon name="agents" class="text-faint" />
            <b class="text-[12.5px] font-medium">{labelOf(subject)}</b>
            {#if labelOf(subject) !== subject}<Id id={subject} />{/if}
            <span class="flex-1"></span>
            {#if unlocked && current.length}
              <button class={buttonClass('ghost', 'xs')} onclick={() => (delegate = labelOf(subject))}><Icon name="add" />{t('grantAnother')}</button>
            {/if}
          </div>
          <ul class="divide-y divide-line">
            {#each group as c (c.id)}
              <li class="flex items-start gap-3 px-5 py-3" data-testid="credential">
                <div class="min-w-0 flex-1">
                  <div class="flex flex-wrap items-center gap-1.5">
                    <Pill tone={statusTone(c.status)}>{statusLabel(c.status)}</Pill>
                    <Id id={c.id} />
                    {#if generationOf(c.controller)}<span class="font-mono text-[11.5px] text-faint">#{generationOf(c.controller)}</span>{/if}
                  </div>
                  <div class="mt-1 flex flex-wrap gap-1">
                    {#each c.scopes as scope (scope)}<Pill tone="forest">{scope}</Pill>{/each}
                    <span class="text-[12px] text-faint">→</span>
                    {#each c.audiences as audience (audience)}<span class="font-mono text-[11.5px] text-muted">{audience}</span>{/each}
                  </div>
                  {#if c.relationship}<p class="mt-1 text-[12px] text-muted">{c.relationship}</p>{/if}
                  <p class="mt-1 text-[11.5px] text-faint">
                    {#if c.expires_at}{t('expires')} {when(c.expires_at)}{/if}{#if c.status === 'suspended'} · {t('suspendedHint')}{/if}
                  </p>
                </div>
                <div class="flex shrink-0 flex-wrap justify-end gap-1.5">
                  <button class={buttonClass('ghost', 'sm', 'size-7 px-0')} aria-label={t('copyHint')} title={t('copyHint')} onclick={() => copyHint(c)} data-testid="credential-hint"
                    ><Icon name="copy" /></button
                  >
                  {#if c.status !== 'revoked' && unlocked}
                    <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => renew(c)} data-testid="credential-renew">{t('renew')}</button>
                    <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => (revoking = { credential: c, reason: '' })} data-testid="credential-revoke">{t('revoke')}</button>
                  {/if}
                </div>
              </li>
            {/each}
          </ul>
        </div>
      {:else}
        <p class="px-5 py-3 text-[13px] text-faint">{t('noCredentials')}</p>
      {/each}
    </div>
    {#if unlocked && current.length}
      <div class="grid gap-2 border-t border-line bg-surface-2/60 px-5 py-3.5" data-testid="grant-form">
        <div class="grid gap-2 sm:grid-cols-[1fr_1fr_1fr_84px] sm:items-end">
          <Field label={t('grantee')}>
            <Suggest bind:value={delegate} testid="grant-subject" suggestions={hosted.agents.map((a: any) => ({ value: a.label, hint: a.agent_id }))} />
          </Field>
          <Field label={t('scopes')}><input bind:value={scopes} placeholder="message.draft" data-testid="grant-scopes" /></Field>
          <Field label={t('audiences')}><input bind:value={audiences} placeholder="https://…" data-testid="grant-audiences" /></Field>
          <Field label={t('days')}><input type="number" min="1" max="366" bind:value={days} class="tabular-nums" /></Field>
        </div>
        <div class="grid gap-2 sm:grid-cols-[1fr_220px_auto] sm:items-end">
          <Field label={t('relationship')}><input bind:value={relationship} placeholder={t('relationshipHint')} data-testid="grant-relationship" /></Field>
          <Field label={t('controller')}>
            <Select
              bind:value={generation}
              testid="grant-controller"
              class="w-full"
              options={[
                { value: '', label: t('controllerAuto') },
                ...current
                  .filter((c: any) => !scopes.trim() || !audiences.trim() || covers(c, list(scopes), list(audiences)))
                  .map((c: any) => ({ value: String(c.generation), label: `#${c.generation} · ${c.name ?? ceilingText(c)}` }))
              ]}
            />
          </Field>
          <button class={buttonClass('primary')} data-testid="grant-submit" disabled={busy || !delegate.trim() || !scopes.trim() || !audiences.trim()} onclick={grant}>
            {t('grant')}
          </button>
        </div>
      </div>
    {/if}
  </Section>

  {#if delegations.length}
    <Section title={t('delegationJournal')} description={t('delegationJournalHint')} icon="history" flush testid="delegation-journal">
      <ul class="divide-y divide-line border-t border-line">
        {#each delegations.slice(0, 20) as job (job.id)}
          <li class="flex flex-wrap items-center gap-2 px-5 py-2.5 text-[12.5px]">
            <Pill tone={job.stage === 'accepted' ? 'ok' : job.stage === 'failed' ? 'danger' : 'warn'}>{stageLabel(job.stage)}</Pill>
            <span class="font-medium">{job.kind.type === 'revoke' ? t('delegationRevoke') : t('delegationGrant')}</span>
            {#if job.kind.subject}<span class="text-muted">{labelOf(job.kind.subject)}</span>{/if}
            <span class="font-mono text-[11.5px] text-faint">#{job.generation}</span>
            {#if job.error}<span class={cn('font-mono text-[11.5px]', job.stage === 'failed' ? 'text-danger' : 'text-warn')}>{job.error}</span>{/if}
            <span class="ms-auto text-[11.5px] text-faint">{when(job.created_at)}</span>
          </li>
        {/each}
      </ul>
    </Section>
  {/if}
{/if}

<Dialog bind:open={() => !!renaming, (open) => !open && (renaming = null)} title={t('renameController')} icon="edit">
  <Field label={t('name')}><input bind:value={() => renaming?.name ?? '', (v) => renaming && (renaming.name = v)} maxlength="64" /></Field>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (renaming = null)}>{t('cancel')}</button>
    <button
      class={buttonClass('primary')}
      disabled={busy}
      onclick={async () => {
        if (!renaming) return
        await request('agent/rename', { generation: renaming.generation, name: renaming.name.trim() || null })
        renaming = null
      }}>{t('rename')}</button
    >
  {/snippet}
</Dialog>

<Dialog
  bind:open={() => !!compromising, (open) => !open && (compromising = null)}
  title={t('markCompromisedTitle', { generation: compromising?.generation ?? '' })}
  description={t('markCompromisedHint')}
  icon="warning"
  tone="danger"
  testid="compromise-dialog"
>
  <Field label={t('untrustedFrom')}>
    <input type="datetime-local" bind:value={() => compromising?.at ?? '', (v) => compromising && (compromising.at = v)} data-testid="compromise-at" />
  </Field>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (compromising = null)}>{t('cancel')}</button>
    <button
      class={buttonClass('danger')}
      disabled={busy || !compromising?.at}
      data-testid="compromise-confirm"
      onclick={async () => {
        if (!compromising) return
        await request('agent/compromise', { generation: compromising.generation, invalid_from: new Date(compromising.at).getTime() })
        compromising = null
      }}>{t('markCompromised')}</button
    >
  {/snippet}
</Dialog>

<Dialog bind:open={() => !!rotating, (open) => !open && (rotating = null)} title={t('rotateTitle', { generation: rotating?.from.generation ?? '' })} description={t('rotateHint')} icon="key" wide testid="rotate-dialog">
  {#if rotating}
    <div class="grid gap-3">
      <Field label={t('name')}><input bind:value={rotating.name} /></Field>
      <div class="grid gap-3 sm:grid-cols-2">
        <Field label={t('scopes')} hint={t('ceilingEmptyHint')}><input bind:value={rotating.scopes} /></Field>
        <Field label={t('audiences')}><input bind:value={rotating.audiences} /></Field>
      </div>
    </div>
  {/if}
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (rotating = null)}>{t('cancel')}</button>
    <button class={buttonClass('primary')} disabled={busy} onclick={rotate} data-testid="rotate-confirm">{t('registerController')}</button>
  {/snippet}
</Dialog>

<Dialog bind:open={() => !!renewing, (open) => !open && (renewing = null)} title={t('renewTitle')} description={t('renewHint')} icon="epoch" wide testid="renew-dialog">
  {#if renewing}
    <div class="grid gap-3">
      <div class="grid gap-1.5">
        <h3 class="text-[12px] font-medium text-faint">{t('scopeLabel')}</h3>
        <div class="flex flex-wrap gap-3">
          {#each Object.keys(renewing.scopes) as scope (scope)}
            <Checkbox bind:checked={() => renewing?.scopes[scope] ?? false, (on) => renewing && (renewing.scopes[scope] = on)}>{scope}</Checkbox>
          {/each}
        </div>
      </div>
      <div class="grid gap-1.5">
        <h3 class="text-[12px] font-medium text-faint">{t('audienceLabel')}</h3>
        <div class="flex flex-wrap gap-3">
          {#each Object.keys(renewing.audiences) as audience (audience)}
            <Checkbox bind:checked={() => renewing?.audiences[audience] ?? false, (on) => renewing && (renewing.audiences[audience] = on)}>{audience}</Checkbox>
          {/each}
        </div>
      </div>
      <div class="grid gap-3 sm:grid-cols-[1fr_96px]">
        <Field label={t('relationship')}><input bind:value={renewing.relationship} /></Field>
        <Field label={t('days')}><input type="number" min="1" max="366" bind:value={renewing.days} class="tabular-nums" data-testid="renew-days" /></Field>
      </div>
    </div>
  {/if}
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (renewing = null)}>{t('cancel')}</button>
    <button
      class={buttonClass('primary')}
      disabled={busy || !renewing || !Object.values(renewing.scopes).some(Boolean) || !Object.values(renewing.audiences).some(Boolean)}
      onclick={submitRenewal}
      data-testid="renew-confirm">{t('renew')}</button
    >
  {/snippet}
</Dialog>

<Dialog bind:open={() => !!revoking, (open) => !open && (revoking = null)} title={t('delegationRevoke')} description={t('delegationWarn')} icon="warning" tone="danger" testid="revoke-dialog">
  <Field label={t('revokeReason')}><input bind:value={() => revoking?.reason ?? '', (v) => revoking && (revoking.reason = v)} data-testid="revoke-reason" /></Field>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (revoking = null)}>{t('cancel')}</button>
    <button
      class={buttonClass('danger')}
      disabled={busy}
      data-testid="revoke-confirm"
      onclick={async () => {
        if (!revoking) return
        await request('agent/revoke', { id: revoking.credential.id, reason: revoking.reason.trim() || null })
        revoking = null
      }}>{t('revoke')}</button
    >
  {/snippet}
</Dialog>
