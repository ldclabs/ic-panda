<script lang="ts">
  import { onDestroy, onMount, untrack } from 'svelte'
  import { accountJoinRequested, deviceRequestQr } from '../shell/session'
  import Dialog from '../components/Dialog.svelte'
  import QrCamera from '../components/QrCamera.svelte'
  import SecretInput from '../components/SecretInput.svelte'
  import Fact from '../components/page/Fact.svelte'
  import Id from '../components/page/Id.svelte'
  import Notice from '../components/page/Notice.svelte'
  import { buttonClass } from '../components/ui'
  import { t } from '../i18n/index.svelte'
  import { bridge, call, code, errorText } from '../rpc'
  import { login, startLogin } from '../shell/login.svelte'

  let { status, refresh }: { status: any; refresh: () => Promise<void> } = $props()
  type Preview = { stage: string; account: string; current_account?: string; principal?: string; device_id?: string; code?: string; expires_at: number }
  let open = $state(false), scanning = $state(false), busy = $state(false)
  let token = $state(''), error = $state(''), view = $state<Preview | null>(null)
  let revision = 0, timer: ReturnType<typeof setTimeout> | undefined
  let paused = $state(false), expired = $state(false), directAfterLogin = $state(false)
  let retryContinuation = $state(false)
  function edit(value: string) {
    revision++; clearTimeout(timer); token = value; view = null; busy = false; paused = false; expired = false; error = ''; retryContinuation = false
  }
  function show(value: boolean) {
    if (!value) accountJoinRequested.set(false)
    open = value; scanning = false; directAfterLogin = false; edit('')
    if (value) {
      const turn = revision
      void call<string | null>('device/saved_invitation').then(saved => {
        if (open && revision === turn && saved) { edit(saved); void check() }
      }).catch(() => {})
    }
  }
  onMount(() => accountJoinRequested.subscribe(value => { if (value && !open) show(true) }))
  onDestroy(() => { revision++; clearTimeout(timer); if (open) accountJoinRequested.set(false) })
  function schedule() {
    clearTimeout(timer)
    if (open && !paused && ['waiting_login','waiting_device','authorized'].includes(view?.stage ?? '')) {
      timer = setTimeout(() => {
        if (document.hidden || !status || login.phase !== 'idle') schedule()
        else void check(true)
      }, 4000)
    }
  }
  async function check(continueJoin = false) {
    if (busy || !token.trim()) return
    const turn = revision
    clearTimeout(timer)
    retryContinuation = continueJoin
    busy = true; error = ''; expired = false
    try {
      if (continueJoin && view?.principal && view.principal === status?.principal && ['waiting_device', 'authorized'].includes(view.stage)) {
        const needsUnlock = view.stage === 'authorized' && status.state === 'locked'
        const unlocked = await call<{ state?: string; account?: string } | null>('session/unlock', needsUnlock ? {} : { pairing_poll: true })
        await refresh()
        if (turn !== revision) return
        const nextState = unlocked?.state ?? status?.state
        const nextAccount = unlocked?.account ?? status?.account
        if (nextState === 'unlocked' && nextAccount === view?.account) { show(false); return }
        if (nextState === 'root_required') { schedule(); return }
        if (view?.stage === 'authorized' && nextState === 'locked') { paused = true; return }
      }
      const result = await call<Preview>('device/join_invitation', { invitation: token.trim(), ...(continueJoin && view?.principal ? {principal:view.principal, bind_login:view.stage === 'bind_login'} : {}) })
      if (turn !== revision) return
      if (result.stage === 'authorized' && status?.state === 'unlocked' && status?.account === result.account) { show(false); return }
      view = result; schedule()
    } catch (e) {
      if (turn !== revision) return
      const failure = code(e)
      error = errorText(e)
      if (failure === 'EXPIRED') { view = null; expired = true; paused = true; return }
      if (['AUTH_REQUIRED', 'LOGIN_EXPIRED'].includes(failure) && view) {
        view = { account: view.account, expires_at: view.expires_at, stage: 'sign_in' }; paused = true; return
      }
      if (!['DAEMON_DISCONNECTED', 'NETWORK_UNAVAILABLE', 'UNAVAILABLE', 'EXECUTION_UNKNOWN', 'POLICY_STALE', 'SYNC_BUSY'].includes(failure)) paused = true
      schedule()
    } finally { if (turn === revision) busy = false }
  }
  async function scan(image: string) {
    const turn = revision
    const result = await call<Preview & {token:string}>('device/pair_scan', { image, invitation:true })
    if (turn === revision && scanning) { edit(result.token); view = result; scanning = false; schedule() }
  }
  async function scanImage(source: 'file' | 'clipboard') {
    const turn = revision
    busy = true; error = ''
    try {
      const result = await bridge<(Preview & {token:string}) | null>(() => window.dmsg.deviceQrImage(source, true))
      if (turn === revision && result) { edit(result.token); view = result; schedule() }
    } catch (e) { if (turn === revision) error = errorText(e) }
    finally { if (turn === revision) busy = false }
  }
  function showRequest() {
    const state = status?.state
    if (state === 'no_account' || state === 'pairing_required') {
      show(false)
      deviceRequestQr.set({ state, principal: status.principal })
    }
    else { directAfterLogin = true; void startLogin() }
  }
  let lastSession: string | undefined
  $effect(() => {
    const checked = status, currentBusy = busy, loggingIn = login.phase !== 'idle'
    untrack(() => {
      if (!checked || currentBusy || loggingIn) return
      const principal = checked.principal
      const session = `${checked.session}:${principal}`
      const changed = session !== lastSession
      lastSession = session
      if (!open) return
      if (directAfterLogin && ['no_account','pairing_required'].includes(checked.state)) { showRequest(); return }
      if (!view || !changed) return
      if (principal && (view.stage === 'sign_in' || view.principal !== principal)) { view = null; void check() }
      else if (!principal && view.principal) show(false)
    })
  })
</script>
<Dialog bind:open={() => open, show} title={t('joinOpen')} description={t(view?.code ? 'joinCompareHint' : 'joinScanHint')} icon="pair" testid="join-account-invitation">
  <div class="grid gap-3" aria-busy={busy}>
    {#if scanning}
      <QrCamera onImage={scan} onCancel={() => { revision++; scanning = false }} />
    {:else if !view}
      <div class="flex flex-wrap gap-2">
        <button class={buttonClass('secondary','sm')} disabled={busy} onclick={() => { edit(''); scanning = true }}>{t('scanQrCamera')}</button>
        <button class={buttonClass('secondary','sm')} disabled={busy} onclick={() => scanImage('file')}>{t('scanQrImage')}</button>
        <button class={buttonClass('secondary','sm')} disabled={busy} onclick={() => scanImage('clipboard')}>{t('scanQrClipboard')}</button>
      </div>
      {#if ['logged_out','no_account', 'pairing_required'].includes(status?.state)}
        <button class={buttonClass('secondary','sm')} disabled={busy} onclick={showRequest} data-testid="join-show-request">{t('joinShowRequest')}</button>
      {/if}
      <SecretInput label={t('pasteRequest')} multiline bind:value={() => token, edit} testid="join-invitation-token" />
      <button class={buttonClass('primary')} disabled={!token.trim() || busy} onclick={() => check()}>{t('review')}</button>
    {:else}
      <dl>
        <Fact label={t('account')}><Id id={view.account} full wrap /></Fact>
        {#if view.stage === 'bind_login' && view.principal}<Fact label={t('loginPrincipal')}><Id id={view.principal} full wrap /></Fact>{/if}
        {#if view.code}<Fact label={t('joinCode')}><b dir="ltr">{view.code}</b></Fact>{/if}
        {#if view.device_id && !view.code}<Fact label={t('device')}><Id id={view.device_id} full wrap /></Fact>{/if}
        {#if view.stage === 'waiting_device'}<Fact label={t('loginPrincipal')}>{t('done')}</Fact>{/if}
      </dl>
      {#if ['sign_in','other_account','linked_login_required'].includes(view.stage)}
        {#if view.stage !== 'sign_in'}<Notice>{t(view.stage === 'other_account' ? 'joinOther' : 'passkeyExistingHint')}</Notice>{/if}
        <button class={buttonClass('primary')} disabled={busy} onclick={() => startLogin()}>{t('login')}</button>
      {:else if view.stage === 'bind_login' || view.stage === 'device'}
        <button class={buttonClass('primary')} disabled={busy} onclick={() => check(true)}>{t(view.stage === 'bind_login' ? 'joinConfirm' : 'pairRequest')}</button>
      {:else}
        <p role="status">{t(status?.state === 'root_required' ? 'pairPreparingVault' : view.stage === 'waiting_login' ? 'joinWaitLogin' : 'joinWaitDevice')}</p>
      {/if}
    {/if}
    {#if busy}<p class="text-[13px] text-muted" role="status">{t('loading')}</p>{/if}
    {#if error}<Notice failure={error} tone="danger">{error}</Notice>{#if !expired && token.trim()}<button class={buttonClass('secondary')} disabled={busy} onclick={() => { paused = false; void check(retryContinuation) }}>{t('retry')}</button>{/if}{/if}
  </div>
</Dialog>
