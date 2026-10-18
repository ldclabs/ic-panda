<script lang="ts">
  import { copyText } from '../../components/clipboard'
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Suggest from '../../components/Suggest.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { call, code } from '../../rpc'
  import { label } from './names.svelte'

  /**
   * Handing a channel's ownership or billing to another member, in steps:
   * the owner proposes and the packet goes into the channel, where the
   * member's device accepts it and the owner commits it, each with one
   * click on the packet in the timeline. The packet can also be copied and
   * settled under Transfer packet. A billing transfer pauses the channel's
   * writes until it completes or the owner cancels it.
   */
  let {
    open = $bindable(false),
    kind,
    channel,
    me,
    onSent
  }: { open?: boolean; kind: 'owner' | 'sponsor'; channel: any; me: string | undefined; onSent: () => void } = $props()

  let target = $state('')
  let packet = $state<any>(null)
  let busy = $state(false)
  let error = $state('')

  $effect(() => {
    if (!open) {
      target = ''
      packet = null
      error = ''
    }
  })

  async function propose() {
    busy = true
    error = ''
    try {
      packet = await call(`channel/${kind}_propose`, { id: channel.id, account: target.trim() })
      // The member's devices find it in the channel.
      await call('channel/send', { id: channel.id, packet, message_id: random() })
      onSent()
    } catch (e) {
      error = `${t('error')}: ${code(e)}`
    } finally {
      busy = false
    }
  }

  const random = () => Array.from(crypto.getRandomValues(new Uint8Array(32)), (b) => b.toString(16).padStart(2, '0')).join('')

  const steps = $derived([
    { label: t('stepPropose'), done: !!packet },
    { label: t('stepAccept'), done: false },
    { label: t('stepCommit'), done: false }
  ])
</script>

<Dialog bind:open title={kind === 'owner' ? t('transferOwner') : t('transferSponsor')} description={channel.name} icon="transfer" testid="transfer-dialog">
  <ol class="mb-4 grid grid-cols-3 gap-2">
    {#each steps as step, i (i)}
      {@const current = steps.findIndex((s) => !s.done) === i}
      <li class="grid gap-1.5">
        <span class={cn('h-1 rounded-full', step.done ? 'bg-forest' : current ? 'bg-forest/40' : 'bg-line')}></span>
        <span class={cn('flex items-center gap-1 text-[11.5px]', step.done || current ? 'text-ink-2' : 'text-faint')}>
          {#if step.done}<Icon name="check" class="size-3 text-forest" />{:else}<span class="font-mono">{i + 1}</span>{/if}
          {step.label}
        </span>
      </li>
    {/each}
  </ol>
  {#if !packet}
    <label class="grid gap-1 text-[12px] text-faint">
      {t('targetMember')}
      <Suggest
        bind:value={target}
        testid="transfer-target"
        class="font-mono text-[12px]"
        suggestions={Object.keys(channel.ledger?.members ?? {})
          .filter((account) => account !== me)
          .map((account) => ({ value: account, hint: label(account) }))}
      />
    </label>
    <p class="mt-3 text-[12.5px] text-muted">{t('transferInChannel')}</p>
    {#if kind === 'sponsor'}<Notice tone="warn" class="mt-3">{t('sponsorPauses')}</Notice>{/if}
  {:else}
    <div class="grid gap-2">
      <p class="text-[12.5px] text-ink-2">{t('transferSent')}</p>
      <textarea readonly class="min-h-24 font-mono text-[11px]" data-testid="channel-handed">{JSON.stringify(packet)}</textarea>
    </div>
  {/if}
  {#if error}<p class="mt-2 text-[12.5px] text-danger">{error}</p>{/if}
  {#snippet footer()}
    <span class="flex-1"></span>
    {#if packet}
      <button class={buttonClass('secondary')} onclick={() => copyText(JSON.stringify(packet))}><Icon name="copy" />{t('copy')}</button>
      <button class={buttonClass('primary')} onclick={() => (open = false)}>{t('done')}</button>
    {:else}
      <button class={buttonClass('ghost')} onclick={() => (open = false)}>{t('cancel')}</button>
      <button class={buttonClass('primary')} disabled={busy || !target.trim()} onclick={propose} data-testid={`${kind}-propose`}>{t('propose')}</button>
    {/if}
  {/snippet}
</Dialog>
