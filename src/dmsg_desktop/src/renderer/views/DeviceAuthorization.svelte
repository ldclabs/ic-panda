<script lang="ts">
  import { deviceCapabilityName, deviceRoleName } from './security'
  import { onDestroy } from 'svelte'
  import Collapsible from '../components/Collapsible.svelte'
  import Dialog from '../components/Dialog.svelte'
  import Icon from '../components/Icon.svelte'
  import SecretInput from '../components/SecretInput.svelte'
  import QrCamera from '../components/QrCamera.svelte'
  import Fact from '../components/page/Fact.svelte'
  import Pill from '../components/page/Pill.svelte'
  import Id from '../components/page/Id.svelte'
  import Notice from '../components/page/Notice.svelte'
  import { buttonClass } from '../components/ui'
  import { copyText } from '../components/clipboard'
  import { t } from '../i18n/index.svelte'
  import { bridge, call, code, errorText, when } from '../rpc'

  let { disabled = false, enrollmentOnly = false, canInvite = true, account }: { account?: string; disabled?: boolean; enrollmentOnly?: boolean; canInvite?: boolean } = $props()
  type Candidate = { token: string; device_id: string; principal?: string; code?: string; role?: string; capabilities?: string[]; expires_at?: number; kind?: string; route?: string; approvers?: string[]; can_enroll?: boolean; key_id?: string; qr?: string }
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
  let loginApproval = $state('')
  let loginApproved = $state(false)
  let otherAdmins = $state<string[] | null>(null)
  let pending = $state('')
  let enrollment = $state<{ token: string; device_id: string; key_id: string; qr?: string } | null>(null)
  let now = $state(Date.now())
  const wrongKind = $derived(enrollmentOnly && !!candidate && candidate.kind !== 'approval_key')
  const handoff = $derived(candidate?.route === 'handoff' && !wrongKind)
  const expired = $derived(!!candidate?.expires_at && candidate.expires_at <= now)

  function cancelInvitation() {
    if (timer) clearTimeout(timer)
    const previous = invitation
    invitation = null
    if (previous) void call('device/pair_invite_cancel', { id: previous.id }).catch(() => {})
  }

  function setOpen(value: boolean) {
    open = value
    if (!value) { revision++; cancelInvitation(); scanning = false; busy = ''; candidate = null; enrollment = null; request = ''; pending = ''; loginApproval = ''; loginApproved = false; otherAdmins = null; error = '' }
  }

  onDestroy(() => { alive = false; revision++; cancelInvitation() })
  const current = (turn: number) => alive && open && revision === turn

  function setRequest(value: string) {
    revision++; request = value; candidate = null; enrollment = null; error = ''; busy = ''
  }

  async function poll(turn: number, invite: Invitation) {
    try {
      const next: Invitation = await call('device/pair_invite_status', { id: invite.id })
      if (!current(turn)) return
      now = Date.now()
      error = ''
      if (loginApproval) {
        const receipt = await call<{ state: string }>('approval/show', { id: loginApproval })
        if (!current(turn)) return
        if (['rejected', 'cancelled', 'failed', 'expired'].includes(receipt.state)) {
          loginApproval = ''; loginApproved = false; error = t('pairRefreshRequest'); cancelInvitation(); return
        }
        if (receipt.state === 'done') loginApproved = true
        if (receipt.state !== 'done' || next.candidate?.kind === 'login_binding' || !next.candidate) {
          timer = setTimeout(() => void poll(turn, invite), 1500); return
        }
      }
      if (next.candidate) {
        if (next.candidate.kind === 'login_binding') {
          request = next.candidate.token; candidate = next.candidate; return
        }
        const token = next.candidate.token
        const preview = await call<Candidate>('device/pair_inspect', { token })
        if (!current(turn)) return
        loginApproval = ''; loginApproved = false
        request = token; candidate = { ...preview, token, code: next.candidate.code }; now = Date.now()
        cancelInvitation()
      } else timer = setTimeout(() => void poll(turn, invite), 1500)
    } catch (e) {
      if (current(turn)) {
        error = requestError(e)
        if (['UNAVAILABLE', 'NETWORK_UNAVAILABLE', 'EXECUTION_UNKNOWN', 'POLICY_STALE'].includes(code(e)) && invite.expires_at > Date.now()) timer = setTimeout(() => void poll(turn, invite), 5000)
        else cancelInvitation()
      }
    }
  }

  async function invite() {
    cancelInvitation()
    const turn = ++revision
    loginApproval = ''; loginApproved = false; otherAdmins = null; manual = false; scanning = false; candidate = null; enrollment = null; request = ''; pending = ''; error = ''; busy = 'invite'
    try {
      const next = await call<Invitation & { route?: string; approvers?: string[] }>('device/pair_invite', { with_login: true })
      if (next.route === 'handoff') { if (current(turn)) otherAdmins = next.approvers ?? []; return }
      if (!current(turn)) { void call('device/pair_invite_cancel', { id: next.id }).catch(() => {}); return }
      invitation = next
      void poll(turn, next)
    } catch (e) { if (current(turn)) error = errorText(e) }
    finally { if (current(turn)) busy = '' }
  }

  function begin() {
    if (disabled || !canInvite) return
    open = true
    void invite()
  }

  function beginRequest() {
    if (disabled) return
    open = true
    paste()
  }

  function paste() {
    revision++; cancelInvitation(); scanning = false; candidate = null; enrollment = null; request = ''; pending = ''; loginApproval = ''; loginApproved = false; otherAdmins = null; busy = ''; error = ''; manual = true
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
    } catch (e) { if (current(turn)) error = requestError(e) }
    finally { if (current(turn)) busy = '' }
  }

  function requestError(e: unknown) {
    return ['EXPIRED', 'VERSION_CONFLICT'].includes(code(e)) ? t('pairRefreshRequest') : errorText(e)
  }

  async function inspect() {
    if (!request.trim() || busy) return
    const turn = ++revision, token = request.trim()
    busy = 'inspect'; error = ''; candidate = null
    try {
      const preview = await call<Candidate>('device/pair_inspect', { token })
      if (current(turn)) { candidate = { ...preview, token }; now = Date.now() }
    } catch (e) { if (current(turn)) error = requestError(e) }
    finally { if (current(turn)) busy = '' }
  }

  async function enrollHere() {
    if (!candidate?.can_enroll || busy) return
    const turn = revision
    busy = 'enroll'; error = ''
    try {
      const value = await call<NonNullable<typeof enrollment>>('device/key_request', { biometry_only: false })
      if (current(turn)) enrollment = value
    } catch (e) { if (current(turn)) error = errorText(e) }
    finally { if (current(turn)) busy = '' }
  }

  async function review() {
    if (!candidate || busy || expired || disabled || (candidate.route && candidate.route !== 'approve') || (enrollmentOnly && candidate.kind !== 'approval_key')) return
    const turn = revision
    busy = 'submit'; error = ''
    try {
      if (!pending) {
        const result = candidate.kind === 'login_binding'
          ? await call<{ id: string }>('account/bind_approve', { token: candidate.token, principal: candidate.principal })
          : await call<{ id: string }>(candidate.kind === 'approval_key' ? 'device/key_approve' : 'device/approve', { token: candidate.token, device: candidate.device_id })
        if (!current(turn)) return
        pending = result.id
      }
      await bridge(() => window.dmsg.openApproval(pending))
      if (!current(turn)) return
      if (candidate?.kind === 'login_binding' && invitation) {
        loginApproval = pending; loginApproved = false; pending = ''; candidate = null
        void poll(turn, invitation)
      } else setOpen(false)
    } catch (e) {
      if (current(turn)) {
        if (!pending && candidate && code(e) === 'VERSION_CONFLICT') candidate = { ...candidate, route: 'refresh' }
        else error = requestError(e)
      }
    }
    finally { if (current(turn)) busy = '' }
  }

  $effect(() => {
    if (!open) return
    const clock = setInterval(() => { now = Date.now() }, 1000)
    return () => clearInterval(clock)
  })
</script>

<div class="flex flex-wrap gap-2">
  {#if !enrollmentOnly}<button class={buttonClass('secondary', 'sm')} disabled={disabled || !canInvite} onclick={begin} data-testid="device-authorize"><Icon name="pair" />{t('authorizeDevice')}</button>{/if}
  <button class={buttonClass('ghost', 'sm')} {disabled} onclick={beginRequest} data-testid={enrollmentOnly ? "security-key-approve" : "device-pair-open-scan"}><Icon name="scan" />{t(enrollmentOnly ? 'approveKeyRequest' : 'scanDeviceRequest')}</button>
</div>

<Dialog bind:open={() => open, setOpen} title={t(handoff ? (enrollment ? 'pairEnableApprovals' : 'pairContinueElsewhere') : enrollmentOnly || candidate?.kind === 'approval_key' ? 'approveKeyRequest' : 'authorizeDevice')} description={otherAdmins ? t('pairContinueElsewhere') : handoff ? t('pairScanWithAdmin') : candidate?.code ? t('joinCompareHint') : manual ? t('scanDeviceRequestHint') : t('joinShowHint')} icon="pair" dismissible={busy !== 'submit'} testid="device-authorization">
  <div class="grid gap-4">
    {#if error}<Notice failure={error} tone="danger" testid="device-authorization-error">{error}</Notice>{/if}
    {#if manual && !handoff}
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
    {:else if otherAdmins}
      <Notice>{t(otherAdmins.length ? 'pairContinueElsewhere' : 'pairNoApprover')}</Notice>
      {#each otherAdmins as id (id)}<Id {id} full wrap />{/each}
    {:else if loginApproval && invitation && !candidate}
      <div class="grid justify-items-start gap-2">
        <p role="status">{t(loginApproved ? 'joinWaitDevice' : 'joinWaitLogin')}</p>
        {#if !loginApproved}
          <button class={buttonClass('secondary', 'sm')} onclick={() => void bridge(() => window.dmsg.openApproval(loginApproval)).catch(e => { error = errorText(e) })} data-testid="device-pair-open-login-approval">{t('securityViewApproval')}</button>
        {/if}
      </div>
    {:else if invitation && !candidate && invitation.expires_at > now}
      <div class="grid justify-items-center gap-2" data-testid="device-invitation">
        {#if account}<Fact label={t('account')}><Id id={account} full wrap /></Fact>{/if}
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
        {#if candidate.code}<Fact label={t('joinCode')}><b dir="ltr">{candidate.code}</b></Fact>{/if}
        {#if !handoff}
        {#if candidate.principal}<Fact label={t('approveLogin')}><Id id={candidate.principal} full wrap /></Fact>{/if}
        {#if candidate.device_id}<div class="grid min-w-0 gap-1.5"><p class="text-[12px] text-muted">{t('deviceShown')}</p><Id id={candidate.device_id} full wrap /></div>{/if}
        <dl>
          {#if candidate.key_id}<Fact label={t('approvalKey')}><Id id={candidate.key_id} full wrap /></Fact>{/if}
          {#if candidate.role}<Fact label={t('role')}>{deviceRoleName(candidate.role, t)}</Fact>{/if}
          {#if candidate.capabilities}<Fact label={t('capabilities')}>{#each candidate.capabilities as capability (capability)}<Pill>{deviceCapabilityName(capability, t)}</Pill>{/each}</Fact>{/if}
          {#if candidate.expires_at}<Fact label={t('expires')}>{when(candidate.expires_at)}</Fact>{/if}
        </dl>
        {/if}
        {#if wrongKind}
          <Notice tone="warn">{t('keyRequestHint')}</Notice>
        {:else if (expired && !enrollment) || candidate.route === 'refresh'}
          <Notice tone="warn">{t('pairRefreshRequest')}</Notice>
        {:else if candidate.route === 'done'}
          <Notice>{t('done')}</Notice>
        {:else if candidate.route === 'handoff'}
          {#if !enrollment}
            {#if candidate.qr}<img src={candidate.qr} alt={t('requestToken')} width="200" height="200" class="max-w-full justify-self-center rounded-lg bg-white p-2 [image-rendering:pixelated]" />{/if}
            {#if candidate.approvers?.length}
              <Fact label={t('devices')}>{#each candidate.approvers as id (id)}<Id {id} full wrap />{/each}</Fact>
            {:else}<Notice>{t('pairNoApprover')}</Notice>{/if}
            <Collapsible>
              {#snippet title()}{t(candidate?.principal ? 'principalShown' : 'deviceShown')}{/snippet}
              <Id id={candidate.principal ?? candidate.device_id} full wrap />
              {#if candidate.key_id}<Fact label={t('approvalKey')}><Id id={candidate.key_id} full wrap /></Fact>{/if}
            </Collapsible>
            <button class={buttonClass('secondary', 'sm', 'justify-self-start')} onclick={() => copyText(candidate!.token)}>{t('copy')} · {t('requestToken')}</button>
          {/if}
          {#if enrollment}
            <Fact label={t('thisDevice')}><Id id={enrollment.device_id} full wrap /></Fact>
            <Fact label={t('approvalKey')}><Id id={enrollment.key_id} full wrap /></Fact>
            {#if enrollment.qr}<img src={enrollment.qr} alt={t('keyRequest')} width="200" height="200" class="max-w-full justify-self-center rounded-lg bg-white p-2" />{/if}
            <button class={buttonClass('secondary', 'sm', 'justify-self-start')} onclick={() => copyText(enrollment!.token)}>{t('copy')} · {t('keyRequest')}</button>
            {#if candidate.kind === 'device_pair'}<Notice testid="device-pair-refresh-after-enrollment">{t('pairAfterRegistration')}</Notice>{/if}
          {:else if candidate.can_enroll}
            <button class={buttonClass('secondary', 'sm', 'justify-self-start')} disabled={!!busy} onclick={enrollHere} data-testid="device-pair-enroll">{t('pairEnableApprovals')}</button>
          {/if}
        {:else}
          <Notice>{t('securityApprovalHint')}</Notice>
        {/if}
        {#if !manual && (expired || candidate.route === 'refresh')}<button class={buttonClass('secondary', 'sm', 'justify-self-start')} disabled={disabled || !canInvite || !!busy} onclick={invite}>{t('retry')}</button>{/if}
      </div>
    {/if}
  </div>
  {#snippet footer()}
    {#if handoff}<button class={buttonClass('ghost', 'sm')} disabled={!!busy} onclick={paste}>{t('scanDeviceRequest')}</button>{/if}
    {#if !candidate && !enrollmentOnly && canInvite}<button class={buttonClass('ghost', 'sm')} disabled={busy === 'submit'} onclick={() => manual ? invite() : paste()} data-testid="device-pair-mode">{manual ? t('authorizeDevice') : t('scanDeviceRequest')}</button>{/if}
    <span class="flex-1"></span>
    <button class={buttonClass('secondary')} disabled={busy === 'submit'} onclick={() => setOpen(false)}>{t('cancel')}</button>
    {#if candidate && (!candidate.route || candidate.route === 'approve') && (!enrollmentOnly || candidate.kind === 'approval_key')}<button class={buttonClass('primary')} disabled={!!busy || expired || disabled} onclick={review} data-testid="device-pair-review">{pending ? t('securityViewApproval') : t('review')}</button>{/if}
  {/snippet}
</Dialog>
