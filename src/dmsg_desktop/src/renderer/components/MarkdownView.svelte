<script lang="ts">
  import type { MarkdownBlock, MarkdownDocument, TableAlignment } from '../../shared/contract'
  import { t } from '../i18n/index.svelte'
  import CopyButton from './CopyButton.svelte'
  import Icon from './Icon.svelte'
  import MarkdownInlines from './MarkdownInlines.svelte'
  import Notice from './page/Notice.svelte'
  import { buttonClass, cn } from './ui'

  let { document }: { document: MarkdownDocument } = $props()

  function alignClass(alignment: TableAlignment | undefined): string {
    if (alignment === 'center') return 'text-center'
    if (alignment === 'right') return 'text-end'
    return 'text-start'
  }
</script>

{#snippet taskBox(checked: boolean)}
  <span
    class={cn(
      'mt-1 grid size-4 shrink-0 place-items-center rounded border',
      checked ? 'border-forest bg-forest text-on-forest' : 'border-edge bg-surface'
    )}
    aria-hidden="true"
  >
    {#if checked}<Icon name="check" class="size-3" />{/if}
  </span>
{/snippet}

{#snippet renderBlocks(blocks: MarkdownBlock[])}
  {#each blocks as block, idx (idx)}
    {#if block.type === 'heading'}
      {#if block.level === 1}
        <h1 class="mt-6 mb-3 border-b border-line pb-1.5 text-title-1 font-semibold text-ink first:mt-0"><MarkdownInlines nodes={block.inlines} /></h1>
      {:else if block.level === 2}
        <h2 class="mt-5 mb-2.5 border-b border-line pb-1 text-title-2 font-semibold text-ink first:mt-0"><MarkdownInlines nodes={block.inlines} /></h2>
      {:else if block.level === 3}
        <h3 class="mt-4 mb-2 text-title-3 font-semibold text-ink first:mt-0"><MarkdownInlines nodes={block.inlines} /></h3>
      {:else}
        <h4 class="mt-3.5 mb-1.5 text-body font-semibold text-ink first:mt-0"><MarkdownInlines nodes={block.inlines} /></h4>
      {/if}
    {:else if block.type === 'paragraph'}
      <p class="my-2.5 text-body leading-relaxed text-ink first:mt-0 last:mb-0"><MarkdownInlines nodes={block.inlines} /></p>
    {:else if block.type === 'code_block'}
      <div class="my-3 overflow-hidden rounded-xl border border-line bg-surface-2" dir="ltr">
        <div class="flex items-center justify-between gap-2 border-b border-line px-3 py-1.5">
          <span class="font-mono text-caption text-faint">{block.language || 'text'}</span>
          <CopyButton class={buttonClass('ghost', 'xs')} value={() => block.code} aria-label={t('copy')} />
        </div>
        <pre class="overflow-x-auto px-3.5 py-3 font-mono text-code leading-relaxed text-ink select-text"><code>{block.code}</code></pre>
      </div>
    {:else if block.type === 'block_quote'}
      <blockquote class="my-3 border-s-3 border-forest/50 bg-wash/60 py-1.5 ps-3.5 pe-2 text-ink-2">
        {@render renderBlocks(block.blocks)}
      </blockquote>
    {:else if block.type === 'list'}
      {#if block.ordered}
        <ol start={block.start} class="my-2.5 list-decimal space-y-1.5 ps-6 text-body text-ink">
          {#each block.items as item, itemIdx (itemIdx)}
            <li class="ps-0.5">
              {#if item.checked !== undefined}
                <div class="flex items-start gap-2">
                  {@render taskBox(item.checked)}
                  <div class="min-w-0 flex-1">{@render renderBlocks(item.blocks)}</div>
                </div>
              {:else}
                {@render renderBlocks(item.blocks)}
              {/if}
            </li>
          {/each}
        </ol>
      {:else}
        {@const hasTasks = block.items.some((item) => item.checked !== undefined)}
        <ul class={cn('my-2.5 space-y-1.5 text-body text-ink', hasTasks ? 'list-none ps-1' : 'list-disc ps-6')}>
          {#each block.items as item, itemIdx (itemIdx)}
            <li class={cn(item.checked !== undefined ? 'flex items-start gap-2' : 'ps-0.5')}>
              {#if item.checked !== undefined}
                {@render taskBox(item.checked)}
                <div class="min-w-0 flex-1">{@render renderBlocks(item.blocks)}</div>
              {:else}
                {@render renderBlocks(item.blocks)}
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    {:else if block.type === 'table'}
      <div class="my-3 overflow-x-auto rounded-xl border border-line">
        <table class="w-full border-collapse text-body text-ink">
          {#if block.headers.length}
            <thead class="bg-surface-2">
              <tr>
                {#each block.headers as cell, colIdx (colIdx)}
                  <th class={cn('border-b border-line px-3 py-2 text-callout font-semibold text-ink', alignClass(block.alignments[colIdx]))}>
                    <MarkdownInlines nodes={cell} />
                  </th>
                {/each}
              </tr>
            </thead>
          {/if}
          <tbody class="divide-y divide-line">
            {#each block.rows as row, rowIdx (rowIdx)}
              <tr>
                {#each row as cell, colIdx (colIdx)}
                  <td class={cn('px-3 py-2 text-body text-ink-2', alignClass(block.alignments[colIdx]))}>
                    <MarkdownInlines nodes={cell} />
                  </td>
                {/each}
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else if block.type === 'thematic_break'}
      <hr class="my-5 border-t border-line" />
    {/if}
  {/each}
{/snippet}

<article class="select-text [overflow-wrap:anywhere]" dir="auto" data-testid="markdown-view">
  {@render renderBlocks(document.blocks)}
  {#if document.truncated}
    <Notice tone="info" class="mt-4">{t('previewTruncated')}</Notice>
  {/if}
</article>
