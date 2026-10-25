<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import { display, watchDisplays, type Mode } from '../display.svelte'
  import CacheSettings from './CacheSettings.svelte'
  import { RadioGroup } from 'bits-ui'
  import { appearances, isAppearance } from '../../shared/appearance'
  import { isLocale, localeNames, locales } from '../../shared/locales'
  import type { Glyph } from '../components/glyphs'
  import Icon from '../components/Icon.svelte'
  import Fact from '../components/page/Fact.svelte'
  import Id from '../components/page/Id.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Page from '../components/page/Page.svelte'
  import Pill from '../components/page/Pill.svelte'
  import Section from '../components/page/Section.svelte'
  import Radio from '../components/Radio.svelte'
  import Select from '../components/Select.svelte'
  import { toast } from '../components/toast.svelte'
  import { buttonClass, cn } from '../components/ui'
  import { appearance, appearanceLabel, setAppearance } from '../appearance.svelte'
  import { locale, sessionLabel, setLocale, t, type Label } from '../i18n/index.svelte'
  import { bridge, call, code, errorText, single, when } from '../rpc'
  import { startLogin } from '../shell/login.svelte'
  import { switchAccount, unlock, unlocking } from '../shell/session'
  import About from './About.svelte'
  import Devices from './Devices.svelte'
  import FilesSettings from './files/FilesSettings.svelte'
  import Legacy from './Legacy.svelte'
  import Onboarding from './Onboarding.svelte'
  import Profile from './Profile.svelte'
  import Security from './Security.svelte'

  /**
   * Settings in sections beside their list: this session and the device's
   * accounts, security, devices, the public profile, connected browsers, the
   * file library, the interface language and appearance, the legacy migration
   * and the app's version and updates. Each section has its own route under
   * /settings.
   */
  let { status, refresh, route, go }: { status: any; refresh: () => Promise<void>; route: string; go: (path: string) => void } = $props()

  type Part = 'session' | 'security' | 'devices' | 'profile' | 'browsers' | 'files' | 'appearance' | 'legacy' | 'about'

  const parts: { value: Part; label: Label; icon: Glyph; locked?: boolean }[] = [
    { value: 'session', label: 'sessionAccounts', icon: 'account' },
    { value: 'security', label: 'security', icon: 'security' },
    { value: 'devices', label: 'devices', icon: 'device' },
    { value: 'profile', label: 'publicProfile', icon: 'profile' },
    { value: 'browsers', label: 'browsers', icon: 'browser' },
    { value: 'files', label: 'files', icon: 'folder' },
    { value: 'appearance', label: 'languageAppearance', icon: 'system' },
    { value: 'legacy', label: 'legacy', icon: 'legacy' },
    { value: 'about', label: 'aboutApp', icon: 'info' }
  ]

  const part = $derived<Part>(parts.find((p) => route === `/settings/${p.value}`)?.value ?? 'session')

  let account = $state<any>(null)
  let accounts = $state<any[]>([])
  /** Null while the device cannot sign for them: locked without a resident key. */
  let pairings = $state<any[] | null>([])
  let invite = $state<any>(null)
  let candidates = $state<any[]>([])
  let error = $state('')
  let timer: ReturnType<typeof setTimeout> | undefined
  let alive = true
  let generation = 0
  let pairingRevision = 0
  let pairingBusy = $state(false)
  let lastSession: string | undefined
  let errors = $state<Record<string, string>>({})
  let loaded = $state<Record<string, boolean>>({})
  const sessionKey = $derived(`${status?.account ?? ''}:${status?.state ?? ''}`)
  const current = (turn: number) => alive && generation === turn

  const unlocked = $derived(status?.state === 'unlocked')
  const opened = $derived(['unlocked', 'locked', 'root_required'].includes(status?.state))
  const own = $derived(account?.devices?.find((d: any) => d.current))

  /** The main process keeps the interface language and appearance for every window. */
  async function pickLanguage(next: string) {
    if (!isLocale(next) || next === locale()) return
    try {
      await setLocale(next)
    } catch {
      toast(() => t('languageFailed'))
    }
  }

  async function pickAppearance(next: string) {
    if (!isAppearance(next) || next === appearance()) return
    try {
      await setAppearance(next)
    } catch {
      toast(() => t('appearanceFailed'))
    }
  }

  function reader(key: string, method: string, visible: () => boolean, apply: (value: any) => void) {
    return single(async (mode: Mode = 'swr') => {
      if (!alive || !visible()) return
      const turn = generation
      try {
        const value = method === 'account/show' && unlocked ? await display(method, {}, mode) : await call(method)
        if (current(turn)) { apply(value); errors[key] = '' }
      } catch (e) {
        if (!current(turn)) return
        if (key === 'pairings' && code(e) === 'LOCKED') { pairings = null; errors[key] = '' }
        else errors[key] = errorText(e)
      } finally { if (current(turn)) loaded[key] = true }
    })
  }
  const loadAccounts = reader('accounts', 'session/accounts', () => part === 'session', (value) => (accounts = value))
  const loadAccount = reader('account', 'account/show', () => part === 'session' && opened, (value) => (account = value))
  const loadPairings = reader('pairings', 'connect/pairings', () => part === 'browsers' && opened, (value) => (pairings = value))
  function reload() { void loadAccounts(); void loadAccount(); void loadPairings() }

  /** A session or browser change; its daemon event reloads what it changed. */
  async function act(method: string, params?: unknown) {
    const turn = generation
    try {
      await call(method, params)
    } catch (e) {
      if (current(turn)) error = errorText(e)
    }
  }

  /** Another account's workspace opens locked; its fast unlock or a login opens it. */
  async function switchTo(id: string, fast: boolean) {
    account = null
    pairings = []
    await switchAccount(id, fast)
  }

  function stopPairing() {
    pairingRevision++
    clearTimeout(timer)
    invite = null; candidates = []; pairingBusy = false
  }

  async function poll(invitation: any, revision: number) {
    const valid = () => alive && part === 'browsers' && revision === pairingRevision && invite === invitation
    if (!valid() || Date.now() >= invitation.expires_at) return
    try {
      const found: any = await call('connect/candidates', { invite: invitation.invite_id })
      if (valid()) { candidates = found.candidates; error = '' }
    } catch (e) { if (valid()) error = errorText(e) }
    finally {
      if (valid() && Date.now() < invitation.expires_at) timer = setTimeout(() => void poll(invitation, revision), 2000)
    }
  }

  async function pair(copy: boolean) {
    if (pairingBusy) return
    stopPairing()
    const revision = pairingRevision
    const turn = generation
    pairingBusy = true
    error = ''
    try {
      const value = await bridge(() => window.dmsg.pair(copy))
      if (!current(turn) || revision !== pairingRevision || part !== 'browsers') return
      invite = value
      timer = setTimeout(() => void poll(invite, revision), 2000)
    } catch (e) { if (current(turn) && revision === pairingRevision) error = errorText(e) }
    finally { if (current(turn) && revision === pairingRevision) pairingBusy = false }
  }

  /** The person compares the label with the browser that opened the link. */
  async function grant(candidate: any) {
    if (pairingBusy || !invite) return
    const revision = ++pairingRevision
    const turn = generation
    const invitation = invite
    clearTimeout(timer)
    pairingBusy = true
    error = ''
    try {
      await bridge(() => window.dmsg.grant(invitation.invite_id, candidate.pairing_id))
      if (current(turn) && revision === pairingRevision) stopPairing()
    } catch (e) {
      if (current(turn) && revision === pairingRevision) {
        error = errorText(e)
        timer = setTimeout(() => void poll(invitation, revision), 2000)
      }
    } finally { if (current(turn) && revision === pairingRevision) pairingBusy = false }
  }

  onMount(() => {
    const off = window.dmsg.onEvent((event) => {
      if (['resync', 'session/changed', 'connect/changed', 'approval/changed'].includes(event.type)) reload()
    })
    const stop = watchDisplays(['account-summary'], (mode) => loadAccount(mode))
    return () => { alive = false; generation++; off(); stop(); stopPairing() }
  })

  $effect(() => {
    const next = sessionKey
    part
    untrack(() => {
      error = ''
      if (lastSession !== next) {
        lastSession = next
        generation++
        account = null; accounts = []; pairings = []; error = ''; errors = {}; loaded = {}
        stopPairing()
      } else if (part !== 'browsers') stopPairing()
      reload()
    })
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
        {#key sessionKey}
        {#each (part === 'session' ? [errors.account, errors.accounts] : part === 'browsers' ? [errors.pairings] : []).filter(Boolean) as failure}<Notice failure={failure} tone="danger">{failure}</Notice>{/each}
        {#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
        {#if part === 'legacy'}
          <Legacy {status} {go} />
        {:else if part === 'files'}
      {#if unlocked}<CacheSettings />{/if}
          <FilesSettings />
        {:else if part === 'appearance'}
          <Section title={t('languageAppearance')} description={t('languageAppearanceHint')} icon="system" testid="language-appearance">
            <dl>
              <Fact label={t('language')}>
                <Select
                  value={locale()}
                  options={locales.map((code) => ({ value: code, label: localeNames[code] }))}
                  label={t('language')}
                  onChange={pickLanguage}
                  testid="settings-language"
                  class="w-56"
                />
              </Fact>
              <Fact label={t('appearance')}>
                <RadioGroup.Root value={appearance()} onValueChange={pickAppearance} aria-label={t('appearance')} class="flex flex-wrap gap-x-5 gap-y-2">
                  {#each appearances as choice (choice)}
                    <Radio value={choice} testid={`appearance-${choice}`}>
                      <span class="inline-flex items-center gap-1.5"><Icon name={choice} class="text-faint" />{t(appearanceLabel[choice])}</span>
                    </Radio>
                  {/each}
                </RadioGroup.Root>
              </Fact>
            </dl>
          </Section>
        {:else if part === 'about'}
          <About />
        {:else if part === 'session'}
          <Onboarding {status} {refresh} />
          <Section title={t('status')} icon="account">
            {#snippet actions()}
              <button class={buttonClass(status?.state === 'logged_out' ? 'primary' : 'secondary', 'sm')} onclick={() => startLogin()} data-testid="settings-login">
                <Icon name="login" />{status?.state === 'logged_out' ? t('login') : t('loginOther')}
              </button>
            {/snippet}
            <dl>
              <Fact label={t('status')} testid="session-state">
                <Pill tone={unlocked ? 'ok' : 'neutral'}><Icon name={unlocked ? 'unlock' : 'lock'} />{status?.state ? sessionLabel(status.state) : '-'}</Pill>
              </Fact>
              <Fact label={t('account')}>{#if status?.account}<Id id={status.account} full />{:else}-{/if}</Fact>
              <Fact label={t('device')}>{#if status?.device_id}<Id id={status.device_id} />{:else}-{/if}</Fact>
              <Fact label={t('formal')}>
                <Pill tone={own?.capabilities?.includes('FormalApprove') ? 'forest' : 'neutral'}>
                  {!loaded.account ? t('loading') : account ? own?.capabilities?.includes('FormalApprove') ? t('formalOn') : t('formalOff') : '-'}
                </Pill>
              </Fact>
              <Fact label={t('syncState')}>{t('pendingSync', { n: status?.pending_sync ?? 0 })} · {when(status?.last_sync?.at)}</Fact>
              <Fact label={t('backgroundOnline')}>
                {status?.background_online ? t('on') : t('off')}{status?.resident ? ` · ${t('deviceKeyResident')}` : ''}
              </Fact>
              {#if status?.account}
                <Fact label={t('fastUnlock')} testid="fast-unlock">
                  <Pill tone={status.fast_unlock ? 'forest' : 'neutral'}>{status.fast_unlock ? t('on') : t('off')}</Pill>
                  {#if status.fast_unlock}
                    <span class="text-[12.5px]" data-testid="fast-unlock-method">
                      {t(status.fast_presence === 'biometrics' ? 'presenceBiometrics' : status.fast_presence === 'password' ? 'presencePassword' : 'presenceKeychain')}
                    </span>
                  {/if}
                  <span class="w-full text-[12px] text-faint">{t('fastUnlockHint')}</span>
                </Fact>
              {/if}
              {#if status?.login_expires_at}
                <Fact label={t('loginExpiresAt')} testid="login-expires">{when(status.login_expires_at)}</Fact>
              {/if}
            </dl>
            <div class="mt-3 flex flex-wrap gap-1.5">
              {#if status?.state === 'locked'}
                <button class={buttonClass('primary', 'sm')} disabled={$unlocking} aria-busy={$unlocking} onclick={() => unlock(status)}><Icon name={$unlocking ? 'sync' : 'unlock'} class={$unlocking ? 'motion-safe:animate-spin' : ''} />{$unlocking ? t('unlocking') : status.fast_unlock ? t('fastUnlock') : t('login')}</button>
              {/if}
              {#if unlocked}
                <button class={buttonClass('secondary', 'sm')} onclick={() => act('session/lock')}><Icon name="lock" />{t('lock')}</button>
                <!-- Turning it on needs the open account's keys; its key goes to the OS keychain. -->
                <button class={buttonClass('secondary', 'sm')} onclick={() => act('session/fast_unlock', { enable: !status.fast_unlock })} data-testid="fast-unlock-toggle">
                  <Icon name="key" />{status.fast_unlock ? t('fastUnlockOff') : t('fastUnlockOn')}
                </button>
              {/if}
              {#if status?.account}
                <button class={buttonClass('secondary', 'sm')} disabled={$unlocking || status.syncing} onclick={() => act('sync/run')}><Icon name="sync-now" />{status.syncing ? t('syncing') : t('sync')}</button>
              {/if}
            </div>
          </Section>
          {#if accounts.length}
            <Section title={t('localAccounts')} description={t('localAccountsHint')} icon="members" flush>
              <ul class="divide-y divide-line" data-testid="local-accounts">
                {#each accounts as a (a.account)}
                  <li class="flex flex-wrap items-center gap-2 px-5 py-3">
                    <Id id={a.account} full class="max-w-[60%]" />
                    {#if a.current}<Pill tone="forest">{t('current')}</Pill>{/if}
                    {#if !a.bound}<Pill tone="warn">{t('waitingDevice')}</Pill>{/if}
                    <span class="flex-1"></span>
                    {#if !a.current}
                      {#if a.fast_unlock}<button class={buttonClass('secondary', 'xs')} disabled={$unlocking} onclick={() => switchTo(a.account, true)}>{t('fastUnlock')}</button>{/if}
                      <button class={buttonClass('secondary', 'xs')} disabled={$unlocking} onclick={() => switchTo(a.account, false)}>{t('switchTo')}</button>
                    {/if}
                  </li>
                {/each}
              </ul>
            </Section>
          {/if}
        {:else if part === 'browsers'}
          {#if opened && pairings}
            <Section title={t('browsers')} description={t('browsersHint')} icon="browser" flush>
              {#snippet actions()}
                <button class={buttonClass('secondary', 'sm')} disabled={pairingBusy || !unlocked} data-testid="browser-copy" onclick={() => pair(true)}><Icon name="copy" />{t('copyLink')}</button>
                <button class={buttonClass('primary', 'sm')} disabled={pairingBusy || !unlocked} data-testid="browser-pair" onclick={() => pair(false)}><Icon name="add" />{t('connectBrowser')}</button>
              {/snippet}
              {#if invite}
                <div class="grid gap-2 border-t border-line bg-surface-2/60 px-5 py-3">
                  <p class="flex items-center gap-2 text-[12.5px] text-muted"><span class="shimmer">{t('waitingBrowser')}</span></p>
                  <bdi dir="ltr" class="font-mono text-[11.5px] break-all text-faint">{invite.url}</bdi>
                  {#each candidates as c (c.pairing_id)}
                    <div class="flex flex-wrap items-center gap-2 rounded-xl border border-forest/30 bg-pale px-3 py-2.5">
                      <Icon name="browser" class="text-forest" />
                      <span class="min-w-0 flex-1 text-[13px]">{t('confirmBrowser')} <b>{c.label}</b> <span class="font-mono text-[11.5px] text-faint">{c.pairing_id.slice(0, 12)}</span></span>
                      <button class={buttonClass('primary', 'sm')} disabled={pairingBusy} data-testid="browser-grant" onclick={() => grant(c)}>{t('connect')}</button>
                    </div>
                  {/each}
                </div>
              {/if}
              <ul class="divide-y divide-line" data-testid="pairings">
                {#each pairings as p (p.pairing_id)}
                  <li class="flex flex-wrap items-center gap-2 px-5 py-3">
                    <Pill tone={p.state === 'active' ? 'ok' : 'neutral'}>{p.state}</Pill>
                    <b class="text-[13px] font-medium">{p.label ?? p.pairing_id.slice(0, 12)}</b>
                    <span class="min-w-0 flex-1 text-[12px] text-faint">
                      {t('issuedBy')} <bdi dir="ltr" class="font-mono">{p.issued_by?.slice(0, 8) ?? '-'}</bdi>{p.issued_by === status?.device_id ? ` (${t('thisDevice')})` : ''}
                      · {t('granted')} {when(p.granted_at)} · {t('lastUsed')} {when(p.last_used_at)}
                    </span>
                    {#if p.state === 'active'}
                      <button class={buttonClass('danger', 'xs')} onclick={() => act('connect/revoke', { pairing: p.pairing_id })}>{t('revoke')}</button>
                    {/if}
                  </li>
                {:else}
                  <li class="px-5 py-3 text-[13px] text-faint">{!loaded.pairings ? t('loading') : errors.pairings ? '' : t('noBrowsers')}</li>
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
        {/key}
      </Page>
    </div>
  </div>
</div>
