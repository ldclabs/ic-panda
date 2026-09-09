<script lang="ts">
  import { onMount } from 'svelte'
  import { session, dateLabel } from '../session.svelte'
  import { listRequests, rejectRequest } from '../requests'
  import {
    assertRequestUnchanged,
    type PendingRequest,
    type SignatureRequest
  } from '../protocol/requests'
  import Icon from './Icon.svelte'
  let request = $state<PendingRequest | null>(null),
    payload = $state<SignatureRequest | null>(null),
    finished = $state(false)
  onMount(() => {
    const id = new URLSearchParams(location.search).get('id')
    void session.run(async () => {
      request = (await listRequests()).find((r) => r.id === id) ?? null
      if (!request) throw new Error('请求不存在，请返回原应用重新发起。')
      const decoded = (await session.crypto.call(
        'readRequest',
        request.payload,
        request.id
      )) as SignatureRequest
      assertRequestUnchanged(decoded, request)
      payload = decoded
    })
    const timer = setInterval(() => {
      void listRequests().then((requests) => {
        const latest = requests.find((r) => r.id === id)
        if (latest && latest.state !== 'awaiting_user') {
          request = latest
          payload = null
          finished = true
        }
      })
    }, 3000)
    return () => clearInterval(timer)
  })
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
      <p>没有从此窗口执行新的签名。</p>
      <button class="secondary" onclick={() => window.close()}>关闭窗口</button>
    </div>
  {:else if payload && request}<div class="request-origin">
      <Icon name="chrome" />
      <div><span>浏览器确认的请求来源</span><strong>{request.source.origin}</strong></div>
    </div>
    <dl class="evidence-list">
      <div>
        <dt>请求类型</dt>
        <dd>{payload.statement.content.kind === 'text' ? '文本声明' : '内容摘要声明'}</dd>
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
      <span class="field-label">实际待签内容</span
      >{#if payload.statement.content.kind === 'text'}<p class="preserve-lines">
          {payload.statement.content.text}
        </p>{:else}<p>
          <strong>内容类型：</strong>{payload.statement.content.contentType ?? '未提供'}
        </p>
        <p><strong>内容位置：</strong>{payload.statement.content.location ?? '未提供'}</p>
        <p><strong>SHA-256：</strong></p>
        <code class="hash">{payload.statement.content.sha256}</code>
        <div class="notice warning">
          <Icon name="info" />
          <p>仅提供摘要，未核对原文件。不能据此确认文件内容或项目权限。</p>
        </div>{/if}
    </section>
    <details>
      <summary>核对协议与载荷摘要</summary><code class="hash">{request.digest}</code>
      <pre>{JSON.stringify(payload, null, 2)}</pre>
    </details>
    <div class="notice">
      <Icon name="info" />
      <p>正式签名服务尚未启用。连接应用或解锁工作台，不会产生签名。</p>
    </div>
    <div class="approval-actions">
      <button class="secondary" onclick={reject}>拒绝请求</button><button
        class="primary"
        disabled
        title="需要已授权设备和通过验证的正式签名服务">批准并签署</button
      >
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
