<script lang="ts">
  import { onDestroy, onMount, untrack } from 'svelte'
  import Checkbox from '../components/Checkbox.svelte'
  import CopyButton from '../components/CopyButton.svelte'
  import Dialog from '../components/Dialog.svelte'
  import Icon from '../components/Icon.svelte'
  import Progress from '../components/Progress.svelte'
  import QrCamera from '../components/QrCamera.svelte'
  import Switch from '../components/Switch.svelte'
  import TextField from '../components/TextField.svelte'
  import Fact from '../components/page/Fact.svelte'
  import Id from '../components/page/Id.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Pill from '../components/page/Pill.svelte'
  import { buttonClass, cn } from '../components/ui'
  import { t } from '../i18n/index.svelte'
  import { call, code, errorText, size, when } from '../rpc'
  import FolderPicker from './files/FolderPicker.svelte'
  import { ROOT } from './files/files'
  import PackageImport from './secrets/PackageImport.svelte'
  import { ux } from './secrets/experience-text'
  import { toggle } from './secrets/model'
  import { openFile } from './secrets/navigation.svelte'
  import { workflowText as w } from './secrets/workflow-text'
  import {
    buildSendStartParams,
    formatSas,
    identityStatusLabelKey,
    isReceiveActive,
    isSendActive,
    isValidNearbyOffer,
    mergeReceiveView,
    progressFraction,
    receiveStateBadge,
    reconcilePeerSelections,
    sendStateBadge,
    transportLabelKey,
    vaultEntryBlocked,
    type DiscoveredPeer,
    type NearbyError,
    type NearbyFileRef,
    type NearbyMode,
    type NearbyPeerIdentity,
    type NearbyReceiveView,
    type NearbySendView,
    type NearbyStatus,
    type NearbyVaultSelection
  } from './nearby'

  /**
   * Nearby Share (`dmsg-nearby-share/1`) dialog for both Cloud and Standalone
   * workspaces: receive files or Vault snapshots via QR/offer or opt-in local
   * discovery, and send selected local files or a Vault item snapshot.
   */
  let {
    open = $bindable(false),
    mode = 'receive',
    files = [],
    vaultEntry = null,
    vaultConflict = false,
    presetVaultSelection = null,
    initialFolder = ROOT
  }: {
    open?: boolean
    mode?: NearbyMode
    files?: NearbyFileRef[]
    vaultEntry?: any | null
    vaultConflict?: boolean
    presetVaultSelection?: NearbyVaultSelection | null
    initialFolder?: string
  } = $props()

  let alive = true
  let sessionRev = 0
  let wasOpen = false
  let activeMode: NearbyMode = 'receive'

  let busy = $state(false)
  let error = $state('')

  // Receive state
  let receiveView = $state<NearbyReceiveView | null>(null)
  let activeReceiveId = $state<number | null>(null)
  let receivePending = false
  let ignoredReceiveId: number | null = null
  let lastMdnsError = $state<{ id: number; error: string } | null>(null)
  let discoverable = $state(false)
  let folders = $state<any[]>([])
  let folder = $state(ROOT)
  let folderPickerOpen = $state(false)
  let peerSelections = $state<Record<string, number[]>>({})
  let peerShareIdentity = $state<Record<string, boolean>>({})

  // Send state
  let sendView = $state<NearbySendView | null>(null)
  let activeSendId = $state<number | null>(null)
  let sendPending = false
  let ignoredSendId: number | null = null
  let offerText = $state('')
  let scanning = $state(false)
  let browsing = $state(false)
  let browsedOnce = $state(false)
  let browseErrors = $state<string[]>([])
  let discoveredPeers = $state<DiscoveredPeer[]>([])
  let selectedPeer = $state<DiscoveredPeer | null>(null)
  let shareIdentity = $state(false)

  const vaultChanged = $derived(mode === 'send-vault' && presetVaultSelection?.revision !== vaultEntry?.revision)
  const vaultBlocked = $derived(mode === 'send-vault' && (vaultChanged || vaultEntryBlocked(vaultEntry, vaultConflict)))
  const offerError = $derived(
    offerText.trim() && !isValidNearbyOffer(offerText) ? t('nearbyOfferInvalid') : ''
  )
  const sendParams = $derived(
    mode === 'receive'
      ? null
      : buildSendStartParams({
          mode,
          offerText,
          selectedPeer,
          files,
          vaultSelection: vaultBlocked ? null : presetVaultSelection,
          shareIdentity
        })
  )
  const totalSendFileBytes = $derived(files.reduce((sum, f) => sum + (f.size ?? 0), 0))
  const receivingActive = $derived(isReceiveActive(receiveView))
  const sendingActive = $derived(isSendActive(sendView))
  const receiveBadge = $derived(receiveStateBadge(receiveView?.state))
  const sendBadge = $derived(sendStateBadge(sendView?.state))
  const receiveProgress = $derived(progressFraction(receiveView?.progress))
  const sendProgress = $derived(progressFraction(sendView?.progress))
  const folderName = $derived(
    folder === ROOT ? t('library') : (folders.find((f) => f.id === folder)?.name ?? folder)
  )
  const dialogTitle = $derived(mode === 'receive' ? t('nearbyReceive') : t('nearbyShare'))
  const dialogDescription = $derived(
    mode === 'receive'
      ? t('nearbyReceiveHint')
      : mode === 'send-vault'
        ? (vaultEntry?.item?.title ?? t('nearbySendVaultHint'))
        : t('nearbySendFilesHint')
  )
  const showSendForm = $derived(mode !== 'receive' && !sendView)

  function describeCallError(e: unknown): string {
    const c = code(e)
    if (c === 'INVALID_INPUT') return t('nearbyOfferInvalid')
    if (c === 'EXPIRED') return t('nearbyExpired')
    if (c === 'QUOTA_EXCEEDED') return t('nearbyQuotaExceeded')
    if (c === 'VERSION_CONFLICT') return t('nearbyVaultChanged')
    return errorText(e)
  }

  function describeViewError(err: NearbyError | null | undefined): string {
    if (!err) return ''
    if (err.code === 'EXPIRED') return t('nearbyExpired')
    if (err.code === 'QUOTA_EXCEEDED') return t('nearbyQuotaExceeded')
    if (err.code === 'VERSION_CONFLICT') return t('nearbyVaultChanged')
    if (err.code === 'LOCKED') return t('locked')
    return err.message || t('nearbyStateFailed')
  }

  function clearStaleSendError() {
    error = ''
    if (sendView && (sendView.state === 'failed' || sendView.state === 'cancelled')) {
      sendView = null
      activeSendId = null
    }
  }

  function applyStatus(status: NearbyStatus | null | undefined, turn: number) {
    if (!alive || !open || turn !== sessionRev || !status) return
    if (mode === 'receive') {
      const next = status.receive
      if (!next) return
      if (receivePending) {
        if (next.id === ignoredReceiveId) return
        activeReceiveId = next.id
      } else if (activeReceiveId !== null && next.id !== activeReceiveId) {
        return
      }
      if (next.mdns_error) lastMdnsError = { id: next.id, error: next.mdns_error }
      const merged = mergeReceiveView(next, lastMdnsError)
      receiveView = merged
      if (merged) {
        discoverable = merged.discoverable
        peerSelections = reconcilePeerSelections(peerSelections, merged.pending_peers ?? [])
      }
    } else {
      const next = status.send
      if (!next) return
      if (sendPending) {
        if (next.id === ignoredSendId) return
        activeSendId = next.id
      } else if (activeSendId === null || next.id !== activeSendId) {
        return
      }
      sendView = next
    }
  }

  async function refreshStatus(turn: number) {
    try {
      const status = await call<NearbyStatus>('nearby/status')
      applyStatus(status, turn)
    } catch {
      /* session may have locked; view will close */
    }
  }

  async function loadFolders(turn: number) {
    try {
      const tree = await call<any[]>('files/tree')
      if (alive && open && turn === sessionRev) folders = tree
    } catch {
      /* keep root fallback */
    }
  }

  async function startReceive(wantDiscoverable: boolean, turn = sessionRev) {
    busy = true
    error = ''
    receivePending = true
    ignoredReceiveId = receiveView?.id ?? activeReceiveId
    try {
      const status = await call<NearbyStatus>('nearby/receive_start', { discoverable: wantDiscoverable })
      if (!alive || !open || turn !== sessionRev) return
      receivePending = false
      if (status.receive) {
        activeReceiveId = status.receive.id
        lastMdnsError = status.receive.mdns_error
          ? { id: status.receive.id, error: status.receive.mdns_error }
          : null
        receiveView = status.receive
        discoverable = status.receive.discoverable
        peerSelections = reconcilePeerSelections({}, status.receive.pending_peers ?? [])
      }
    } catch (e) {
      receivePending = false
      if (alive && open && turn === sessionRev) {
        error = describeCallError(e)
        discoverable = receiveView?.discoverable ?? false
      }
    } finally {
      if (alive && turn === sessionRev) busy = false
    }
  }

  async function toggleDiscoverable(checked: boolean) {
    if (busy || receiveView?.state === 'running') return
    discoverable = checked
    await startReceive(checked)
  }

  async function acceptPeer(peerId: string, kind: 'files' | 'secret_package') {
    if (busy) return
    const turn = sessionRev
    const selected = kind === 'files' ? (peerSelections[peerId] ?? []) : []
    if (kind === 'files' && !selected.length) return
    busy = true
    error = ''
    try {
      const status = await call<NearbyStatus>('nearby/receive_accept', {
        peer_id: peerId,
        folder,
        selected,
        share_identity: Boolean(peerShareIdentity[peerId])
      })
      applyStatus(status, turn)
    } catch (e) {
      if (alive && open && turn === sessionRev) error = describeCallError(e)
    } finally {
      if (alive && turn === sessionRev) busy = false
    }
  }

  async function rejectPeer(peerId: string) {
    if (busy) return
    const turn = sessionRev
    busy = true
    error = ''
    try {
      const status = await call<NearbyStatus>('nearby/receive_reject', { peer_id: peerId })
      applyStatus(status, turn)
    } catch (e) {
      if (alive && open && turn === sessionRev) error = describeCallError(e)
    } finally {
      if (alive && turn === sessionRev) busy = false
    }
  }

  async function cancelReceive() {
    if (busy) return
    const turn = sessionRev
    busy = true
    error = ''
    try {
      const status = await call<NearbyStatus>('nearby/receive_cancel', {})
      applyStatus(status, turn)
    } catch (e) {
      if (alive && open && turn === sessionRev) error = describeCallError(e)
    } finally {
      if (alive && turn === sessionRev) busy = false
    }
  }

  async function openFolderPicker() {
    const turn = sessionRev
    await loadFolders(turn)
    if (alive && open && turn === sessionRev) folderPickerOpen = true
  }

  async function scanQr(image: string) {
    const turn = sessionRev
    const result = await call<{ offer: string }>('nearby/scan', { image })
    if (!alive || !open || turn !== sessionRev || !scanning) return
    offerText = result.offer
    selectedPeer = null
    scanning = false
    clearStaleSendError()
  }

  async function browsePeers() {
    if (browsing || busy) return
    const turn = sessionRev
    browsing = true
    error = ''
    browseErrors = []
    try {
      const result = await call<{ peers: DiscoveredPeer[]; errors: string[] }>('nearby/browse', {
        timeout_ms: 2500
      })
      if (!alive || !open || turn !== sessionRev) return
      discoveredPeers = result.peers ?? []
      browseErrors = result.errors ?? []
      browsedOnce = true
      if (
        selectedPeer &&
        !discoveredPeers.some(
          (p) => p.advert === selectedPeer?.advert && p.transport === selectedPeer?.transport
        )
      ) {
        selectedPeer = null
      }
    } catch (e) {
      if (alive && open && turn === sessionRev) error = describeCallError(e)
    } finally {
      if (alive && turn === sessionRev) browsing = false
    }
  }

  function pickDiscoveredPeer(peer: DiscoveredPeer) {
    selectedPeer = peer
    offerText = ''
    scanning = false
    clearStaleSendError()
  }

  function onOfferInput() {
    if (offerText.trim()) selectedPeer = null
    clearStaleSendError()
  }

  async function startSend() {
    const params = sendParams
    if (busy || !params) return
    const turn = sessionRev
    busy = true
    error = ''
    scanning = false
    sendPending = true
    ignoredSendId = sendView?.id ?? activeSendId
    try {
      const status = await call<NearbyStatus>('nearby/send_start', params)
      if (!alive || !open || turn !== sessionRev) return
      sendPending = false
      if (status.send) {
        activeSendId = status.send.id
        sendView = status.send
      }
    } catch (e) {
      sendPending = false
      if (alive && open && turn === sessionRev) error = describeCallError(e)
    } finally {
      if (alive && turn === sessionRev) busy = false
    }
  }

  async function confirmSas() {
    if (busy) return
    const turn = sessionRev
    busy = true
    error = ''
    try {
      const status = await call<NearbyStatus>('nearby/send_confirm_sas', {})
      applyStatus(status, turn)
    } catch (e) {
      if (alive && open && turn === sessionRev) error = describeCallError(e)
    } finally {
      if (alive && turn === sessionRev) busy = false
    }
  }

  async function cancelSend() {
    if (busy) return
    const turn = sessionRev
    busy = true
    error = ''
    try {
      const status = await call<NearbyStatus>('nearby/send_cancel', {})
      applyStatus(status, turn)
    } catch (e) {
      if (alive && open && turn === sessionRev) error = describeCallError(e)
    } finally {
      if (alive && turn === sessionRev) busy = false
    }
  }

  function resetDraft() {
    sendView = null
    activeSendId = null
    error = ''
  }

  function resetAllState() {
    busy = false
    error = ''
    receiveView = null
    activeReceiveId = null
    receivePending = false
    ignoredReceiveId = null
    lastMdnsError = null
    discoverable = false
    folder = initialFolder || ROOT
    folderPickerOpen = false
    peerSelections = {}
    peerShareIdentity = {}
    sendView = null
    activeSendId = null
    sendPending = false
    ignoredSendId = null
    offerText = ''
    scanning = false
    browsing = false
    browsedOnce = false
    browseErrors = []
    discoveredPeers = []
    selectedPeer = null
    shareIdentity = false
  }

  // Open / close lifecycle with clean cancellation of active sessions.
  $effect(() => {
    const isOpen = open
    untrack(() => {
      if (isOpen && !wasOpen) {
        wasOpen = true
        activeMode = mode
        sessionRev += 1
        const turn = sessionRev
        resetAllState()
        if (mode === 'receive') {
          void loadFolders(turn)
          void startReceive(false, turn)
        }
      } else if (!isOpen && wasOpen) {
        wasOpen = false
        const cancelRecv = activeMode === 'receive' && (isReceiveActive(receiveView) || receivePending)
        const cancelSnd = activeMode !== 'receive' && (isSendActive(sendView) || sendPending)
        sessionRev += 1
        resetAllState()
        if (cancelRecv) void call('nearby/receive_cancel', {}).catch(() => {})
        if (cancelSnd) void call('nearby/send_cancel', {}).catch(() => {})
      }
    })
  })

  onMount(() => {
    const off = window.dmsg.onEvent((event) => {
      if (!alive || !open) return
      if (event.type === 'nearby/changed') {
        applyStatus(event.data as NearbyStatus | undefined, sessionRev)
      } else if (event.type === 'resync' || event.type === 'session/changed') {
        void refreshStatus(sessionRev)
      }
    })
    return off
  })

  onDestroy(() => {
    alive = false
    const cancelRecv = wasOpen && activeMode === 'receive' && (isReceiveActive(receiveView) || receivePending)
    const cancelSnd = wasOpen && activeMode !== 'receive' && (isSendActive(sendView) || sendPending)
    sessionRev += 1
    if (cancelRecv) void call('nearby/receive_cancel', {}).catch(() => {})
    if (cancelSnd) void call('nearby/send_cancel', {}).catch(() => {})
  })
</script>

{#snippet identityNotice(identity: NearbyPeerIdentity)}
  <div class="rounded-xl border border-line bg-surface-2 p-3 text-callout" data-testid="nearby-peer-identity">
    <div class="mb-1.5 flex flex-wrap items-center gap-1.5">
      <span class="font-medium text-ink">{t('nearbyPeerIdentity')}</span>
      <Pill tone={identity.status === 'signature_only' ? 'neutral' : 'ok'}>
        <Icon name="verified" />{t(identityStatusLabelKey(identity.status))}
      </Pill>
    </div>
    <dl>
      {#if identity.handle}
        <Fact label={t('name')}><bdi dir="ltr" class="font-mono text-code select-text">{identity.handle}</bdi></Fact>
      {/if}
      <Fact label={t('account')}><Id id={identity.account} /></Fact>
      <Fact label={t('device')}><Id id={identity.device} /></Fact>
    </dl>
  </div>
{/snippet}

<Dialog
  bind:open
  title={dialogTitle}
  description={dialogDescription}
  icon="transfer"
  testid="nearby-share-dialog"
>
  <div class="grid gap-4">
    {#if error}
      <Notice failure={error} tone="danger" testid="nearby-error">{error}</Notice>
    {/if}

    {#if mode === 'receive'}
      {#if receiveView}
        <div class="flex flex-wrap items-center gap-1.5" data-testid="nearby-receive-status">
          <Pill tone={receiveBadge.tone}>{t(receiveBadge.label)}</Pill>
          {#if receiveView.state === 'waiting'}
            <Pill tone={receiveView.relay_ready ? 'ok' : 'neutral'} testid="nearby-relay-pill">
              <Icon name={receiveView.relay_ready ? 'relay' : 'device'} />
              {t(receiveView.relay_ready ? 'nearbyRelayReady' : 'nearbyLocalOnly')}
            </Pill>
          {/if}
          {#if transportLabelKey(receiveView.transport)}
            <Pill tone="forest">{t(transportLabelKey(receiveView.transport)!)}</Pill>
          {/if}
          {#if receiveView.state === 'waiting' && receiveView.expires_at}
            <span class="text-caption text-faint">{t('nearbyExpires', { time: when(receiveView.expires_at) })}</span>
          {/if}
        </div>

        {#if receiveView.state === 'waiting'}
          <div class="grid justify-items-center gap-3 rounded-xl border border-line bg-surface-2 p-4">
            {#if receiveView.qr}
              <img
                src={receiveView.qr}
                alt={t('nearbyOfferQrAlt')}
                width="224"
                height="224"
                class="size-[min(224px,calc(100vh-360px))] min-h-40 min-w-40 max-w-full rounded-lg bg-white p-2 [image-rendering:pixelated]"
                data-testid="nearby-receive-qr"
              />
            {/if}
            <div class="flex w-full min-w-0 items-center gap-2 rounded-lg border border-line bg-surface px-2.5 py-1.5">
              <code class="min-w-0 flex-1 truncate font-mono text-code text-ink select-all" dir="ltr" data-testid="nearby-receive-offer">{receiveView.offer}</code>
              <CopyButton
                class={buttonClass('subtle', 'xs')}
                value={() => receiveView?.offer ?? ''}
                label={t('copy')}
                testid="nearby-copy-offer"
              />
            </div>
          </div>

          <div class="grid gap-1 rounded-xl border border-line bg-surface p-3">
            <Switch
              checked={discoverable}
              disabled={busy}
              onCheckedChange={(checked) => void toggleDiscoverable(checked)}
              descriptionId="nearby-discoverable-hint"
              testid="nearby-discoverable-switch"
            >
              <span class="text-body font-medium text-ink">{t('nearbyDiscoverable')}</span>
            </Switch>
            <p id="nearby-discoverable-hint" class="text-callout text-muted">{t('nearbyDiscoverableHint')}</p>
          </div>

          {#if receiveView.mdns_error}
            <Notice tone="warn" testid="nearby-mdns-error">
              <p class="font-medium">{t('nearbyMdnsWarning')}</p>
              <p class="mt-0.5 text-callout">{t('nearbyFirewallHint')}</p>
              <p class="mt-1 font-mono text-code text-muted select-text" dir="ltr">{receiveView.mdns_error}</p>
            </Notice>
          {/if}

          {#if !receiveView.pending_peers.length || receiveView.verification_codes?.length}
            <div role="status">
              {#each receiveView.verification_codes ?? [] as sas}
                <p class="text-callout text-muted">{t('nearbySasPrompt')}</p>
                <bdi dir="ltr" class="font-mono text-title-1 font-semibold tabular-nums select-text">{formatSas(sas)}</bdi>
              {:else}
                <p class="text-center text-callout text-muted" data-testid="nearby-waiting-sender">{t('nearbyWaitingSender')}</p>
              {/each}
            </div>
          {/if}
          {#if receiveView.pending_peers.length > 0}
            <div class="grid gap-3" data-testid="nearby-pending-peers">
              {#each receiveView.pending_peers as peer (peer.peer_id)}
                {@const selectedIndices = peerSelections[peer.peer_id] ?? []}
                {@const peerTransport = transportLabelKey(peer.transport)}
                <article class="grid gap-3 rounded-xl border border-line bg-surface-2 p-3.5" data-testid="nearby-pending-peer">
                  <div class="flex flex-wrap items-start justify-between gap-2">
                    <div class="min-w-0">
                      <h3 class="truncate text-title-3 font-semibold text-ink">{peer.alias || t('nearbyUnknownDevice')}</h3>
                      <p class="text-caption text-muted">{t('nearbyUntrustedAlias')}</p>
                    </div>
                    <div class="flex flex-wrap items-center gap-1">
                      <Pill tone={peer.qr_bound ? 'ok' : 'warn'}>
                        <Icon name={peer.qr_bound ? 'verified' : 'warning'} />
                        {t(peer.qr_bound ? 'nearbyQrVerified' : 'nearbyBrowseUnverified')}
                      </Pill>
                      {#if peerTransport}
                        <Pill>{t(peerTransport)}</Pill>
                      {/if}
                    </div>
                  </div>

                  <div class="rounded-lg border border-line bg-surface px-3 py-2.5 text-center">
                    <p class="text-caption text-muted">{t('nearbySasPrompt')}</p>
                    <bdi
                      dir="ltr"
                      class="mt-1 block font-mono text-title-1 font-semibold tracking-[0.14em] text-ink tabular-nums select-text"
                      data-testid="nearby-receive-sas"
                    >
                      {formatSas(peer.sas)}
                    </bdi>
                  </div>

                  {#if peer.kind === 'files'}
                    <div class="grid gap-2">
                      <fieldset class="grid gap-1.5 border-0 p-0" disabled={busy}>
                        <legend class="mb-1 text-callout font-medium text-muted">{t('files')} · {peer.files.length}</legend>
                        {#each peer.files as file (file.index)}
                          <div>
                            <Checkbox
                              checked={selectedIndices.includes(file.index)}
                              onCheckedChange={(checked) => {
                                peerSelections = {
                                  ...peerSelections,
                                  [peer.peer_id]: toggle(selectedIndices, file.index, checked)
                                }
                              }}
                              testid={`nearby-peer-file-${file.index}`}
                            >
                              <span class="break-all text-body text-ink">{file.name}</span>
                              <span class="ms-1.5 text-caption text-muted tabular-nums">{size(file.size)}</span>
                            </Checkbox>
                          </div>
                        {/each}
                      </fieldset>
                      <div class="flex flex-wrap items-center justify-between gap-2 pt-1 text-callout">
                        <span class="min-w-0 truncate text-muted">{t('nearbyDestinationFolder')}: <strong class="font-medium text-ink">{folderName}</strong></span>
                        <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={openFolderPicker} data-testid="nearby-choose-folder">{t('choose')}</button>
                      </div>
                    </div>
                  {:else}
                    <div class="grid gap-1 text-callout text-muted">
                      <p class="font-medium text-ink">{t('nearbyVaultPackageSize', { size: size(peer.package_size ?? 0) })}</p>
                      <p>{t('nearbyVaultReceiveNote')}</p>
                    </div>
                  {/if}

                  <div class="grid gap-1 pt-0.5">
                    <Checkbox
                      checked={Boolean(peerShareIdentity[peer.peer_id])}
                      disabled={busy}
                      onCheckedChange={(checked) => {
                        peerShareIdentity = { ...peerShareIdentity, [peer.peer_id]: checked }
                      }}
                      descriptionId={`nearby-recv-id-${peer.peer_id}`}
                      testid="nearby-receive-share-identity"
                    >
                      <span class="text-callout text-ink">{t('nearbyShareIdentity')}</span>
                    </Checkbox>
                    <p id={`nearby-recv-id-${peer.peer_id}`} class="text-caption text-muted">{t('nearbyShareIdentityHint')}</p>
                  </div>

                  <div class="flex flex-wrap justify-end gap-2 pt-1">
                    <button
                      class={buttonClass('secondary', 'sm')}
                      disabled={busy}
                      onclick={() => rejectPeer(peer.peer_id)}
                      data-testid="nearby-peer-reject"
                    >
                      {t('nearbyReject')}
                    </button>
                    <button
                      class={buttonClass('primary', 'sm')}
                      disabled={busy || (peer.kind === 'files' && !selectedIndices.length)}
                      onclick={() => acceptPeer(peer.peer_id, peer.kind)}
                      data-testid="nearby-peer-accept"
                    >
                      {t('nearbyAccept')}
                    </button>
                  </div>
                </article>
              {/each}
            </div>
          {/if}
        {:else if receiveView.state === 'running'}
          <div class="grid gap-2">
            <Progress
              value={receiveProgress.value}
              max={receiveProgress.max}
              label={receiveView.progress.total_bytes > 0
                ? t('nearbyTransferProgress', {
                    done: size(receiveView.progress.bytes),
                    total: size(receiveView.progress.total_bytes)
                  })
                : t('nearbyStateRunning')}
              testid="nearby-receive-progress"
            />
            {#if receiveView.progress.current_name}
              <p class="truncate text-callout text-muted">{receiveView.progress.current_name}</p>
            {/if}
          </div>
        {:else if receiveView.state === 'done'}
          {#if receiveView.outcome?.identity}
            {@render identityNotice(receiveView.outcome.identity)}
          {/if}
          {#if receiveView.outcome?.kind === 'secret_package' && receiveView.outcome.package_preview}
            <PackageImport received={receiveView.outcome.package_preview} onNavigate={() => (open = false)} />
          {:else}
            {@const receivedFiles = receiveView.outcome?.files ?? []}
            <Notice tone="ok" testid="nearby-receive-done">
              {t('nearbyReceiveDone', { n: receivedFiles.length })}
            </Notice>
            {#if receivedFiles.length > 0}
              <ul class="grid gap-1.5" data-testid="nearby-received-files">
                {#each receivedFiles as item (item.file_id)}
                  <li class="flex items-center gap-2 rounded-lg bg-surface-2 px-3 py-2 text-callout">
                    <Icon name="file" class="shrink-0 text-forest" />
                    <span class="min-w-0 flex-1 truncate text-ink">{item.name}</span>
                    <span class="shrink-0 text-caption text-muted tabular-nums">{size(item.size)}</span>
                    <button
                      class={buttonClass('ghost', 'xs')}
                      onclick={() => {
                        open = false
                        openFile(item.file_id)
                      }}
                    >
                      {t('open')}
                    </button>
                  </li>
                {/each}
              </ul>
            {/if}
          {/if}
        {:else if receiveView.state === 'failed' || receiveView.state === 'cancelled'}
          <Notice tone={receiveView.state === 'failed' ? 'danger' : 'warn'} testid="nearby-receive-ended">
            {receiveView.error ? describeViewError(receiveView.error) : t(receiveBadge.label)}
          </Notice>
        {/if}
      {:else if !error}
        <Progress label={t('loading')} testid="nearby-receive-loading" />
      {/if}
    {:else}
      <!-- Send Files / Send Vault -->
      {#if showSendForm}
        {#if mode === 'send-files'}
          {#if files.length === 0}
            <Notice tone="warn">{t('nearbyNoFilesSelected')}</Notice>
          {:else}
            <div class="grid gap-2 rounded-xl border border-line bg-surface-2 p-3" data-testid="nearby-send-files-summary">
              <div class="flex items-center justify-between gap-2 text-callout">
                <span class="font-medium text-ink">{t('nearbySelectedFiles', { n: files.length, size: size(totalSendFileBytes) })}</span>
              </div>
              <ul class="grid max-h-36 gap-1 overflow-y-auto text-callout">
                {#each files as f (f.file_id + ':' + (f.version_id ?? ''))}
                  <li class="flex items-center gap-2">
                    <Icon name="file" class="size-4 shrink-0 text-faint" />
                    <span class="min-w-0 flex-1 truncate text-ink">{f.name}</span>
                    {#if f.size != null}
                      <span class="shrink-0 text-caption text-muted tabular-nums">{size(f.size)}</span>
                    {/if}
                  </li>
                {/each}
              </ul>
            </div>
          {/if}
        {:else if mode === 'send-vault'}
          {#if vaultBlocked}
            <Notice tone="warn" testid="nearby-vault-blocked">{vaultChanged ? t('nearbyVaultChanged') : ux('blockedSource')}</Notice>
          {:else if presetVaultSelection && vaultEntry}
            <div class="grid gap-1 rounded-xl border border-line bg-surface-2 p-3 text-callout" data-testid="nearby-vault-preset">
              <span class="font-medium text-ink">{vaultEntry.item?.title}</span>
              <span class="text-muted">
                {[
                  presetVaultSelection.field_ids.length ? `${presetVaultSelection.field_ids.length} ${t('content')}` : '',
                  presetVaultSelection.include_notes ? w('notes') : '',
                  presetVaultSelection.attachments.length ? `${presetVaultSelection.attachments.length} ${t('files')}` : ''
                ].filter(Boolean).join(' · ')}
              </span>
            </div>
          {/if}
        {/if}

        {#if scanning}
          <QrCamera onImage={scanQr} onCancel={() => (scanning = false)} />
        {:else}
          <div class="grid gap-3">
            <div class="flex flex-wrap gap-2">
              <button
                class={buttonClass('secondary', 'sm')}
                disabled={busy || vaultBlocked}
                onclick={() => {
                  clearStaleSendError()
                  scanning = true
                }}
                data-testid="nearby-scan-qr"
              >
                <Icon name="scan" />{t('scanQrCamera')}
              </button>
              <button
                class={buttonClass('secondary', 'sm')}
                disabled={busy || browsing || vaultBlocked}
                onclick={browsePeers}
                data-testid="nearby-browse-peers"
              >
                <Icon name="sync-now" class={cn(browsing && 'animate-spin')} />
                {browsing ? t('nearbySearching') : t('nearbyBrowse')}
              </button>
            </div>

            <TextField
              label={t('nearbyOfferLabel')}
              hint={t('nearbyOfferHint')}
              bind:value={offerText}
              mono
              disabled={busy || vaultBlocked}
              error={offerError}
              oninput={onOfferInput}
              placeholder="dmsg-nearby/1:…"
              data-testid="nearby-offer-input"
            />

            {#if browseErrors.length > 0}
              <Notice tone="warn" testid="nearby-browse-errors">
                <p>{t('nearbyFirewallHint')}</p>
                <p class="mt-1 font-mono text-code text-muted select-text" dir="ltr">{browseErrors.join('; ')}</p>
              </Notice>
            {/if}

            {#if discoveredPeers.length > 0}
              <div class="grid gap-1.5" role="group" aria-label={t('nearbyDiscoveredDevices')} data-testid="nearby-peer-list">
                <span class="text-callout font-medium text-muted">{t('nearbyDiscoveredDevices')}</span>
                {#each discoveredPeers as peer (`${peer.advert}:${peer.transport}`)}
                  {@const picked =
                    selectedPeer?.advert === peer.advert && selectedPeer?.transport === peer.transport}
                  {@const peerTransport = transportLabelKey(peer.transport)}
                  <button
                    type="button"
                    class={cn(
                      'flex w-full items-center justify-between gap-2 rounded-xl border px-3 py-2 text-start outline-none transition-colors focus-visible:ring-3 focus-visible:ring-focus/35',
                      picked
                        ? 'border-forest bg-pale text-forest'
                        : 'border-line bg-surface hover:bg-wash text-ink'
                    )}
                    aria-pressed={picked}
                    disabled={busy || vaultBlocked}
                    onclick={() => pickDiscoveredPeer(peer)}
                    data-testid="nearby-discovered-peer"
                  >
                    <span class="min-w-0 flex-1">
                      <span class="block truncate text-body font-medium">{peer.alias || t('nearbyUnknownDevice')}</span>
                      <span class="block truncate font-mono text-caption text-muted" dir="ltr">
                        {peer.pk_hint}{peer.host && peer.port ? ` · ${peer.host}:${peer.port}` : ''}
                      </span>
                    </span>
                    {#if peerTransport}
                      <Pill tone={picked ? 'forest' : 'neutral'}>{t(peerTransport)}</Pill>
                    {/if}
                  </button>
                {/each}
              </div>
            {:else if browsedOnce && !browsing}
              <Notice tone="info" testid="nearby-no-peers">{t('nearbyNoPeersFoundHint')}</Notice>
            {/if}

            <div class="grid gap-1">
              <Checkbox
                bind:checked={shareIdentity}
                disabled={busy || vaultBlocked}
                descriptionId="nearby-send-identity-hint"
                testid="nearby-send-share-identity"
              >
                <span class="text-callout text-ink">{t('nearbyShareIdentity')}</span>
              </Checkbox>
              <p id="nearby-send-identity-hint" class="text-caption text-muted">{t('nearbyShareIdentityHint')}</p>
            </div>
          </div>
        {/if}
      {:else if sendView}
        <div class="flex flex-wrap items-center gap-1.5" data-testid="nearby-send-status">
          <Pill tone={sendBadge.tone}>{t(sendBadge.label)}</Pill>
          <Pill tone={sendView.qr_bound ? 'ok' : 'warn'}>
            <Icon name={sendView.qr_bound ? 'verified' : 'warning'} />
            {t(sendView.qr_bound ? 'nearbyQrVerified' : 'nearbyBrowseUnverified')}
          </Pill>
          {#if transportLabelKey(sendView.transport)}
            <Pill tone="forest">{t(transportLabelKey(sendView.transport)!)}</Pill>
          {/if}
          <span class="text-callout text-muted">{sendView.peer_label}</span>
        </div>

        {#if sendView.sas && sendView.state !== 'done'}
          <div class="rounded-xl border border-line bg-surface-2 px-4 py-3 text-center">
            <p class="text-caption text-muted">{t('nearbySasPrompt')}</p>
            <bdi
              dir="ltr"
              class="mt-1 block font-mono text-title-1 font-semibold tracking-[0.14em] text-ink tabular-nums select-text"
              data-testid="nearby-send-sas"
            >
              {formatSas(sendView.sas)}
            </bdi>
          </div>
        {/if}

        {#if sendView.state === 'connecting'}
          <Progress label={t('nearbyStateConnecting')} testid="nearby-send-progress" />
        {:else if sendView.state === 'awaiting_sas'}
          <Notice tone="warn" testid="nearby-sas-confirm-notice">{t('nearbyConfirmSasHint')}</Notice>
          <button
            class={buttonClass('primary', 'lg', 'w-full')}
            disabled={busy}
            onclick={confirmSas}
            data-testid="nearby-confirm-sas"
          >
            <Icon name="verified" />{t('nearbyConfirmSas')}
          </button>
        {:else if sendView.state === 'awaiting_accept'}
          <Progress label={t('nearbyStateAwaitingAccept')} testid="nearby-send-progress" />
        {:else if sendView.state === 'running'}
          <div class="grid gap-2">
            <Progress
              value={sendProgress.value}
              max={sendProgress.max}
              label={sendView.progress.total_bytes > 0
                ? t('nearbyTransferProgress', {
                    done: size(sendView.progress.bytes),
                    total: size(sendView.progress.total_bytes)
                  })
                : t('nearbyStateRunning')}
              testid="nearby-send-progress"
            />
            {#if sendView.progress.current_name}
              <p class="truncate text-callout text-muted">{sendView.progress.current_name}</p>
            {/if}
          </div>
        {:else if sendView.state === 'done'}
          <Notice tone="ok" testid="nearby-send-done">
            {#if sendView.outcome?.kind === 'secret_package'}
              {t('nearbySendVaultDone', { size: size(sendView.outcome.bytes) })}
            {:else}
              {t('nearbySendFilesDone', {
                n: sendView.outcome?.sent_items ?? files.length,
                size: size(sendView.outcome?.bytes ?? 0)
              })}
            {/if}
          </Notice>
          {#if sendView.outcome?.identity}
            {@render identityNotice(sendView.outcome.identity)}
          {/if}
        {:else if sendView.state === 'failed' || sendView.state === 'cancelled'}
          <Notice tone={sendView.state === 'failed' ? 'danger' : 'warn'} testid="nearby-send-ended">
            {sendView.error ? describeViewError(sendView.error) : t(sendBadge.label)}
          </Notice>
        {/if}
      {/if}
    {/if}
  </div>

  {#snippet footer()}
    <span class="flex-1"></span>
    {#if mode === 'receive'}
      {#if receivingActive}
        <button class={buttonClass('secondary')} disabled={busy} onclick={cancelReceive} data-testid="nearby-receive-cancel">
          {t('cancel')}
        </button>
      {:else if receiveView?.state === 'failed' || receiveView?.state === 'cancelled' || error}
        <button class={buttonClass('ghost')} onclick={() => (open = false)}>{t('close')}</button>
        <button class={buttonClass('primary')} disabled={busy} onclick={() => startReceive(discoverable)} data-testid="nearby-receive-retry">
          {t('retry')}
        </button>
      {:else}
        <button class={buttonClass('primary')} onclick={() => (open = false)} data-testid="nearby-done">
          {t('done')}
        </button>
      {/if}
    {:else if showSendForm}
      <button class={buttonClass('ghost')} disabled={busy} onclick={() => (open = false)}>{t('cancel')}</button>
      <button
        class={buttonClass('primary')}
        disabled={busy || !sendParams || vaultBlocked}
        onclick={startSend}
        data-testid="nearby-send-start"
      >
        <Icon name="transfer" />{t('nearbyStartSend')}
      </button>
    {:else if sendingActive}
      <button class={buttonClass('secondary')} disabled={busy} onclick={cancelSend} data-testid="nearby-send-cancel">
        {t('cancel')}
      </button>
    {:else if sendView?.state === 'failed' || sendView?.state === 'cancelled'}
      <button class={buttonClass('ghost')} onclick={() => (open = false)}>{t('close')}</button>
      <button class={buttonClass('secondary')} disabled={busy} onclick={resetDraft} data-testid="nearby-send-edit">
        {t('nearbyEditOptions')}
      </button>
      <button
        class={buttonClass('primary')}
        disabled={busy || !sendParams || vaultBlocked}
        onclick={startSend}
        data-testid="nearby-send-retry"
      >
        {t('retry')}
      </button>
    {:else}
      <button class={buttonClass('primary')} onclick={() => (open = false)} data-testid="nearby-done">
        {t('done')}
      </button>
    {/if}
  {/snippet}
</Dialog>

<FolderPicker
  bind:open={folderPickerOpen}
  {folders}
  title={t('nearbyDestinationFolder')}
  action={t('choose')}
  onPick={(picked) => (folder = picked)}
/>
