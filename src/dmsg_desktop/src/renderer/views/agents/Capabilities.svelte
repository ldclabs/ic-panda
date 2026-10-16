<script lang="ts">
  import Checkbox from '../../components/Checkbox.svelte'
  import Icon from '../../components/Icon.svelte'
  import Field from '../../components/page/Field.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import Select from '../../components/Select.svelte'
  import Tooltip from '../../components/Tooltip.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { when } from '../../rpc'
  import { matrix, profileWrite, services, vocabulary } from './agents'

  /**
   * What each hosted Agent ID may reach, as a matrix of the Agent Protocols
   * services and the origins they are granted at, then every grant with its
   * revoke, and a new grant: services ticked at a chosen origin and other
   * capabilities typed.
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
  let capabilities = $state('')

  const reach = $derived(matrix(grants))
  const columns = [...services.flatMap((s) => [s.read, s.write]), profileWrite]
  const labelOf = (did: string) => hosted.agents.find((a: any) => a.agent_id === did)?.label ?? did

  const chosen = $derived([
    ...Object.keys(ticked)
      .filter((name) => ticked[name])
      .map((name) => `${name}:${origin}`),
    ...capabilities.split(/\s+/).filter(Boolean)
  ])

  async function grant() {
    await run('grants/add', { agent: grantee, capabilities: chosen })
    ticked = {}
    capabilities = ''
  }
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
          <th rowspan="2" class="border-s border-line px-2 py-2 text-center font-medium text-ink-2">{t('publishProfile')}</th>
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
              <td class={cn('px-2 py-2.5 text-center', (i % 2 === 0 || i === columns.length - 1) && 'border-s border-line')}>{@render cell(row?.services[column])}</td>
            {/each}
            <td class="border-s border-line px-3 py-2">
              <div class="flex flex-wrap gap-1">
                {#each row?.other ?? [] as capability (capability)}<Pill class="font-mono font-normal">{capability}</Pill>{:else}<span class="text-faint">-</span>{/each}
              </div>
            </td>
          </tr>
        {:else}
          <tr class="border-t border-line"><td colspan="9" class="px-5 py-3 text-faint">{t('noAgents')}</td></tr>
        {/each}
      </tbody>
    </table>
  </div>
</Section>

{#if unlocked}
  <Section title={t('serviceGrants')} description={t('serviceGrantsHint')} icon="add" testid="grants-card">
    <div class="grid gap-4">
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
              ...hosted.hosts.map((h: any) => ({ value: h.host, label: h.label ? `${h.host} · ${h.label}` : h.host, mono: true }))
            ]}
          />
        </Field>
      </div>
      <div class="grid gap-1.5 rounded-xl border border-line bg-surface-2/60 p-3" data-testid="service-grants">
        {#each services as service (service.read)}
          <div class="flex flex-wrap items-center justify-between gap-2">
            <span class="text-[13px]">{service.name}</span>
            <span class="flex gap-4">
              <Checkbox bind:checked={() => ticked[service.read] ?? false, (on) => (ticked[service.read] = on)} testid="service-{service.read}">{t('canRead')}</Checkbox>
              <Checkbox bind:checked={() => ticked[service.write] ?? false, (on) => (ticked[service.write] = on)} testid="service-{service.write}">{t('canWrite')}</Checkbox>
            </span>
          </div>
        {/each}
        <div class="flex flex-wrap items-center justify-between gap-2">
          <span class="text-[13px]">Agent Profile</span>
          <Checkbox bind:checked={() => ticked[profileWrite] ?? false, (on) => (ticked[profileWrite] = on)} testid="service-{profileWrite}">{t('publishProfile')}</Checkbox>
        </div>
      </div>
      <Field label={t('capabilityList')} hint={`${t('capabilityVocabulary')}${vocabulary.join(' ')}`}>
        <input bind:value={capabilities} class="font-mono text-[12.5px]" spellcheck="false" data-testid="grant-other" />
      </Field>
      <div class="flex flex-wrap items-center gap-2">
        <div class="flex min-w-0 flex-1 flex-wrap gap-1">
          {#each chosen as capability (capability)}<Pill tone="forest" class="font-mono font-normal">{capability}</Pill>{/each}
        </div>
        <button class={buttonClass('primary')} data-testid="grant-capabilities" disabled={busy || !grantee || !chosen.length} onclick={grant}>{t('grant')}</button>
      </div>
    </div>
  </Section>
{/if}

<Section title={t('grantsList')} icon="audit" flush>
  <ul class="divide-y divide-line border-t border-line" data-testid="capability-grants">
    {#each grants as g (g.id)}
      <li class="flex items-start gap-3 px-5 py-3">
        <div class="min-w-0 flex-1">
          <div class="flex flex-wrap items-center gap-1.5">
            <b class="text-[13px] font-medium">{labelOf(g.grantee)}</b>
            {#if g.revoked_at}<Pill>{t('retired')}</Pill>{:else if g.expires_at}<span class="text-[11.5px] text-faint">{t('expires')} {when(g.expires_at)}</span>{/if}
          </div>
          <div class="mt-1 flex flex-wrap gap-1">
            {#each g.capabilities as capability (capability)}<Pill class={cn('font-mono font-normal', g.revoked_at && 'line-through opacity-60')}>{capability}</Pill>{/each}
          </div>
        </div>
        {#if !g.revoked_at && unlocked}
          <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => run('grants/revoke', { id: g.id })}>{t('revoke')}</button>
        {/if}
      </li>
    {:else}
      <li class="px-5 py-3 text-[13px] text-faint">{t('noGrants')}</li>
    {/each}
  </ul>
</Section>
