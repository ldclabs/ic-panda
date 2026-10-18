<script lang="ts">
  import Checkbox from '../../components/Checkbox.svelte'
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Select from '../../components/Select.svelte'
  import { toast } from '../../components/toast.svelte'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { call, errorText } from '../../rpc'

  /**
   * What the hosted Agent IDs may do with a folder and everything under it:
   * read its files, write new files or versions. The grants naming the
   * folder are listed with their revoke; others stay in Agents.
   */
  let { folder = $bindable(null), go }: { folder?: any; go: (path: string) => void } = $props()

  let agents = $state<any[]>([])
  let grants = $state<any[]>([])
  let agent = $state('')
  let read = $state(true)
  let write = $state(false)
  let busy = $state(false)

  const names = $derived(folder ? [`files.read:${folder.id}`, `files.write:${folder.id}`] : [])
  const here = $derived(grants.filter((g) => !g.revoked_at && g.capabilities.some((c: string) => names.includes(c))))
  const labelOf = (did: string) => agents.find((a) => a.agent_id === did)?.label ?? did

  const load = async () => {
    const [hosted, list] = await Promise.all([call<any>('agent/ids').catch(() => null), call<any[]>('grants/list').catch(() => [])])
    agents = hosted?.agents ?? []
    grants = list
    agent ||= agents[0]?.agent_id ?? ''
  }

  $effect(() => {
    if (!folder) return
    read = true
    write = false
    void load()
  })

  async function attempt(work: () => Promise<unknown>) {
    busy = true
    try {
      await work()
      await load()
    } catch (e) {
      toast(() => errorText(e))
    } finally {
      busy = false
    }
  }

  const grant = () =>
    attempt(async () => {
      const capabilities = [read && names[0], write && names[1]].filter(Boolean)
      await call('grants/add', { agent, capabilities })
      toast(() => t('accessGranted'))
    })
</script>

<Dialog bind:open={() => !!folder, (next) => !next && (folder = null)} title={t('grantAgentTitle', { name: folder?.name ?? '' })} icon="agents" testid="files-grant-dialog">
  {#if !agents.length}
    <Notice tone="info">{t('noAgents')}</Notice>
  {:else}
    <div class="grid gap-3">
      <div class="grid gap-1 text-[12px] text-faint">
        {t('grantee')}
        <Select bind:value={agent} options={agents.map((a) => ({ value: a.agent_id, label: a.label }))} label={t('grantee')} testid="files-grant-agent" class="w-full" />
      </div>
      <Checkbox bind:checked={read}>{t('grantRead')}</Checkbox>
      <Checkbox bind:checked={write}>{t('grantWrite')}</Checkbox>
      {#if here.length}
        <ul class="divide-y divide-line rounded-xl border border-line bg-surface" data-testid="files-grant-list">
          {#each here as g (g.id)}
            <li class="flex items-center gap-2 px-3 py-2">
              <div class="min-w-0 flex-1">
                <b class="block truncate text-[12.5px] font-medium">{labelOf(g.grantee)}</b>
                <div class="mt-0.5 flex flex-wrap gap-1">
                  {#each g.capabilities.filter((c: string) => names.includes(c)) as capability (capability)}
                    <Pill class="font-normal">{capability.startsWith('files.write') ? t('grantWriteShort') : t('grantReadShort')}</Pill>
                  {/each}
                </div>
              </div>
              <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => attempt(() => call('grants/revoke', { id: g.id }))}>{t('revoke')}</button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  {/if}
  {#snippet footer()}
    {#if !agents.length}
      <button class={buttonClass('ghost')} onclick={() => ((folder = null), go('/agents'))}><Icon name="agents" />{t('agents')}</button>
    {/if}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (folder = null)}>{t('cancel')}</button>
    <button class={buttonClass('primary')} disabled={busy || !agent || (!read && !write)} onclick={grant} data-testid="files-grant-confirm">{t('grant')}</button>
  {/snippet}
</Dialog>
