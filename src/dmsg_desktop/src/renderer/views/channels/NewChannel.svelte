<script lang="ts">
  import Notice from '../../components/page/Notice.svelte'
  import { RadioGroup, Tabs } from 'bits-ui'
  import { onDestroy, untrack } from 'svelte'
  import { copyText } from '../../components/clipboard'
  import Dialog from '../../components/Dialog.svelte'
  import type { Glyph } from '../../components/glyphs'
  import Icon from '../../components/Icon.svelte'
  import Id from '../../components/page/Id.svelte'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { call, CallError, errorText } from '../../rpc'
  import { invitationLink, readInvitation } from './channels'
  import { accountOf, label } from './names.svelte'
  import TextArea from '../../components/TextArea.svelte'
  import TextField from '../../components/TextField.svelte'

  /**
   * A new channel of a kind, or joining one with an invitation another
   * member gave, pasted as its link or JSON or opened from a `dmsg://` link.
   * The usual new channel is a conversation with one person, which needs only
   * them: it takes their name unless given another. Once it exists they are
   * invited, and the dialog shows the link to hand them.
   */
  let {
    open = $bindable(false),
    tab = $bindable<'create' | 'join'>('create'),
    invitation = $bindable(''),
    peer = $bindable<{ account: string; name?: string } | null>(null),
    onOpened,
    onDirectInvited
  }: {
    open?: boolean
    tab?: 'create' | 'join'
    invitation?: string
    peer?: { account: string; name?: string } | null
    onOpened: (id: string) => void
    onDirectInvited?: (id: string, account: string) => void
  } = $props()

  let name = $state('')
  let kind = $state('direct')
  let target = $state('')
  let busy = $state(false)
  let failure = $state.raw<unknown>(null)
  const error = $derived(failure ? errorText(failure) : '')
  type DirectInvite = { kind: 'created'; channel: string; account: string } | { kind: 'invited'; link: string }
  let direct = $state.raw<DirectInvite | null>(null)
  let generation = 0
  onDestroy(() => { generation++ })

  const kinds: { value: string; icon: Glyph }[] = [
    { value: 'direct', icon: 'user' },
    { value: 'collaboration', icon: 'members' },
    { value: 'distribution', icon: 'send' }
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
      generation++
      name = ''
      target = ''
      kind = 'direct'
      invitation = ''
      failure = null
      busy = false
      direct = null
      peer = null
    }
  })

  async function submit() {
    if (!open || busy || direct?.kind === 'invited') return
    const run = generation
    const current = () => open && run === generation
    busy = true
    failure = null
    try {
      if (!direct && tab === 'join') {
        const parsed = readInvitation(invitation)
        if (!parsed) throw new CallError('INVALID_INPUT: invitation')
        const ledger = await call<{ channel_id: string }>('channel/join', { invitation: parsed })
        if (!current()) return
        onOpened(ledger.channel_id)
        open = false
        return
      }
      if (!direct) {
        const account = kind === 'direct' ? await accountOf(target) : null
        if (!current()) return
        const called = name.trim() || (account ? label(account) : '')
        const created = await call<{ id: string }>('channel/create', { name: called, type: kind })
        if (!current()) return
        onOpened(created.id)
        if (!account) { open = false; return }
        direct = { kind: 'created', channel: created.id, account }
      }
      const packet = await call('channel/invite', { id: direct.channel, account: direct.account })
      if (current()) {
        onDirectInvited?.(direct.channel, direct.account)
        direct = { kind: 'invited', link: invitationLink(packet) }
      }
    } catch (e) {
      if (current()) failure = e
    } finally {
      if (current()) busy = false
    }
  }

  const tabClass =
    'h-7 flex-1 rounded-lg px-3 text-[12.5px] text-muted outline-none transition-colors duration-150 hover:text-ink focus-visible:ring-3 focus-visible:ring-focus/35 data-[state=active]:bg-surface data-[state=active]:font-medium data-[state=active]:text-ink'
</script>

<Dialog bind:open title={tab === 'create' ? t('newChannel') : t('joinChannel')} icon="channels" testid="channel-new-dialog">
  {#if direct?.kind === 'invited'}
    {@const link = direct.link}
    <div class="grid gap-2" data-testid="channel-invitation-box">
      <p class="text-[12.5px] text-ink-2">{t('directInvited')}</p>
      <TextField readonly value={link} label={t('invitation')} hideLabel mono class="text-[11px]" data-testid="channel-invitation-link" />
      <button class={buttonClass('secondary', 'sm', 'justify-self-start')} onclick={() => copyText(link)}><Icon name="link" />{t('copyLink')}</button>
    </div>
  {:else if direct}
    <div class="grid gap-2 text-[12.5px] text-ink-2" data-testid="channel-invite-pending">
      <p role="status">{t('directInvitePending')}</p>
      <p>{t('directWith')} <Id id={direct.account} full wrap /></p>
    </div>
  {:else}
    <Tabs.Root bind:value={() => tab, (next) => (tab = next as 'create' | 'join')}>
      <Tabs.List class="mb-4 flex gap-0.5 rounded-[10px] border border-line bg-surface-2 p-0.5">
        <Tabs.Trigger value="create" class={tabClass} disabled={busy}>{t('newChannel')}</Tabs.Trigger>
        <Tabs.Trigger value="join" class={tabClass} disabled={busy} data-testid="channel-join-tab">{t('joinChannel')}</Tabs.Trigger>
      </Tabs.List>
      <Tabs.Content value="create" class="grid gap-3 outline-none">
        <RadioGroup.Root bind:value={kind} disabled={busy} class="grid grid-cols-3 gap-1.5" aria-label={t('type')}>
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
          <TextField bind:value={target} disabled={busy} label={t('directWith')} placeholder={t('accountOrName')} mono spellcheck="false" data-testid="channel-peer" />
        {/if}
        <TextField bind:value={name} disabled={busy} label={kind === 'direct' ? t('channelNameOptional') : t('channelName')} data-testid="channel-name" />
      </Tabs.Content>
      <Tabs.Content value="join" class="grid gap-2 outline-none">
        <TextArea bind:value={invitation} disabled={busy} label={t('invitation')} rows={6} mono spellcheck="false" data-testid="channel-join-text" />
      </Tabs.Content>
    </Tabs.Root>
  {/if}
  {#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
  {#snippet footer()}
    <span class="flex-1"></span>
    {#if direct?.kind === 'invited'}
      <button class={buttonClass('primary')} onclick={() => (open = false)}>{t('done')}</button>
    {:else}
      <button class={buttonClass('ghost')} onclick={() => (open = false)}>{t('cancel')}</button>
      <button
        class={buttonClass('primary')}
        disabled={busy || (!direct && (tab === 'create' ? (kind === 'direct' ? !target.trim() : !name.trim()) : !invitation.trim()))}
        onclick={submit}
        data-testid="channel-create"
      >
        {direct ? t(error ? 'retry' : 'invite') : t(tab === 'create' ? 'create' : 'join')}
      </button>
    {/if}
  {/snippet}
</Dialog>
