<script lang="ts">
  import Icon from '../components/Icon.svelte'
  import Tooltip from '../components/Tooltip.svelte'
  import { iconButtonClass } from '../components/ui'
  import { t } from '../i18n/index.svelte'

  /**
   * The buttons beside the traffic lights, as in anda-bot's title bar: the
   * sidebar toggle, then back and forward through the views this window
   * showed. The expanded sidebar holds them; collapsed, the title bar does,
   * at the same place, so a second click on the toggle undoes the first.
   */
  let {
    collapsed,
    canBack,
    canForward,
    keys,
    onToggle,
    onTravel
  }: {
    collapsed: boolean
    canBack: boolean
    canForward: boolean
    /** The shortcuts of back and forward, as the tooltips show them. */
    keys: { back: string; forward: string }
    onToggle: () => void
    onTravel: (step: -1 | 1) => void
  } = $props()

  const toggle = $derived(collapsed ? t('expandSidebar') : t('collapseSidebar'))
  // Plain icons in a row, as in anda-bot; an expanded sidebar is no open menu.
  const button = iconButtonClass('ghost', 'md', 'aria-expanded:bg-transparent')
</script>

{#snippet travel(step: -1 | 1, enabled: boolean)}
  {@const label = step < 0 ? t('back') : t('forward')}
  <Tooltip content={`${label} · ${step < 0 ? keys.back : keys.forward}`} side="bottom">
    {#snippet children(props)}
      <button
        {...props}
        class={button}
        aria-label={label}
        disabled={!enabled}
        onclick={() => onTravel(step)}
        data-testid={step < 0 ? 'nav-back' : 'nav-forward'}
      >
        <Icon name={step < 0 ? 'back' : 'forward'} />
      </button>
    {/snippet}
  </Tooltip>
{/snippet}

<div class="flex shrink-0 items-center gap-0.5 [-webkit-app-region:no-drag]">
  <Tooltip content={toggle} side="bottom">
    {#snippet children(props)}
      <button {...props} class={button} aria-label={toggle} aria-expanded={!collapsed} onclick={onToggle} data-testid="sidebar-toggle">
        <Icon name={collapsed ? 'sidebar-unfold' : 'sidebar-fold'} />
      </button>
    {/snippet}
  </Tooltip>
  {@render travel(-1, canBack)}
  {@render travel(1, canForward)}
</div>
