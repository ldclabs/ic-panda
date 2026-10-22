<script lang="ts">
  import { onMount } from 'svelte'
  import { DropdownMenu } from 'bits-ui'
  import { localeDir } from '../../shared/locales'
  import Icon from '../components/Icon.svelte'
  import Tooltip from '../components/Tooltip.svelte'
  import { cn, menuContentClass, menuItemClass } from '../components/ui'
  import { locale, sessionLabel, t } from '../i18n/index.svelte'
  import { call, onChange } from '../rpc'
  import { startLogin } from './login.svelte'
  import { logout, shortId, switchAccount, unlocking } from './session'

  /**
   * The account this device works in, at the top of the sidebar: its public
   * name and avatar once unlocked, and a menu of the device's other accounts.
   * Each account keeps its own workspace; another one opens locked.
   */
  let { status, collapsed = false }: { status: any; collapsed?: boolean } = $props()

  let open = $state(false)
  let content = $state<HTMLElement | null>(null)
  let accounts = $state<any[]>([])
  let name = $state('')
  let avatar = $state<string | null>(null)
  // Read on unlock and publication, without polling on each status refresh.
  const unlockedAccount = $derived(status?.state === 'unlocked' ? (status.account as string) : null)
  const account = $derived<string | null>(status?.account ?? null)
  let profileRead = 0

  $effect(() => {
    const id = unlockedAccount
    name = ''
    avatar = null
    if (id) void showProfile(id)
    return () => { profileRead++ }
  })

  onMount(() => onChange(['profile/changed'], () => {
    if (unlockedAccount) void showProfile(unlockedAccount)
  }))

  async function showProfile(id: string) {
    const read = ++profileRead
    const current = () => read === profileRead && unlockedAccount === id
    const published: any = await call('profile/show').catch(() => null)
    if (!published || !current()) return
    name = published.name ?? ''
    avatar = null
    if (!published.avatar_url) return
    const image: any = await call('profile/avatar_image', { avatar_url: published.avatar_url }).catch(() => null)
    if (current()) avatar = image?.url ?? null
  }

  // Arrow keys move through the menu once its content has focus.
  $effect(() => {
    if (open) content?.focus()
  })

  $effect(() => {
    if (!open) return
    call<any[]>('session/accounts')
      .then((list) => (accounts = list))
      .catch(() => (accounts = []))
  })

  const initial = $derived((name || 'd').slice(0, 1).toUpperCase())
</script>

<DropdownMenu.Root bind:open dir={localeDir(locale())}>
  <Tooltip content={account ?? sessionLabel(status?.state ?? '')} side="right" disabled={!collapsed}>
    {#snippet children(props)}
      <DropdownMenu.Trigger
        {...props}
        class={cn(
          'flex w-full items-center gap-2.5 rounded-[10px] text-start outline-none transition-colors duration-150 hover:bg-wash focus-visible:ring-3 focus-visible:ring-focus/35 data-[state=open]:bg-wash [-webkit-app-region:no-drag]',
          collapsed ? 'size-11 justify-center p-0' : 'px-2 py-1.5'
        )}
        aria-label={t('localAccounts')}
        disabled={$unlocking}
        data-testid="account-switcher"
      >
        {#if avatar}
          <img src={avatar} alt="" class="size-8 shrink-0 rounded-[9px] object-cover" />
        {:else}
          <span class="grid size-8 shrink-0 place-items-center rounded-[9px] bg-forest text-[13px] font-semibold text-on-forest">{initial}</span>
        {/if}
        {#if !collapsed}
          <span class="min-w-0 flex-1">
            <span class="block truncate text-[13.5px] font-semibold">{name || t('account')}</span>
            <span class="block truncate font-mono text-[11px] text-faint"
              >{#if account}<bdi dir="ltr">{shortId(account)}</bdi>{:else}{sessionLabel(status?.state ?? '')}{/if}</span
            >
          </span>
          <Icon name="chevron-down" class="text-faint" />
        {/if}
      </DropdownMenu.Trigger>
    {/snippet}
  </Tooltip>
  <DropdownMenu.Portal>
    <DropdownMenu.Content bind:ref={content} class={cn(menuContentClass, 'w-64')} align="start" sideOffset={6} collisionPadding={8}>
      <DropdownMenu.Group>
        <DropdownMenu.GroupHeading class="px-2.5 pt-1.5 pb-1 text-[11px] font-medium text-faint">{t('localAccounts')}</DropdownMenu.GroupHeading>
        {#each accounts as a (a.account)}
          <DropdownMenu.Item
            class={cn(menuItemClass, 'data-disabled:cursor-default data-disabled:opacity-100')}
            disabled={a.current}
            onSelect={() => switchAccount(a.account, a.fast_unlock)}
            data-value={a.account}
          >
            <span class="min-w-0">
              <span class="block truncate font-mono text-xs"><bdi dir="ltr">{shortId(a.account)}</bdi></span>
              {#if !a.bound}<span class="block text-[11.5px] text-faint">{t('waitingDevice')}</span>{/if}
            </span>
            {#if a.current}<Icon name="check" class="size-3.5 text-forest" />{:else if a.fast_unlock}<span class="text-[11.5px] text-faint">{t('fastUnlock')}</span>{/if}
          </DropdownMenu.Item>
        {/each}
      </DropdownMenu.Group>
      <DropdownMenu.Separator class="my-1 h-px bg-line" />
      <DropdownMenu.Item class={menuItemClass} onSelect={() => startLogin()} data-testid="account-login">
        <span class="flex items-center gap-2"><Icon name="login" />{status?.state === 'logged_out' ? t('login') : t('loginOther')}</span>
      </DropdownMenu.Item>
      {#if status?.state && status.state !== 'logged_out'}
        <DropdownMenu.Item class={menuItemClass} onSelect={logout} data-testid="account-logout">
          <span class="flex items-center gap-2"><Icon name="leave" />{t('logout')}</span>
        </DropdownMenu.Item>
      {/if}
    </DropdownMenu.Content>
  </DropdownMenu.Portal>
</DropdownMenu.Root>
