<script lang="ts">
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import { toast } from '../../components/toast.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { t, type Label } from '../../i18n/index.svelte'
  import { call, code, when } from '../../rpc'
  import { differences, lineDiff, type Compared } from './vault'

  /**
   * Conflict copies side by side with the current version, one at a time:
   * the fields that differ, the body line by line. Keeping one settles the
   * copy; the daemon records it as resolved either way.
   */
  let {
    open = $bindable(false),
    conflicts,
    start = 0,
    reload
  }: { open?: boolean; conflicts: any[]; start?: number; reload: () => Promise<void> } = $props()

  let index = $state(0)
  let head = $state<any>(null)
  let busy = $state(false)
  let error = $state('')

  /** The copy shown, kept within the list as settled copies leave it. */
  const at = $derived(Math.max(0, Math.min(index, conflicts.length - 1)))
  const copy = $derived(conflicts[at])
  const changed = $derived<Compared[]>(head && copy ? differences(head, copy) : [])
  const body = $derived(head && copy && changed.includes('body') ? lineDiff(head.item.body, copy.item.body) : null)

  const names: Record<Compared, Label> = { title: 'title', username: 'username', url: 'url', tags: 'tags', favorite: 'starred', body: 'body', deleted: 'inTrash' }

  $effect(() => {
    if (open) index = start
  })

  // The current version, deleted or not, as the daemon holds it.
  $effect(() => {
    const id = open ? copy?.id : null
    head = null
    error = ''
    if (!id) return
    call('vault/show', { id })
      .then((entry) => {
        if (copy?.id === id) head = entry
      })
      .catch((e) => (error = `${t('error')}: ${code(e)}`))
  })

  $effect(() => {
    if (open && !conflicts.length) open = false
  })

  async function resolve(keep: 'current' | 'theirs') {
    busy = true
    try {
      await call('vault/resolve', { key: `${copy.id}:${copy.revision}`, keep })
      toast(() => (keep === 'current' ? t('keptCurrent') : t('usedCopy')))
      await reload()
    } catch (e) {
      error = `${t('error')}: ${code(e)}`
    } finally {
      busy = false
    }
  }

  function value(entry: any, field: Compared): string {
    if (field === 'deleted') return entry.deleted ? t('inTrash') : '-'
    if (field === 'favorite') return entry.item.favorite ? t('starred') : '-'
    if (field === 'tags') return entry.item.tags.map((tag: string) => `#${tag}`).join(' ') || '-'
    return String(entry.item[field] || '-')
  }
</script>

{#snippet side(label: string, entry: any, tone: 'current' | 'copy')}
  <div class="min-w-0">
    <div class="mb-2 flex items-baseline justify-between gap-2 border-b border-line pb-1.5">
      <b class="text-[12.5px] font-semibold">{label}</b>
      <span class="text-[11.5px] text-faint">{when(entry.item.updatedAt)}</span>
    </div>
    {#each changed.filter((field) => field !== 'body') as field (field)}
      <div class="mb-2">
        <div class="text-[11.5px] text-faint">{t(names[field])}</div>
        <div class={cn('rounded-md px-1.5 py-0.5 text-[13px] [overflow-wrap:anywhere]', tone === 'current' ? 'bg-danger-pale' : 'bg-ok-pale')}>{value(entry, field)}</div>
      </div>
    {/each}
    {#if body}
      <div class="text-[11.5px] text-faint">{t('body')}</div>
      <ol class="mt-0.5 rounded-md border border-line bg-surface font-mono text-[12px] leading-relaxed">
        {#each tone === 'current' ? body.current : body.copy as line, i (i)}
          <li
            class={cn(
              'min-h-[1.6em] px-2 whitespace-pre-wrap [overflow-wrap:anywhere]',
              line.changed && (tone === 'current' ? 'bg-danger-pale text-danger line-through decoration-danger/40' : 'bg-ok-pale text-ok')
            )}
          >
            {line.text}
          </li>
        {/each}
      </ol>
    {/if}
  </div>
{/snippet}

{#if copy}
  <Dialog bind:open title={t('conflictTitle', { title: copy.item.title })} description={t('conflictDialogHint')} icon="conflict" tone="warn" wide testid="conflict-dialog">
    {#if head}
      {#if changed.length}
        <div class="grid gap-5 sm:grid-cols-2">
          {@render side(t('currentVersion'), head, 'current')}
          {@render side(t('theCopy'), copy, 'copy')}
        </div>
      {:else}
        <p class="text-[13px] text-muted">{t('hiddenOnly')}</p>
      {/if}
    {:else if !error}
      <p class="shimmer text-[13px]">{t('loading')}</p>
    {/if}
    {#if error}<p class="mt-2 text-[12.5px] text-danger">{error}</p>{/if}
    {#snippet footer()}
      {#if conflicts.length > 1}
        <span class="flex items-center gap-0.5">
          <button class={buttonClass('ghost', 'sm', 'size-7 px-0')} aria-label={t('previousOne')} disabled={at === 0} onclick={() => (index = at - 1)}>
            <Icon name="chevron-end" class="rotate-180" />
          </button>
          <span class="text-[12px] text-faint tabular-nums">{t('conflictOf', { index: at + 1, n: conflicts.length })}</span>
          <button class={buttonClass('ghost', 'sm', 'size-7 px-0')} aria-label={t('nextOne')} disabled={at >= conflicts.length - 1} onclick={() => (index = at + 1)}>
            <Icon name="chevron-end" />
          </button>
        </span>
      {/if}
      <span class="min-w-0 flex-1 truncate text-[12px] text-faint">
        {changed.length ? `${t('differs')}: ${changed.map((field) => t(names[field])).join(', ')}` : ''}
      </span>
      <button class={buttonClass()} disabled={busy || !head} onclick={() => resolve('current')} data-testid="conflict-keep">{t('keepCurrent')}</button>
      <button class={buttonClass()} disabled={busy || !head} onclick={() => resolve('theirs')} data-testid="conflict-use">{t('useCopy')}</button>
    {/snippet}
  </Dialog>
{/if}
