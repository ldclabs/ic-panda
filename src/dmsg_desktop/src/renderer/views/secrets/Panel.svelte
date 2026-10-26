<script lang="ts">
  import type { Snippet } from 'svelte'
  import type { Panel } from './workflow-state.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { ux } from './experience-text'
  import { workflowText as w } from './workflow-text'
  let {
    model,
    title,
    description,
    children,
    actions
  }: {
    model: Panel
    title: string
    description?: string
    children: Snippet
    actions?: Snippet
  } = $props()
</script>

<section class="min-w-0 space-y-4" aria-busy={model.state.loading}>
  <div class="flex flex-wrap items-start gap-3">
    <div class="min-w-0 flex-1">
      <h2 class="text-base font-semibold">{title}</h2>
      {#if description}<p
          class="mt-1 max-w-2xl text-[13px] leading-relaxed text-muted"
        >
          {description}
        </p>{/if}
    </div>
    <button
      class={buttonClass('secondary')}
      disabled={model.state.loading || model.state.busy}
      onclick={() => model.refresh()}
      >{model.state.loading ? t('loading') : w('refresh')}</button
    >{#if actions}{@render actions()}{/if}
  </div>
  {#each Object.entries(model.state.errors) as [key, error]}<Notice
      tone="danger"
      failure={error}
      testid={`workflow-error-${key}`}
      >{error}{#snippet action()}{#if key !== 'action'}<button
            class={buttonClass('secondary', 'sm')}
            disabled={model.state.busy}
            onclick={() => model.retry(key)}>{ux('retry')}</button
          >{/if}{/snippet}</Notice
    >{/each}
  {@render children()}
</section>
