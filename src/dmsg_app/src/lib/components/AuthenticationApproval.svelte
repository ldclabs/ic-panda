<script lang="ts">
  import { onMount } from 'svelte'
  import { session, dateLabel } from '../session.svelte'
  import { getRequest, rejectRequest } from '../requests'
  import type { PendingRequest } from '../protocol/requests'
  import type { AuthenticationPayload } from '../bridge-requests'
  import { config } from '../config'
  import { connectAccount } from '../connection'
  import { AuthenticationClient } from '../services/authentication'
  let record = $state<PendingRequest | null>(null)
  let payload = $state<AuthenticationPayload | null>(null)
  let client = $state.raw<AuthenticationClient | null>(null)
  let review = $state<Awaited<ReturnType<AuthenticationClient['prepare']>> | null>(null)
  let journal = $state<Awaited<ReturnType<AuthenticationClient['journal']>>>(null)
  let connected = $state(false)
  let finished = $state(false)
  let derivation = $state(config.derivationOrigins[0])
  const id = new URLSearchParams(location.search).get('id') ?? ''
  onMount(() => {
    void session.run(async () => {
      record = id ? await getRequest(id) : null
      if (!record) throw new Error('请求不存在。')
      payload = (await session.crypto.call(
        'readRequest',
        record.payload,
        record.id
      )) as AuthenticationPayload
      finished = ['cancelled', 'rejected', 'expired'].includes(record.state)
    })
    const listener = (
      message: any,
      sender: chrome.runtime.MessageSender,
      respond: (value: unknown) => void
    ) => {
      if (
        message?.type !== 'dmsg-read-authentication-result' ||
        sender.id !== chrome.runtime.id ||
        sender.tab ||
        (sender.url && sender.url !== chrome.runtime.getURL('service_worker.js')) ||
        !session.unlocked ||
        !client ||
        message.requestId !== id ||
        message.digest !== record?.digest
      )
        return
      void client.result(id).then(
        (result) =>
          respond({ ok: true, digest: record!.digest, origin: record!.source.origin, result }),
        () => respond({ ok: false })
      )
      return true
    }
    chrome.runtime.onMessage.addListener(listener)
    return () => chrome.runtime.onMessage.removeListener(listener)
  })
  async function connect() {
    await session.run(async () => {
      if (!record || !payload || !session.meta?.account)
        throw new Error('请先解锁已注册的 dMsg 账号。')
      client = new AuthenticationClient((await connectAccount(derivation)).account)
      journal = await client.journal(id)
      if (!journal) review = await client.prepare(record, payload)
      connected = true
    })
  }
  async function approve() {
    await session.run(async () => {
      if (!client || !record || !payload) throw new Error('请先连接并核对应用。')
      try {
        if (journal) await client.resume(id)
        else await client.execute(record, payload)
      } finally {
        journal = await client.journal(id)
      }
    })
  }
</script>

<main class="approval-page">
  <span class="eyebrow">CONNECT WITH DMSG</span>
  <h1>确认你的账号连接</h1>
  {#if record}
    <dl class="evidence-list">
      <div>
        <dt>请求来源</dt>
        <dd>{record.source.origin}</dd>
      </div>
      <div>
        <dt>应用</dt>
        <dd>{record.bridge?.appId}</dd>
      </div>
      <div>
        <dt>dMsg 账号</dt>
        <dd><code>{record.bridge?.accountId}</code></dd>
      </div>
      <div>
        <dt>有效期</dt>
        <dd>{dateLabel(record.expiresAt)}</dd>
      </div>
      {#if review}<div>
          <dt>操作</dt>
          <dd>
            {review.request.purpose === 'Login'
              ? '注册或登录应用'
              : review.request.purpose === 'Link'
                ? '关联到当前应用账号'
                : '为当前操作重新认证'}
          </dd>
        </div>{/if}
    </dl>
    <p>确认后，应用会取得本次会话的账号证明。每次文档签署、业务操作和付款仍需分别批准。</p>
    {#if finished}<p role="status">本次请求已结束。</p>
    {:else if journal?.stage === 'complete'}<p role="status">
        认证已完成，请返回原应用。保持此窗口解锁，应用才能取回证明。
      </p>
    {:else}
      {#if !connected}<label
          >登录来源<select bind:value={derivation}
            >{#each config.derivationOrigins as origin}<option value={origin}>{origin}</option
              >{/each}</select
          ></label
        >
        <button class="primary" disabled={session.busy} onclick={connect}>连接并核对</button>
      {:else}<button class="primary" disabled={session.busy} onclick={approve}
          >{journal ? '核对原认证结果' : '确认本次认证'}</button
        >{/if}
      {#if !journal}<button
          class="secondary"
          disabled={session.busy}
          onclick={() =>
            session.run(async () => {
              await rejectRequest(id)
              finished = true
            })}>拒绝</button
        >{/if}
    {/if}
  {/if}
  {#if session.error}<p class="form-error" role="alert">{session.error}</p>{/if}
</main>
