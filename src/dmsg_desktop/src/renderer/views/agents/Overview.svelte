<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Fact from '../../components/page/Fact.svelte'
  import Field from '../../components/page/Field.svelte'
  import Id from '../../components/page/Id.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import Select from '../../components/Select.svelte'
  import Suggest from '../../components/Suggest.svelte'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { when } from '../../rpc'

  /**
   * The account's principal: its controllers, each limited to what its
   * delegation names, and the credentials granted to agents. Every change
   * is an approval request.
   */
  let {
    principal,
    credentials,
    hosted,
    unlocked,
    busy,
    request,
    run
  }: {
    principal: any
    credentials: any[]
    hosted: any
    unlocked: boolean
    busy: boolean
    request: (method: string, params?: Record<string, unknown>) => Promise<void>
    run: (method: string, params?: Record<string, unknown>) => Promise<void>
  } = $props()

  let kind = $state('person')
  let controllerName = $state('')
  let ceilingScopes = $state('')
  let ceilingAudiences = $state('')
  let subject = $state('')
  let scopes = $state('')
  let audiences = $state('')
  let days = $state(30)

  const list = (text: string) =>
    text
      .split(',')
      .map((v) => v.trim())
      .filter(Boolean)

  const current = $derived((principal?.controllers ?? []).filter((c: any) => !c.retired_at))
  const labelOf = (did: string) => hosted.agents.find((a: any) => a.agent_id === did)?.label ?? did

  const register = () =>
    request('agent/register', {
      name: controllerName.trim() || null,
      ...(ceilingScopes.trim() || ceilingAudiences.trim() ? { scopes: list(ceilingScopes), audiences: list(ceilingAudiences) } : {})
    })

  const grant = () => request('agent/grant', { subject: subject.trim(), scopes: list(scopes), audiences: list(audiences), days })
</script>

<Section title={t('principal')} description={t('principalHint')} icon="plan">
  {#if principal}
    <dl class="divide-y divide-line">
      <Fact label="ID"><Id id={principal.principal_id} full testid="principal-id" /></Fact>
      <Fact label={t('status')} testid="principal-published">
        {#if principal.published}
          <Pill tone="ok"><Icon name="check" />{t('published')}</Pill>
        {:else}
          <Pill tone="warn">{t('unpublished')}</Pill>
          {#if unlocked}<button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => run('agent/publish')}>{t('publish')}</button>{/if}
        {/if}
      </Fact>
    </dl>
  {:else if unlocked}
    <div class="flex flex-wrap items-end gap-2">
      <Field label={t('type')}>
        <Select bind:value={kind} options={['person', 'organization', 'team', 'project', 'other'].map((k) => ({ value: k, label: k }))} />
      </Field>
      <button class={buttonClass('primary')} disabled={busy} data-testid="enable-principal" onclick={() => request('agent/enable', { type: kind })}>
        {t('enablePrincipal')}
      </button>
    </div>
  {:else}
    <p class="text-[13px] text-muted">{t('locked')}</p>
  {/if}
</Section>

{#if principal}
  <Section title={t('controllers')} icon="key" flush testid="controllers">
    <ul class="divide-y divide-line border-t border-line">
      {#each principal.controllers as c (c.generation)}
        <li class="flex items-start gap-3 px-5 py-3">
          <span class="grid size-8 shrink-0 place-items-center rounded-lg bg-surface-2 font-mono text-[12px] text-muted">#{c.generation}</span>
          <div class="min-w-0 flex-1">
            <div class="flex flex-wrap items-center gap-1.5">
              <b class="text-[13px] font-medium">{c.name ?? t('controller')}</b>
              <Id id={c.agent_id} />
              {#if c.retired_at}<Pill>{t('retired')}</Pill>{/if}
            </div>
            <p class="mt-0.5 text-[12px] text-muted">
              {c.delegation === '*' ? t('everything') : `${c.delegation.scopes.join(', ')} → ${c.delegation.audiences.join(', ')}`}
            </p>
          </div>
          {#if !c.retired_at && unlocked}
            <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => request('agent/retire', { generation: c.generation })}>{t('retire')}</button>
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
    <ul class="divide-y divide-line border-t border-line" data-testid="credentials">
      {#each credentials as c (c.id)}
        <li class="flex items-start gap-3 px-5 py-3">
          <div class="min-w-0 flex-1">
            <div class="flex flex-wrap items-center gap-1.5">
              <b class="text-[13px] font-medium">{labelOf(c.subject)}</b>
              <Pill tone={c.status === 'active' ? 'ok' : 'neutral'}>{c.status}</Pill>
            </div>
            <div class="mt-1 flex flex-wrap gap-1">
              {#each c.scopes as scope (scope)}<Pill tone="forest">{scope}</Pill>{/each}
              <span class="text-[12px] text-faint">→</span>
              {#each c.audiences as audience (audience)}<span class="font-mono text-[11.5px] text-muted">{audience}</span>{/each}
            </div>
            {#if c.expires_at}<p class="mt-1 text-[11.5px] text-faint">{t('expires')} {when(c.expires_at)}</p>{/if}
          </div>
          {#if c.status === 'active' && unlocked}
            <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => request('agent/revoke', { id: c.id })}>{t('revoke')}</button>
          {/if}
        </li>
      {:else}
        <li class="px-5 py-3 text-[13px] text-faint">{t('noCredentials')}</li>
      {/each}
    </ul>
    {#if unlocked && current.length}
      <div class="grid gap-2 border-t border-line bg-surface-2/60 px-5 py-3.5 sm:grid-cols-[1fr_1fr_1fr_84px_auto] sm:items-end">
        <Field label={t('grantee')}>
          <Suggest bind:value={subject} testid="grant-subject" suggestions={hosted.agents.map((a: any) => ({ value: a.label, hint: a.agent_id }))} />
        </Field>
        <Field label={t('scopes')}><input bind:value={scopes} placeholder="message.draft" data-testid="grant-scopes" /></Field>
        <Field label={t('audiences')}><input bind:value={audiences} placeholder="https://…" data-testid="grant-audiences" /></Field>
        <Field label={t('days')}><input type="number" min="1" max="366" bind:value={days} class="tabular-nums" /></Field>
        <button class={buttonClass('primary')} data-testid="grant-submit" disabled={busy || !subject.trim() || !scopes.trim() || !audiences.trim()} onclick={grant}>
          {t('grant')}
        </button>
      </div>
    {/if}
  </Section>
{/if}
