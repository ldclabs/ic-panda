<script lang="ts">
  import { onDestroy } from 'svelte'
  import Dialog from '../components/Dialog.svelte'
  import Icon from '../components/Icon.svelte'
  import SecretInput from '../components/SecretInput.svelte'
  import QrCamera from '../components/QrCamera.svelte'
  import Fact from '../components/page/Fact.svelte'
  import Id from '../components/page/Id.svelte'
  import Notice from '../components/page/Notice.svelte'
  import { buttonClass } from '../components/ui'
  import { copyText } from '../components/clipboard'
  import { t } from '../i18n/index.svelte'
  import { bridge, call, code, errorText, when } from '../rpc'

  let { disabled = false }: { disabled?: boolean } = $props()
  type Candidate = { token: string; device_id: string; role: string; capabilities: string[]; expires_at: number }
  type Invitation = { id: string; token: string; qr: string; expires_at: number; candidate?: Candidate }
  let alive = true
  let revision = 0
  let timer: ReturnType<typeof setTimeout> | undefined
  let open = $state(false)
  let manual = $state(false)
  let scanning = $state(false)
  let cameraTurn = 0
  let request = $state('')
  let candidate = $state<Candidate | null>(null)
  let invitation = $state<Invitation | null>(null)
  let busy = $state('')
  let error = $state('')
  let pending = $state('')
  let now = $state(Date.now())
  const expired = $derived(!!candidate && candidate.expires_at <= now)

  function cancelInvitation() {
    if (timer) clearTimeout(timer)
    const previous = invitation
    invitation = null
    if (previous) void call('device/pair_invite_cancel', { id: previous.id }).catch(() => {})
  }

  function setOpen(value: boolean) {
    open = value
    if (!value) { revision++; cancelInvitation(); scanning = false; busy = ''; candidate = null; request = ''; pending = ''; error = '' }
  }

  onDestroy(() => { alive = false; revision++; cancelInvitation() })
  const current = (turn: number) => alive && open && revision === turn

  function setRequest(value: string) {
    revision++; request = value; candidate = null; error = ''; busy = ''
  }

  async function poll(turn: number, invite: Invitation) {
    try {
      const next: Invitation = await call('device/pair_invite_status', { id: invite.id })
      if (!current(turn)) return
      now = Date.now()
      error = ''
      if (next.candidate) {
        candidate = next.candidate
        request = next.candidate.token
        cancelInvitation()
      } else timer = setTimeout(() => void poll(turn, invite), 1500)
    } catch (e) {
      if (current(turn)) {
        error = errorText(e)
        if (['UNAVAILABLE', 'EXECUTION_UNKNOWN'].includes(code(e)) && invite.expires_at > Date.now()) timer = setTimeout(() => void poll(turn, invite), 5000)
        else cancelInvitation()
      }
    }
  }

  async function invite() {
    cancelInvitation()
    const turn = ++revision
    manual = false; scanning = false; candidate = null; request = ''; pending = ''; error = ''; busy = 'invite'
    try {
      const next: Invitation = await call('device/pair_invite')
      if (!current(turn)) { void call('device/pair_invite_cancel', { id: next.id }).catch(() => {}); return }
      invitation = next
      void poll(turn, next)
    } catch (e) { if (current(turn)) error = errorText(e) }
    finally { if (current(turn)) busy = '' }
  }

  function begin() {
    if (disabled) return
    open = true
    void invite()
  }

  function beginRequest() {
    if (disabled) return
    open = true
    paste()
  }

  function paste() {
    revision++; cancelInvitation(); scanning = false; candidate = null; request = ''; pending = ''; busy = ''; error = ''; manual = true
  }

  function camera() {
    if (busy || pending) return
    cameraTurn = ++revision
    candidate = null; error = ''; scanning = true
  }

  async function readFrame(image: string) {
    const turn = cameraTurn
    const preview = await call<Candidate>('device/pair_scan', { image })
    if (!current(turn) || !scanning) return
    request = preview.token; candidate = preview; now = Date.now(); scanning = false
  }

  async function scanImage(source: 'file' | 'clipboard') {
    if (busy || pending) return
    const turn = ++revision
    busy = 'scan'; error = ''; candidate = null
    try {
      const preview = await bridge<Candidate | null>(() => window.dmsg.deviceQrImage(source))
      if (current(turn) && preview) { request = preview.token; candidate = preview; now = Date.now() }
    } catch (e) { if (current(turn)) error = errorText(e) }
    finally { if (current(turn)) busy = '' }
  }

  async function inspect() {
    if (!request.trim() || busy) return
    const turn = ++revision, token = request.trim()
    busy = 'inspect'; error = ''; candidate = null
    try {
      const preview = await call<Candidate>('device/pair_inspect', { token })
      if (current(turn)) { candidate = { ...preview, token }; now = Date.now() }
    } catch (e) { if (current(turn)) error = errorText(e) }
    finally { if (current(turn)) busy = '' }
  }

  async function review() {
    if (!candidate || busy || expired || disabled) return
    const turn = revision
    busy = 'submit'; error = ''
    try {
      if (!pending) {
        const result = await call<{ id: string }>('device/approve', { token: candidate.token, device: candidate.device_id })
        if (!current(turn)) return
        pending = result.id
      }
      await bridge(() => window.dmsg.openApproval(pending))
      if (current(turn)) setOpen(false)
    } catch (e) { if (current(turn)) error = errorText(e) }
    finally { if (current(turn)) busy = '' }
  }

  $effect(() => {
    if (!open) return
    const clock = setInterval(() => { now = Date.now() }, 1000)
    return () => clearInterval(clock)
  })
</script>

<div class="flex flex-wrap gap-2">
  <button class={buttonClass('secondary', 'sm')} {disabled} onclick={begin} data-testid="device-authorize"><Icon name="pair" />{t('authorizeDevice')}</button>
  <button class={buttonClass('ghost', 'sm')} {disabled} onclick={beginRequest} data-testid="device-pair-open-scan"><Icon name="scan" />{t('scanDeviceRequest')}</button>
</div>

<Dialog bind:open={() => open, setOpen} title={t('authorizeDevice')} description={manual ? t('scanDeviceRequestHint') : t('authorizeDeviceHint')} icon="pair" dismissible={busy !== 'submit'} testid="device-authorization">
  <div class="grid gap-4">
    {#if error}<Notice failure={error} tone="danger" testid="device-authorization-error">{error}</Notice>{/if}
    {#if manual}
      {#if scanning}
        <QrCamera onImage={readFrame} onCancel={() => { revision++; scanning = false }} />
      {:else}
      <div class="flex flex-wrap gap-2">
        <button class={buttonClass('secondary', 'sm')} disabled={!!busy || !!pending} onclick={camera} data-testid="device-pair-camera"><Icon name="scan" />{t('scanQrCamera')}</button>
        <button class={buttonClass('secondary', 'sm')} disabled={!!busy || !!pending} onclick={() => scanImage('file')} data-testid="device-pair-image">{t('scanQrImage')}</button>
        <button class={buttonClass('secondary', 'sm')} disabled={!!busy || !!pending} onclick={() => scanImage('clipboard')} data-testid="device-pair-clipboard">{t('scanQrClipboard')}</button>
      </div>
      <SecretInput label={t('pasteRequest')} bind:value={() => request, setRequest} multiline disabled={busy === 'submit' || !!pending} testid="device-pair-request" />
      <button class={buttonClass('secondary', 'sm', 'justify-self-start')} disabled={!!busy || !request.trim() || !!pending} onclick={inspect} data-testid="device-pair-inspect">{t('review')}</button>
      {/if}
    {:else if invitation && !candidate && invitation.expires_at > now}
      <div class="grid justify-items-center gap-2" data-testid="device-invitation">
        <img src={invitation.qr} alt={t('authorizeDevice')} width="224" height="224" class="size-[min(224px,calc(100vh-340px))] min-h-40 min-w-40 max-w-full rounded-lg bg-white p-2 [image-rendering:pixelated]" data-testid="device-invitation-qr" />
        <p class="text-center text-[13px] text-muted" role="status">{t('pairInvitationWaiting')}</p>
        <p class="text-[12px] text-faint">{t('expires')}: {when(invitation.expires_at)}</p>
        <button class={buttonClass('secondary', 'xs')} onclick={() => copyText(invitation!.token)} data-testid="device-invitation-copy"><Icon name="copy" />{t('copy')}</button>
      </div>
    {:else if busy === 'invite'}
      <p class="py-12 text-center text-muted" role="status">{t('loading')}</p>
    {:else if !candidate}
      {#if invitation && invitation.expires_at <= now}<Notice tone="warn">{t('reasonExpired')}</Notice>{/if}
      <button class={buttonClass('secondary', 'sm', 'justify-self-start')} onclick={invite} data-testid="device-invitation-retry">{t('retry')}</button>
    {/if}
    {#if candidate}
      <div class="grid min-w-0 gap-3" data-testid="device-pair-preview">
        <div class="grid min-w-0 gap-1.5"><p class="text-[12px] text-muted">{t('deviceShown')}</p><Id id={candidate.device_id} full wrap /></div>
        <dl><Fact label={t('role')}>{candidate.role}</Fact><Fact label={t('capabilities')}>{candidate.capabilities.join(', ')}</Fact><Fact label={t('expires')}>{when(candidate.expires_at)}</Fact></dl>
        <Notice tone={expired ? 'warn' : 'info'}>{expired ? t('reasonExpired') : t('securityApprovalHint')}</Notice>
        {#if expired && !manual}<button class={buttonClass('secondary', 'sm', 'justify-self-start')} disabled={!!busy} onclick={invite}>{t('retry')}</button>{/if}
      </div>
    {/if}
  </div>
  {#snippet footer()}
    {#if !candidate}<button class={buttonClass('ghost', 'sm')} disabled={busy === 'submit'} onclick={() => manual ? invite() : paste()} data-testid="device-pair-mode">{manual ? t('authorizeDevice') : t('scanDeviceRequest')}</button>{/if}
    <span class="flex-1"></span>
    <button class={buttonClass('secondary')} disabled={busy === 'submit'} onclick={() => setOpen(false)}>{t('cancel')}</button>
    {#if candidate}<button class={buttonClass('primary')} disabled={!!busy || expired || disabled} onclick={review} data-testid="device-pair-review">{pending ? t('securityViewApproval') : t('review')}</button>{/if}
  {/snippet}
</Dialog>
