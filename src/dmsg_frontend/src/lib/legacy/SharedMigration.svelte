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
  import Picker from './ui/Picker.svelte'
  import Step from './ui/Step.svelte'
  import { area, field, panel } from './ui/classes'
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
  const anonymous = $derived($authStore.identity.getPrincipal().isAnonymous())
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

{#snippet fact(label: string, value: string, mono = true)}
  <dt class="text-[14px] text-[var(--muted)]">{label}</dt>
  <dd
    class={[
      'm-0 text-[14px] [overflow-wrap:anywhere]',
      mono && 'font-mono text-[13px]'
    ]}
  >
    {value}
  </dd>
{/snippet}

<section
  class="migration grid max-w-[860px] gap-6 border-t border-[var(--line)] py-12"
>
  <div class="grid gap-3">
    <h2>{$t('Shared channel succession and member claims')}</h2>
    <p class="mb-0!">
      {$t(
        'This needs the old service to be frozen. The one official successor exists only after every frozen manager agrees to the same new owner and initial channel record.'
      )}
    </p>
  </div>
  <ol class={panel}>
    <Step n={1} title={$t('Export the frozen evidence')}>
      <div class="grid gap-4 sm:grid-cols-[minmax(0,1fr)_200px]">
        <div class="grid gap-2">
          <span class="text-[14px] font-medium"
            >{$t('Source channel service')}</span
          >
          <Picker
            bind:value={source}
            options={channels}
            label={$t('Source channel service')}
            mono
          />
        </div>
        <label class="grid gap-2">
          <span class="text-[14px] font-medium"
            >{$t('Legacy channel number')}</span
          >
          <input
            type="number"
            min="0"
            max="4294967295"
            class={[field, 'font-mono tabular-nums']}
            bind:value={channel}
          />
        </label>
      </div>
      <div class="flex flex-wrap gap-3">
        <button
          class="button secondary disabled:cursor-not-allowed disabled:opacity-60"
          disabled={busy || anonymous}
          onclick={inventory}>{$t('Export frozen authority evidence')}</button
        >
        <button
          class="button secondary disabled:cursor-not-allowed disabled:opacity-60"
          disabled={busy || anonymous}
          onclick={frozenHistory}
          >{$t('Export final history evidence for this channel')}</button
        >
      </div>
    </Step>
    <Step n={2} title={$t('Approve a request')} done={!!responseText}>
      <label class="grid gap-2">
        <span class="text-[14px] font-medium"
          >{$t('Approval request from the desktop app')}</span
        >
        <textarea class={area} bind:value={requestText} rows="5"></textarea>
      </label>
      {#if request}<dl
          class="m-0 grid gap-x-6 gap-y-2 rounded-xl border border-[var(--accent)] bg-[var(--pale)] p-4 sm:grid-cols-[max-content_minmax(0,1fr)]"
        >
          {@render fact(
            $t('Approval type'),
            request.kind === 'manager'
              ? $t('Manager consent to the succession proposal')
              : $t('Former member claim'),
            false
          )}
          {@render fact(
            $t('Legacy channel'),
            `${request.proposal.source} / ${request.proposal.channel}`
          )}
          {@render fact($t('New owner'), request.proposal.owner)}
          {@render fact($t('New channel'), request.proposal.channel_id)}
          {@render fact(
            $t('Proposal version'),
            String(request.proposal.version),
            false
          )}
          {@render fact($t('Legacy identity to act for'), request.identity)}
          {#if request.kind === 'member'}{@render fact(
              $t('Target account / device'),
              `${request.claim.account} / ${request.claim.device}`
            )}{/if}
          {@render fact(
            $t('Expires'),
            new Date(request.expires_at).toLocaleString($locale),
            false
          )}
          {@render fact($t('Intent digest'), request.digest)}
        </dl>{/if}
      <p class="mb-0! text-[14px]">
        {$t(
          'Signing identity: {principal}. Shared names must be approved with the frozen manager’s original login.',
          { principal: $authStore.identity.getPrincipal().toText() }
        )}
      </p>
      <div>
        <button
          class="button primary disabled:cursor-not-allowed disabled:opacity-60"
          disabled={!request || busy || anonymous}
          onclick={approve}
          >{$t('I have checked it: approve this version')}</button
        >
      </div>
      {#if responseText}<label class="grid gap-2">
          <span class="text-[14px] font-medium"
            >{$t('Approval result to copy into the desktop app')}</span
          >
          <textarea class={area} readonly value={responseText} rows="5"
          ></textarea>
        </label>{/if}
    </Step>
  </ol>
  {#if status}<p
      class="mb-0! rounded-xl bg-[var(--pale)] p-4 text-[15px] text-[var(--ink)]!"
      role="status"
    >
      {$t(status)}
    </p>{/if}
  {#if error}<p
      class="mb-0! rounded-xl bg-[#fef2f2] p-4 text-[15px] text-[#991b1b]!"
      role="alert"
    >
      {$t(error)}
    </p>{/if}
</section>
