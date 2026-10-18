<script lang="ts">
  import { t } from '../i18n/index.svelte'
  import Icon from './Icon.svelte'
  import { cn } from './ui'

  /**
   * Tags as chips: Enter, a comma or leaving the field adds what was typed,
   * Backspace in an empty field takes the last one off. Duplicates,
   * blanks and tags longer than `maxLength` are dropped; an item may already carry a tag twice, so chips go
   * by position.
   */
  let {
    tags = $bindable<string[]>([]),
    placeholder = '',
    max = 20,
    maxLength,
    testid,
    class: className
  }: { tags?: string[]; placeholder?: string; max?: number; maxLength?: number; testid?: string; class?: string } = $props()

  let text = $state('')
  let input = $state<HTMLInputElement | null>(null)

  function commit() {
    const added = new Set(
      text
        .split(',')
        .map((tag) => tag.trim())
        .filter((tag) => tag && !tags.includes(tag) && (!maxLength || [...tag].length <= maxLength))
    )
    if (added.size) tags = [...tags, ...added].slice(0, max)
    text = ''
  }

  function onKey(event: KeyboardEvent) {
    if (event.key === 'Enter' || event.key === ',') {
      if (!text.trim()) return
      event.preventDefault()
      commit()
    } else if (event.key === 'Backspace' && !text && tags.length) {
      tags = tags.slice(0, -1)
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div
  class={cn(
    'flex min-h-8 w-full flex-wrap items-center gap-1 rounded-lg border border-edge bg-surface px-1.5 py-1 transition-[border-color,box-shadow] duration-150 focus-within:border-forest focus-within:ring-3 focus-within:ring-forest/18',
    className
  )}
  onclick={() => input?.focus()}
  data-testid={testid}
>
  {#each tags as tag, i (i)}
    <span class="inline-flex h-6 max-w-full items-center gap-0.5 rounded-md bg-wash ps-2 pe-0.5 text-xs text-ink-2">
      <span class="truncate">{tag}</span>
      <button
        type="button"
        class="grid size-5 shrink-0 place-items-center rounded text-faint outline-none hover:bg-paper-dim hover:text-ink focus-visible:ring-2 focus-visible:ring-focus/35"
        aria-label={`${t('remove')} ${tag}`}
        onclick={(event) => {
          event.stopPropagation()
          tags = tags.filter((_, j) => j !== i)
        }}
      >
        <Icon name="close" class="size-3" />
      </button>
    </span>
  {/each}
  <input
    bind:this={input}
    bind:value={text}
    class="h-6 min-w-24 flex-1 border-0 bg-transparent px-1 py-0 text-[13px] shadow-none focus-visible:shadow-none focus-visible:ring-0"
    placeholder={tags.length ? '' : placeholder}
    disabled={tags.length >= max}
    maxlength={maxLength}
    onkeydown={onKey}
    onblur={commit}
  />
</div>
