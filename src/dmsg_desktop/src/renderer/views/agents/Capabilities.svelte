<script lang="ts">
  import { untrack } from 'svelte'
  import Checkbox from '../../components/Checkbox.svelte'
  import Confirm from '../../components/Confirm.svelte'
  import Icon from '../../components/Icon.svelte'
  import Field from '../../components/page/Field.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import Select from '../../components/Select.svelte'
  import TextField from '../../components/TextField.svelte'
  import Tooltip from '../../components/Tooltip.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { t, type Label } from '../../i18n/index.svelte'
  import { call, when } from '../../rpc'
  import { grantState, matrix, serviceOrigins, sensitive, services, unknownCapabilities } from './agents'

  /**
   * What each hosted Agent ID may reach, as a matrix of the Agent Protocols
   * services and the origins they are granted at, then every grant with its
   * revoke, and a new grant: services ticked at a chosen origin and other
   * capabilities picked from the daemon's vocabulary, each with the target it
   * names filled in, for a number of days or until revoked. A grant reaching every target, a secret's value or a
   * wallet's execution asks once more.
   */
  let {
    hosted,
    grants,
    unlocked,
    busy,
    active, selectAgent, attempt, onReview
  }: {
    hosted: any
    grants: any[]
    unlocked: boolean
    busy: boolean
    active: string
    selectAgent: (agent: string) => void
    attempt: (work: () => Promise<unknown>) => Promise<boolean>
    onReview: () => Promise<void>
  } = $props()

  let grantee = $state('')
  /** The origin the service capabilities name, `*` for every allowed host. */
  let origin = $state('')
  const origins = $derived(serviceOrigins(hosted.hosts.filter((h: any) => h.allowed !== false)))
  const selectedOrigin = $derived(origin ? (origin === '*' || origins.some(h => h.host === origin) ? origin : '') : origins.length === 1 ? origins[0].host : '')
  let revoking = $state<any>(null)
  let submitting = $state(false)
  let now = $state(Date.now())
  $effect(() => {
    const expires = Math.min(...grants.filter(g => !g.revoked_at && g.expires_at > now).map(g => g.expires_at))
    if (!Number.isFinite(expires)) return
    const timer = setTimeout(() => (now = Date.now()), Math.max(1, Math.min(2_147_483_647, expires - Date.now())))
    return () => clearTimeout(timer)
  })
  let ticked = $state<Record<string, boolean>>({})
  /** The capabilities picked from the vocabulary, targets filled in. */
  let others = $state<string[]>([])
  /** The vocabulary entry whose target is being filled in, such as `vault.read:<tag>`. */
  let asking = $state<string | null>(null)
  let target = $state('')
  /** Days, or empty until revoked. */
  let days = $state('')
  let confirming = $state(false)

  $effect(() => {
    const recipient = active
    untrack(() => {
      if (grantee && grantee !== recipient) {
        ticked = {}; others = []; asking = null; target = ''; confirming = false
      }
      grantee = recipient
    })
  })

  const reach = $derived(matrix(grants, now))
  const selectedServices = $derived(Object.keys(ticked).filter(name => ticked[name]))
  const columns = services.flatMap((s) => [s.read, s.write])
  const labelOf = (did: string) => hosted.agents.find((a: any) => a.agent_id === did)?.label ?? did

  /** The ticked services at the origin and the picked capabilities, each once. */
  const chosen = $derived([
    ...new Set([
      ...(selectedOrigin ? selectedServices.map(name => name + ':' + selectedOrigin) : []),
      ...others
    ])
  ])

  const vocabulary = $derived<string[]>(hosted.vocabulary ?? [])
  const otherCapabilities = $derived(vocabulary.filter((v) => !services.some((s) => v.startsWith(`${s.read}:`) || v.startsWith(`${s.write}:`))))
  const wide = $derived(chosen.filter(sensitive))
  /** The capability the target makes; refused with what the vocabulary allows. */
  const filled = $derived(asking && target.trim() ? `${asking.slice(0, asking.indexOf(':<'))}:${target.trim()}` : '')
  const refused = $derived(!!filled && unknownCapabilities([filled], vocabulary).length > 0)
  /** Whether any capability picked comes from this vocabulary entry. */
  const pickedFrom = (entry: string) => others.some((c) => c === entry || (entry.includes(':<') && c.startsWith(entry.slice(0, entry.indexOf('<')))))

  /** A capability with no target is ticked on or off; one with a target asks for it. */
  function pick(entry: string) {
    if (!entry.includes(':<')) {
      others = others.includes(entry) ? others.filter((c) => c !== entry) : [...others, entry]
      return
    }
    asking = entry
    target = ''
  }

  function addFilled() {
    if (!filled || refused) return
    if (!others.includes(filled)) others = [...others, filled]
    asking = null
    target = ''
  }

  async function grant() {
    if (!ready || submitting) return
    submitting = true
    const recipient = grantee
    try {
      await call('grants/add', { agent: recipient, capabilities: chosen, days: days ? Number(days) : null })
      if (grantee === recipient) { ticked = {}; others = []; asking = null; target = '' }
      await onReview()
    } finally { submitting = false }
  }

  const groups: Record<string, Label> = {
    account: 'account', vault: 'vault', files: 'files', channel: 'channels', inbox: 'inbox', wallet: 'wallet',
    profile: 'profile', 'agent.delegation': 'agentPermissionDelegations'
  }
  const verbs: Record<string, Label> = {
    read: 'canRead', write: 'canWrite', list: 'agentPermissionList', publish: 'agentPermissionPublish',
    contact: 'agentPermissionLetter', propose: 'agentPermissionPropose', execute: 'agentPermissionExecute'
  }
  const actions: Record<string, Label> = {
    'secret.use': 'agentPermissionUse', 'attest.request': 'agentPermissionSign',
    'action.request': 'agentPermissionAction', 'events.endpoint': 'agentPermissionEndpoints'
  }
  function describe(capability: string) {
    const colon = capability.indexOf(':')
    const code = colon < 0 ? capability : capability.slice(0, colon)
    const target = colon < 0 ? '' : capability.slice(colon + 1)
    const service = services.find(s => s.read === code || s.write === code)
    const dot = code.lastIndexOf('.')
    const group = service?.label ?? groups[code.slice(0, dot)]
    const verb = verbs[code.slice(dot + 1)]
    const title = actions[code] ? t(actions[code]) : group && verb ? t(group) + ' · ' + t(verb) : code
    return target ? title + ' · ' + (target === '*' ? t(service ? 'everyHost' : 'everything') : target) : title
  }

  const ready = $derived(!!grantee && chosen.length > 0 && (!selectedServices.length || !!selectedOrigin))
  const submit = () => { if (ready && !busy && !submitting) { if (wide.length) confirming = true; else void attempt(grant) } }
</script>

{#snippet cell(origins: string[] | undefined)}
  {#if origins?.length}
    <Tooltip content={origins.map((o) => (o === '*' ? t('everyHost') : o)).join('\n')}>
      {#snippet children(props)}
        <button type="button" {...props} aria-label={origins.map(o => o === '*' ? t('everyHost') : o).join(', ')} class="inline-grid size-6 place-items-center rounded-md bg-pale text-forest">
          {#if origins.includes('*')}<Icon name="check" class="size-3.5" />{:else}<span class="font-mono text-code">{origins.length}</span>{/if}
        </button>
      {/snippet}
    </Tooltip>
  {:else}
    <span class="inline-block size-1 rounded-full bg-line" aria-hidden="true"></span><span class="sr-only">{t('noGrants')}</span>
  {/if}
{/snippet}

<Section title={t('capabilityGrants')} description={t('capabilitiesHint')} icon="security" flush>
  <div class="overflow-x-auto border-t border-line">
    <table class="w-full min-w-[640px] border-collapse text-callout" data-testid="capability-matrix">
      <caption class="sr-only">{t('capabilityGrants')}</caption>
      <thead>
        <tr class="text-caption text-faint">
          <th rowspan="2" class="px-5 py-2 text-start font-medium">{t('agentIds')}</th>
          {#each services as service (service.read)}
            <th scope="colgroup" colspan="2" class="border-s border-line px-2 pt-2 text-center font-medium text-ink-2">{t(service.label)}</th>
          {/each}
          <th rowspan="2" class="border-s border-line px-3 py-2 text-start font-medium">{t('otherCapabilities')}</th>
        </tr>
        <tr class="text-caption text-faint">
          {#each services as service (service.read)}
            <th class="border-s border-line px-2 pb-2 text-center font-normal">{t('canRead')}</th>
            <th class="px-2 pb-2 text-center font-normal">{t('canWrite')}</th>
          {/each}
        </tr>
      </thead>
      <tbody>
        {#each hosted.agents as agent (agent.agent_id)}
          {@const row = reach.get(agent.agent_id)}
          <tr class="border-t border-line">
            <th scope="row" class="px-5 py-2.5 text-start font-medium">{agent.label}</th>
            {#each columns as column, i (column)}
              <td class={cn('px-2 py-2.5 text-center', (i % 2 === 0) && 'border-s border-line')}>{@render cell(row?.services[column])}</td>
            {/each}
            <td class="border-s border-line px-3 py-2">
              <div class="flex flex-wrap gap-1">
                {#each row?.other ?? [] as capability (capability)}<Pill class="font-mono font-normal">{capability}</Pill>{:else}<span class="text-faint">-</span>{/each}
              </div>
            </td>
          </tr>
        {:else}
          <tr class="border-t border-line"><td colspan={columns.length + 2} class="px-5 py-3 text-faint">{t('noAgents')}</td></tr>
        {/each}
      </tbody>
    </table>
  </div>
</Section>

{#if unlocked && hosted.agents.length}
  <Section title={t('serviceGrants')} description={t('serviceGrantsHint')} icon="add" testid="grants-card">
    <fieldset class="grid min-w-0 gap-4" disabled={busy || submitting}>
      <p class="text-callout text-muted">{t('agentContentBoundary')}</p>
      <div class="grid gap-3 sm:grid-cols-2">
        <Field label={t('grantee')}>
          <Select bind:value={() => grantee, selectAgent} label={t('grantee') + (grantee ? ' · ' + labelOf(grantee) : '')} placeholder={t('choose')} testid="grant-agent" class="w-full" options={hosted.agents.map((a: any) => ({ value: a.agent_id, label: a.label }))} />
        </Field>
        <Field label={t('origin')}>
          <Select
            bind:value={() => selectedOrigin, (value) => (origin = value)} label={t('origin') + (selectedOrigin ? ' · ' + (selectedOrigin === '*' ? t('everyHost') : selectedOrigin) : '')}
            aria-describedby={selectedServices.length && !selectedOrigin ? 'grant-origin-needed' : undefined} placeholder={t('choose')}
            testid="grant-origin"
            class="w-full"
            options={[
              { value: '*', label: t('everyHost') },
              ...origins.map((h) => ({ value: h.host, label: h.label ? `${h.host} · ${h.label}` : h.host, mono: true }))
            ]}
          />
          {#if selectedServices.length && !selectedOrigin}<span id="grant-origin-needed" class="text-callout text-muted" role="status">{t('keysRequired')}</span>{/if}
        </Field>
      </div>
      <div class="grid gap-1.5 rounded-lg bg-surface-2/60 p-3" data-testid="service-grants">
        {#each services as service (service.read)}
          <div class="flex flex-wrap items-center justify-between gap-2">
            <span class="text-body">{t(service.label)}</span>
            <span class="flex gap-4">
              <Checkbox label={t(service.label) + ' · ' + t('canRead')} bind:checked={() => ticked[service.read] ?? false, (on) => (ticked[service.read] = on)} testid="service-{service.read}">{t('canRead')}</Checkbox>
              <Checkbox label={t(service.label) + ' · ' + t('canWrite')} bind:checked={() => ticked[service.write] ?? false, (on) => (ticked[service.write] = on)} testid="service-{service.write}">{t('canWrite')}</Checkbox>
            </span>
          </div>
        {/each}
      </div>
      <div class="grid gap-2">
        <p class="text-callout font-medium">{t('capabilityList')}</p>
        <p class="text-caption text-faint">{t('capabilityPickHint')}</p>
        <div class="flex flex-wrap gap-1.5" data-testid="grant-options">
          {#each otherCapabilities as capability (capability)}
            <button
              type="button"
              class={buttonClass(asking === capability ? 'primary' : 'secondary', 'xs', 'font-mono font-normal')}
              aria-pressed={pickedFrom(capability)}
              disabled={busy}
              onclick={() => pick(capability)}
            >
              {#if pickedFrom(capability)}<Icon name="check" />{/if}{capability}
            </button>
          {/each}
        </div>
        {#if asking}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="flex flex-wrap items-start gap-2 rounded-lg bg-surface-2/60 p-3"
            onkeydown={(event) => {
              if (!event.isComposing && event.key === 'Escape') { event.stopPropagation(); asking = null }
            }}
            data-testid="grant-parameter"
          >
            <!-- svelte-ignore a11y_autofocus -->
            <TextField
              label={asking}
              bind:value={target}
              mono
              autofocus
              spellcheck={false}
              onkeydown={(event) => {
                if (!event.isComposing && event.key === 'Enter') { event.preventDefault(); addFilled() }
              }}
              error={refused ? t('unknownCapabilities', { list: filled }) : undefined}
              class="min-w-48 flex-1"
              data-testid="grant-target"
            />
            <div class="flex gap-1.5 pt-5">
              <button class={buttonClass('secondary')} disabled={!filled || refused} onclick={addFilled} data-testid="grant-target-add">{t('add')}</button>
              <button class={buttonClass('ghost')} onclick={() => (asking = null)}>{t('cancel')}</button>
            </div>
          </div>
        {/if}
      </div>
      <div class="flex flex-wrap items-center gap-2">
        <div class="flex min-w-0 flex-1 flex-wrap gap-1" data-testid="grant-chosen">
          {#each chosen as capability (capability)}
            <Pill tone={sensitive(capability) ? 'warn' : 'forest'} class="font-mono font-normal">
              {capability}
              {#if others.includes(capability)}
                <button class="-me-1 grid size-4 place-items-center rounded-full hover:bg-ink/10" aria-label={`${t('remove')} ${capability}`} onclick={() => (others = others.filter((c) => c !== capability))} data-testid="grant-remove"><Icon name="close" class="size-3" /></button>
              {/if}
            </Pill>
          {/each}
        </div>
        <Select
          bind:value={days}
          testid="grant-days"
          label={t('grantLifetime') + ' · ' + (days ? t('daysCount', { n: Number(days) }) : t('untilRevoked'))}
          options={[
            { value: '', label: t('untilRevoked') },
            ...[7, 30, 90, 366].map((n) => ({ value: String(n), label: t('daysCount', { n }) }))
          ]}
        />
        <button class={buttonClass('primary')} data-testid="grant-capabilities" disabled={busy || submitting || !ready} onclick={submit}>{t('grant')}</button>
      </div>
    </fieldset>
  </Section>
{/if}

<Section title={t('grantsList')} icon="audit" flush>
  <ul class="divide-y divide-line" data-testid="capability-grants">
    {#each grants as g (g.id)}
      {@const state = grantState(g, now)}
      <li class="flex items-start gap-3 px-5 py-3">
        <div class="min-w-0 flex-1">
          <div class="flex flex-wrap items-center gap-1.5">
            <b class="text-body font-medium">{labelOf(g.grantee)}</b>
            {#if state === 'revoked'}
              <Pill testid="grant-revoked">{t('revoked')} {when(g.revoked_at)}</Pill>
            {:else if state === 'expired'}
              <Pill>{t('grantExpired')} {when(g.expires_at)}</Pill>
            {:else}
              <span class="text-caption text-faint">{g.expires_at ? `${t('expires')} ${when(g.expires_at)}` : t('untilRevoked')}</span>
            {/if}
          </div>
          <div class="mt-1 flex flex-wrap gap-1">
            {#each g.capabilities as capability, i (i)}<Pill class={cn('font-mono font-normal', state !== 'live' && 'line-through opacity-60')}>{capability}</Pill>{/each}
          </div>
        </div>
        {#if state === 'live' && unlocked}
          <button class={buttonClass('secondary', 'sm')} disabled={busy} aria-label={t('revoke') + ' · ' + labelOf(g.grantee)} onclick={() => (revoking = g)} data-testid="grant-revoke">{t('revoke')}</button>
        {/if}
      </li>
    {:else}
      <li class="px-5 py-3 text-body text-faint">{t('noGrants')}</li>
    {/each}
  </ul>
</Section>

{#snippet reviewPermissions(values: string[])}
  <ul class="grid gap-2">
    {#each values as capability, index (index)}
      <li class="min-w-0 rounded-lg bg-surface-2 px-3 py-2">
        <p class={cn('text-body font-medium [overflow-wrap:anywhere]', sensitive(capability) && 'text-warn')}>{describe(capability)}</p>
        <bdi dir="ltr" class="font-mono text-code text-muted [overflow-wrap:anywhere]">{capability}</bdi>
      </li>
    {/each}
  </ul>
{/snippet}

<Confirm
  bind:open={confirming}
  title={t('grant') + ' · ' + labelOf(grantee)}
  confirm={t('grant')}
  tone="warn"
  {busy}
  testid="grant-confirm-dialog"
  onConfirm={grant}
  {onReview}
>
  {#snippet body()}
    <p class="mb-2 text-body">{t('grantLifetime')} · {days ? t('daysCount', { n: Number(days) }) : t('untilRevoked')}</p>
    {@render reviewPermissions(chosen)}
  {/snippet}
</Confirm>

<Confirm bind:open={() => !!revoking, (next) => !next && (revoking = null)}
  title={t('revoke') + ' · ' + (revoking ? labelOf(revoking.grantee) : '')}
  description={t('revokeAllOfGrant')} confirm={t('revoke')} tone="danger" {busy} testid="grant-revoke-dialog"
  onConfirm={async () => { await call('grants/revoke', { id: revoking.id }); await onReview() }} {onReview}>
  {#snippet body()}
    {@render reviewPermissions(revoking?.capabilities ?? [])}
  {/snippet}
</Confirm>
