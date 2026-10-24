<script lang="ts">
  import { onDestroy } from 'svelte'
  import { DropdownMenu } from 'bits-ui'
  import { localeDir } from '../../../shared/locales'
  import Confirm from '../../components/Confirm.svelte'
  import { copyText } from '../../components/clipboard'
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import Field from '../../components/page/Field.svelte'
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
    unlocked,
    busy,
    run,
    attempt,
    delegateTo
  }: {
    hosted: any
    unlocked: boolean
    busy: boolean
    run: (method: string, params?: Record<string, unknown>) => Promise<void>
    attempt: (work: () => Promise<unknown>) => Promise<void>
    delegateTo: (agent: string) => void
  } = $props()

  let alive = true
  onDestroy(() => { alive = false })
  let label = $state('')
  let issued = $state<({ label: string; credential: string } & ReturnType<typeof mcpSetup>) | null>(null)
  let removing = $state<any>(null)
  let withdrawing = $state<any>(null)
  /** Published profiles by Agent ID, once asked for. */
  let profiles = $state<Record<string, { profiles?: any[]; error?: string; loading?: boolean }>>({})

  const issue = (agent: any) =>
    attempt(async () => {
      const [created, runtime]: any[] = await Promise.all([call('agent/credential', { agent: agent.agent_id }), bridge(() => window.dmsg.runtime())])
      if (!alive) return
      issued = { label: agent.label, credential: created.credential, ...mcpSetup(runtime, agent.label, created.credential) }
    })

  async function create() {
    await run('agent/create', { label: label.trim() })
    label = ''
  }

  async function profile(agent: string) {
    if (profiles[agent]?.loading) return
    if (profiles[agent]?.profiles && !profiles[agent].loading) {
      delete profiles[agent]
      return
    }
    profiles[agent] = { loading: true }
    try {
      const read: any = await call('agent/profile', { agent })
      if (!alive) return
      profiles[agent] = { profiles: read.profiles }
    } catch (e) {
      if (!alive) return
      profiles[agent] = { error: errorText(e) }
    }
  }
</script>

<Section title={t('agentIds')} description={t('agentIdsHint')} icon="agents" flush>
  <ul class="divide-y divide-line" data-testid="agent-ids">
    {#each hosted.agents as a (a.agent_id)}
      {@const shown = profiles[a.agent_id]}
      <li class="px-5 py-3.5" data-testid="agent-id">
        <div class="flex flex-wrap items-start gap-3">
          <span class="grid size-9 shrink-0 place-items-center rounded-xl bg-pale text-forest"><Icon name="agents" /></span>
          <div class="min-w-40 flex-1">
            <div class="flex flex-wrap items-center gap-1.5">
              <b class="text-[13.5px] font-semibold">{a.label}</b>
              {#if hosted.active === a.agent_id}<Pill tone="forest">{t('active')}</Pill>{/if}
              {#if a.resident}<Pill tone="gem"><Icon name="background" />{t('resident')}</Pill>{/if}
              {#if a.paused}<Pill tone="warn" testid="agent-paused">{t('paused')}</Pill>{/if}
            </div>
            <div class="mt-1"><Id id={a.agent_id} /></div>
            <p class="mt-1 text-[11.5px] text-faint">
              {a.credential ? t('credentialIssued', { at: when(a.credential_at) }) : t('noCredential')}
              {#if a.sessions?.length}· {t('sessionsLine', { n: a.sessions.length, at: ago(a.sessions[0].updated_at) })}{/if}
            </p>
          </div>
          <div class="flex shrink-0 flex-wrap justify-end gap-1.5">
            <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => run('agent/pause', { agent: a.agent_id, on: !a.paused })} data-testid="agent-pause">
              {a.paused ? t('resume') : t('pause')}
            </button>
            {#if !a.credential}<button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => issue(a)} data-testid="agent-connect"><Icon name="plug" />{t('connectAgent')}</button>{/if}
            <DropdownMenu.Root dir={localeDir(locale())}>
              <DropdownMenu.Trigger class={iconButtonClass()} disabled={busy} aria-label={t('moreActions')} title={t('moreActions')} data-testid="agent-actions"><Icon name="more" /></DropdownMenu.Trigger>
              <DropdownMenu.Portal>
                <DropdownMenu.Content class={menuContentClass} align="end">
                  {#if hosted.active !== a.agent_id}<DropdownMenu.Item class={menuItemClass} onSelect={() => run('agent/use', { agent: a.agent_id })}>{t('use')}</DropdownMenu.Item>{/if}
                  <DropdownMenu.Item class={menuItemClass} disabled={!unlocked} onSelect={() => run('agent/resident', { agent: a.agent_id, on: !a.resident })}>{a.resident ? t('residentOff') : t('residentOn')}</DropdownMenu.Item>
                  {#if a.credential}<DropdownMenu.Item class={menuItemClass} onSelect={() => issue(a)} data-testid="agent-connect"><span class="flex items-center gap-2"><Icon name="plug" />{t('reissueCredential')}</span></DropdownMenu.Item>{/if}
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
          <button class={buttonClass('ghost', 'xs')} onclick={() => profile(a.agent_id)} data-testid="agent-profile"><Icon name="profile" />{t('agentProfile')}</button>
          {#if unlocked}<button class={buttonClass('ghost', 'xs')} onclick={() => delegateTo(a.label)}><Icon name="epoch" />{t('delegateTo')}</button>{/if}
          {#if a.credential}<button class={buttonClass('ghost', 'xs')} disabled={busy} onclick={() => (withdrawing = a)} data-testid="agent-withdraw">{t('withdrawCredential')}</button>{/if}
        </div>
        {#if shown}
          <div class="mt-3 ms-12 grid gap-2" data-testid="agent-profile-view">
            {#if shown.loading}
              <p class="text-[12px] text-faint">{t('loading')}</p>
            {:else if shown.error}
              <Notice tone="danger">{shown.error}</Notice>
            {:else if !shown.profiles?.length}
              <p class="text-[12px] text-faint">{t('noProfileServices')}</p>
            {:else}
              {#each shown.profiles as entry (entry.service)}
                <div class="rounded-lg bg-surface-2 p-3 text-[12.5px]">
                  <p class="font-mono text-[11.5px] text-faint">{entry.service}</p>
                  {#if entry.error}
                    <p class="mt-1 text-danger">{t('error')}: {entry.error.code}</p>
                  {:else if !entry.profile}
                    <p class="mt-1 text-faint">{t('noProfile')}</p>
                  {:else}
                    {@const p = entry.profile}
                    <div class="mt-1 flex flex-wrap items-center gap-1.5">
                      <b class="text-[13px] font-semibold">{p.name}</b>
                      {#if p.provider}<span class="text-faint">· {p.provider}</span>{/if}
                      <Pill tone="ok"><Icon name="verified" />{t('signatureChecked')}</Pill>
                    </div>
                    {#if p.description}<p class="mt-1 whitespace-pre-wrap text-muted">{p.description}</p>{/if}
                    {#if p.capabilities?.length}
                      <div class="mt-1.5 flex flex-wrap gap-1">{#each p.capabilities as c (c)}<Pill class="font-mono font-normal">{c}</Pill>{/each}</div>
                    {/if}
                    {#each p.service_endpoints ?? [] as e (e.url)}<p class="mt-1 font-mono text-[11.5px] text-muted">{e.kind} · {e.url}</p>{/each}
                    {#each p.links ?? [] as l (l.url)}<p class="mt-1 text-[11.5px] text-muted">{l.name} ({l.rel}) · <span class="font-mono">{l.url}</span></p>{/each}
                    {#each p.delegations ?? [] as d, i (i)}
                      <p class="mt-1 text-[11.5px] text-muted">{t('delegationHintLine')}: <span class="font-mono">{d.principal.id}</span>{d.scopes ? ` · ${d.scopes.join(', ')}` : ''}</p>
                    {/each}
                    <p class="mt-1.5 text-[11px] text-faint">{t('selfDescribed')} · {when(p.updated_at)}</p>
                  {/if}
                </div>
              {/each}
            {/if}
          </div>
        {/if}
      </li>
    {:else}
      <li class="px-5 py-3 text-[13px] text-faint">{t('noAgents')}</li>
    {/each}
  </ul>
  {#if unlocked}
    <div class="flex items-end gap-2 border-t border-line bg-surface-2/60 px-5 py-3.5">
      <Field label={t('label')} class="max-w-xs flex-1"><input bind:value={label} data-testid="agent-label" /></Field>
      <button class={buttonClass('primary')} disabled={busy || !label.trim()} onclick={create}>{t('create')}</button>
    </div>
  {/if}
</Section>

<Section title={t('hosts')} description={t('hostsHint')} icon="website">
  <ul class="flex flex-wrap gap-1.5">
    {#each hosted.hosts as h (h.host)}
      <li class="inline-flex items-center gap-1.5 rounded-lg bg-surface-2 px-2 py-1 text-[12px]">
        <Icon name="lock" class="size-3 text-faint" />
        <bdi dir="ltr" class="font-mono">{h.host}</bdi>
        {#if h.label}<span class="text-faint">· {h.label}</span>{/if}
      </li>
    {:else}
      <li class="text-[13px] text-faint">-</li>
    {/each}
  </ul>
</Section>

{#snippet block(title: string, value: string, testid: string)}
  <div class="grid gap-1">
    <div class="flex items-center justify-between gap-2">
      <h3 class="text-[12px] font-medium text-faint">{title}</h3>
      <button class={buttonClass('ghost', 'xs')} onclick={() => copyText(value)}><Icon name="copy" />{t('copy')}</button>
    </div>
    <pre class="rounded-lg bg-surface-2 px-2.5 py-2 font-mono text-[11.5px] break-all whitespace-pre-wrap text-ink select-all" data-testid={testid}>{value}</pre>
  </div>
{/snippet}

<Dialog bind:open={() => !!issued, (open) => !open && (issued = null)} title={t('connectTitle', { label: issued?.label ?? '' })} description={t('credentialOnce')} icon="plug" wide testid="agent-connect-dialog">
  {#if issued}
    <div class="grid gap-3">
      {@render block(t('connectCodexCommand'), issued.codexCommand, 'agent-connect-codex-command')}
      {@render block(t('connectCodexConfig'), issued.codexConfig, 'agent-connect-codex-config')}
      <p class="text-[12px] text-muted">{t('connectCodexHint')}</p>
      {@render block(t('connectClaudeCode'), issued.command, 'agent-connect-command')}
      {@render block(t('connectConfig'), issued.config, 'agent-connect-config')}
      {@render block(t('connectAndaConfig'), issued.andaConfig, 'agent-connect-anda-config')}
      <p class="text-[12px] text-muted">{t('connectAndaHint')}</p>
      {@render block('DMSG_AGENT_TOKEN', issued.credential, 'agent-credential')}
      <p class="text-[12px] text-muted">{t('connectNext')}</p>
      <p class="text-[12px] text-warn">{t('connectStored')}</p>
    </div>
  {/if}
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('primary')} onclick={() => (issued = null)}>{t('done')}</button>
  {/snippet}
</Dialog>

<Confirm
  bind:open={() => !!withdrawing, (open) => !open && (withdrawing = null)}
  title={t('withdrawTitle', { label: withdrawing?.label ?? '' })}
  description={t('withdrawHint')}
  confirm={t('withdrawCredential')}
  tone="warn"
  {busy}
  testid="agent-withdraw-dialog"
  onConfirm={() => withdrawing && run('agent/credential_revoke', { agent: withdrawing.agent_id })}
/>

<Confirm
  bind:open={() => !!removing, (open) => !open && (removing = null)}
  title={t('agentRemoveTitle', { label: removing?.label ?? '' })}
  description={t('agentRemoveHint')}
  confirm={t('removeAgent')}
  {busy}
  testid="agent-remove-dialog"
  onConfirm={() => removing && run('agent/remove', { agent: removing.agent_id })}
/>
