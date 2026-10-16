<script lang="ts">
  import { copyText } from '../../components/clipboard'
  import Icon from '../../components/Icon.svelte'
  import Field from '../../components/page/Field.svelte'
  import Id from '../../components/page/Id.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { call } from '../../rpc'

  /**
   * The Agent IDs this device hosts: which one acts for the desktop, which
   * keep their key resident while locked, and the MCP credential, shown
   * once. The hosts they may reach are listed, never edited here.
   */
  let {
    hosted,
    unlocked,
    busy,
    run,
    attempt,
    reload
  }: {
    hosted: any
    unlocked: boolean
    busy: boolean
    run: (method: string, params?: Record<string, unknown>) => Promise<void>
    attempt: (work: () => Promise<unknown>) => Promise<void>
    reload: () => Promise<void>
  } = $props()

  let label = $state('')
  let issued = $state<{ agent_id: string; credential: string } | null>(null)

  const issue = (agent: string) =>
    attempt(async () => {
      issued = await call('agent/credential', { agent })
      await reload()
    })

  async function create() {
    await run('agent/create', { label: label.trim() })
    label = ''
  }
</script>

<Section title={t('agentIds')} description={t('agentIdsHint')} icon="agents" flush>
  <ul class="divide-y divide-line border-t border-line" data-testid="agent-ids">
    {#each hosted.agents as a (a.agent_id)}
      <li class="px-5 py-3.5">
        <div class="flex items-start gap-3">
          <span class="grid size-9 shrink-0 place-items-center rounded-xl bg-pale text-forest"><Icon name="agents" /></span>
          <div class="min-w-0 flex-1">
            <div class="flex flex-wrap items-center gap-1.5">
              <b class="text-[13.5px] font-semibold">{a.label}</b>
              {#if hosted.active === a.agent_id}<Pill tone="forest">{t('active')}</Pill>{/if}
              {#if a.resident}<Pill tone="gem"><Icon name="background" />{t('resident')}</Pill>{/if}
            </div>
            <div class="mt-1"><Id id={a.agent_id} /></div>
          </div>
          <div class="flex shrink-0 flex-wrap justify-end gap-1.5">
            {#if hosted.active !== a.agent_id}
              <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => run('agent/use', { agent: a.agent_id })}>{t('use')}</button>
            {/if}
            <button class={buttonClass('secondary', 'sm')} disabled={busy || !unlocked} onclick={() => run('agent/resident', { agent: a.agent_id, on: !a.resident })}>
              {a.resident ? t('residentOff') : t('residentOn')}
            </button>
            <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => issue(a.agent_id)}>{t('issueCredential')}</button>
          </div>
        </div>
        {#if issued?.agent_id === a.agent_id}
          <Notice tone="warn" class="mt-3">
            <p>{t('credentialOnce')}</p>
            <pre class="mt-1.5 rounded-lg bg-surface px-2.5 py-2 font-mono text-[11.5px] break-all whitespace-pre-wrap text-ink select-all" data-testid="agent-credential">{issued?.credential}</pre>
            {#snippet action()}
              <button class={buttonClass('secondary', 'xs')} onclick={() => issued && copyText(issued.credential)}><Icon name="copy" />{t('copy')}</button>
            {/snippet}
          </Notice>
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
      <li class="inline-flex items-center gap-1.5 rounded-lg border border-line bg-surface-2 px-2 py-1 text-[12px]">
        <Icon name="lock" class="size-3 text-faint" />
        <bdi dir="ltr" class="font-mono">{h.host}</bdi>
        {#if h.label}<span class="text-faint">· {h.label}</span>{/if}
      </li>
    {:else}
      <li class="text-[13px] text-faint">-</li>
    {/each}
  </ul>
</Section>
