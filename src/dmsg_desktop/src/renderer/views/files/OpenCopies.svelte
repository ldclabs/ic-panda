<script lang="ts">
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import { toast } from '../../components/toast.svelte'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { ago, call, errorText } from '../../rpc'

  /**
   * The plaintext copies handed to other programs and not yet removed: each
   * goes now, a changed one kept as a new version first or let go.
   */
  let { open = $bindable(false) }: { open?: boolean } = $props()

  let copies = $state<any[]>([])
  let busy = $state(false)

  const load = async () => {
    copies = await call<any[]>('files/open_list').catch(() => [])
  }

  $effect(() => {
    if (open) void load()
  })

  async function settle(nonce: string, save: boolean) {
    busy = true
    try {
      await call('files/open_cleanup', { nonce, save })
      await load()
    } catch (e) {
      toast(() => errorText(e))
    } finally {
      busy = false
    }
  }
</script>

<Dialog bind:open title={t('openCopiesTitle')} icon="external" testid="files-open-copies">
  <Notice tone="info">{t('openCopiesHint')}</Notice>
  {#if copies.length}
    <ul class="mt-3 divide-y divide-line rounded-xl border border-line bg-surface">
      {#each copies as copy (copy.nonce)}
        <li class="flex items-center gap-2.5 px-3 py-2">
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-1.5">
              <span class="truncate text-[13px] font-medium">{copy.name}</span>
              {#if copy.changed}<Pill tone="warn">{t('copyChangedBadge')}</Pill>{/if}
            </div>
            <p class="truncate font-mono text-[11px] text-faint" dir="ltr" title={copy.path}>{copy.path}</p>
            <p class="text-[11px] text-faint">{t('openedAgo', { time: ago(copy.opened_at) })}</p>
          </div>
          {#if copy.changed}
            <button class={buttonClass('ghost', 'xs')} disabled={busy} onclick={() => settle(copy.nonce, false)}>{t('discardChange')}</button>
            <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => settle(copy.nonce, true)}><Icon name="history" />{t('saveAsVersion')}</button>
          {:else}
            <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => settle(copy.nonce, false)} data-testid="files-copy-remove"><Icon name="trash" />{t('removeCopy')}</button>
          {/if}
        </li>
      {/each}
    </ul>
  {:else}
    <p class="mt-3 py-6 text-center text-[13px] text-muted">{t('noOpenCopies')}</p>
  {/if}
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('primary')} onclick={() => (open = false)}>{t('done')}</button>
  {/snippet}
</Dialog>
