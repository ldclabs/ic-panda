<script lang="ts">
  import { RadioGroup, Tabs } from 'bits-ui'
  import { untrack } from 'svelte'
  import { copyText } from '../../components/clipboard'
  import Dialog from '../../components/Dialog.svelte'
  import type { Glyph } from '../../components/glyphs'
  import Icon from '../../components/Icon.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { call, CallError, errorText } from '../../rpc'
  import { invitationLink, readInvitation } from './channels'
  import { accountOf } from './names.svelte'

  /**
   * A new channel of a kind, or joining one with an invitation another
   * member gave, pasted as its link or JSON or opened from a `dmsg://` link.
   * A conversation with one person names them: once the channel exists they
   * are invited, and the dialog shows the link to hand them.
   */
  let {
    open = $bindable(false),
    tab = $bindable<'create' | 'join'>('create'),
    invitation = $bindable(''),
    peer = $bindable<{ account: string; name?: string } | null>(null),
    onOpened
  }: {
    open?: boolean
    tab?: 'create' | 'join'
    invitation?: string
    peer?: { account: string; name?: string } | null
    onOpened: (id: string) => void
  } = $props()

  let name = $state('')
  let kind = $state('collaboration')
  let target = $state('')
  let busy = $state(false)
  let error = $state('')
  /** The invitation of a new conversation, to hand to its other person. */
  let made = $state<any>(null)

  const kinds: { value: string; icon: Glyph }[] = [
    { value: 'collaboration', icon: 'members' },
    { value: 'distribution', icon: 'send' },
    { value: 'direct', icon: 'user' }
  ]

  // A conversation started from a profile or a letter names its person as
  // the dialog opens; what is typed afterwards stays.
  $effect(() => {
    if (!open || !peer) return
    const { account, name: called } = peer
    untrack(() => {
      kind = 'direct'
      target = account
      name ||= called ?? ''
      tab = 'create'
    })
  })

  $effect(() => {
    if (!open) {
      name = ''
      target = ''
      invitation = ''
      error = ''
      made = null
      peer = null
    }
  })

  async function submit() {
    busy = true
    error = ''
    try {
      if (tab === 'create') {
        const account = kind === 'direct' ? await accountOf(target) : null
        const created: any = await call('channel/create', { name: name.trim(), type: kind })
        onOpened(created.id)
        if (account) {
          made = await call('channel/invite', { id: created.id, account })
          return
        }
      } else {
        const parsed = readInvitation(invitation)
        if (!parsed) throw new CallError('INVALID_INPUT: invitation')
        const ledger: any = await call('channel/join', { invitation: parsed })
        onOpened(ledger.channel_id)
      }
      open = false
    } catch (e) {
      error = errorText(e)
    } finally {
      busy = false
    }
  }

  const tabClass =
    'h-7 flex-1 rounded-lg px-3 text-[12.5px] text-muted outline-none transition-colors duration-150 hover:text-ink focus-visible:ring-3 focus-visible:ring-focus/35 data-[state=active]:bg-surface data-[state=active]:font-medium data-[state=active]:text-ink'
</script>

<Dialog bind:open title={tab === 'create' ? t('newChannel') : t('joinChannel')} icon="channels" testid="channel-new-dialog">
  {#if made}
    <div class="grid gap-2" data-testid="channel-invitation-box">
      <p class="text-[12.5px] text-ink-2">{t('directInvited')}</p>
      <input readonly value={invitationLink(made)} class="font-mono text-[11px]" data-testid="channel-invitation-link" />
      <button class={buttonClass('secondary', 'sm', 'justify-self-start')} onclick={() => copyText(invitationLink(made))}><Icon name="link" />{t('copyLink')}</button>
    </div>
  {:else}
    <Tabs.Root bind:value={() => tab, (next) => (tab = next as 'create' | 'join')}>
      <Tabs.List class="mb-4 flex gap-0.5 rounded-[10px] border border-line bg-surface-2 p-0.5">
        <Tabs.Trigger value="create" class={tabClass}>{t('newChannel')}</Tabs.Trigger>
        <Tabs.Trigger value="join" class={tabClass} data-testid="channel-join-tab">{t('joinChannel')}</Tabs.Trigger>
      </Tabs.List>
      <Tabs.Content value="create" class="grid gap-3 outline-none">
        <label class="grid gap-1 text-[12px] text-faint">
          {t('channelName')}
          <input bind:value={name} class="text-[13.5px] text-ink" data-testid="channel-name" />
        </label>
        <RadioGroup.Root bind:value={kind} class="grid grid-cols-3 gap-1.5" aria-label={t('type')}>
          {#each kinds as option (option.value)}
            <RadioGroup.Item
              value={option.value}
              data-testid="channel-kind-{option.value}"
              class="flex flex-col items-center gap-1 rounded-xl border border-line bg-surface px-2 py-3 text-[12px] text-muted outline-none transition-colors duration-150 hover:border-edge-soft hover:text-ink focus-visible:ring-3 focus-visible:ring-focus/35 data-[state=checked]:border-forest data-[state=checked]:bg-pale data-[state=checked]:text-forest"
            >
              <Icon name={option.icon} class="size-5" />
              {t(option.value as 'collaboration' | 'distribution' | 'direct')}
            </RadioGroup.Item>
          {/each}
        </RadioGroup.Root>
        {#if kind === 'direct'}
          <label class="grid gap-1 text-[12px] text-faint">
            {t('directWith')}
            <input bind:value={target} placeholder={t('accountOrName')} class="font-mono text-[12.5px] text-ink placeholder:font-sans" spellcheck="false" data-testid="channel-peer" />
          </label>
        {/if}
      </Tabs.Content>
      <Tabs.Content value="join" class="grid gap-2 outline-none">
        <textarea bind:value={invitation} class="min-h-32 font-mono text-[11px]" placeholder={t('invitation')} spellcheck="false" data-testid="channel-join-text"></textarea>
      </Tabs.Content>
    </Tabs.Root>
  {/if}
  {#if error}<p class="mt-2 text-[12.5px] text-danger">{error}</p>{/if}
  {#snippet footer()}
    <span class="flex-1"></span>
    {#if made}
      <button class={buttonClass('primary')} onclick={() => (open = false)}>{t('done')}</button>
    {:else}
      <button class={buttonClass('ghost')} onclick={() => (open = false)}>{t('cancel')}</button>
      <button
        class={cn(buttonClass('primary'))}
        disabled={busy || (tab === 'create' ? !name.trim() || (kind === 'direct' && !target.trim()) : !invitation.trim())}
        onclick={submit}
        data-testid="channel-create"
      >
        {tab === 'create' ? t('create') : t('join')}
      </button>
    {/if}
  {/snippet}
</Dialog>
