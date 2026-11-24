<script lang="ts">
  import type { MarkdownInline } from '../../shared/contract'
  import { t } from '../i18n/index.svelte'
  import { copyText } from './clipboard'
  import Icon from './Icon.svelte'
  import { followLink } from './link-confirm.svelte'
  import MarkdownInlines from './MarkdownInlines.svelte'

  let { nodes }: { nodes: MarkdownInline[] } = $props()

  function openLink(event: MouseEvent, url: string) {
    event.preventDefault()
    event.stopPropagation()
    if (/^mailto:/i.test(url)) {
      void copyText(url.slice(7))
      return
    }
    try {
      const parsed = new URL(url)
      followLink(url, parsed.host)
    } catch { /* validated upstream */ }
  }
</script>

{#each nodes as node, idx (idx)}{#if node.type === 'text'}{node.text}{:else if node.type === 'strong'}<strong class="font-semibold"><MarkdownInlines nodes={node.children} /></strong>{:else if node.type === 'emphasis'}<em class="italic"><MarkdownInlines nodes={node.children} /></em>{:else if node.type === 'strikethrough'}<del class="line-through opacity-75"><MarkdownInlines nodes={node.children} /></del>{:else if node.type === 'code'}<code class="rounded bg-surface-2 px-1 py-0.5 font-mono text-code text-ink" dir="ltr">{node.code}</code>{:else if node.type === 'link'}<a href={node.url} class="text-forest underline underline-offset-2 hover:no-underline" onclick={(event) => openLink(event, node.url)} data-testid="markdown-link"><MarkdownInlines nodes={node.children} /></a>{:else if node.type === 'blocked_image'}<span class="inline-flex items-center gap-1 rounded-md border border-line bg-surface-2 px-1.5 py-0.5 align-baseline text-caption text-muted" data-testid="markdown-blocked-image"><Icon name="image" class="size-3.5 shrink-0 text-faint" /><span>{node.alt ? `${t('previewExternalImageBlocked')}: ${node.alt}` : t('previewExternalImageBlocked')}</span></span>{:else if node.type === 'hard_break'}<br />{:else if node.type === 'soft_break'}{' '}{/if}{/each}
