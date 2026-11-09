<script lang="ts">
  import Notice from '../../components/page/Notice.svelte'
  import { onDestroy } from 'svelte'
  import Checkbox from '../../components/Checkbox.svelte'
  import Dialog from '../../components/Dialog.svelte'
  import TextArea from '../../components/TextArea.svelte'
  import TextField from '../../components/TextField.svelte'
  import { toast } from '../../components/toast.svelte'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { bridge, call, code, errorText } from '../../rpc'
  import { rememberedPlain, rememberPlain } from '../../view-memory'
  import { accountOf } from '../channels/names.svelte'

  /**
   * A letter to an account, named by its ID or its dMsg name. A free one is
   * delivered at once; a paid one comes back quoted and is paid only once the
   * approval window agrees. The daemon's `inbox/changed` reloads the letters.
   * `to` fills the recipient of a reply; what was being written stays until it
   * is sent or the account locks.
   */
  let { open = $bindable(false), to = '', mod }: { open?: boolean; to?: string; mod: string } = $props()

  type Pending = { key: string; order: string } | null
  const draft = rememberedPlain<{ recipient: string; text: string; pay: boolean; pending: Pending }>('inbox.compose')
  let recipient = $state(draft?.recipient ?? '')
  let text = $state(draft?.text ?? '')
  let pay = $state(draft?.pay ?? false)
  let busy = $state(false)
  let error = $state('')
  let generation = 0
  let pending: Pending = draft?.pending ?? null

  $effect(() => {
    if (!open) { generation += 1; error = '' }
    else if (to) recipient = to
  })

  onDestroy(() => {
    generation += 1
    rememberPlain('inbox.compose', { recipient, text, pay, pending })
  })

  async function send() {
    if (busy || !recipient.trim() || !text.trim()) return
    const at = generation
    busy = true
    error = ''
    try {
      const account = await accountOf(recipient)
      if (at !== generation || !open) return
      const key = JSON.stringify([account, text, pay])
      if (pending?.key !== key) pending = { key, order: Array.from(crypto.getRandomValues(new Uint8Array(32)), (n) => n.toString(16).padStart(2, '0')).join('') }
      const sent: any = await call('inbox/send', { recipient: account, text, pay, order: pending.order })
      if (at !== generation || !open) return
      if (sent.state === 'quoted') {
        const request: any = await call('inbox/pay', { recipient: account, order: sent.order_id })
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
      if (at === generation && open) {
        error = code(e) === 'VERSION_CONFLICT' ? t('letterPendingContact')
          : code(e) === 'REKEY_REQUIRED' ? t('letterRecipientKeyStale')
          : code(e) === 'POLICY_STALE' ? t('letterPolicyStale')
          : errorText(e)
      }
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
    <TextField label={t('to')} disabled={busy} bind:value={recipient} placeholder={t('accountOrName')} spellcheck={false} data-testid="letter-recipient" />
    <TextArea label={t('message')} disabled={busy} bind:value={text} rows={7} data-testid="letter-text" />
    <Checkbox disabled={busy} bind:checked={pay} class="text-[12.5px] text-ink-2">{t('paidLetterHint')}</Checkbox>
    {#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
  </div>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (open = false)}>{t('cancel')}</button>
    <button class={buttonClass('primary')} disabled={busy || !recipient.trim() || !text.trim()} onclick={send} data-testid="letter-send">
      {t('send')}<kbd class="font-mono text-[10.5px] opacity-75">{mod}↵</kbd>
    </button>
  {/snippet}
</Dialog>
