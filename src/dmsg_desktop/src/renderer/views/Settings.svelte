<script lang="ts">
  import { onMount } from 'svelte'
  import type { Glyph } from '../components/glyphs'
  import Icon from '../components/Icon.svelte'
  import Fact from '../components/page/Fact.svelte'
  import Id from '../components/page/Id.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Page from '../components/page/Page.svelte'
  import Pill from '../components/page/Pill.svelte'
  import Section from '../components/page/Section.svelte'
  import { buttonClass, cn } from '../components/ui'
  import { sessionLabel, t, type Label } from '../i18n/index.svelte'
  import { bridge, call, code, onChange, single, when } from '../rpc'
  import { startLogin } from '../shell/login.svelte'
  import { unlock } from '../shell/session'
  import About from './About.svelte'
  import Devices from './Devices.svelte'
  import Legacy from './Legacy.svelte'
  import Onboarding from './Onboarding.svelte'
  import Profile from './Profile.svelte'
  import Security from './Security.svelte'

  /**
   * Settings in sections beside their list: this session and the device's
   * accounts, security, devices, the public profile, connected browsers, the
   * legacy migration and the app's version and updates. Each section has its
   * own route under /settings.
   */
  let { status, refresh, route, go }: { status: any; refresh: () => Promise<void>; route: string; go: (path: string) => void } = $props()

  type Part = 'session' | 'security' | 'devices' | 'profile' | 'browsers' | 'legacy' | 'about'

  const parts: { value: Part; label: Label; icon: Glyph; locked?: boolean }[] = [
    { value: 'session', label: 'sessionAccounts', icon: 'account' },
    { value: 'security', label: 'security', icon: 'security' },
    { value: 'devices', label: 'devices', icon: 'device' },
    { value: 'profile', label: 'publicProfile', icon: 'profile' },
    { value: 'browsers', label: 'browsers', icon: 'browser' },
    { value: 'legacy', label: 'legacy', icon: 'legacy' },
    { value: 'about', label: 'aboutApp', icon: 'info' }
  ]

  const part = $derived<Part>(parts.find((p) => route === `/settings/${p.value}`)?.value ?? 'session')

  let account = $state<any>(null)
  let accounts = $state<any[]>([])
  let pairings = $state<any[]>([])
  let invite = $state<any>(null)
  let candidates = $state<any[]>([])
  let error = $state('')
  let timer: ReturnType<typeof setInterval> | undefined

  const unlocked = $derived(status?.state === 'unlocked')
  const opened = $derived(['unlocked', 'locked', 'root_required'].includes(status?.state))
  const own = $derived(account?.devices?.find((d: any) => d.current))

  const reload = single(async () => {
    error = ''
    try {
      accounts = await call('session/accounts')
      if (opened) [account, pairings] = await Promise.all([call('account/show'), call('connect/pairings')])
    } catch (e) {
      error = `${t('error')}: ${code(e)}`
    }
  })

  /** A session or browser change; its daemon event reloads what it changed. */
  async function act(method: string, params?: unknown) {
    try {
      await call(method, params)
    } catch (e) {
      error = `${t('error')}: ${code(e)}`
    }
  }

  /** Another account's workspace opens locked; its fast unlock or a login opens it. */
  async function switchTo(id: string, fast: boolean) {
    account = null
    pairings = []
    await act(fast ? 'session/unlock' : 'session/switch', fast ? { fast: true, account: id } : { account: id })
  }

  async function pair(copy: boolean) {
    try {
      invite = await bridge(() => window.dmsg.pair(copy))
    } catch (e) {
      error = `${t('error')}: ${code(e)}`
      return
    }
    candidates = []
    clearInterval(timer)
    timer = setInterval(async () => {
      if (!invite || Date.now() > invite.expires_at) return clearInterval(timer)
      const found: any = await call('connect/candidates', { invite: invite.invite_id }).catch(() => null)
      if (found) candidates = found.candidates
    }, 2000)
  }

  /** The person compares the label with the browser that opened the link. */
  async function grant(candidate: any) {
    try {
      await bridge(() => window.dmsg.grant(invite.invite_id, candidate.pairing_id))
    } catch (e) {
      error = `${t('error')}: ${code(e)}`
    }
    clearInterval(timer)
    invite = null
    candidates = []
  }

  onMount(() => {
    void reload()
    const off = onChange(['session/changed', 'connect/changed'], reload)
    return () => {
      off()
      clearInterval(timer)
    }
  })
</script>

<div class="@container flex h-full min-h-0">
  <nav class="hidden w-[228px] shrink-0 flex-col gap-px overflow-y-auto border-e border-line p-2.5 @min-[720px]:flex" aria-label={t('settings')}>
    {#each parts as item (item.value)}
      <button
        class={cn(
          'flex h-9 items-center gap-2.5 rounded-lg px-2.5 text-start text-[13px] outline-none transition-colors duration-100 focus-visible:ring-3 focus-visible:ring-focus/35',
          part === item.value ? 'bg-pale font-medium text-forest' : 'text-ink-2 hover:bg-wash hover:text-ink'
        )}
        aria-current={part === item.value ? 'page' : undefined}
        onclick={() => go(`/settings/${item.value}`)}
        data-testid={item.value === 'legacy' ? 'settings-legacy' : `settings-${item.value}`}
      >
        <Icon name={item.icon} class={part === item.value ? 'text-forest' : 'text-faint'} />
        <span class="truncate">{t(item.label)}</span>
      </button>
    {/each}
  </nav>
  <div class="flex min-w-0 flex-1 flex-col">
    <div class="flex gap-1 overflow-x-auto border-b border-line px-4 py-2 @min-[720px]:hidden">
      {#each parts as item (item.value)}
        <button
          class={buttonClass(part === item.value ? 'secondary' : 'ghost', 'sm')}
          aria-current={part === item.value ? 'page' : undefined}
          onclick={() => go(`/settings/${item.value}`)}
        >
          <Icon name={item.icon} />{t(item.label)}
        </button>
      {/each}
    </div>
    <div class="min-h-0 flex-1">
      <Page>
        {#if error}<Notice tone="danger">{error}</Notice>{/if}
        {#if part === 'legacy'}
          <Legacy {status} />
        {:else if part === 'about'}
          <About />
        {:else if part === 'session'}
          <Section title={t('status')} icon="account">
            {#snippet actions()}
              <button class={buttonClass(status?.state === 'logged_out' ? 'primary' : 'secondary', 'sm')} onclick={() => startLogin()} data-testid="settings-login">
                <Icon name="login" />{status?.state === 'logged_out' ? t('login') : t('loginOther')}
              </button>
            {/snippet}
            <dl class="divide-y divide-line">
              <Fact label={t('status')} testid="session-state">
                <Pill tone={unlocked ? 'ok' : 'neutral'}><Icon name={unlocked ? 'unlock' : 'lock'} />{status?.state ? sessionLabel(status.state) : '-'}</Pill>
              </Fact>
              <Fact label={t('account')}>{#if status?.account}<Id id={status.account} full />{:else}-{/if}</Fact>
              <Fact label={t('device')}>{#if status?.device_id}<Id id={status.device_id} />{:else}-{/if}</Fact>
              <Fact label={t('formal')}>
                <Pill tone={own?.capabilities?.includes('FormalApprove') ? 'forest' : 'neutral'}>
                  {own?.capabilities?.includes('FormalApprove') ? t('formalOn') : t('formalOff')}
                </Pill>
              </Fact>
              <Fact label={t('syncState')}>{t('pendingSync', { n: status?.pending_sync ?? 0 })} · {when(status?.last_sync?.at)}</Fact>
              <Fact label={t('backgroundOnline')}>
                {status?.background_online ? t('on') : t('off')}{status?.resident ? ` · ${t('deviceKeyResident')}` : ''}
              </Fact>
            </dl>
            <div class="mt-3 flex flex-wrap gap-1.5">
              {#if status?.state === 'locked'}
                <button class={buttonClass('primary', 'sm')} onclick={() => unlock(status)}><Icon name="unlock" />{status.fast_unlock ? t('fastUnlock') : t('login')}</button>
              {/if}
              {#if unlocked}
                <button class={buttonClass('secondary', 'sm')} onclick={() => act('session/lock')}><Icon name="lock" />{t('lock')}</button>
              {/if}
              {#if status?.account}
                <button class={buttonClass('secondary', 'sm')} onclick={() => act('sync/run')}><Icon name="sync-now" />{t('sync')}</button>
              {/if}
            </div>
          </Section>
          {#if accounts.length}
            <Section title={t('localAccounts')} description={t('localAccountsHint')} icon="members" flush>
              <ul class="divide-y divide-line border-t border-line" data-testid="local-accounts">
                {#each accounts as a (a.account)}
                  <li class="flex flex-wrap items-center gap-2 px-5 py-3">
                    <Id id={a.account} full class="max-w-[60%]" />
                    {#if a.current}<Pill tone="forest">{t('current')}</Pill>{/if}
                    {#if !a.bound}<Pill tone="warn">{t('waitingDevice')}</Pill>{/if}
                    <span class="flex-1"></span>
                    {#if !a.current}
                      {#if a.fast_unlock}<button class={buttonClass('secondary', 'xs')} onclick={() => switchTo(a.account, true)}>{t('fastUnlock')}</button>{/if}
                      <button class={buttonClass('secondary', 'xs')} onclick={() => switchTo(a.account, false)}>{t('switchTo')}</button>
                    {/if}
                  </li>
                {/each}
              </ul>
            </Section>
          {/if}
          <Onboarding {status} {refresh} />
        {:else if part === 'browsers'}
          {#if opened}
            <Section title={t('browsers')} description={t('browsersHint')} icon="browser" flush>
              {#snippet actions()}
                <button class={buttonClass('secondary', 'sm')} onclick={() => pair(true)}><Icon name="copy" />{t('copyLink')}</button>
                <button class={buttonClass('primary', 'sm')} onclick={() => pair(false)}><Icon name="add" />{t('connectBrowser')}</button>
              {/snippet}
              {#if invite}
                <div class="grid gap-2 border-t border-line bg-surface-2/60 px-5 py-3">
                  <p class="flex items-center gap-2 text-[12.5px] text-muted"><span class="shimmer">{t('waitingBrowser')}</span></p>
                  <bdi dir="ltr" class="font-mono text-[11.5px] break-all text-faint">{invite.url}</bdi>
                  {#each candidates as c (c.pairing_id)}
                    <div class="flex flex-wrap items-center gap-2 rounded-xl border border-forest/30 bg-pale px-3 py-2.5">
                      <Icon name="browser" class="text-forest" />
                      <span class="min-w-0 flex-1 text-[13px]">{t('confirmBrowser')} <b>{c.label}</b> <span class="font-mono text-[11.5px] text-faint">{c.pairing_id.slice(0, 12)}</span></span>
                      <button class={buttonClass('primary', 'sm')} onclick={() => grant(c)}>{t('connect')}</button>
                    </div>
                  {/each}
                </div>
              {/if}
              <ul class="divide-y divide-line border-t border-line" data-testid="pairings">
                {#each pairings as p (p.pairing_id)}
                  <li class="flex flex-wrap items-center gap-2 px-5 py-3">
                    <Pill tone={p.state === 'active' ? 'ok' : 'neutral'}>{p.state}</Pill>
                    <b class="text-[13px] font-medium">{p.label ?? p.pairing_id.slice(0, 12)}</b>
                    <span class="min-w-0 flex-1 text-[12px] text-faint">
                      {t('issuedBy')} <bdi dir="ltr" class="font-mono">{p.issued_by?.slice(0, 8) ?? '-'}</bdi>{p.issued_by === own?.device_id ? ` (${t('thisDevice')})` : ''}
                      · {t('granted')} {when(p.granted_at)} · {t('lastUsed')} {when(p.last_used_at)}
                    </span>
                    {#if p.state === 'active'}
                      <button class={buttonClass('danger', 'xs')} onclick={() => act('connect/revoke', { pairing: p.pairing_id })}>{t('revoke')}</button>
                    {/if}
                  </li>
                {:else}
                  <li class="px-5 py-3 text-[13px] text-faint">{t('noBrowsers')}</li>
                {/each}
              </ul>
            </Section>
          {:else}
            <Notice>{t('locked')}</Notice>
          {/if}
        {:else if !unlocked}
          <Notice icon="lock">{t('locked')}</Notice>
        {:else if part === 'security'}
          <Security />
        {:else if part === 'devices'}
          <Devices {status} />
        {:else}
          {#key status.account}<Profile {status} />{/key}
        {/if}
      </Page>
    </div>
  </div>
</div>
