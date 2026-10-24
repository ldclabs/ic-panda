<script lang="ts">
  import Notice from '../../components/page/Notice.svelte'
  import Checkbox from '../../components/Checkbox.svelte'
  import Dialog from '../../components/Dialog.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Select from '../../components/Select.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { call, errorText, when } from '../../rpc'
  import { live, sensitive, unknownCapabilities } from './agents'

  /**
   * What this account's Agent IDs may do with one thing, such as a vault
   * tag or a team wallet: the live grants that reach it with their revoke,
   * and a new grant of the `choices` ticked, for a number of days or until
   * revoked. A choice the daemon's vocabulary cannot name, such as a tag
   * with a space, is not offered. Grants of other things stay in Agents.
   */
  let {
    open = $bindable(false),
    title,
    description,
    choices,
    testid
  }: {
    open?: boolean
    title: string
    description: string
    /** What may be granted here, each with its label; `name:*` grants reach it too. */
    choices: { capability: string; label: string }[]
    testid?: string
  } = $props()

  let agents = $state<any[]>([])
  let grants = $state<any[]>([])
  let vocabulary = $state<string[]>([])
  let agent = $state('')
  let ticked = $state<Record<string, boolean>>({})
  let days = $state('')
  let busy = $state(false)
  let error = $state('')

  /** The capabilities that reach this thing: those offered, and their `name:*`. */
  const names = $derived([...new Set(choices.flatMap((c) => [c.capability, c.capability.replace(/:.*$/, ':*')]))])
  const reaching = $derived(grants.filter((g) => live(g) && g.capabilities.some((c: string) => names.includes(c))))
  const refused = $derived(vocabulary.length ? unknownCapabilities(choices.map((c) => c.capability), vocabulary) : [])
  const offered = $derived(choices.filter((c) => !refused.includes(c.capability)))
  const chosen = $derived(offered.filter((c) => ticked[c.capability]).map((c) => c.capability))
  const labelOf = (did: string) => agents.find((a) => a.agent_id === did)?.label ?? did

  async function load() {
    const [hosted, list] = await Promise.all([call<any>('agent/ids').catch(() => null), call<any[]>('grants/list').catch(() => [])])
    agents = hosted?.agents ?? []
    vocabulary = hosted?.vocabulary ?? []
    grants = list
    agent ||= agents[0]?.agent_id ?? ''
  }

  $effect(() => {
    if (!open) return
    ticked = {}
    error = ''
    void load()
  })

  async function run(method: string, params: Record<string, unknown>) {
    busy = true
    error = ''
    try {
      await call(method, params)
      await load()
      ticked = {}
    } catch (e) {
      error = errorText(e)
    } finally {
      busy = false
    }
  }
</script>

<Dialog bind:open {title} {description} icon="agents" wide {testid}>
  <div class="grid gap-3">
    <ul class="divide-y divide-line rounded-xl border border-line">
      {#each reaching as g (g.id)}
        <li class="flex items-start gap-3 px-3 py-2">
          <div class="min-w-0 flex-1">
            <b class="text-[13px] font-medium">{labelOf(g.grantee)}</b>
            <div class="mt-1 flex flex-wrap gap-1">
              {#each g.capabilities as capability (capability)}
                <Pill class={cn('font-mono font-normal', !names.includes(capability) && 'opacity-50')}>{capability}</Pill>
              {/each}
            </div>
            <p class="mt-1 text-[11px] text-faint">{g.expires_at ? `${t('expires')} ${when(g.expires_at)}` : t('untilRevoked')}</p>
          </div>
          <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => run('grants/revoke', { id: g.id })}>{t('revoke')}</button>
        </li>
      {:else}
        <li class="px-3 py-2.5 text-[12.5px] text-faint">{t('noAgentReach')}</li>
      {/each}
    </ul>
    {#if agents.length}
      <div class="grid gap-2 rounded-xl border border-line bg-surface-2 p-2.5">
        <div class="flex flex-wrap items-center gap-2">
          <Select bind:value={agent} options={agents.map((a) => ({ value: a.agent_id, label: a.label }))} label={t('agentId')} testid={testid ? `${testid}-agent` : undefined} />
          <Select
            bind:value={days}
            label={t('grantLifetime')}
            options={[{ value: '', label: t('untilRevoked') }, ...[7, 30, 90, 366].map((n) => ({ value: String(n), label: t('daysCount', { n }) }))]}
          />
        </div>
        <div class="flex flex-wrap gap-3">
          {#each offered as choice (choice.capability)}
            <Checkbox bind:checked={() => ticked[choice.capability] ?? false, (on) => (ticked[choice.capability] = on)} testid={testid ? `${testid}-${choice.capability.replace(/:.*$/, '')}` : undefined}>
              {choice.label}
            </Checkbox>
          {/each}
        </div>
        {#if refused.length}<p class="text-[12px] text-faint" data-testid={testid ? `${testid}-refused` : undefined}>{t('unknownCapabilities', { list: refused.join(', ') })}</p>{/if}
        {#if chosen.some(sensitive)}<p class="text-[12px] text-warn">{t('wideGrantHint')}</p>{/if}
        <button
          class={buttonClass('primary', 'sm', 'justify-self-end')}
          disabled={busy || !agent || !chosen.length}
          onclick={() => run('grants/add', { agent, capabilities: chosen, days: days ? Number(days) : null })}
          data-testid={testid ? `${testid}-grant` : undefined}>{t('grant')}</button
        >
      </div>
    {:else}
      <p class="text-[12px] text-faint">{t('noAgentIds')}</p>
    {/if}
    {#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
  </div>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('primary')} onclick={() => (open = false)}>{t('done')}</button>
  {/snippet}
</Dialog>
