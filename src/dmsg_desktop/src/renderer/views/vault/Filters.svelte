<script lang="ts">
  import type { Glyph } from '../../components/glyphs'
  import Icon from '../../components/Icon.svelte'
  import { cn } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { typeIcon, typeLabel, types, type Facets, type Scope } from './vault'

  /** The vault's filters: everything, starred, each type and tag that has items, and the trash. */
  let { scope = $bindable<Scope>('all'), facets }: { scope?: Scope; facets: Facets } = $props()
</script>

{#snippet entry(value: Scope, icon: Glyph | null, label: string, count?: number)}
  <button
    class={cn(
      'flex h-8 w-full min-w-0 items-center gap-2 rounded-lg px-2 text-start text-[13px] outline-none transition-colors duration-100 focus-visible:ring-3 focus-visible:ring-focus/35',
      scope === value ? 'bg-pale font-medium text-forest' : 'text-ink-2 hover:bg-wash hover:text-ink'
    )}
    aria-current={scope === value ? 'true' : undefined}
    onclick={() => (scope = value)}
    data-testid={`vault-scope-${value}`}
  >
    {#if icon}
      <Icon name={icon} class={cn('shrink-0', scope === value ? 'text-forest' : 'text-faint')} />
    {:else}
      <span class="w-4 shrink-0 text-center font-mono text-[13px] text-faint">#</span>
    {/if}
    <span class="min-w-0 flex-1 truncate">{label}</span>
    {#if count !== undefined}<span class="text-[11.5px] text-faint tabular-nums">{count}</span>{/if}
  </button>
{/snippet}

<nav class="flex min-h-0 flex-1 flex-col gap-px overflow-y-auto overflow-x-hidden p-2.5" aria-label={t('vaultFilters')}>
  {@render entry('all', 'all', t('allItems'), facets.all)}
  {@render entry('starred', 'star', t('starred'), facets.starred)}
  <div class="px-2 pt-3 pb-1 text-[11px] font-medium text-faint">{t('types')}</div>
  {#each types as type (type)}
    {#if facets.types[type]}{@render entry(`type:${type}`, typeIcon[type], t(typeLabel[type]), facets.types[type])}{/if}
  {/each}
  {#if facets.tags.length}
    <div class="px-2 pt-3 pb-1 text-[11px] font-medium text-faint">{t('tags')}</div>
    {#each facets.tags as [tag, count] (tag)}
      {@render entry(`tag:${tag}`, null, tag, count)}
    {/each}
  {/if}
  <div class="mt-auto pt-3">
    {@render entry('trash', 'trash', t('trash'))}
  </div>
</nav>
