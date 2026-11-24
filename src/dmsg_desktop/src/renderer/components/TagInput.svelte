<script lang="ts">
  import { flip } from 'svelte/animate'
  import { t } from '../i18n/index.svelte'
  import Icon from './Icon.svelte'
  import { flipDuration, pop, shrink } from './motion'
  import { cn } from './ui'

  /**
   * Tags as chips: Enter, a comma or leaving the field adds what was typed,
   * Backspace in an empty field takes the last one off. Duplicates,
   * blanks and tags longer than `maxLength` are dropped. A new chip pops in;
   * a removed one shrinks away and the rest move up. An item may already
   * carry a tag twice, so a chip goes by its tag and which copy it is.
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
  const chips = $derived.by(() => {
    const seen = new Map<string, number>()
    return tags.map((tag, i) => {
      const n = (seen.get(tag) ?? 0) + 1
      seen.set(tag, n)
      return { tag, i, key: `${tag}#${n}` }
    })
  })
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
    if (event.isComposing) return
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
    'flex min-h-8 w-full flex-wrap items-center gap-1 rounded-lg border border-field-edge bg-surface shadow-(--field-inset) hover:not-focus-within:border-field-edge-hover px-1.5 py-1 transition-[border-color,box-shadow] duration-150 focus-within:border-forest focus-within:ring-3 focus-within:ring-forest/18',
    className
  )}
  onclick={() => input?.focus()}
  data-testid={testid}
>
  {#each chips as { tag, i, key } (key)}
    <span class="inline-flex h-6 max-w-full items-center gap-0.5 rounded-md bg-wash ps-2 pe-0.5 text-callout text-ink-2" animate:flip={{ duration: flipDuration(0) }} in:pop out:shrink>
      <span class="truncate">{tag}</span>
      <button
        type="button"
        class="grid size-6 shrink-0 place-items-center rounded text-faint outline-none hover:bg-paper-dim hover:text-ink focus-visible:ring-2 focus-visible:ring-focus/35"
        aria-label={`${t('remove')} ${tag}`}
        onclick={(event) => {
          event.stopPropagation()
          tags = tags.filter((_, j) => j !== i)
        }}
      >
        <Icon name="close" class="size-3.5" />
      </button>
    </span>
  {/each}
  <input
    bind:this={input}
    bind:value={text}
    aria-label={t('tags')}
    class="h-6 min-w-24 flex-1 border-0 bg-transparent px-1 py-0 text-body shadow-none focus-visible:shadow-none focus-visible:ring-0"
    placeholder={tags.length ? '' : placeholder}
    disabled={tags.length >= max}
    onkeydown={onKey}
    onblur={commit}
  />
</div>
