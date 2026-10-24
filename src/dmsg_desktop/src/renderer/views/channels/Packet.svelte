<script lang="ts">
  import Notice from '../../components/page/Notice.svelte'
  import { copyText } from '../../components/clipboard'
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import { toast } from '../../components/toast.svelte'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { call, CallError, errorText } from '../../rpc'

  /**
   * A transfer packet another device handed over: a proposal to this
   * account is accepted here and goes back to the owner; an accepted one
   * is committed here by the owner who proposed it. The daemon's
   * `channel/changed` reloads the channels.
   */
  let { open = $bindable(false) }: { open?: boolean } = $props()

  let packet = $state('')
  let handed = $state('')
  let busy = $state(false)
  let error = $state('')

  $effect(() => {
    if (!open) {
      packet = ''
      handed = ''
      error = ''
    }
  })

  async function settle() {
    busy = true
    error = ''
    try {
      let parsed: any
      try {
        parsed = JSON.parse(packet)
      } catch {
        throw new CallError('INVALID_INPUT: packet')
      }
      const owner = parsed?.format === 'dmsg-channel-owner-transfer/1'
      if (!owner && parsed?.format !== 'dmsg-channel-sponsor/1') throw new CallError('INVALID_INPUT: packet')
      const step = parsed.acceptance ? 'commit' : 'accept'
      const result = await call(`channel/${owner ? 'owner' : 'sponsor'}_${step}`, { packet: parsed })
      if (step === 'accept') {
        handed = JSON.stringify(result)
        toast(() => t('packetAccepted'))
      } else {
        toast(() => t('packetCommitted'))
        open = false
      }
    } catch (e) {
      error = errorText(e)
    } finally {
      busy = false
    }
  }
</script>

<Dialog bind:open title={t('transferPacket')} description={t('transferPacketHint')} icon="transfer" testid="packet-dialog">
  {#if handed}
    <div class="grid gap-2">
      <p class="text-[12.5px] text-ink-2">{t('packetAccepted')}</p>
      <textarea readonly class="min-h-28 font-mono text-[11px]" data-testid="channel-handed">{handed}</textarea>
    </div>
  {:else}
    <textarea bind:value={packet} class="min-h-36 font-mono text-[11px]" placeholder={t('transferPacket')} spellcheck="false" data-testid="channel-packet"></textarea>
  {/if}
  {#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
  {#snippet footer()}
    <span class="flex-1"></span>
    {#if handed}
      <button class={buttonClass('secondary')} onclick={() => copyText(handed)}><Icon name="copy" />{t('copy')}</button>
      <button class={buttonClass('primary')} onclick={() => (open = false)}>{t('done')}</button>
    {:else}
      <button class={buttonClass('ghost')} onclick={() => (open = false)}>{t('cancel')}</button>
      <button class={buttonClass('primary')} disabled={busy || !packet.trim()} onclick={settle} data-testid="channel-packet-settle">{t('settlePacket')}</button>
    {/if}
  {/snippet}
</Dialog>
