<script lang="ts">
  import { reminderVisibility } from '../notification-visibility'
  import { syncState } from '../sync-state'
  import { onMount, untrack } from 'svelte'
  import { display, watchDisplays, type Mode } from '../display.svelte'
  import Confirm from '../components/Confirm.svelte'
  import CacheSettings from './CacheSettings.svelte'
  import NotificationSettings from './NotificationSettings.svelte'
  import { RadioGroup } from 'bits-ui'
  import { appearances, isAppearance } from '../../shared/appearance'
  import { isLocale, localeNames, locales } from '../../shared/locales'
  import Icon from '../components/Icon.svelte'
  import Collapsible from '../components/Collapsible.svelte'
  import Fact from '../components/page/Fact.svelte'
  import Group from '../components/page/Group.svelte'
  import Id from '../components/page/Id.svelte'
  import Locked from '../components/page/Locked.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Page from '../components/page/Page.svelte'
  import Pill from '../components/page/Pill.svelte'
  import Section from '../components/page/Section.svelte'
  import Radio from '../components/Radio.svelte'
  import Select from '../components/Select.svelte'
  import { toast } from '../components/toast.svelte'
  import { buttonClass, cn } from '../components/ui'
  import { appearance, appearanceLabel, setAppearance } from '../appearance.svelte'
  import { locale, sessionLabel, setLocale, t } from '../i18n/index.svelte'
  import { bridge, call, code, errorText, single, when } from '../rpc'
  import { settingsGroupsFor, type SettingsPage } from '../shell/nav'
  import LocalAccount from '../shell/LocalAccount.svelte'
  import { lock, unlock, unlocking } from '../shell/session'
  import About from './About.svelte'
  import Devices from './Devices.svelte'
  import FilesSettings from './files/FilesSettings.svelte'
  import Legacy from './Legacy.svelte'
  import LockSettings from './LockSettings.svelte'
  import ShortcutSettings from './ShortcutSettings.svelte'
  import Moved from './Moved.svelte'
  import Profile from './Profile.svelte'
  import Security from './Security.svelte'
  import Recovery from './Recovery.svelte'
  import Feedback from './Feedback.svelte'

  /**
   * Settings in sections beside their list: this session, account security,
   * devices, the public profile, connected browsers, the
   * file library, the interface language and appearance, the legacy migration
   * and the app's version and updates. Each section has its own route under
   * /settings.
   */
  let { status, refresh, route, go }: { status: any; refresh: () => Promise<void>; route: string; go: (path: string) => void } = $props()

  const local = $derived(status?.account_kind === 'local')
  const settingsGroups = $derived(settingsGroupsFor(local))
  const settingsPages = $derived(settingsGroups.flatMap(group => group.pages))
  const currentPage = $derived(settingsPages.find(p => route.split('?')[0] === `/settings/${p.value}`) ?? settingsPages[0])
  const part = $derived(currentPage.value)

  let account = $state<any>(null)
  /** Null while the device cannot sign for them: locked without a resident key. */
  type Pairing = { pairing_id: string; label?: string; state: string; issued_by?: string; granted_at: number; last_used_at: number }
  let pairings = $state<Pairing[] | null>([])
  let revokeTarget = $state<Pairing | null>(null)
  let pollError = $state('')
  let granted = $state('')
  let copiedUntil = $state(0)
  type BrowserInvite = Awaited<ReturnType<typeof window.dmsg.pair>>
  type BrowserCandidate = { pairing_id: string; label: string }
  let invite = $state.raw<BrowserInvite | null>(null)
  let candidates = $state<BrowserCandidate[]>([])
  let now = $state(Date.now())
  onMount(() => { const clock = setInterval(() => { now = Date.now() }, 1000); return () => clearInterval(clock) })
  const syncStatus = $derived(syncState(status, now))
  let error = $state('')
  let actionBusy = $state(false)
  let applyingLanguage = $state('')
  let applyingAppearance = $state('')
  let timer: ReturnType<typeof setTimeout> | undefined
  let alive = true
  let generation = 0
  let pairingRevision = 0
  let pairingBusy = $state(false)
  let lastSession: string | undefined
  let lastPart: SettingsPage | undefined
  let errors = $state<Record<string, string>>({})
  let loaded = $state<Record<string, boolean>>({})
  const reportReminder = reminderVisibility(() => ({ status, ready: !local && part === 'session' && !!loaded.account && !errors.account }), '[data-testid="settings-sync"]')
  $effect(() => { void status; void route; void loaded.account; void errors.account; reportReminder() })
  const sessionKey = $derived(`${status?.account ?? ''}:${status?.state ?? ''}`)
  const current = (turn: number) => alive && generation === turn

  const unlocked = $derived(status?.state === 'unlocked')
  const opened = $derived(['unlocked', 'locked', 'root_required'].includes(status?.state))
  const own = $derived(account?.devices?.find((d: any) => d.current))

  /** The main process keeps the interface language and appearance for every window. */
  async function pickLanguage(next: string) {
    if (applyingLanguage || !isLocale(next) || next === locale()) return
    applyingLanguage = next
    try {
      await setLocale(next)
    } catch {
      toast(() => t('languageFailed'))
    } finally { applyingLanguage = '' }
  }

  async function pickAppearance(next: string) {
    if (applyingAppearance || !isAppearance(next) || next === appearance()) return
    applyingAppearance = next
    try {
      await setAppearance(next)
    } catch {
      toast(() => t('appearanceFailed'))
    } finally { applyingAppearance = '' }
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
  const loadAccount = reader('account', 'account/show', () => !local && part === 'session' && opened, (value) => (account = value))
  const loadPairings = reader('pairings', 'connect/pairings', () => part === 'browsers' && opened, (value) => (pairings = value))
  function reload() { void loadAccount(); void loadPairings() }

  /** A session or browser change; its daemon event reloads what it changed. */
  async function act(method: string, params?: unknown) {
    if (actionBusy) return
    const turn = generation
    actionBusy = true
    error = ''
    try {
      await call(method, params)
    } catch (e) {
      if (current(turn)) error = errorText(e)
    } finally { if (current(turn)) actionBusy = false }
  }

  function stopPairing() {
    pairingRevision++
    clearTimeout(timer)
    invite = null; candidates = []; pollError = ''; copiedUntil = 0; pairingBusy = false
  }

  const expired = (invitation: BrowserInvite, at: number) => at >= invitation.expires_at

  async function poll(invitation: BrowserInvite, revision: number) {
    const valid = () => alive && part === 'browsers' && revision === pairingRevision && invite === invitation && !expired(invitation, Date.now())
    if (!valid()) return
    try {
      const found = await call<{ candidates: BrowserCandidate[] }>('connect/candidates', { invite: invitation.invite_id })
      if (valid()) { candidates = found.candidates; pollError = '' }
    } catch (e) { if (valid()) pollError = errorText(e) }
    finally {
      if (valid()) timer = setTimeout(() => void poll(invitation, revision), 2000)
    }
  }

  async function pair(copy: boolean) {
    if (pairingBusy) return
    const existing = invite && !expired(invite, Date.now()) ? invite : null
    if (existing && !copy) return
    if (!existing) stopPairing()
    const revision = pairingRevision
    const turn = generation
    pairingBusy = true
    error = granted = ''
    try {
      const value = existing ?? await bridge<BrowserInvite>(() => window.dmsg.pair(copy))
      if (!current(turn) || revision !== pairingRevision) return
      if (!existing) {
        invite = value
        timer = setTimeout(() => void poll(value, revision), 2000)
      }
      if (copy) {
        try {
          const result = await bridge<{ clear_after_ms: number }>(() => window.dmsg.copyShareLink(value.url))
          if (current(turn) && revision === pairingRevision) copiedUntil = Math.min(value.expires_at, Date.now() + result.clear_after_ms)
        } catch {
          if (current(turn) && revision === pairingRevision) { copiedUntil = 0; error = t('copyFailed') }
        }
      }
    } catch (e) { if (current(turn) && revision === pairingRevision) error = errorText(e) }
    finally { if (current(turn) && revision === pairingRevision) pairingBusy = false }
  }

  /** The person compares the label with the browser that opened the link. */
  async function grant(candidate: BrowserCandidate) {
    if (pairingBusy || !invite || expired(invite, Date.now())) return
    const revision = ++pairingRevision
    const turn = generation
    const invitation = invite
    clearTimeout(timer)
    pairingBusy = true
    error = ''
    try {
      await bridge(() => window.dmsg.grant(invitation.invite_id, candidate.pairing_id))
      if (current(turn) && revision === pairingRevision) {
        stopPairing()
        granted = candidate.label
        void loadPairings()
      }
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
      if (lastPart !== part) {
        revokeTarget = null
        granted = ''
        lastPart = part
        generation++
        actionBusy = false
      }
      if (lastSession !== next) {
        lastSession = next
        generation++
        account = null; pairings = []; revokeTarget = null; granted = ''; error = ''; errors = {}; loaded = {}; actionBusy = false
        stopPairing()
      } else if (part !== 'browsers') stopPairing()
      reload()
    })
  })

</script>

<div class="@container flex h-full min-h-0">
  <nav data-list class="hidden w-[228px] shrink-0 flex-col gap-px overflow-y-auto border-e border-line p-2.5 @min-[720px]:flex" aria-label={t('settings')}>
    {#each settingsGroups as group (group.label)}
      <div role="group" aria-label={t(group.label)} class="mb-2 grid gap-px">
        <p class="px-2.5 pt-1.5 pb-1 text-caption font-medium text-muted" aria-hidden="true">{t(group.label)}</p>
      {#each group.pages as item (item.value)}
      <button
        class={cn(
          'flex h-9 items-center gap-2.5 rounded-lg px-2.5 text-start text-body outline-none transition-colors duration-100 focus-visible:ring-3 focus-visible:ring-focus/35',
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
      </div>
    {/each}
  </nav>
  <div class="flex min-w-0 flex-1 flex-col">
    <nav class="border-b border-line px-4 py-2 @min-[720px]:hidden" aria-label={t('settings')}>
      <Select
        value={part}
        options={settingsPages.map(item => ({ value: item.value, label: t(item.label) }))}
        label={`${t('settings')} · ${t(currentPage.label)}`}
        onChange={next => go(`/settings/${next}`)}
        testid="settings-section"
      />
    </nav>
    <div class="min-h-0 flex-1">
      {#key `${sessionKey}:${part}`}<Page testid="settings-content">
        {#if part === 'session' && errors.account || part === 'browsers' && errors.pairings}
          <Notice tone="danger" testid="settings-read-error">{part === 'session' ? errors.account : errors.pairings}
            {#snippet action()}<button class={buttonClass('subtle', 'xs')}
              onclick={() => part === 'session' ? loadAccount('fresh') : loadPairings()}>{t('retry')}</button>{/snippet}
          </Notice>
        {/if}
        {#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
        {#if part === 'feedback'}
          <Feedback {status} />
        {:else if part === 'legacy'}
          <Legacy {status} {go} />
        {:else if part === 'files'}
          <FilesSettings />
          {#if unlocked}<CacheSettings />{/if}
        {:else if part === 'notifications'}
          <NotificationSettings {status} />
        {:else if part === 'shortcuts'}
          <ShortcutSettings />
        {:else if part === 'appearance'}
          <Section title={t('languageAppearance')} description={t('languageAppearanceHint')} icon="system" testid="language-appearance">
            <Group>
              <Fact label={t('language')}>
                <Select
                  value={applyingLanguage || locale()}
                  options={locales.map((code) => ({ value: code, label: localeNames[code] }))}
                  label={t('language')}
                  onChange={pickLanguage}
                  disabled={!!applyingLanguage}
                  testid="settings-language"
                  class="w-56"
                />
              </Fact>
              <Fact label={t('appearance')}>
                <RadioGroup.Root value={applyingAppearance || appearance()} onValueChange={pickAppearance} disabled={!!applyingAppearance} aria-label={t('appearance')} class="flex flex-wrap gap-x-5 gap-y-2">
                  {#each appearances as choice (choice)}
                    <Radio value={choice} testid={`appearance-${choice}`}>
                      <span class="inline-flex items-center gap-1.5"><Icon name={choice} class="text-faint" />{t(appearanceLabel[choice])}</span>
                    </Radio>
                  {/each}
                </RadioGroup.Root>
              </Fact>
            </Group>
          </Section>
        {:else if part === 'about'}
          <About />
        {:else if part === 'session' && local}
          <Section title={t('offlineAccount')} description={t('savedOnDevice')} icon="account" testid="local-settings">
            <div class="flex flex-wrap gap-2">
              <button class={buttonClass('secondary', 'sm')} onclick={lock}><Icon name="lock" />{t('lock')}</button>
              {#if status.local_unlock_method === 'password'}<LocalAccount action="password" {status} />{/if}
              <LocalAccount action="delete" {status} {refresh} />
            </div>
          </Section>
        {:else if part === 'session'}
          <Moved {status} {refresh} />
          <Section title={t('status')} icon="account">
            <Group>
              <Fact label={t('status')} testid="session-state">
                <Pill tone={unlocked ? 'ok' : 'neutral'}><Icon name={unlocked ? 'unlock' : 'lock'} />{status?.state ? sessionLabel(status.state) : '-'}</Pill>
              </Fact>
              <Fact label={t('account')}>{#if status?.account}<Id id={status.account} full />{:else}-{/if}</Fact>
              <Fact label={t('syncState')} testid="settings-sync-state">
                <span>{t(syncStatus.label, { n: syncStatus.pending })}</span>
                <span class="w-full text-callout text-muted">{syncStatus.okAt ? t('syncLastSuccess', { time: when(syncStatus.okAt) }) : t('localOnlyNever')}</span>
                {#if syncStatus.retryAt}<span class="w-full text-callout text-muted">{t('syncRetryAt', { time: when(syncStatus.retryAt) })}</span>{/if}
                <span class="w-full text-callout text-muted">{t('syncScope')}</span>
              </Fact>
              {#if status?.login_expires_at}
                <Fact label={t('loginExpiresAt')} testid="login-expires">{when(status.login_expires_at)}</Fact>
              {/if}
            </Group>
            <div class="mt-3 flex flex-wrap gap-1.5">
              {#if status?.state === 'locked'}
                <button class={buttonClass('primary', 'sm')} disabled={$unlocking} aria-busy={$unlocking} onclick={() => unlock(status)}><Icon name={$unlocking ? 'pending' : 'unlock'} class={$unlocking ? 'motion-safe:animate-spin' : ''} />{$unlocking ? t('unlocking') : status.fast_unlock ? t('fastUnlock') : t('login')}</button>
              {/if}
              {#if unlocked}
                <button class={buttonClass('secondary', 'sm')} disabled={actionBusy} onclick={() => act('session/lock')}><Icon name="lock" />{t('lock')}</button>
              {/if}
              {#if status?.account}
                <button class={buttonClass('secondary', 'sm')} disabled={actionBusy || $unlocking || status.syncing || syncStatus.paused || syncStatus.coolingDown} data-testid="settings-sync" onclick={() => act('sync/run')}><Icon name="sync-now" />{status.syncing ? t('syncing') : t('sync')}</button>
              {/if}
            </div>
            <Collapsible class="mt-3" testid="session-details">
              {#snippet title()}<span class="text-callout text-muted">{t('technicalDetails')}</span>{/snippet}
              <Group>
                <Fact label={t('device')}>{#if status?.device_id}<Id id={status.device_id} />{:else}-{/if}</Fact>
                <Fact label={t('formal')}>
                  <span class="text-callout text-muted">
                    {!loaded.account ? t('loading') : account ? own?.capabilities?.includes('FormalApprove') ? t('formalOn') : t('formalOff') : '-'}
                  </span>
                </Fact>
                <Fact label={t('backgroundOnline')}>
                  {status?.background_online ? t('on') : t('off')}{status?.resident ? ` · ${t('deviceKeyResident')}` : ''}
                </Fact>
              </Group>
            </Collapsible>
          </Section>
        {:else if part === 'browsers'}
          {#if opened && pairings}
            <Section title={t('browsers')} description={t('browsersHint')} icon="browser" flush>
              <div class="mb-3 flex flex-wrap gap-2 px-5">
                <button class={buttonClass('secondary', 'sm')} disabled={pairingBusy || !unlocked || actionBusy} data-testid="browser-copy" onclick={() => pair(true)}><Icon name="copy" />{t('copyLink')}</button>
                {#if !invite || expired(invite, now)}<button class={buttonClass('primary', 'sm')} disabled={pairingBusy || !unlocked || actionBusy} data-testid="browser-pair" onclick={() => pair(false)}><Icon name="add" />{t('connectBrowser')}</button>{/if}
              </div>
              {#if granted}<p class="px-5 pb-3 text-body text-ok" role="status" data-testid="browser-connected">{granted} · {t('granted')}</p>{/if}
              {#if invite && expired(invite, now)}
                <Notice tone="warn" testid="browser-expired">
                  {t('browserInviteExpired')}
                  {#snippet action()}<button class={buttonClass('ghost', 'xs')} disabled={pairingBusy} onclick={stopPairing} data-testid="browser-cancel">{t('dismiss')}</button>{/snippet}
                </Notice>
              {:else if invite}
                <div class="grid gap-2 border-t border-line bg-surface-2/60 px-5 py-3">
                  <button class={buttonClass('ghost', 'xs', 'justify-self-end')} disabled={pairingBusy} onclick={stopPairing} data-testid="browser-cancel">{t('cancel')}</button>
                  {#if pollError}<Notice tone="warn">{pollError}</Notice>{/if}
                  {#if !candidates.length}<p class="text-callout text-muted" role="status">{t('waitingBrowser')}</p>{/if}
                  {#if copiedUntil > now}<span class="text-callout text-ok" role="status" data-testid="browser-copied"><Icon name="check" class="inline size-3.5" /> {t('copiedText')}</span>{/if}
                  <p class="text-callout text-muted">{t('expires')} · {when(invite.expires_at)}</p>
                  <bdi dir="ltr" class="font-mono text-code break-all text-faint">{invite.url}</bdi>
                  {#each candidates as c (c.pairing_id)}
                    <div class="flex flex-wrap items-center gap-2 rounded-xl border border-forest/30 bg-pale px-3 py-2.5">
                      <Icon name="browser" class="text-forest" />
                      <span class="min-w-0 flex-1 text-body">{t('confirmBrowser')} <b>{c.label}</b> <span class="font-mono text-code text-faint">{c.pairing_id.slice(0, 12)}</span></span>
                      <button class={buttonClass('primary', 'sm')} disabled={pairingBusy} data-testid="browser-grant" onclick={() => grant(c)}>{t('connect')}</button>
                    </div>
                  {/each}
                </div>
              {/if}
              <ul class="divide-y divide-line" data-testid="pairings">
                {#each pairings as p (p.pairing_id)}
                  <li class="flex flex-wrap items-center gap-2 px-5 py-3">
                    <Pill tone={p.state === 'active' ? 'ok' : 'neutral'}>{p.state === 'active' ? t('pairingActive') : p.state === 'revoked' ? t('pairingRevoked') : p.state}</Pill>
                    <b class="text-body font-medium">{p.label ?? p.pairing_id.slice(0, 12)}</b>
                    <span class="min-w-0 flex-1 text-callout text-muted">
                      {t('issuedBy')} <bdi dir="ltr" class="font-mono">{p.issued_by?.slice(0, 8) ?? '-'}</bdi>{p.issued_by === status?.device_id ? ` (${t('thisDevice')})` : ''}
                      · {t('granted')} {when(p.granted_at)} · {t('lastUsed')} {when(p.last_used_at)}
                    </span>
                    {#if p.state === 'active'}
                      <button class={buttonClass('danger', 'xs')} disabled={actionBusy || pairingBusy || !!errors.pairings} aria-label={`${t('revoke')} · ${p.label || p.pairing_id.slice(0,12)}`} data-testid="browser-revoke" onclick={() => (revokeTarget = p)}>{t('revoke')}</button>
                    {/if}
                  </li>
                {:else}
                  <li class="px-5 py-3 text-body text-faint">{!loaded.pairings ? t('loading') : errors.pairings ? '' : t('noBrowsers')}</li>
                {/each}
              </ul>
            </Section>
          {:else}
            <Locked {status} testid="settings-locked" />
          {/if}
        {:else if !unlocked}
          <Locked {status} testid="settings-locked" />
        {:else if part === 'security'}
          <LockSettings {status} />
          <Security {status} {go} />
        {:else if part === 'recovery'}
          <Recovery {status} />
        {:else if part === 'devices'}
          <Devices {status} {go} />
        {:else}
          {#key status.account}<Profile {status} />{/key}
        {/if}
      </Page>{/key}
    </div>
  </div>
</div>

{#if revokeTarget}
  {@const target = revokeTarget}
  <Confirm focusCancel bind:open={() => !!revokeTarget, (open) => { if (!open) revokeTarget = null }}
    title={t('settingsRevokeBrowser')} description={t('settingsRevokeBrowserHint')} confirm={t('revoke')}
    testid="browser-revoke-dialog" busy={!pairings?.some(p => p.pairing_id === target.pairing_id && p.state === 'active') || !!errors.pairings}
    onConfirm={async () => {
      const turn = generation
      await call('connect/revoke', { pairing: target.pairing_id })
      if (current(turn)) await loadPairings()
    }} onReview={() => loadPairings()}>
    {#snippet body()}<p class="font-medium">{target.label || t('browsers')}</p><Id id={target.pairing_id} full />{/snippet}
  </Confirm>
{/if}
