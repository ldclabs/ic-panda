<script lang="ts">
  import { onMount } from 'svelte'
  import {
    canonical,
    decodeCanonical,
    type CheckoutRequest,
    type CheckoutView,
    type PandaClaimView
  } from '@dmsg/sdk'
  import { unbase64, base64 } from '@dmsg/sdk/browser'
  import { session } from '../session.svelte'
  import { listRequests, assertLiveSource, setRequestState } from '../requests'
  import type { PendingRequest } from '../protocol/requests'
  import CheckoutPanel from './CheckoutPanel.svelte'
  let record = $state<PendingRequest | null>(null),
    request = $state<CheckoutRequest | null>(null),
    result = $state<unknown>(null)
  const id = new URLSearchParams(location.search).get('id') ?? ''
  onMount(() => {
    void session.run(async () => {
      record = (await listRequests()).find((r) => r.id === id) ?? null
      if (!record || record.kind !== 'checkout') throw new Error('请求不存在。')
      const p = (await session.crypto.call('readRequest', record.payload, record.id)) as {
        checkoutCbor: string
      }
      request = decodeCanonical(unbase64(p.checkoutCbor)) as unknown as CheckoutRequest
    })
    const listener = (
      m: any,
      s: chrome.runtime.MessageSender,
      reply: (v: unknown) => void
    ) => {
      if (
        m?.type !== 'dmsg-read-checkout-result' ||
        s.id !== chrome.runtime.id ||
        s.tab ||
        (s.url && s.url !== chrome.runtime.getURL('service_worker.js')) ||
        !session.unlocked ||
        !result ||
        !record ||
        m.requestId !== id ||
        m.digest !== record?.digest
      )
        return
      reply({ ok: true, digest: record.digest, origin: record.source.origin, result })
    }
    chrome.runtime.onMessage.addListener(listener)
    return () => chrome.runtime.onMessage.removeListener(listener)
  })
  async function beforeAuthorize() {
    if (!record) throw new Error('请求不存在。')
    await assertLiveSource(record)
    const current = (await listRequests()).find((r) => r.id === id)
    if (
      !current ||
      !['awaiting_user', 'authorized', 'execution_unknown'].includes(current.state)
    )
      throw new Error('原请求不可再批准。')
    await setRequestState(id, 'authorized')
  }
  async function onStatus(view: CheckoutView | PandaClaimView) {
    const complete =
      'progress' in view
        ? ['Applied', 'RefundCommitted', 'Rejected'].includes(view.progress.status)
        : ['Active', 'Terminated', 'Cancelled', 'Rejected', 'Released'].includes(view.status)
    if (complete) {
      result = { checkoutCbor: base64(canonical(view)) }
      await setRequestState(id, 'signed')
    }
  }
</script>

<main class="approval-page">
  <span class="eyebrow">CHECKOUT WITH DMSG</span>{#if request && record}<CheckoutPanel
      {request}
      origin={record.source.origin}
      {beforeAuthorize}
      {onStatus}
    />{/if}
</main>
