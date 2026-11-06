<script lang="ts" generics="T">
  import { ContextMenu } from 'bits-ui'
  import type { Snippet } from 'svelte'
  import { localeDir } from '../../shared/locales'
  import { locale } from '../i18n/index.svelte'
  import { cn, menuContentClass } from './ui'

  /**
   * One right-click menu for a whole list instead of one per row: `find` names
   * the row under the pointer. Off every row the system's own text menu stays.
   */
  let {
    find,
    children,
    menu,
    testid
  }: { find: (element: Element) => T | null; children: Snippet; menu: Snippet<[T]>; testid?: string } = $props()

  let target = $state<T | null>(null)
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="contents" oncontextmenucapture={(event) => (target = event.target instanceof Element ? find(event.target) : null)}>
  <ContextMenu.Root dir={localeDir(locale())}>
    <ContextMenu.Trigger disabled={target === null} class="contents">
      {@render children()}
    </ContextMenu.Trigger>
    <ContextMenu.Portal>
      <ContextMenu.Content class={cn(menuContentClass, 'min-w-48')} data-testid={testid}>
        {#if target !== null}{@render menu(target)}{/if}
      </ContextMenu.Content>
    </ContextMenu.Portal>
  </ContextMenu.Root>
</div>
