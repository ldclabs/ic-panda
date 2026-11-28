<script lang="ts">
  import type { Glyph } from '../../components/glyphs'
  import Icon from '../../components/Icon.svelte'
  import { cn } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { typeIcon, typeLabel, types, workflowIcon, type Facets, type Scope } from './vault'

  /**
   * The vault's filters: everything, starred, what the security check
   * flags, each type and tag that has items, one entry for what secrets are
   * used for (whose parts are tabs on its page), and the trash. The chosen
   * tag opens what this account's Agent IDs may do with its items.
   */
  let {
    scope,
    workflows,
    workflow,
    onScope,
    onWorkflow,
    facets,
    flagged,
    onAccess,
    onNearbyReceive
  }: {
    scope: Scope
    workflows: string[]
    workflow: string | null
    onScope: (next: Scope) => void
    onWorkflow: () => void
    facets: Facets
    flagged: number
    onAccess?: (tag: string) => void
    onNearbyReceive?: () => void
  } = $props()
</script>

{#snippet entry(value: Scope, icon: Glyph | null, label: string, count?: number)}
  {@render row(!workflow && scope === value, () => onScope(value), `vault-scope-${value}`, icon, label, count)}
{/snippet}

{#snippet row(on: boolean, onclick: () => void, testid: string, icon: Glyph | null, label: string, count?: number)}
  <button
    class={cn(
      'flex h-8 w-full min-w-0 items-center gap-2 rounded-lg px-2 text-start text-body outline-none transition-colors duration-100 focus-visible:ring-3 focus-visible:ring-focus/35',
      on ? 'bg-pale font-medium text-forest' : 'text-ink-2 hover:bg-wash hover:text-ink'
    )}
    aria-current={on ? 'true' : undefined}
    {onclick}
    data-testid={testid}
  >
    {#if icon}
      <Icon name={icon} class={cn('shrink-0', on ? 'text-forest' : 'text-faint')} />
    {:else}
      <span class="w-4 shrink-0 text-center font-mono text-body text-faint">#</span>
    {/if}
    <span class="min-w-0 flex-1 truncate">{label}</span>
    {#if count !== undefined}<span class="text-caption text-faint tabular-nums">{count}</span>{/if}
  </button>
{/snippet}

<nav data-list class="flex min-h-0 flex-1 flex-col gap-px overflow-y-auto overflow-x-hidden p-2.5" aria-label={t('vaultFilters')}>
  {@render entry('all', 'all', t('allItems'), facets.all)}
  {@render entry('starred', 'star', t('starred'), facets.starred)}
  {@render entry('health', 'health', t('securityCheck'), flagged)}
  <div class="px-2 pt-3 pb-1 text-caption font-medium text-faint">{t('types')}</div>
  {#each types as type (type)}
    {#if facets.types[type]}{@render entry(`type:${type}`, typeIcon[type], t(typeLabel[type]), facets.types[type])}{/if}
  {/each}
  {#if facets.tags.length}
    <div class="px-2 pt-3 pb-1 text-caption font-medium text-faint">{t('tags')}</div>
    {#each facets.tags as [tag, count] (tag)}
      {@render entry(`tag:${tag}`, null, tag, count)}
      {#if onAccess && !workflow && scope === `tag:${tag}` && !tag.startsWith('dmsg-')}
        <button
          class="ms-6 flex h-7 items-center gap-1.5 rounded-md px-2 text-callout text-muted hover:bg-wash hover:text-ink"
          onclick={() => onAccess?.(tag)}
          data-testid="vault-tag-access"><Icon name="agents" class="size-3.5" />{t('agentAccessButton')}</button
        >
      {/if}
    {/each}
  {/if}
  <div class="h-3"></div>
  {#if workflows.length}{@render row(!!workflow, onWorkflow, 'vault-workflows', workflowIcon.policies, t('secretUses'))}{/if}
  <div class="mt-auto grid gap-px pt-3">
    {#if onNearbyReceive}
      {@render row(false, onNearbyReceive, 'vault-sidebar-nearby-receive', 'pair', t('nearbyReceive'))}
    {/if}
    {@render entry('trash', 'trash', t('trash'))}
  </div>
</nav>
