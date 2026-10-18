<script lang="ts">
  import Checkbox from '../../components/Checkbox.svelte'
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Select from '../../components/Select.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { call, errorText, when } from '../../rpc'

  /**
   * Who reaches a channel besides its members. This account's Agent IDs
   * read and write it through the daemon, never with the epoch keys, under
   * `channel.read:<channel>` and `channel.publish:<channel>`; a message an
   * agent writes says so. Grants to applications in the channel's own log
   * are listed with their withdraw.
   */
  let {
    open = $bindable(false),
    channel,
    manages,
    onControl
  }: { open?: boolean; channel: any; manages: boolean; onControl: (action: Record<string, unknown>) => void } = $props()

  let agents = $state<any[]>([])
  let grants = $state<any[]>([])
  let agent = $state('')
  let reads = $state(true)
  let writes = $state(false)
  let busy = $state(false)
  let error = $state('')

  const names = $derived([`channel.read:${channel.id}`, `channel.publish:${channel.id}`, 'channel.read:*', 'channel.publish:*'])
  /** The live grants that reach this channel, with what they reach of it. */
  const reaching = $derived(
    grants
      .filter((g) => !g.revoked_at)
      .map((g) => ({ ...g, here: g.capabilities.filter((c: string) => names.includes(c)) }))
      .filter((g) => g.here.length)
  )
  const apps = $derived<any[]>(Object.values(channel.ledger?.grants ?? {}).filter((g: any) => !g.revoked && g.expires_at > Date.now()))
  const labelOf = (did: string) => agents.find((a) => a.agent_id === did)?.label ?? did

  async function load() {
    try {
      const [hosted, list] = await Promise.all([call<any>('agent/ids'), call<any[]>('grants/list')])
      agents = hosted?.agents ?? []
      grants = list
      agent ||= agents[0]?.agent_id ?? ''
      error = ''
    } catch (e) {
      error = errorText(e)
    }
  }

  $effect(() => {
    if (open) void load()
  })

  async function run(method: string, params: Record<string, unknown>) {
    busy = true
    try {
      await call(method, params)
      await load()
    } catch (e) {
      error = errorText(e)
    } finally {
      busy = false
    }
  }

  const grant = () =>
    run('grants/add', {
      agent,
      capabilities: [reads && `channel.read:${channel.id}`, writes && `channel.publish:${channel.id}`].filter(Boolean)
    })
</script>

<Dialog bind:open title={t('channelAccess')} description={t('channelAccessHint')} icon="agents" wide testid="channel-access">
  <div class="grid gap-4">
    <section class="grid gap-2">
      <h3 class="text-[12px] font-medium text-faint">{t('agentAccess')}</h3>
      <ul class="divide-y divide-line rounded-xl border border-line" data-testid="channel-access-agents">
        {#each reaching as g (g.id)}
          <li class="flex items-start gap-3 px-3 py-2">
            <div class="min-w-0 flex-1">
              <b class="text-[13px] font-medium">{labelOf(g.grantee)}</b>
              <div class="mt-1 flex flex-wrap gap-1">
                {#each g.capabilities as capability (capability)}
                  <Pill class={cn('font-mono font-normal', !g.here.includes(capability) && 'opacity-50')}>{capability}</Pill>
                {/each}
              </div>
              {#if g.expires_at}<p class="mt-1 text-[11px] text-faint">{t('expires')} {when(g.expires_at)}</p>{/if}
              {#if g.capabilities.length > g.here.length}<p class="mt-1 text-[11px] text-faint">{t('revokeAllOfGrant')}</p>{/if}
            </div>
            <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => run('grants/revoke', { id: g.id })} data-testid="access-revoke">{t('revoke')}</button>
          </li>
        {:else}
          <li class="px-3 py-2.5 text-[12.5px] text-faint">{t('noAgentAccess')}</li>
        {/each}
      </ul>
      {#if agents.length}
        <div class="flex flex-wrap items-center gap-2 rounded-xl border border-line bg-surface-2 p-2.5">
          <Select
            bind:value={agent}
            options={agents.map((a) => ({ value: a.agent_id, label: a.label }))}
            label={t('agentId')}
            testid="access-agent"
          />
          <Checkbox bind:checked={reads} testid="access-read">{t('canRead')}</Checkbox>
          <Checkbox bind:checked={writes} testid="access-publish">{t('canWrite')}</Checkbox>
          <span class="flex-1"></span>
          <button class={buttonClass('primary', 'sm')} disabled={busy || !agent || !(reads || writes)} onclick={grant} data-testid="access-grant">{t('grant')}</button>
        </div>
      {:else}
        <p class="text-[12px] text-faint">{t('noAgentIds')}</p>
      {/if}
    </section>
    {#if apps.length}
      <section class="grid gap-2">
        <h3 class="text-[12px] font-medium text-faint">{t('appAccess')}</h3>
        <ul class="divide-y divide-line rounded-xl border border-line">
          {#each apps as g (g.grant_id)}
            <li class="flex items-center gap-3 px-3 py-2">
              <Icon name="website" class="shrink-0 text-faint" />
              <div class="min-w-0 flex-1">
                <p class="truncate font-mono text-[12px]">{g.origin}</p>
                <p class="text-[11px] text-faint">{g.actions.join(', ')} · {t('epochRange', { from: g.epoch_min, to: g.epoch_max })} · {t('expires')} {when(g.expires_at)}</p>
              </div>
              {#if manages}
                <button class={buttonClass('secondary', 'sm')} onclick={() => onControl({ type: 'revoke_grant', grant_id: g.grant_id })}>{t('revoke')}</button>
              {/if}
            </li>
          {/each}
        </ul>
      </section>
    {/if}
    {#if error}<p class="text-[12.5px] text-danger">{error}</p>{/if}
  </div>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('primary')} onclick={() => (open = false)}>{t('done')}</button>
  {/snippet}
</Dialog>
