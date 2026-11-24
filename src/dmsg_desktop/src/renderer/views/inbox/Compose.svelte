<script lang="ts">
  import Notice from '../../components/page/Notice.svelte'
  import { onDestroy, untrack } from 'svelte'
  import Checkbox from '../../components/Checkbox.svelte'
  import Dialog from '../../components/Dialog.svelte'
  import TextArea from '../../components/TextArea.svelte'
  import TextField from '../../components/TextField.svelte'
  import { toast } from '../../components/toast.svelte'
  import { buttonClass } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { bridge, call, code, errorText } from '../../rpc'
  import { rememberedPlain, rememberPlain } from '../../view-memory'
  import { accountOf } from '../channels/names.svelte'
  import { bytes } from '../channels/channels'

  /**
   * A letter to an account, named by its ID or its dMsg name. A free one is
   * delivered at once; a paid one comes back quoted and is paid only once the
   * approval window agrees. The daemon's `inbox/changed` reloads the letters.
   * `to` fills the recipient of a reply; what was being written stays until it
   * is sent or the account locks.
   */
  let { open = $bindable(false), to = '', mod }: { open?: boolean; to?: string; mod: string } = $props()

  type Pending = { key: string; order: string; account: string } | null
  const replyRecipient = untrack(() => to)
  const draftKey = replyRecipient ? `inbox.reply.${replyRecipient}` : 'inbox.compose'
  const draft = rememberedPlain<{ recipient: string; text: string; pay: boolean; pending: Pending }>(draftKey)
  let recipient = $state(replyRecipient || (draft?.recipient ?? ''))
  let text = $state(draft?.text ?? '')
  let pay = $state(draft?.pay ?? false)
  let busy = $state(false)
  let error = $state('')
  let generation = 0
  let pending = $state.raw<Pending>(draft?.pending ?? null)
  const maxBytes = 6000
  const used = $derived(bytes(text))
  const counter = $derived(`${used.toLocaleString(locale())} / ${new Intl.NumberFormat(locale(), { style: 'unit', unit: 'byte' }).format(maxBytes)} (UTF-8)`)
  const canSend = $derived(!busy && !!recipient.trim() && !!text.trim() && used <= maxBytes)

  $effect(() => {
    if (!open) { generation += 1; error = '' }
  })

  $effect(() => {
    void recipient; void text; void pay
    error = ''
  })

  $effect(() => rememberPlain(draftKey, { recipient, text, pay, pending }))
  onDestroy(() => { generation += 1 })

  async function send() {
    if (!canSend) return
    const at = generation
    busy = true
    error = ''
    let stage: 'recipient' | 'send' | 'payment' = 'recipient'
    try {
      const key = JSON.stringify([recipient.trim().replace(/^@/, '').toLowerCase(), text, pay])
      // Keep the resolved account after an uncertain send. A transferred
      // handle must not redirect a retry to a different person.
      const account = pending?.key === key ? pending.account : await accountOf(recipient)
      if (at !== generation || !open) return
      if (pending?.key !== key) pending = { key, account, order: Array.from(crypto.getRandomValues(new Uint8Array(32)), (n) => n.toString(16).padStart(2, '0')).join('') }
      stage = 'send'
      const sent: any = await call('inbox/send', { recipient: account, text, pay, order: pending.order })
      if (at !== generation || !open) return
      if (sent.state === 'quoted') {
        stage = 'payment'
        const request: any = await call('inbox/pay', { recipient: account, order: sent.order_id })
        if (at !== generation || !open) return
        await bridge(() => window.dmsg.openApproval(request.id))
        toast(() => t('letterQuoted'), { testid: 'letter-state' })
      } else {
        toast(() => t('letterSent'), { testid: 'letter-state' })
      }
      if (at !== generation || !open) return
      pending = null
      recipient = replyRecipient
      text = ''
      pay = false
      open = false
    } catch (e) {
      if (at === generation && open) {
        error = stage === 'recipient' && code(e) === 'NOT_FOUND' ? t('letterHandleNotFound')
          : stage === 'recipient' && code(e) === 'INVALID_INPUT' ? t('letterRecipientInvalid')
          : stage === 'send' && code(e) === 'NOT_FOUND' ? t('letterReceivingNotFound')
          : stage === 'send' && code(e) === 'FORBIDDEN' ? t('letterContactNotAllowed')
          : code(e) === 'VERSION_CONFLICT' ? t('letterPendingContact')
          : code(e) === 'REKEY_REQUIRED' ? t('letterRecipientKeyStale')
          : code(e) === 'POLICY_STALE' ? t('letterPolicyStale')
          : errorText(e)
      }
    } finally {
      busy = false
    }
  }

  function onKey(event: KeyboardEvent) {
    if (event.isComposing) return
    if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
      event.preventDefault()
      void send()
    }
  }
</script>

<Dialog bind:open title={t('writeLetter')} description={t('letterHint')} icon="mail" testid="letter-dialog">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="grid gap-3" onkeydown={onKey}>
    <TextField label={t('to')} hint={replyRecipient ? undefined : t('accountOrName')} dir="ltr" readonly={!!replyRecipient} disabled={busy} bind:value={recipient} placeholder="panda / @panda" spellcheck={false} data-testid="letter-recipient" />
    <TextArea label={t('message')} hint={used >= maxBytes * 0.8 && used <= maxBytes ? counter : undefined} error={used > maxBytes ? counter : undefined} disabled={busy} bind:value={text} rows={7} data-testid="letter-text" />
    <Checkbox disabled={busy} bind:checked={pay} class="text-callout text-ink-2">{t('paidLetterHint')}</Checkbox>
    {#if error}<Notice failure={error} tone="danger" testid="letter-send-error">{error}</Notice>{/if}
  </div>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (open = false)}>{t('cancel')}</button>
    <button class={buttonClass('primary')} disabled={!canSend} onclick={send} data-testid="letter-send">
      {t('send')}<kbd class="font-mono text-caption opacity-75">{mod}↵</kbd>
    </button>
  {/snippet}
</Dialog>
