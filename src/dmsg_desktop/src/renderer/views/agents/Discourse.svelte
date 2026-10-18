<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import Select from '../../components/Select.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { ago, call, code, errorText, when } from '../../rpc'

  /**
   * What a hosted Agent ID's connector holds of Agent Discourse, read only:
   * its rooms, a room's state, members and latest records, the drafts it
   * holds for a decision and its pending inbox. Everything comes from the
   * connector's local state (`ap/read`): nothing is fetched from a host,
   * nothing is marked read or presented, so the agent's own view stays as
   * it was. Its next sync brings what is new. Room content comes from
   * others and is shown as text.
   */
  let { agents, active }: { agents: any[]; active: string | null } = $props()

  let agent = $state('')
  let rooms = $state<any[] | null>(null)
  let drafts = $state<any[]>([])
  let inbox = $state<any[]>([])
  let room = $state('')
  let state_ = $state<any>(null)
  let members = $state<any[]>([])
  let timeline = $state<any[]>([])
  let error = $state('')
  let busy = $state(false)

  $effect(() => {
    if (!agent && agents.length) {
      agent = active ?? agents[0].agent_id
      void load()
    }
  })

  const read = async (uri: string) => ((await call('ap/read', { agent, uri })) as any).value

  async function attempt(work: () => Promise<void>) {
    busy = true
    error = ''
    try {
      await work()
    } catch (e) {
      error = code(e) === 'LOCKED' ? t('agentLocked') : errorText(e)
    } finally {
      busy = false
    }
  }

  const load = () =>
    attempt(async () => {
      const [listed, held, pending] = await Promise.all([
        read('agent-protocols://rooms'),
        read('agent-protocols://drafts/held'),
        read('agent-protocols://inbox/pending')
      ])
      rooms = listed.rooms ?? []
      drafts = held.drafts ?? []
      inbox = pending.items ?? []
      if (room && !rooms!.some((r) => r.room_id === room)) room = ''
      if (room) await open(room)
    })

  async function open(id: string) {
    room = id
    const base = `agent-protocols://rooms/${id}`
    const [shown, listed, recent] = await Promise.all([read(base), read(`${base}/members`), read(`${base}/timeline`)])
    state_ = shown.room
    members = listed.members ?? []
    timeline = [...(recent.items ?? [])].reverse()
  }

  const choose = (id: string) => attempt(() => open(id))
  const labelOf = (did?: string) => (did ? (agents.find((a) => a.agent_id === did)?.label ?? did) : '-')
</script>

{#if !agents.length}
  <Notice>{t('noAgents')}</Notice>
{:else}
  <Section title={t('discourse')} description={t('discourseHint')} icon="members" testid="agent-discourse">
    {#snippet actions()}
      <Select
        bind:value={agent}
        onChange={() => ((room = ''), (rooms = null), void load())}
        testid="discourse-agent"
        options={agents.map((a) => ({ value: a.agent_id, label: a.label }))}
      />
      <button class={buttonClass('secondary', 'md')} disabled={busy || !agent} onclick={load} data-testid="discourse-reload"><Icon name="sync-now" />{t('reload')}</button>
    {/snippet}
    {#if error}<Notice tone="danger" class="mb-3" testid="discourse-error">{error}</Notice>{/if}
    <div class="grid gap-4 lg:grid-cols-[minmax(220px,300px)_1fr]">
      <div class="grid content-start gap-3">
        <div>
          <h3 class="mb-1.5 text-[12px] font-medium text-faint">{t('rooms')}</h3>
          <ul class="grid gap-1" data-testid="discourse-rooms">
            {#each rooms ?? [] as r (`${r.host} ${r.room_id}`)}
              <li>
                <button
                  class={cn('grid w-full gap-0.5 rounded-lg border px-3 py-2 text-start', room === r.room_id ? 'border-forest bg-pale' : 'border-line hover:bg-surface-2')}
                  onclick={() => choose(r.room_id)}
                >
                  <span class="flex items-center gap-1.5">
                    <b class="min-w-0 flex-1 truncate text-[13px] font-medium">{r.topic ?? r.room_id}</b>
                    {#if r.unread_count}<Pill tone="forest">{r.unread_count}</Pill>{/if}
                  </span>
                  <span class="truncate font-mono text-[11px] text-faint">{r.host} · {r.status}{r.role ? ` · ${r.role}` : ''}</span>
                </button>
              </li>
            {:else}
              <li class="text-[12.5px] text-faint">{rooms ? t('noRooms') : t('loading')}</li>
            {/each}
          </ul>
        </div>
        <div>
          <h3 class="mb-1.5 text-[12px] font-medium text-faint">{t('heldDrafts')}</h3>
          <ul class="grid gap-1" data-testid="discourse-drafts">
            {#each drafts as d (d.id)}
              <li class="rounded-lg border border-warn/40 bg-warn/5 px-3 py-2 text-[12px]">
                <p class="font-medium">{d.kind} · <span class="font-mono">{d.room_id}</span></p>
                <p class="mt-0.5 text-muted">{d.reason}</p>
                <p class="mt-0.5 text-[11px] text-faint">{ago(d.created_at)}</p>
              </li>
            {:else}
              <li class="text-[12.5px] text-faint">{t('noHeldDrafts')}</li>
            {/each}
          </ul>
        </div>
        <div>
          <h3 class="mb-1.5 text-[12px] font-medium text-faint">{t('pendingInbox')}</h3>
          <ul class="grid gap-1" data-testid="discourse-inbox">
            {#each inbox as item (item.id)}
              <li class="rounded-lg border border-line px-3 py-2 text-[12px]">
                <p class="flex items-center gap-1.5"><Pill>{item.kind}</Pill>{#if item.requires_response}<Pill tone="warn">{t('needsResponse')}</Pill>{/if}</p>
                <p class="mt-0.5 text-muted">{item.reason}</p>
                <p class="mt-0.5 text-[11px] text-faint">{labelOf(item.actor)} · {ago(item.created_at)}</p>
              </li>
            {:else}
              <li class="text-[12.5px] text-faint">{t('inboxEmpty')}</li>
            {/each}
          </ul>
        </div>
      </div>
      <div class="min-w-0">
        {#if state_}
          <div class="grid gap-3" data-testid="discourse-room">
            <div class="rounded-xl border border-line p-3">
              <div class="flex flex-wrap items-center gap-1.5">
                <b class="text-[14px] font-semibold">{state_.topic ?? state_.room_id}</b>
                <Pill>{state_.status}</Pill>
                {#if state_.visibility}<Pill>{state_.visibility}</Pill>{/if}
              </div>
              <p class="mt-0.5 font-mono text-[11.5px] text-faint">{state_.host} · {state_.room_id}</p>
              {#if state_.agenda}<p class="mt-1.5 text-[12.5px] whitespace-pre-wrap text-muted">{state_.agenda}</p>{/if}
            </div>
            <div>
              <h3 class="mb-1.5 text-[12px] font-medium text-faint">{t('members')} · {members.length}</h3>
              <ul class="flex flex-wrap gap-1.5" data-testid="discourse-members">
                {#each members as m (m.agent_id)}
                  <li class="inline-flex items-center gap-1.5 rounded-lg border border-line px-2 py-1 text-[12px]">
                    <span class={cn(m.status === 'active' ? 'text-ink' : 'text-faint')}>{m.profile?.name ?? labelOf(m.agent_id)}</span>
                    <span class="text-[11px] text-faint">{m.role}{m.is_creator ? ' · ★' : ''}</span>
                  </li>
                {/each}
              </ul>
            </div>
            <div>
              <h3 class="mb-1.5 text-[12px] font-medium text-faint">{t('latestRecords')}</h3>
              <ol class="divide-y divide-line rounded-xl border border-line" data-testid="discourse-timeline">
                {#each timeline as item (item.seq)}
                  <li class="grid gap-0.5 px-3 py-2">
                    <div class="flex flex-wrap items-center gap-1.5 text-[11.5px]">
                      <span class="font-mono text-faint">#{item.seq}</span>
                      <span class="font-medium">{labelOf(item.actor)}</span>
                      <Pill class="font-mono font-normal">{item.type}</Pill>
                      <time class="ms-auto text-faint" title={when(item.accepted_at)}>{ago(item.accepted_at)}</time>
                    </div>
                    <p class="text-[12.5px] whitespace-pre-wrap [overflow-wrap:anywhere]">{item.summary}</p>
                  </li>
                {:else}
                  <li class="px-3 py-2 text-[12.5px] text-faint">{t('noRecords')}</li>
                {/each}
              </ol>
            </div>
          </div>
        {:else}
          <p class="text-[12.5px] text-faint">{t('pickRoom')}</p>
        {/if}
        <p class="mt-3 text-[11.5px] text-faint">{t('discourseLocal')}</p>
      </div>
    </div>
  </Section>
{/if}
