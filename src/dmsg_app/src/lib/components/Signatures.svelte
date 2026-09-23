<script lang="ts">
  import { onMount } from 'svelte'
  import { session, dateLabel } from '../session.svelte'
  import { listRequests, openApproval, rejectRequest } from '../requests'
  import type { PendingRequest } from '../protocol/requests'
  import Icon from './Icon.svelte'
  import FileVerify from './FileVerify.svelte'
  let tab = $state('requests'),
    requests = $state<PendingRequest[]>([])
  const labels: Record<string, string> = {
    awaiting_user: '待确认',
    rejected: '已拒绝',
    cancelled: '已取消',
    expired: '已过期',
    authorized: '已授权执行',
    execution_unknown: '执行结果待对账',
    signed: '已生成签名',
    returned: '应用已取得结果',
    failed: '执行失败',
    result_expired: '结果已过期'
  }
  onMount(() => {
    void listRequests().then((r) => (requests = r))
    const timer = setInterval(() => {
      void listRequests().then((r) => (requests = r))
    }, 10000)
    return () => clearInterval(timer)
  })
</script>

<div class="page-heading">
  <div>
    <span class="eyebrow">SIGN WITH CLARITY</span>
    <h1>签名与授权</h1>
    <p>看清谁在请求，确认你在签署什么。</p>
  </div>
</div>
<div class="filter-bar" aria-label="签名中心视图">
  {#each [['requests', '请求与历史'], ['verify', '文件核对'], ['grants', '应用授权']] as [value, label]}<button
      class:active={tab === value}
      aria-pressed={tab === value}
      onclick={() => (tab = value)}>{label}</button
    >{/each}
</div>
{#if tab === 'requests'}
  <div class="notice">
    <Icon name="info" />
    <p>
      正式签名需要已授权设备，并在独立窗口逐次批准。未知结果按原请求对账；签名与执行回执不等于可信时间戳。
    </p>
  </div>
  {#if requests.length}<div class="request-list">
      {#each requests as request}<article class="request-row">
          <span class="item-icon"
            ><Icon name={request.kind === 'authentication' ? 'key' : 'signature'} /></span
          >
          <div>
            <strong>{request.source.origin}</strong>
            <p>
              {dateLabel(request.createdAt)} · {request.kind === 'authentication' &&
              request.state === 'signed'
                ? '认证已完成'
                : labels[request.state]}
            </p>
            <code class="caption">{request.id.slice(0, 16)}</code>
          </div>
          {#if request.state === 'awaiting_user'}<button
              class="secondary"
              onclick={() =>
                session.run(async () => {
                  await rejectRequest(request.id)
                  requests = await listRequests()
                })}>拒绝</button
            ><button
              class="primary"
              onclick={() =>
                session.run(async () => {
                  await session.lock()
                  await openApproval(request.id)
                })}>查看请求<Icon name="arrow-up-right" /></button
            >{/if}
          {#if ['authorized', 'execution_unknown', 'signed', 'returned', 'failed', 'result_expired'].includes(request.state)}<button
              class="secondary"
              onclick={() =>
                session.run(async () => {
                  await session.lock()
                  await openApproval(request.id)
                })}>查看或对账</button
            >{/if}
        </article>{/each}
    </div>
  {:else}<div class="empty-state signature-empty">
      <Icon name="signature" size={48} /><span class="eyebrow"
        >EVERY SIGNATURE IS A DECISION</span
      >
      <h2>还没有需要你确认的事项。</h2>
      <p>应用连接不等于签名许可。<br />正式签署会在独立扩展窗口中逐次确认。</p>
      <div class="signature-steps">
        <span>01 接收请求</span><Icon name="arrow-right" size={16} /><span>02 核对载荷</span
        ><Icon name="arrow-right" size={16} /><span>03 明确批准</span>
      </div>
    </div>{/if}
{:else if tab === 'verify'}<FileVerify />
{:else}<section class="empty-state">
    <Icon name="key" size={44} />
    <h2>权限，从明确的范围开始。</h2>
    <p>当前没有生效的应用授权。内容访问与正式签名权限分别管理。</p>
    <div class="permission-guide">
      <div>
        <strong>内容授权</strong>
        <p>指定频道或文件、指定操作、限定期限。不会导出频道根密钥。</p>
      </div>
      <div>
        <strong>敏感操作政策</strong>
        <p>设备管理、恢复与正式执行由账户服务检查，不能由普通应用连接获得。</p>
      </div>
    </div>
  </section>{/if}
