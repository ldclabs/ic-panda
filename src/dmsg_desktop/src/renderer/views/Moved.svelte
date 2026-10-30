<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import Confirm from '../components/Confirm.svelte'
  import Icon from '../components/Icon.svelte'
  import Id from '../components/page/Id.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Section from '../components/page/Section.svelte'
  import { buttonClass } from '../components/ui'
  import { t } from '../i18n/index.svelte'
  import { bridge, call, errorText, onChange, size, when } from '../rpc'

  /**
   * A workspace copied here from another Mac never acts as the old device.
   * Setting it aside keeps it, read-only, for import and makes this Mac a
   * new device of the account; the copies kept stay listed until deleted.
   * Before this Mac joins, a copy of an administrator device can approve
   * this Mac in its place; once it has joined, a copy's files import.
   */
  let { status, refresh }: { status: any; refresh: () => Promise<void> } = $props()

  let copies = $state<any[]>([])
  let tasks = $state<any[]>([])
  let error = $state('')
  let notice = $state('')
  let busy = $state(false)
  let discarding = $state<any>(null)
  let confirming = $state(false)
  let alive = true
  onDestroy(() => { alive = false })

  async function load() {
    try {
      const value: any = await call('device/moved_list')
      if (alive) copies = value ?? []
      const list: any = status?.state === 'unlocked' ? await call('device/successions').catch(() => []) : []
      if (alive) tasks = list ?? []
    } catch (e) {
      if (alive) error = errorText(e)
    }
  }

  /** This account's copies can act for it: approve this Mac, or have their files imported. */
  const ours = (copy: any) => !!copy.account && copy.account === status?.account

  /** The handover replacing the copy's device whose content step is open, which the import settles. */
  const handover = (copy: any) =>
    tasks.find((task) => task.side === 'successor' && task.predecessor === copy.device_id && task.available.includes('content'))

  async function attempt(work: () => Promise<unknown>) {
    if (busy || !alive) return
    busy = true
    error = ''
    notice = ''
    try {
      await work()
    } catch (e) {
      if (alive) error = errorText(e)
    } finally {
      if (alive) {
        busy = false
        await refresh()
        await load()
      }
    }
  }

  const setAside = () => attempt(() => call('device/moved_set_aside'))

  function ask(copy: any) {
    discarding = copy
    confirming = true
  }

  const discard = () => attempt(() => call('device/moved_discard', { id: discarding.id }))

  const replace = (copy: any) =>
    attempt(async () => {
      const request: any = await call('device/moved_approve_successor', { id: copy.id })
      await bridge(() => window.dmsg.openApproval(request.id))
    })

  const importFiles = (copy: any) =>
    attempt(async () => {
      const report: any = await call('device/moved_import', { id: copy.id, ...(handover(copy) ? { succession: handover(copy).id } : {}) })
      if (alive) notice = t('movedImported', { records: report.records, size: size(report.bytes) })
    })

  onMount(() => {
    void load()
    return onChange(['session/changed', 'approval/changed'], () => void load())
  })
</script>

{#if status?.state === 'moved'}
  <Section title={t('movedTitle')} icon="warning" testid="moved">
    <div class="grid gap-3 px-5">
      <Notice tone="warn" icon="device">{t('movedBody')}</Notice>
      <p class="text-[12.5px] text-muted">{t('movedSetAsideHint')}</p>
      <p class="text-[12.5px] text-muted">{t('movedReplaceHint')}</p>
      <div>
        <button class={buttonClass('primary', 'sm')} disabled={busy} aria-busy={busy} onclick={setAside} data-testid="moved-set-aside">
          <Icon name="device" />{t('movedSetAside')}
        </button>
      </div>
    </div>
  </Section>
{/if}
{#if error}
  <Notice tone="danger" icon="warning" class="mx-5">{error}</Notice>
{/if}
{#if notice}
  <Notice tone="ok" icon="check" class="mx-5" testid="moved-notice">{notice}</Notice>
{/if}
{#if copies.length}
  <Section title={t('movedCopies')} description={t('movedCopiesHint')} icon="archive" flush testid="moved-copies">
    <ul class="divide-y divide-line">
      {#each copies as copy (copy.id)}
        <li class="flex flex-wrap items-center gap-2 px-5 py-3">
          {#if copy.account}<Id id={copy.account} />{/if}
          <Id id={copy.device_id} />
          <span class="min-w-0 flex-1 text-[12px] text-faint">{when(copy.moved_at)}</span>
          {#if ours(copy) && status?.state === 'pairing_required'}
            <button class={buttonClass('primary', 'xs')} disabled={busy} onclick={() => replace(copy)} data-testid="moved-replace">{t('movedReplace')}</button>
          {:else if ours(copy) && status?.state === 'unlocked'}
            <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => importFiles(copy)} data-testid="moved-import">{t('movedImport')}</button>
          {/if}
          <button class={buttonClass('danger', 'xs')} disabled={busy} onclick={() => ask(copy)} data-testid="moved-discard">{t('movedDelete')}</button>
        </li>
      {/each}
    </ul>
  </Section>
{/if}
<Confirm
  bind:open={confirming}
  title={t('movedDelete')}
  description={t('movedDeleteBody')}
  confirm={t('movedDelete')}
  icon="trash"
  {busy}
  onConfirm={discard}
  testid="moved-discard-dialog"
/>
