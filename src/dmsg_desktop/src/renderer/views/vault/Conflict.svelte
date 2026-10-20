<script lang="ts">
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import { toast } from '../../components/toast.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { t, type Label } from '../../i18n/index.svelte'
  import { call, errorText, when } from '../../rpc'
  import SecretField from './SecretField.svelte'
  import { differences, fieldName, isHidden, lineDiff, type Field, type Part } from './vault'

  /**
   * Conflict copies beside the current version, one at a time: each part
   * that differs, the notes line by line. Keep one side whole, or pick a
   * side part by part (and edit the merged notes) and save the merge as
   * the new current version. Hidden values that differ say so; both are
   * shown only on request. The daemon records the copy as resolved either
   * way, and its `vault/changed` reloads the list.
   */
  let { open = $bindable(false), conflicts, start = 0 }: { open?: boolean; conflicts: any[]; start?: number } = $props()

  let index = $state(0)
  let busy = $state(false)
  let error = $state('')
  let picks = $state<Record<string, 'current' | 'theirs'>>({})
  let notes = $state<string | null>(null)

  /** The copy shown, kept within the list as settled copies leave it. */
  const at = $derived(Math.max(0, Math.min(index, conflicts.length - 1)))
  const conflict = $derived(conflicts[at])
  const head = $derived(conflict?.head)
  const copy = $derived(conflict?.copy)
  const parts = $derived<Part[]>(conflict ? differences(head, copy, conflict.hidden) : [])
  const mergeable = $derived(!!conflict && !head.deleted && !copy.deleted)
  const body = $derived(parts.includes('notes') ? lineDiff(head.item.notes, copy.item.notes) : null)

  const names: Record<string, Label> = { title: 'title', tags: 'tags', notes: 'notes', favorite: 'starred', files: 'attachments', deleted: 'inTrash' }

  $effect(() => {
    if (open) index = start
  })

  // Each copy starts from the current version, part by part.
  $effect(() => {
    void conflict?.copy.revision
    picks = {}
    notes = null
    error = ''
  })

  $effect(() => {
    if (open && !conflicts.length) open = false
  })

  const fieldOf = (entry: any, part: string): Field | undefined => entry.item.fields.find((f: Field) => `field:${f.id}` === part)

  function label(part: Part): string {
    if (part.startsWith('field:')) {
      const field = fieldOf(head, part) ?? fieldOf(copy, part)
      return field ? fieldName(field, t) : part.slice(6)
    }
    return t(names[part])
  }

  function value(entry: any, part: Part): string {
    if (part === 'deleted') return entry.deleted ? t('inTrash') : '-'
    if (part === 'favorite') return entry.item.favorite ? t('starred') : '-'
    if (part === 'tags') return entry.item.tags.map((tag: string) => `#${tag}`).join(' ') || '-'
    if (part === 'files') return t('fileCount', { n: entry.item.files?.length ?? 0 })
    if (part.startsWith('field:')) {
      const field = fieldOf(entry, part)
      return field ? field.value || '-' : t('noSuchField')
    }
    return String(entry.item[part] || '-')
  }

  async function resolve(keep: 'current' | 'theirs' | 'merge') {
    busy = true
    try {
      const params: Record<string, unknown> = { key: `${copy.id}:${copy.revision}`, keep }
      if (keep === 'merge') {
        params.picks = picks
        if (notes !== null) params.notes = notes
      }
      await call('vault/resolve', params)
      toast(() => (keep === 'current' ? t('keptCurrent') : keep === 'theirs' ? t('usedCopy') : t('merged')))
    } catch (e) {
      error = errorText(e)
    } finally {
      busy = false
    }
  }

  function choose(part: Part, side: 'current' | 'theirs') {
    picks = { ...picks, [part]: side }
    if (part === 'notes') notes = null
  }
</script>

{#snippet side(part: Part, entry: any, tone: 'current' | 'theirs')}
  {@const picked = (picks[part] ?? 'current') === tone}
  {@const field = part.startsWith('field:') ? fieldOf(entry, part) : undefined}
  <div class={cn('min-w-0 rounded-lg border px-2.5 py-1.5 text-[12.5px] transition-colors', picked ? 'border-forest bg-pale' : 'border-line bg-surface')}>
    {#if mergeable}
      <button
        class={cn('mb-1 flex items-center gap-1.5 text-[11.5px] outline-none focus-visible:underline', picked ? 'text-forest' : 'text-faint hover:text-ink')}
        onclick={() => choose(part, tone)}
        aria-pressed={picked}
        data-testid={`conflict-pick-${tone}`}
        data-part={part}
      >
        <span class={cn('grid size-3.5 place-items-center rounded-full border', picked ? 'border-forest' : 'border-edge')}>
          {#if picked}<span class="size-1.5 rounded-full bg-forest"></span>{/if}
        </span>
        {t('takeThis')}
      </button>
    {/if}
    {#if field && isHidden(field) && field.value}
      {#if field.type === 'concealed'}
        <SecretField id={entry.id} field={field.id} revision={entry.revision} unlocked copyable={false} />
      {:else}
        <span class="font-mono text-faint">••••••</span>
      {/if}
    {:else if part === 'notes' && body}
      <ol class="font-mono text-[12px] leading-relaxed">
        {#each tone === 'current' ? body.current : body.copy as line, i (i)}
          <li
            class={cn(
              'min-h-[1.6em] px-1 whitespace-pre-wrap [overflow-wrap:anywhere]',
              line.changed && (tone === 'current' ? 'bg-danger-pale text-danger line-through decoration-danger/40' : 'bg-ok-pale text-ok')
            )}
          >
            {line.text}
          </li>
        {/each}
      </ol>
    {:else}
      <span class="block whitespace-pre-wrap select-text [overflow-wrap:anywhere]">{value(entry, part)}</span>
    {/if}
  </div>
{/snippet}

{#if conflict}
  <Dialog bind:open title={t('conflictTitle', { title: copy.item.title })} description={t('conflictDialogHint')} icon="conflict" tone="warn" wide testid="conflict-dialog">
    {#if parts.length}
      <div class="grid gap-3">
        <div class="grid grid-cols-[1fr_1fr] gap-3 border-b border-line pb-1.5 text-[12px]">
          <span><b class="font-semibold">{t('currentVersion')}</b> <span class="text-faint">{when(head.item.updatedAt)}</span></span>
          <span><b class="font-semibold">{t('theCopy')}</b> <span class="text-faint">{when(copy.item.updatedAt)}</span></span>
        </div>
        {#each parts as part (part)}
          <div class="grid gap-1" data-testid="conflict-part" data-part={part}>
            <div class="flex items-center gap-1.5 text-[11.5px] text-faint">
              {label(part)}
              {#if conflict.hidden.includes(part.slice(6))}<span class="rounded bg-warn-pale px-1 text-[10.5px] text-warn">{t('hiddenDiffers')}</span>{/if}
            </div>
            <div class="grid grid-cols-[1fr_1fr] gap-3">
              {@render side(part, head, 'current')}
              {@render side(part, copy, 'theirs')}
            </div>
            {#if part === 'notes' && mergeable}
              {#if notes === null}
                <button class={buttonClass('ghost', 'xs', 'justify-self-start')} onclick={() => (notes = picks.notes === 'theirs' ? copy.item.notes : head.item.notes)} data-testid="conflict-edit-notes">
                  <Icon name="edit" />{t('editMergedNotes')}
                </button>
              {:else}
                <textarea bind:value={notes} rows="5" class="text-[13px] text-ink" data-testid="conflict-notes"></textarea>
              {/if}
            {/if}
          </div>
        {/each}
      </div>
    {:else}
      <p class="text-[13px] text-muted">{t('sameContent')}</p>
    {/if}
    {#if !mergeable}<p class="mt-3 text-[12.5px] text-muted">{t('mergeDeleted')}</p>{/if}
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
      <span class="min-w-0 flex-1"></span>
      <button class={buttonClass()} disabled={busy} onclick={() => resolve('current')} data-testid="conflict-keep">{t('keepCurrent')}</button>
      <button class={buttonClass()} disabled={busy} onclick={() => resolve('theirs')} data-testid="conflict-use">{t('useCopy')}</button>
      {#if mergeable && parts.length}
        <button class={buttonClass('primary')} disabled={busy} onclick={() => resolve('merge')} data-testid="conflict-merge">{t('saveMerge')}</button>
      {/if}
    {/snippet}
  </Dialog>
{/if}
