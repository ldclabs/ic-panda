<script lang="ts">
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
  import { t } from '../../i18n/index.svelte'
  import { when } from '../../rpc'
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
    run
  }: {
    hosted: any
    grants: any[]
    unlocked: boolean
    busy: boolean
    run: (method: string, params?: Record<string, unknown>) => Promise<void>
  } = $props()

  let grantee = $state('')
  /** The origin the service capabilities name, `*` for every allowed host. */
  let origin = $state('*')
  let ticked = $state<Record<string, boolean>>({})
  /** The capabilities picked from the vocabulary, targets filled in. */
  let others = $state<string[]>([])
  /** The vocabulary entry whose target is being filled in, such as `vault.read:<tag>`. */
  let asking = $state<string | null>(null)
  let target = $state('')
  /** Days, or empty until revoked. */
  let days = $state('')
  let confirming = $state(false)

  const reach = $derived(matrix(grants))
  const columns = services.flatMap((s) => [s.read, s.write])
  const labelOf = (did: string) => hosted.agents.find((a: any) => a.agent_id === did)?.label ?? did

  /** The ticked services at the origin and the picked capabilities, each once. */
  const chosen = $derived([
    ...new Set([
      ...Object.keys(ticked)
        .filter((name) => ticked[name])
        .map((name) => `${name}:${origin}`),
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
    await run('grants/add', { agent: grantee, capabilities: chosen, days: days ? Number(days) : null })
    ticked = {}
    others = []
    asking = null
    target = ''
  }

  const submit = () => (wide.length ? (confirming = true) : grant())
</script>

{#snippet cell(origins: string[] | undefined)}
  {#if origins?.length}
    <Tooltip content={origins.map((o) => (o === '*' ? t('everyHost') : o)).join('\n')}>
      {#snippet children(props)}
        <span {...props} class="inline-grid size-6 place-items-center rounded-md bg-pale text-forest">
          {#if origins.includes('*')}<Icon name="check" class="size-3.5" />{:else}<span class="font-mono text-[10.5px]">{origins.length}</span>{/if}
        </span>
      {/snippet}
    </Tooltip>
  {:else}
    <span class="inline-block size-1 rounded-full bg-line" aria-label="-"></span>
  {/if}
{/snippet}

<Section title={t('capabilityGrants')} description={t('capabilitiesHint')} icon="security" flush>
  <div class="overflow-x-auto border-t border-line">
    <table class="w-full min-w-[640px] border-collapse text-[12.5px]" data-testid="capability-matrix">
      <thead>
        <tr class="text-[11px] text-faint">
          <th rowspan="2" class="px-5 py-2 text-start font-medium">{t('agentIds')}</th>
          {#each services as service (service.read)}
            <th colspan="2" class="border-s border-line px-2 pt-2 text-center font-medium text-ink-2">{service.name.replace('Agent ', '')}</th>
          {/each}
          <th rowspan="2" class="border-s border-line px-3 py-2 text-start font-medium">{t('otherCapabilities')}</th>
        </tr>
        <tr class="text-[10.5px] text-faint">
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
            <td class="px-5 py-2.5 font-medium">{agent.label}</td>
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

{#if unlocked}
  <Section title={t('serviceGrants')} description={t('serviceGrantsHint')} icon="add" testid="grants-card">
    <div class="grid gap-4">
      <p class="text-[12.5px] text-muted">{t('agentContentBoundary')}</p>
      <div class="grid gap-3 sm:grid-cols-2">
        <Field label={t('grantee')}>
          <Select bind:value={grantee} placeholder={t('grantee')} testid="grant-agent" class="w-full" options={hosted.agents.map((a: any) => ({ value: a.agent_id, label: a.label }))} />
        </Field>
        <Field label={t('origin')}>
          <Select
            bind:value={origin}
            testid="grant-origin"
            class="w-full"
            options={[
              { value: '*', label: t('everyHost') },
              ...serviceOrigins(hosted.hosts).map((h) => ({ value: h.host, label: h.label ? `${h.host} · ${h.label}` : h.host, mono: true }))
            ]}
          />
        </Field>
      </div>
      <div class="grid gap-1.5 rounded-lg bg-surface-2/60 p-3" data-testid="service-grants">
        {#each services as service (service.read)}
          <div class="flex flex-wrap items-center justify-between gap-2">
            <span class="text-[13px]">{service.name}</span>
            <span class="flex gap-4">
              <Checkbox bind:checked={() => ticked[service.read] ?? false, (on) => (ticked[service.read] = on)} testid="service-{service.read}">{t('canRead')}</Checkbox>
              <Checkbox bind:checked={() => ticked[service.write] ?? false, (on) => (ticked[service.write] = on)} testid="service-{service.write}">{t('canWrite')}</Checkbox>
            </span>
          </div>
        {/each}
      </div>
      <div class="grid gap-2">
        <p class="text-[12.5px] font-medium">{t('capabilityList')}</p>
        <p class="text-[11.5px] text-faint">{t('capabilityPickHint')}</p>
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
              if (event.key === 'Enter') { event.preventDefault(); addFilled() }
              else if (event.key === 'Escape') { event.stopPropagation(); asking = null }
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
          label={t('grantLifetime')}
          options={[
            { value: '', label: t('untilRevoked') },
            ...[7, 30, 90, 366].map((n) => ({ value: String(n), label: t('daysCount', { n }) }))
          ]}
        />
        <button class={buttonClass('primary')} data-testid="grant-capabilities" disabled={busy || !grantee || !chosen.length} onclick={submit}>{t('grant')}</button>
      </div>
    </div>
  </Section>
{/if}

<Section title={t('grantsList')} icon="audit" flush>
  <ul class="divide-y divide-line" data-testid="capability-grants">
    {#each grants as g (g.id)}
      {@const state = grantState(g)}
      <li class="flex items-start gap-3 px-5 py-3">
        <div class="min-w-0 flex-1">
          <div class="flex flex-wrap items-center gap-1.5">
            <b class="text-[13px] font-medium">{labelOf(g.grantee)}</b>
            {#if state === 'revoked'}
              <Pill testid="grant-revoked">{t('revoked')} {when(g.revoked_at)}</Pill>
            {:else if state === 'expired'}
              <Pill>{t('grantExpired')} {when(g.expires_at)}</Pill>
            {:else}
              <span class="text-[11.5px] text-faint">{g.expires_at ? `${t('expires')} ${when(g.expires_at)}` : t('untilRevoked')}</span>
            {/if}
          </div>
          <div class="mt-1 flex flex-wrap gap-1">
            {#each g.capabilities as capability, i (i)}<Pill class={cn('font-mono font-normal', state !== 'live' && 'line-through opacity-60')}>{capability}</Pill>{/each}
          </div>
        </div>
        {#if state === 'live' && unlocked}
          <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => run('grants/revoke', { id: g.id })}>{t('revoke')}</button>
        {/if}
      </li>
    {:else}
      <li class="px-5 py-3 text-[13px] text-faint">{t('noGrants')}</li>
    {/each}
  </ul>
</Section>

<Confirm
  bind:open={confirming}
  title={t('wideGrantTitle', { label: labelOf(grantee) })}
  description={t('wideGrantHint')}
  confirm={t('grant')}
  tone="warn"
  {busy}
  testid="grant-confirm-dialog"
  onConfirm={grant}
>
  {#snippet body()}
    <div class="flex flex-wrap gap-1">{#each wide as capability (capability)}<Pill tone="warn" class="font-mono font-normal">{capability}</Pill>{/each}</div>
  {/snippet}
</Confirm>
