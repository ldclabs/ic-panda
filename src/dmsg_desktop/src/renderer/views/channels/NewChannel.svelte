<script lang="ts">
  import { RadioGroup, Tabs } from 'bits-ui'
  import Dialog from '../../components/Dialog.svelte'
  import type { Glyph } from '../../components/glyphs'
  import Icon from '../../components/Icon.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { call, CallError, errorText } from '../../rpc'

  /** A new channel of a kind, or joining one with an invitation another member gave. */
  let {
    open = $bindable(false),
    tab = $bindable<'create' | 'join'>('create'),
    onOpened
  }: { open?: boolean; tab?: 'create' | 'join'; onOpened: (id: string) => void } = $props()

  let name = $state('')
  let kind = $state('collaboration')
  let invitation = $state('')
  let busy = $state(false)
  let error = $state('')

  const kinds: { value: string; icon: Glyph }[] = [
    { value: 'collaboration', icon: 'members' },
    { value: 'distribution', icon: 'send' },
    { value: 'direct', icon: 'user' }
  ]

  $effect(() => {
    if (!open) {
      name = ''
      invitation = ''
      error = ''
    }
  })

  async function submit() {
    busy = true
    error = ''
    try {
      if (tab === 'create') {
        const created: any = await call('channel/create', { name: name.trim(), type: kind })
        onOpened(created.id)
      } else {
        let parsed: unknown
        try {
          parsed = JSON.parse(invitation)
        } catch {
          throw new CallError('INVALID_INPUT: invitation')
        }
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
    'h-7 flex-1 rounded-lg px-3 text-[12.5px] text-muted outline-none transition-colors duration-150 hover:text-ink focus-visible:ring-3 focus-visible:ring-focus/35 data-[state=active]:bg-surface data-[state=active]:font-medium data-[state=active]:text-ink data-[state=active]:shadow-xs'
</script>

<Dialog bind:open title={tab === 'create' ? t('newChannel') : t('joinChannel')} icon="channels" testid="channel-new-dialog">
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
            class="flex flex-col items-center gap-1 rounded-xl border border-line bg-surface px-2 py-3 text-[12px] text-muted outline-none transition-colors duration-150 hover:border-edge-soft hover:text-ink focus-visible:ring-3 focus-visible:ring-focus/35 data-[state=checked]:border-forest data-[state=checked]:bg-pale data-[state=checked]:text-forest"
          >
            <Icon name={option.icon} class="size-5" />
            {t(option.value as 'collaboration' | 'distribution' | 'direct')}
          </RadioGroup.Item>
        {/each}
      </RadioGroup.Root>
    </Tabs.Content>
    <Tabs.Content value="join" class="grid gap-2 outline-none">
      <textarea bind:value={invitation} class="min-h-32 font-mono text-[11px]" placeholder={t('invitation')} spellcheck="false" data-testid="channel-join-text"></textarea>
    </Tabs.Content>
  </Tabs.Root>
  {#if error}<p class="mt-2 text-[12.5px] text-danger">{error}</p>{/if}
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (open = false)}>{t('cancel')}</button>
    <button
      class={cn(buttonClass('primary'))}
      disabled={busy || (tab === 'create' ? !name.trim() : !invitation.trim())}
      onclick={submit}
      data-testid="channel-create"
    >
      {tab === 'create' ? t('create') : t('join')}
    </button>
  {/snippet}
</Dialog>
