<script lang="ts">
  import { onMount } from 'svelte'
  import {
    collectFolderProof,
    pack,
    collectSnapshot,
    managerConsentDigest,
    memberClaimDigest,
    sharedProposalSchema,
    memberClaimSchema,
    type SnapshotProof
  } from '@dmsg/legacy'
  import { createLegacyReader } from './reader'
  import { dynAgent } from '$lib/utils/auth'
  import { authStore } from '$lib/stores/auth'
  import { locale, t } from '$lib/i18n'
  const channels = [
    'nvdn4-5qaaa-aaaaj-qa4pq-cai',
    'zof5a-5yaaa-aaaai-acr2q-cai',
    '4jxyd-pqaaa-aaaah-qdqtq-cai'
  ]
  const identity = '2rgax-kyaaa-aaaap-anvba-cai'
  let source = $state(channels[0]!),
    channel = $state(1),
    requestText = $state(''),
    responseText = $state(''),
    status = $state(''),
    busy = $state(false),
    error = $state('')
  const toHex = (bytes: Uint8Array) =>
    Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('')
  const base64 = (bytes: Uint8Array) => {
    let text = ''
    for (let i = 0; i < bytes.length; i += 8192)
      text += String.fromCharCode(...bytes.subarray(i, i + 8192))
    return btoa(text)
      .replaceAll('+', '-')
      .replaceAll('/', '_')
      .replace(/=+$/, '')
  }
  const wire = (p: SnapshotProof) => ({
    ...p,
    args: base64(p.args),
    nonce: base64(p.nonce),
    requestId: base64(p.requestId),
    certificate: base64(p.certificate),
    reply: base64(p.reply)
  })
  // The desktop client may open this page with the request in the fragment.
  // Errors and statuses hold English message keys and render through $t.
  onMount(() => {
    const match = /^#approve=([A-Za-z0-9_-]{1,8000})$/.exec(location.hash)
    if (!match) return
    try {
      requestText = new TextDecoder().decode(
        Uint8Array.from(
          atob(match[1]!.replaceAll('-', '+').replaceAll('_', '/')),
          (c) => c.charCodeAt(0)
        )
      )
    } catch {
      error = 'This approval request link is invalid.'
    }
    history.replaceState(null, '', location.pathname + location.search)
  })
  const request = $derived.by(() => {
    try {
      if (requestText.length > 20000) return null
      const value = JSON.parse(requestText),
        proposal = sharedProposalSchema.parse(value.proposal)
      if (
        value.format !== 'dmsg-legacy-approval-request/1' ||
        !channels.includes(proposal.source) ||
        !Number.isSafeInteger(value.expires_at) ||
        value.expires_at <= Date.now() ||
        value.expires_at > Date.now() + 7 * 86400000
      )
        return null
      const digest =
        value.kind === 'manager'
          ? managerConsentDigest(proposal, value.identity)
          : value.kind === 'member'
            ? memberClaimDigest(memberClaimSchema.parse(value.claim))
            : null
      if (
        !digest ||
        toHex(digest) !== value.digest ||
        (value.kind === 'member' && value.claim.member !== value.identity)
      )
        return null
      return { ...value, proposal, bytes: digest }
    } catch {
      return null
    }
  })
  async function run(task: () => Promise<void>) {
    if (busy) return
    busy = true
    error = ''
    try {
      await task()
    } catch (cause) {
      error =
        cause instanceof Error ? cause.message : 'The operation did not finish.'
    } finally {
      busy = false
    }
  }
  function download(value: unknown, name: string) {
    const url = URL.createObjectURL(
      new Blob([JSON.stringify(value, null, 2)], { type: 'application/json' })
    )
    const a = document.createElement('a')
    a.href = url
    a.download = name
    a.click()
    setTimeout(() => URL.revokeObjectURL(url), 30000)
  }
  async function inventory() {
    await run(async () => {
      const control = await collectSnapshot(
        dynAgent,
        source,
        { ChannelAuthority: channel },
        channels
      )
      const names = await collectSnapshot(
        dynAgent,
        identity,
        { Authorities: null },
        [identity]
      )
      // The desktop client verifies the cutover against its reviewed configuration.
      const { verifySnapshotProof } = await import('@dmsg/legacy')
      const checked = await verifySnapshotProof(control.proofs[0]!, {
        rootKey: dynAgent.rootKey!,
        canister: source,
        caller: dynAgent.id.getPrincipal().toText(),
        scope: { ChannelAuthority: channel },
        after: null
      })
      download(
        {
          format: 'dmsg-legacy-shared-source/1',
          source,
          identity,
          cutover: toHex(checked.page.freeze.cutover_id[0]!),
          input: {
            channel,
            authority: wire(control.proofs[0]!),
            authorities: names.proofs.map(wire)
          }
        },
        'legacy-shared-source.json'
      )
      status =
        'Frozen authority evidence exported. It contains the membership, so give it only to the people migrating this channel.'
    })
  }
  async function frozenHistory() {
    await run(async () => {
      const authority = await collectSnapshot(
        dynAgent,
        source,
        { ChannelAuthority: channel },
        channels
      )
      const messages = await collectSnapshot(
        dynAgent,
        source,
        { Messages: channel },
        channels
      )
      const { reader, checkpoint } = await createLegacyReader('Local')
      let folder: Awaited<ReturnType<typeof collectFolderProof>> = []
      try {
        const info = await reader.call(
          'channel',
          source,
          'get_channel_if_update',
          [channel, 0n]
        )
        if (info[0]?.files_state?.length) {
          const token = await reader.call(
            'channel',
            source,
            'download_files_token',
            [channel]
          )
          folder = await collectFolderProof(
            dynAgent,
            token.storage[0].toText(),
            token.storage[1],
            new Uint8Array(token.access_token),
            ['532er-faaaa-aaaaj-qncpa-cai', 'sb6zj-3aaaa-aaaaj-qndla-cai']
          )
        }
      } finally {
        checkpoint.close()
      }
      const bytes = pack([
        {
          channel: `${source}/channel/${channel}`,
          authority: authority.proofs[0]!,
          messages: messages.proofs,
          folder
        }
      ])
      const url = URL.createObjectURL(
          new Blob([new Uint8Array(bytes)], { type: 'application/cbor' })
        ),
        a = document.createElement('a')
      a.href = url
      a.download = 'legacy-frozen-history.cbor'
      a.click()
      setTimeout(() => URL.revokeObjectURL(url), 30000)
      status =
        'Frozen evidence for the messages and attachments exported. The file holds no download token; import it only into your own desktop app.'
    })
  }
  async function approve() {
    await run(async () => {
      if (!request)
        throw new Error('First check a complete proposal that has not expired.')
      const expectedCaller = dynAgent.id.getPrincipal().toText()
      const result = await collectSnapshot(
        dynAgent,
        request.proposal.source,
        {
          Attestation: {
            digest: request.bytes,
            expires_at: BigInt(request.expires_at)
          }
        },
        channels
      )
      if (dynAgent.id.getPrincipal().toText() !== expectedCaller)
        throw new Error('The legacy identity changed. Check again.')
      responseText = JSON.stringify(
        { proof: wire(result.proofs[0]!), expires_at: request.expires_at },
        null,
        2
      )
      status =
        'Your explicit approval of this version is ready. Copy the result back to the desktop app to finish the same proposal.'
    })
  }
</script>

<section class="migration">
  <h2>{$t('Shared channel succession and member claims')}</h2>
  <p>
    {$t(
      'This needs the old service to be frozen. The one official successor exists only after every frozen manager agrees to the same new owner and initial channel record.'
    )}
  </p>
  <label
    >{$t('Source channel service')}<select bind:value={source}
      >{#each channels as value}<option>{value}</option>{/each}</select
    ></label
  >
  <label
    >{$t('Legacy channel number')}<input
      type="number"
      min="0"
      max="4294967295"
      bind:value={channel}
    /></label
  >
  <button
    class="button"
    disabled={busy || $authStore.identity.getPrincipal().isAnonymous()}
    onclick={inventory}>{$t('Export frozen authority evidence')}</button
  ><button
    class="button"
    disabled={busy || $authStore.identity.getPrincipal().isAnonymous()}
    onclick={frozenHistory}
    >{$t('Export final history evidence for this channel')}</button
  >
  <label
    >{$t('Approval request from the desktop app')}<textarea
      bind:value={requestText}
      rows="5"></textarea></label
  >
  {#if request}<dl>
      <dt>{$t('Approval type')}</dt>
      <dd>
        {request.kind === 'manager'
          ? $t('Manager consent to the succession proposal')
          : $t('Former member claim')}
      </dd>
      <dt>{$t('Legacy channel')}</dt>
      <dd>{request.proposal.source} / {request.proposal.channel}</dd>
      <dt>{$t('New owner')}</dt>
      <dd>{request.proposal.owner}</dd>
      <dt>{$t('New channel')}</dt>
      <dd>{request.proposal.channel_id}</dd>
      <dt>{$t('Proposal version')}</dt>
      <dd>{request.proposal.version}</dd>
      <dt>{$t('Legacy identity to act for')}</dt>
      <dd>{request.identity}</dd>
      {#if request.kind === 'member'}<dt>{$t('Target account / device')}</dt>
        <dd>{request.claim.account} / {request.claim.device}</dd>{/if}
      <dt>{$t('Expires')}</dt>
      <dd>{new Date(request.expires_at).toLocaleString($locale)}</dd>
      <dt>{$t('Intent digest')}</dt>
      <dd>{request.digest}</dd>
    </dl>{/if}
  <p>
    {$t(
      'Signing identity: {principal}. Shared names must be approved with the frozen manager’s original login.',
      { principal: $authStore.identity.getPrincipal().toText() }
    )}
  </p>
  <button
    class="button primary"
    disabled={!request ||
      busy ||
      $authStore.identity.getPrincipal().isAnonymous()}
    onclick={approve}>{$t('I have checked it: approve this version')}</button
  >
  {#if responseText}<label
      >{$t('Approval result to copy into the desktop app')}<textarea
        readonly
        value={responseText}
        rows="5"></textarea></label
    >{/if}
  {#if status}<p role="status">{$t(status)}</p>{/if}{#if error}<p role="alert"
      >{$t(error)}</p
    >{/if}
</section>

<style>
  .migration {
    padding: 32px 0;
    border-top: 1px solid #cdd8cf;
    max-width: 780px;
  }
  label {
    display: block;
    margin: 16px 0;
  }
  textarea {
    display: block;
    width: 100%;
  }
  dd {
    overflow-wrap: anywhere;
    margin-bottom: 12px;
  }
  button {
    margin: 8px 12px 8px 0;
  }
</style>
