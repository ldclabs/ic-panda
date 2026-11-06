<script lang="ts">
  import { t } from '../i18n/index.svelte'
  import { bridge, errorText } from '../rpc'
  import Dialog from './Dialog.svelte'
  import { leaving } from './link-confirm.svelte'
  import { toast } from './toast.svelte'
  import { buttonClass } from './ui'

  /** Before a link from someone else opens in the browser: where it goes, in its real host name. */
  function close() {
    leaving.url = leaving.host = null
  }

  function go() {
    const url = leaving.url
    close()
    if (url) void bridge(() => window.dmsg.openUrl(url)).catch((error) => toast(() => errorText(error)))
  }
</script>

<Dialog bind:open={() => leaving.url !== null, (open) => !open && close()} title={t('openLinkTitle')} icon="external" testid="link-confirm">
  <p class="text-[15px] font-semibold [overflow-wrap:anywhere]" dir="ltr" data-testid="link-host">{leaving.host}</p>
  <p class="mt-1 text-[12px] text-muted [overflow-wrap:anywhere]" dir="ltr">{leaving.url}</p>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} onclick={close}>{t('cancel')}</button>
    <button class={buttonClass('primary')} onclick={go} data-testid="link-open">{t('openInBrowser')}</button>
  {/snippet}
</Dialog>
