<script lang="ts">
  import { onDestroy } from 'svelte'
  import Checkbox from '../../components/Checkbox.svelte'
  import Dialog from '../../components/Dialog.svelte'
  import { toast } from '../../components/toast.svelte'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { bridge, call, errorText } from '../../rpc'

  /**
   * A letter to an account. A free one is delivered at once; a paid one
   * comes back quoted and is paid only once the approval window agrees.
   * The daemon's `inbox/changed` reloads the letters.
   */
  let { open = $bindable(false), mod }: { open?: boolean; mod: string } = $props()

  let recipient = $state('')
  let text = $state('')
  let pay = $state(false)
  let busy = $state(false)
  let error = $state('')
  let generation = 0
  let pending: { key: string; order: string } | null = null

  $effect(() => {
    if (!open) { generation += 1; error = '' }
  })

  onDestroy(() => { generation += 1 })

  async function send() {
    if (busy || !recipient.trim() || !text.trim()) return
    const at = generation
    busy = true
    error = ''
    try {
      const to = recipient.trim()
      const key = JSON.stringify([to, text, pay])
      if (pending?.key !== key) pending = { key, order: Array.from(crypto.getRandomValues(new Uint8Array(32)), (n) => n.toString(16).padStart(2, '0')).join('') }
      const sent: any = await call('inbox/send', { recipient: to, text, pay, order: pending.order })
      if (at !== generation || !open) return
      if (sent.state === 'quoted') {
        const request: any = await call('inbox/pay', { recipient: to, order: sent.order_id })
        if (at !== generation || !open) return
        await bridge(() => window.dmsg.openApproval(request.id))
        toast(() => t('letterQuoted'), { testid: 'letter-state' })
      } else {
        toast(() => t('letterSent'), { testid: 'letter-state' })
      }
      if (at !== generation || !open) return
      pending = null
      recipient = ''
      text = ''
      pay = false
      open = false
    } catch (e) {
      if (at === generation && open) error = errorText(e)
    } finally {
      busy = false
    }
  }

  function onKey(event: KeyboardEvent) {
    if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
      event.preventDefault()
      void send()
    }
  }
</script>

<Dialog bind:open title={t('writeLetter')} description={t('letterHint')} icon="mail" testid="letter-dialog">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="grid gap-3" onkeydown={onKey}>
    <label class="grid gap-1 text-[12px] text-faint">
      {t('to')}
      <input disabled={busy} bind:value={recipient} class="font-mono text-[12.5px] text-ink placeholder:font-sans" placeholder={t('accountId')} spellcheck="false" data-testid="letter-recipient" />
    </label>
    <label class="grid gap-1 text-[12px] text-faint">
      {t('message')}
      <textarea disabled={busy} bind:value={text} rows="7" class="text-[13.5px] text-ink" data-testid="letter-text"></textarea>
    </label>
    <Checkbox disabled={busy} bind:checked={pay} class="text-[12.5px] text-ink-2">{t('paidLetterHint')}</Checkbox>
    {#if error}<p class="text-[12.5px] text-danger">{error}</p>{/if}
  </div>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (open = false)}>{t('cancel')}</button>
    <button class={buttonClass('primary')} disabled={busy || !recipient.trim() || !text.trim()} onclick={send} data-testid="letter-send">
      {t('send')}<kbd class="font-mono text-[10.5px] opacity-75">{mod}↵</kbd>
    </button>
  {/snippet}
</Dialog>
