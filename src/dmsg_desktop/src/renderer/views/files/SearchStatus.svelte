<script lang="ts">
  import Notice from '../../components/page/Notice.svelte'
  import { buttonClass } from '../../components/ui'
  import { failureTone } from '../../failure'
  import { t } from '../../i18n/index.svelte'
  import { errorText } from '../../rpc'
  import type { searchLibrary } from './search.svelte'

  let { search, empty }: { search: ReturnType<typeof searchLibrary>; empty: string } = $props()
  const failure = $derived(search.result.kind === 'failed' ? errorText(search.result.cause) || t('loadFailed') : '')
</script>

{#if failure}
  <Notice tone={failureTone(failure)} testid="file-search-error">
    <div class="flex flex-wrap items-start gap-x-2 gap-y-1.5 text-start">
      <p class="min-w-0 flex-[1_1_12rem]">{failure}</p>
      <button class={buttonClass('secondary', 'xs')} onclick={search.retry}>{t('retry')}</button>
    </div>
  </Notice>
{:else}
  <p class="text-center text-[12.5px] text-muted [overflow-wrap:anywhere]" role="status">{search.result.kind === 'loading' ? t('loading') : empty}</p>
{/if}
