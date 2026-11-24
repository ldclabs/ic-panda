<script lang="ts">
  import { onDestroy, tick } from 'svelte'
  import { DropdownMenu } from 'bits-ui'
  import { localeDir } from '../../../shared/locales'
  import Confirm from '../../components/Confirm.svelte'
  import { copyText } from '../../components/clipboard'
  import Dialog from '../../components/Dialog.svelte'
  import Empty from '../../components/page/Empty.svelte'
  import Icon from '../../components/Icon.svelte'
  import Segmented from '../../components/Segmented.svelte'
  import TextField from '../../components/TextField.svelte'
  import Id from '../../components/page/Id.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import { buttonClass, iconButtonClass, menuContentClass, menuItemClass } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { ago, bridge, call, errorText, when } from '../../rpc'
  import { mcpSetup } from './agents'

  /**
   * The Agent IDs this device hosts: which one acts for the desktop, which
   * keep their key resident while locked, which are paused, the sessions
   * they were last used in, and their MCP credential, shown once with what
   * an MCP client runs. Each one's published Agent Profile is shown as it
   * signed it. The hosts they may reach are listed, never edited here.
   */
  let {
    hosted,
    quota,
    unlocked,
    busy,
    run,
    attempt,
    onReview,
    delegateTo,
    manage
  }: {
    hosted: any
    quota: { used: number; limit: number | null } | null
    unlocked: boolean
    busy: boolean
    run: (method: string, params?: Record<string, unknown>, failed?: (message: string) => void) => Promise<boolean>
    attempt: (work: () => Promise<unknown>) => Promise<boolean>
    onReview: () => Promise<void>
    manage: (agent: string) => void
    delegateTo: (agent: string) => void
  } = $props()

  let alive = true
  onDestroy(() => { alive = false })
  let label = $state('')
  let touched = $state(false)
  let createError = $state('')
  let created = $state('')
  let replacing = $state<any>(null)
  let client = $state('codex')
  const name = $derived(label.trim())
  const valid = $derived(/^[a-z0-9_-]{1,40}$/.test(name))
  const existing = $derived(hosted.agents.find((a: any) => a.label === name))
  const clients = [{ value: 'codex', label: 'Codex' }, { value: 'claude', label: 'Claude Code' }, { value: 'anda', label: 'Anda' }, { value: 'token', label: 'Token' }]
  let issued = $state<({ label: string; credential: string } & ReturnType<typeof mcpSetup>) | null>(null)
  let removing = $state<any>(null)
  let withdrawing = $state<any>(null)
  /** Published profiles by Agent ID, once asked for. */
  let profileRead = 0
  let profiles = $state<Record<string, { profiles?: any[]; error?: string; loading?: boolean; read?: number }>>({})

  async function issue(agent: any) {
    // Resolve the setup commands before replacing the one-time credential.
    const runtime = await bridge(() => window.dmsg.runtime())
    if (!alive) return
    const result = await call<{ credential: string }>('agent/credential', { agent: agent.agent_id })
    if (!alive) return
    client = 'codex'
    issued = { label: agent.label, credential: result.credential, ...mcpSetup(runtime, agent.label, result.credential) }
  }

  async function create() {
    if (busy) return
    touched = true
    createError = ''
    if (!valid) { document.getElementById('hosted-agent-label')?.focus(); return }
    const submitted = label, target = name
    if (!existing) {
      if (!(await run('agent/create', { label: target }, message => { if (alive) { createError = message; void onReview() } })) || !alive) return
      await onReview()
      if (!alive || label !== submitted) return
      created = target
    }
    label = ''; touched = false
    await tick()
    document.getElementById('hosted-agent-' + target)?.focus()
  }

  async function profile(agent: string) {
    const read = ++profileRead
    profiles[agent] = { loading: true, read }
    try {
      const result: any = await call('agent/profile', { agent })
      if (!alive || profiles[agent]?.read !== read) return
      profiles[agent] = { profiles: result.profiles }
    } catch (e) {
      if (!alive || profiles[agent]?.read !== read) return
      profiles[agent] = { error: errorText(e) }
    }
  }
</script>

<Section title={t('agentIds')} description={t('agentIdsHint')} icon="agents" flush>
  {#if quota}
    <p class="px-5 py-2 text-callout text-muted" data-testid="agent-account-quota">{t('account')} · {t('hostedAgentIds')} · <bdi dir="ltr">{quota.used} / {quota.limit ?? '—'}</bdi></p>
  {/if}
  <ul class="divide-y divide-line" data-testid="agent-ids">
    {#each hosted.agents as a (a.agent_id)}
      {@const shown = profiles[a.agent_id]}
      <li id={'hosted-agent-' + a.label} tabindex="-1" class="px-5 py-3.5 focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-forest" data-testid="agent-id">
        <div class="flex flex-wrap items-start gap-3">
          <span class="grid size-9 shrink-0 place-items-center rounded-xl bg-pale text-forest"><Icon name="agents" /></span>
          <div class="min-w-[min(100%,10rem)] flex-1">
            <div class="flex flex-wrap items-center gap-1.5">
              <b class="min-w-0 text-body font-semibold [overflow-wrap:anywhere]">{a.label}</b>
              {#if hosted.active === a.agent_id}<Pill tone="forest">{t('active')}</Pill>{/if}
              {#if a.resident}<Pill tone="gem"><Icon name="background" />{t('resident')}</Pill>{/if}
              {#if a.paused}<Pill tone="warn" testid="agent-paused">{t('paused')}</Pill>{/if}
            </div>
            <div class="mt-1"><Id id={a.agent_id} /></div>
            <p class="mt-1 text-caption text-faint">
              {a.credential ? t('credentialIssued', { at: when(a.credential_at) }) : t('noCredential')}
              {#if a.sessions?.length}· {t('sessionsLine', { n: a.sessions.length, at: ago(a.sessions[0].updated_at) })}{/if}
            </p>
          </div>
          <div class="flex max-w-full flex-wrap items-center gap-1.5">
            <button class={buttonClass('secondary', 'sm')} aria-label={t('tabCapabilities') + ' · ' + a.label} onclick={() => manage(a.agent_id)} data-testid="agent-manage"><Icon name="security" />{t('tabCapabilities')}</button>
            {#if !a.credential}<button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => attempt(() => issue(a))} data-testid="agent-connect"><Icon name="plug" />{t('connectAgent')}</button>{/if}
            <DropdownMenu.Root dir={localeDir(locale())}>
              <DropdownMenu.Trigger class={iconButtonClass()} disabled={busy} aria-label={t('moreActions') + ' · ' + a.label} title={t('moreActions')} data-testid="agent-actions"><Icon name="more" /></DropdownMenu.Trigger>
              <DropdownMenu.Portal>
                <DropdownMenu.Content class={menuContentClass} align="end">
                  {#if hosted.active !== a.agent_id}<DropdownMenu.Item class={menuItemClass} onSelect={() => run('agent/use', { agent: a.agent_id })}>{t('use')}</DropdownMenu.Item>{/if}
                  <DropdownMenu.Item class={menuItemClass} disabled={!unlocked} onSelect={() => run('agent/resident', { agent: a.agent_id, on: !a.resident })}>{a.resident ? t('residentOff') : t('residentOn')}</DropdownMenu.Item>
                  {#if a.credential}<DropdownMenu.Item class={menuItemClass} onSelect={() => (replacing = a)} data-testid="agent-connect"><span class="flex items-center gap-2"><Icon name="plug" />{t('reissueCredential')}</span></DropdownMenu.Item>{/if}
                  {#if unlocked}
                    <DropdownMenu.Separator class="my-1 h-px bg-line" />
                    <DropdownMenu.Item class={menuItemClass + ' text-danger'} onSelect={() => (removing = a)} data-testid="agent-remove">{t('removeAgent')}</DropdownMenu.Item>
                  {/if}
                </DropdownMenu.Content>
              </DropdownMenu.Portal>
            </DropdownMenu.Root>
          </div>
        </div>
        <div class="mt-2 flex flex-wrap gap-1.5 ps-12">
          <button class={buttonClass('ghost', 'xs')} aria-expanded={!!shown} aria-controls={'agent-profile-' + a.label} aria-label={t('agentProfile') + ' · ' + a.label} onclick={() => { if (shown) delete profiles[a.agent_id]; else void profile(a.agent_id) }} data-testid="agent-profile"><Icon name="profile" />{t('agentProfile')}</button>
          <button class={buttonClass('ghost', 'xs')} disabled={busy} aria-label={(a.paused ? t('resume') : t('pause')) + ' · ' + a.label} onclick={() => run('agent/pause', { agent: a.agent_id, on: !a.paused })} data-testid="agent-pause">
            {a.paused ? t('resume') : t('pause')}
          </button>
          {#if unlocked}<button class={buttonClass('ghost', 'xs')} onclick={() => delegateTo(a.label)}><Icon name="epoch" />{t('delegateTo')}</button>{/if}
          {#if a.credential}<button class={buttonClass('ghost', 'xs')} disabled={busy} onclick={() => (withdrawing = a)} data-testid="agent-withdraw">{t('withdrawCredential')}</button>{/if}
        </div>
        {#if shown}
          <div id={'agent-profile-' + a.label} class="mt-3 grid min-w-0 gap-2 sm:ms-12" data-testid="agent-profile-view">
            {#if shown.loading}
              <p class="text-callout text-muted">{t('loading')}</p>
            {:else if shown.error}
              <Notice tone="danger">{shown.error}
                {#snippet action()}<button class={buttonClass('secondary', 'sm')} onclick={() => profile(a.agent_id)}>{t('retry')}</button>{/snippet}
              </Notice>
            {:else if !shown.profiles?.length}
              <p class="text-callout text-muted">{t('noProfileServices')}</p>
            {:else}
              {#each shown.profiles as entry (entry.service)}
                <div class="rounded-lg bg-surface-2 p-3 text-callout">
                  <p class="font-mono text-code text-faint">{entry.service}</p>
                  {#if entry.error}
                    <p class="mt-1 text-danger">{t('error')}: {entry.error.code}</p>
                    <button class={buttonClass('subtle', 'xs')} onclick={() => profile(a.agent_id)}>{t('retry')}</button>
                  {:else if !entry.profile}
                    <p class="mt-1 text-faint">{t('noProfile')}</p>
                  {:else}
                    {@const p = entry.profile}
                    <div class="mt-1 flex flex-wrap items-center gap-1.5">
                      <b class="text-body font-semibold">{p.name}</b>
                      {#if p.provider}<span class="text-faint">· {p.provider}</span>{/if}
                      <Pill tone="ok"><Icon name="verified" />{t('signatureChecked')}</Pill>
                    </div>
                    {#if p.description}<p class="mt-1 whitespace-pre-wrap select-text text-muted">{p.description}</p>{/if}
                    {#if p.capabilities?.length}
                      <div class="mt-1.5 flex flex-wrap gap-1">{#each p.capabilities as c (c)}<Pill class="font-mono font-normal">{c}</Pill>{/each}</div>
                    {/if}
                    {#each p.service_endpoints ?? [] as e (e.url)}<p class="mt-1 font-mono text-code text-muted">{e.kind} · {e.url}</p>{/each}
                    {#each p.links ?? [] as l (l.url)}<p class="mt-1 text-caption text-muted">{l.name} ({l.rel}) · <span class="font-mono">{l.url}</span></p>{/each}
                    {#each p.delegations ?? [] as d, i (i)}
                      <p class="mt-1 text-caption text-muted">{t('delegationHintLine')}: <span class="font-mono">{d.principal.id}</span>{d.scopes ? ` · ${d.scopes.join(', ')}` : ''}</p>
                    {/each}
                    <p class="mt-1.5 text-caption text-faint">{t('selfDescribed')} · {when(p.updated_at)}</p>
                  {/if}
                </div>
              {/each}
            {/if}
          </div>
        {/if}
      </li>
    {:else}
      <li><Empty icon="agents" title={t('noAgents')} class="py-8" /></li>
    {/each}
  </ul>
  {#if unlocked}
    <form class="grid gap-2 border-t border-line bg-surface-2/60 px-5 py-3.5" onsubmit={(event) => { event.preventDefault(); void create() }}>
      <div class="flex flex-wrap items-start gap-2">
        <TextField id="hosted-agent-label" label={t('label')} hint={touched && name && !valid ? undefined : t('agentLabelFormat')} error={touched && name && !valid ? t('agentLabelFormat') : createError || undefined}
          bind:value={label} oninput={() => { createError = ''; created = '' }} onblur={() => (touched = true)} spellcheck={false} autocapitalize="none" autocomplete="off"
          class="min-w-40 max-w-xs flex-1" data-testid="agent-label" />
        <button class={buttonClass('primary', 'md', 'mt-5')} type="submit" disabled={busy || !name} data-testid="agent-create">{existing ? t('open') : t('create')}</button>
      </div>
      <p role="status" class="text-callout text-muted empty:hidden">{created ? t('created') + ' · ' + created : ''}</p>
    </form>
  {/if}
</Section>

<Section title={t('hosts')} description={t('hostsHint')} icon="website">
  <ul class="flex flex-wrap gap-1.5">
    {#each hosted.hosts as h (h.host)}
      <li class="inline-flex items-center gap-1.5 rounded-lg bg-surface-2 px-2 py-1 text-callout">
        <Icon name="lock" class="size-3 text-faint" />
        <bdi dir="ltr" class="font-mono">{h.host}</bdi>
        {#if h.label}<span class="text-faint">· {h.label}</span>{/if}
      </li>
    {:else}
      <li class="text-body text-faint">{t('noRecords')}</li>
    {/each}
  </ul>
</Section>

{#snippet block(title: string, value: string, testid: string)}
  <div class="grid gap-1">
    <div class="flex items-center justify-between gap-2">
      <h3 class="text-callout font-medium text-muted">{title}</h3>
      <button class={buttonClass('ghost', 'xs')} aria-label={t('copy') + ' · ' + title} onclick={() => copyText(value)}><Icon name="copy" />{t('copy')}</button>
    </div>
    <pre class="rounded-lg bg-surface-2 px-2.5 py-2 font-mono text-code break-all whitespace-pre-wrap text-ink select-all" data-testid={testid}>{value}</pre>
  </div>
{/snippet}

<Dialog bind:open={() => !!issued, (open) => !open && (issued = null)} title={t('connectTitle', { label: issued?.label ?? '' })} description={t('credentialOnce')} icon="plug" wide testid="agent-connect-dialog">
  {#if issued}
    <div class="grid gap-3">
      <Segmented bind:value={client} options={clients} label={t('connectAgent')} testid="agent-client" class="w-fit max-w-full flex-wrap" />
      {#if client === 'codex'}
        {@render block(t('connectCodexCommand'), issued.codexCommand, 'agent-connect-codex-command')}
        <details><summary class="text-callout text-forest">{t('connectCodexConfig')}</summary>
          {@render block(t('connectCodexConfig'), issued.codexConfig, 'agent-connect-codex-config')}
        </details>
        <p class="text-callout text-muted">{t('connectCodexHint')}</p>
      {:else if client === 'claude'}
        {@render block(t('connectClaudeCode'), issued.command, 'agent-connect-command')}
        <details><summary class="text-callout text-forest">{t('connectConfig')}</summary>
          {@render block(t('connectConfig'), issued.config, 'agent-connect-config')}
        </details>
      {:else if client === 'anda'}
        {@render block(t('connectAndaConfig'), issued.andaConfig, 'agent-connect-anda-config')}
        <p class="text-callout text-muted">{t('connectAndaHint')}</p>
      {:else}
        {@render block('DMSG_AGENT_TOKEN', issued.credential, 'agent-credential')}
      {/if}
      <p class="text-callout text-muted">{t('connectNext')}</p>
      <p class="text-callout text-warn">{t('connectStored')}</p>
    </div>
  {/if}
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('primary')} onclick={() => (issued = null)}>{t('done')}</button>
  {/snippet}
</Dialog>

<Confirm bind:open={() => !!replacing, (open) => !open && (replacing = null)}
  title={t('reissueCredential') + ' · ' + (replacing?.label ?? '')} description={t('credentialOnce')}
  confirm={t('reissueCredential')} icon="plug" tone="warn" {busy} testid="agent-reissue-dialog"
  onConfirm={() => issue(replacing)} {onReview} />

<Confirm
  bind:open={() => !!withdrawing, (open) => !open && (withdrawing = null)}
  title={t('withdrawTitle', { label: withdrawing?.label ?? '' })}
  description={t('withdrawHint')}
  confirm={t('withdrawCredential')}
  tone="warn"
  {busy}
  testid="agent-withdraw-dialog"
  onConfirm={() => call<void>('agent/credential_revoke', { agent: withdrawing.agent_id })}
  {onReview}
/>

<Confirm
  bind:open={() => !!removing, (open) => !open && (removing = null)}
  title={t('agentRemoveTitle', { label: removing?.label ?? '' })}
  description={t('agentRemoveHint')}
  confirm={t('removeAgent')}
  {busy}
  testid="agent-remove-dialog"
  onConfirm={() => call<void>('agent/remove', { agent: removing.agent_id })}
  {onReview}
/>
