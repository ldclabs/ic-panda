<script lang="ts">
  import { onMount } from 'svelte'
  import { session, dateLabel, downloadBlob } from '../session.svelte'
  import { getRequest, rejectRequest, assertLiveSource } from '../requests'
  import {
    assertRequestUnchanged,
    type PendingRequest,
    type SignatureRequest
  } from '../protocol/requests'
  import { config } from '../config'
  import { connectAccount } from '../connection'
  import { SigningClient } from '../services/signing'
  import Icon from './Icon.svelte'
  import ActionDetails from './ActionDetails.svelte'
  import { actionBody } from '../protocol/requests'
  let request = $state<PendingRequest | null>(null),
    payload = $state<SignatureRequest | null>(null),
    finished = $state(false)
  let client = $state.raw<SigningClient | null>(null)
  let review = $state<Awaited<ReturnType<SigningClient['prepare']>> | null>(null)
  let result = $state<Awaited<ReturnType<SigningClient['journal']>>>(null)
  let derivation = $state(config.derivationOrigins[0])
  onMount(() => {
    const id = new URLSearchParams(location.search).get('id')
    void session.run(async () => {
      request = id ? await getRequest(id) : null
      if (!request) throw new Error('请求不存在，请返回原应用重新发起。')
      const decoded = (await session.crypto.call(
        'readRequest',
        request.payload,
        request.id
      )) as SignatureRequest
      if (request.state === 'awaiting_user') assertRequestUnchanged(decoded, request)
      payload = decoded
      const saved = await session.crypto.call('controlGet', `formal:${request.id}`)
      result = saved ? JSON.parse(saved) : null
      finished = ['rejected', 'cancelled', 'expired'].includes(request.state)
    })
    const timer = setInterval(() => {
      if (id)
        void getRequest(id).then((latest) => {
          if (latest) {
            request = latest
            finished = ['rejected', 'cancelled', 'expired'].includes(latest.state)
          }
        })
    }, 3000)
    const listener = (
      message: any,
      sender: chrome.runtime.MessageSender,
      respond: (value: unknown) => void
    ) => {
      if (
        message?.type !== 'dmsg-read-formal-result' ||
        sender.id !== chrome.runtime.id ||
        sender.tab ||
        (sender.url && sender.url !== chrome.runtime.getURL('service_worker.js')) ||
        !session.unlocked ||
        message.requestId !== id
      )
        return
      void session.crypto.call('controlGet', `formal:${id}`).then(
        async (value) => {
          let job = value ? JSON.parse(value) : null
          if (!client || job?.stage !== 'complete' || job.digest !== message.digest)
            return respond({ ok: false })
          try {
            job = await client.freshResult(id!)
          } catch {
            return respond({ ok: false })
          }
          respond({
            ok: true,
            digest: job.digest,
            origin: job.origin,
            result: {
              artifact: job.artifact,
              receipt: job.receipt,
              executionId: job.executionId,
              keyDescriptorCbor: job.keyDescriptorCbor,
              executionCertificateCbor: job.executionCertificateCbor,
              authorization: 'verified',
              timestamp: 'unsupported'
            }
          })
        },
        () => respond({ ok: false })
      )
      return true
    }
    if (typeof chrome !== 'undefined') chrome.runtime.onMessage.addListener(listener)
    return () => {
      clearInterval(timer)
      if (typeof chrome !== 'undefined') chrome.runtime.onMessage.removeListener(listener)
    }
  })
  async function connect() {
    await session.run(async () => {
      if (!payload || !request || !session.meta?.account)
        throw new Error('请先绑定正式工作区。')
      const { api, account } = await connectAccount(derivation)
      client = new SigningClient(account, api.cose!, assertLiveSource)
      result = await client.journal(request.id)
      if (!result) review = await client.prepare(request, payload)
    })
  }
  async function execute(resume = false) {
    await session.run(async () => {
      if (!client || !request) throw new Error('请先连接并核对签名服务。')
      result = resume ? await client.resume(request.id) : await client.execute()
      request = (await getRequest(request.id)) ?? request
    })
  }
  async function reject() {
    await session.run(async () => {
      if (request) await rejectRequest(request.id)
      payload = null
      finished = true
    }, '请求已拒绝，没有执行签名。')
  }
</script>

<main class="approval-page">
  <span class="eyebrow">REVIEW THE REQUEST</span>
  <h1>先看清，再决定。</h1>
  {#if finished}<div class="empty-state">
      <Icon name="check" size={36} />
      <h2>本次请求已结束。</h2>
      <p>此请求已拒绝、取消或过期。已提交的操作只能按原编号对账。</p>
      <button class="secondary" onclick={() => window.close()}>关闭窗口</button>
    </div>
  {:else if payload && request}<div class="request-origin">
      <Icon name="chrome" />
      <div><span>浏览器确认的请求来源</span><strong>{request.source.origin}</strong></div>
    </div>
    <dl class="evidence-list">
      <div>
        <dt>请求类型</dt>
        <dd>
          {payload.statement.content.kind === 'app_action'
            ? '项目动作签署'
            : payload.statement.content.kind === 'text'
              ? '文本声明'
              : payload.statement.content.kind === 'file_statement'
                ? '针对文件的声明'
                : '内容摘要声明'}
        </dd>
      </div>
      <div>
        <dt>账户 Xid</dt>
        <dd><code class="hash">{payload.accountId}</code></dd>
      </div>
      <div>
        <dt>签署者 URI</dt>
        <dd>{payload.statement.issuer}</dd>
      </div>
      <div>
        <dt>声明对象</dt>
        <dd>{payload.statement.subject ?? '未提供'}</dd>
      </div>
      <div>
        <dt>声明时间（Unix 秒）</dt>
        <dd>{payload.statement.issuedAt ?? '未提供'}</dd>
      </div>
      <div>
        <dt>批准到期时间</dt>
        <dd>{dateLabel(Number(payload.expiresAt))}</dd>
      </div>
    </dl>
    <section class="review-content">
      {#if payload.statement.content.kind === 'app_action'}<ActionDetails
          action={actionBody(payload.statement.content.actionCbor)}
        />{/if}
      <span class="field-label">实际待签内容</span
      >{#if payload.statement.content.kind === 'text' || payload.statement.content.kind === 'file_statement'}<p
          class="preserve-lines"
        >
          {payload.statement.content.text}
        </p>{/if}{#if payload.statement.content.kind !== 'text' && payload.statement.content.kind !== 'app_action'}<p
        >
          <strong>内容类型：</strong>{payload.statement.content.contentType ?? '未提供'}
        </p>
        <p><strong>内容位置：</strong>{payload.statement.content.location ?? '未提供'}</p>
        <p><strong>SHA-256：</strong></p>
        <code class="hash">{payload.statement.content.sha256}</code>
        <div class="notice warning">
          <Icon name="info" />
          <p>
            未提供原文件，未核对文件内容。签署将绑定{payload.statement.content.kind ===
            'file_statement'
              ? '上述声明与'
              : ''}此摘要，不能据此确认已审阅原文件或拥有项目权限。
          </p>
        </div>{/if}
    </section>
    <details>
      <summary>核对协议与载荷摘要</summary><code class="hash">{request.digest}</code>
      <pre>{JSON.stringify(payload, null, 2)}</pre>
    </details>
    <label
      >账户登录来源<select bind:value={derivation}
        >{#each config.derivationOrigins as origin}<option>{origin}</option>{/each}</select
      ></label
    >
    <button class="secondary" disabled={session.busy} onclick={connect}
      >连接并核对签名服务</button
    >
    {#if review}<p>
        本月正式执行额度：剩余 {review.usage.remaining} / {review.usage.allowed}；已预留 {review
          .usage.held}。并发操作与执行权重仍由链上在提交时核对。
      </p>
      <dl class="evidence-list">
        <div>
          <dt>公钥指纹</dt>
          <dd><code>{review.fingerprint}</code></dd>
        </div>
        <div>
          <dt>费用上限</dt>
          <dd>{review.maxCycles} cycles</dd>
        </div>
        <div>
          <dt>最终载荷摘要</dt>
          <dd><code>{review.toBeSignedDigest}</code></dd>
        </div>
      </dl>{/if}
    {#if result}<p role="status">
        {result.stage === 'complete'
          ? '签名与链上执行回执已核验'
          : result.stage === 'failed'
            ? `执行已失败：${result.error}`
            : result.stage === 'result_expired'
              ? '结果已过期，不能自动重签'
              : '原请求已持久化，结果需对账'}
      </p>
      {#if result.artifact}<button
          class="secondary"
          onclick={() =>
            downloadBlob(
              new Blob(
                [
                  JSON.stringify(
                    {
                      format: 'dmsg-signature-evidence/1',
                      artifact: result!.artifact,
                      receipt: result!.receipt,
                      keyDescriptorCbor: result!.keyDescriptorCbor,
                      executionCertificateCbor: result!.executionCertificateCbor,
                      executionId: result!.executionId
                    },
                    null,
                    2
                  )
                ],
                { type: 'application/json' }
              ),
              'signature-evidence.json'
            )}>导出签名与执行证据</button
        >{/if}{/if}
    <div class="notice">
      <Icon name="info" />
      <p>
        普通连接不授予签名权。此流程核验实际签名与执行批准，不提供可信时间戳，也不证明外部应用当前权限。
      </p>
    </div>
    <div class="approval-actions">
      {#if request.state === 'awaiting_user'}<button class="secondary" onclick={reject}
          >拒绝请求</button
        ><button class="primary" disabled={!review || session.busy} onclick={() => execute()}
          >批准并签署</button
        >{:else}<button
          class="primary"
          disabled={!client || session.busy}
          onclick={() => execute(true)}>按原请求对账</button
        >{/if}
    </div>
  {:else}<p role="status">
      {session.busy ? '正在解密并核对请求…' : '没有可批准的有效请求。'}
    </p>{/if}
  {#if session.error}<p class="form-error" role="alert">
      {session.error}
    </p>{/if}{#if session.message}<p class="form-success" role="status">
      {session.message}
    </p>{/if}
</main>
