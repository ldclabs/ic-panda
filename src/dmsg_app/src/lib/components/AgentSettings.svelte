<script lang="ts">
  import { session, dateLabel } from '../session.svelte'
  import { config } from '../config'
  import { connectAccount, loginOrigin } from '../connection'
  import { CloudClient } from '../services/relay'
  import { CloudSession } from '../services/cloud-session'
  import {
    AgentClient,
    isCurrent,
    type AgentJob,
    type Authority,
    type Credential
  } from '../services/agent'
  import { agentId, delegationId, MAX_GRANT_DAYS } from '../protocol/agent'
  import type { HostedController, PrincipalInfo } from '../canisters/generated/user'
  import Modal from './Modal.svelte'

  const types = [
    ['Person', '个人'],
    ['Organization', '组织'],
    ['Team', '团队'],
    ['Project', '项目'],
    ['Other', '其他']
  ] as const
  const split = (text: string) =>
    text
      .split(/[\s,]+/)
      .map((v) => v.trim())
      .filter(Boolean)

  let derivation = $state(loginOrigin()),
    status = $state('')
  let client = $state.raw<AgentClient | null>(null),
    principal = $state.raw<PrincipalInfo | null>(null),
    credentials = $state.raw<Credential[]>([]),
    jobs = $state.raw<AgentJob[]>([])
  let principalType = $state<(typeof types)[number][0]>('Person')
  // Registration: authority is always an explicit owner choice.
  let authorityKind = $state<'restricted' | 'all'>('restricted'),
    ceilingScopes = $state(''),
    ceilingAudiences = $state('https://dmsg.net'),
    controllerName = $state(''),
    preview = $state<{ generation: number; agentId: string } | null>(null)
  // Grant.
  let signer = $state(0),
    subject = $state(''),
    scopes = $state(''),
    audiences = $state(''),
    relationship = $state(''),
    days = $state(30),
    review = $state<null | { kind: 'grant' } | { kind: 'revoke'; id: string }>(null)

  const account = () => session.meta?.account?.id ?? ''
  const current = $derived(principal?.state.controllers.filter(isCurrent) ?? [])
  const published = $derived(
    principal ? principal.published_version === principal.state.version : false
  )

  async function load() {
    if (!client) return
    principal = await client.principal(account())
    jobs = await client.jobs()
    if (client.cloud && principal) credentials = await client.credentials(account())
    if (!current.some((c) => c.generation === signer)) signer = current[0]?.generation ?? 0
  }
  async function connect() {
    await session.run(async () => {
      if (!session.meta?.account || !config.principalOrigin || !config.agentOrigin)
        throw new Error('先完成正式工作区绑定，并配置 principal 与 delegation 服务地址。')
      const { account: accountClient } = await connectAccount(derivation)
      const cloud = config.relayOrigin
        ? new CloudSession(
            accountClient,
            new CloudClient({ origin: config.relayOrigin, environment: config.environment }),
            session.meta.account.id
          )
        : null
      client = new AgentClient(accountClient, cloud)
      await load()
      status = principal ? '已读取 principal 与凭证状态。' : '此账户尚未启用 Agent principal。'
    })
  }
  async function enable() {
    await session.run(async () => {
      await client!.enable(account(), principalType)
      await load()
    }, '已启用并发布 principal；公开文档会显示此账户使用 dMsg。')
  }
  async function previewController() {
    await session.run(async () => {
      const generation = (principal!.state.controllers.at(-1)?.generation ?? 0) + 1
      preview = { generation, agentId: (await client!.controllerKey(account(), generation)).agentId }
    })
  }
  async function register() {
    await session.run(async () => {
      const authority: Authority =
        authorityKind === 'all'
          ? { kind: 'all' }
          : { kind: 'restricted', scopes: split(ceilingScopes), audiences: split(ceilingAudiences) }
      await client!.register(account(), {
        authority,
        name: controllerName.trim() || undefined
      })
      preview = null
      await load()
      if (client!.cloud) await client!.refreshService(account())
    }, 'controller 已登记；发布完成后第三方即可验证。')
  }
  async function change(action: 'retire' | 'compromise', c: HostedController) {
    await session.run(async () => {
      if (action === 'retire') await client!.retire(account(), c.generation)
      else await client!.compromise(account(), c.generation, Number(c.valid_from))
      await load()
      if (client!.cloud) await client!.refreshService(account())
    }, action === 'retire' ? '已退役；此 key 不会再签出新事件。' : '已标记泄露；其签出的凭证将被暂停。')
  }
  async function publish() {
    await session.run(async () => {
      await client!.publish(account())
      await load()
    }, '已重新发布 principal 文档。')
  }
  async function sign() {
    const pending = review
    review = null
    await session.run(async () => {
      const job =
        pending?.kind === 'revoke'
          ? await client!.revoke(account(), signer, pending.id)
          : await client!.grant(account(), signer, {
              id: delegationId(account()),
              subject: subject.trim(),
              scopes: split(scopes),
              audiences: split(audiences),
              days,
              relationship: relationship.trim() || undefined
            })
      await load()
      status =
        job.stage === 'accepted'
          ? `已签发并提交 ${job.delegationId}。`
          : `签名已记录（${job.stage}${job.error ? `：${job.error}` : ''}），可在待处理记录中继续。`
    })
  }
  async function resume(job: AgentJob) {
    await session.run(async () => {
      const result = await client!.resume(job.id)
      await load()
      status = `记录 ${result.delegationId}：${result.stage}${result.error ? `（${result.error}）` : ''}`
    })
  }
  const authorityLabel = (c: HostedController) =>
    'Unrestricted' in c.delegation
      ? '全部（"*"）'
      : `${c.delegation.Restricted.scopes.join(', ')} → ${c.delegation.Restricted.audiences.join(', ')}`
</script>

<section class="settings-section">
  <h2>Agent 授权</h2>
  <p>
    启用后，此账户成为 Agent Delegation principal，公开文档位于
    <code>{config.principalOrigin || '（未配置）'}/{account() || '…'}</code>。controller key
    保存在你的 vault 中，持根设备都能签发或撤销授权。
  </p>
  <label
    >登录来源<select bind:value={derivation}
      >{#each config.derivationOrigins as value}<option>{value}</option>{/each}</select
    ></label
  >
  <button class="secondary" disabled={session.busy} onclick={connect}>连接并读取</button>
  {#if status}<p role="status">{status}</p>{/if}
</section>

{#if client && !principal}
  <section class="settings-section">
    <h2>启用 principal</h2>
    <p>公开文档会显示“此账户使用 dMsg”及 controller 来源；名称与头像不写入文档。</p>
    <label
      >类型<select bind:value={principalType}
        >{#each types as [value, label]}<option {value}>{label}</option>{/each}</select
      ></label
    >
    <button class="primary" disabled={session.busy} onclick={enable}>批准并启用</button>
  </section>
{:else if client && principal}
  <section class="settings-section">
    <h2>Principal</h2>
    <dl class="evidence-list">
      <div>
        <dt>ID</dt>
        <dd><code>{principal.principal_id}</code></dd>
      </div>
      <div>
        <dt>版本</dt>
        <dd>
          {String(principal.state.version)}（已发布 {String(principal.published_version)}）
        </dd>
      </div>
    </dl>
    {#if !published}<p role="alert">
        最新变更尚未发布；发布前 delegation 服务会拒绝新事件。
      </p>
      <button class="secondary" disabled={session.busy} onclick={publish}>重新发布</button>{/if}
  </section>

  <section class="settings-section">
    <h2>controller</h2>
    {#each principal.state.controllers as c (c.generation)}
      <article class="history-message">
        <strong>#{c.generation} {c.name[0] ?? ''}</strong>
        <p><code>{agentId(Uint8Array.from(c.public_key))}</code></p>
        <p>权限：{authorityLabel(c)}</p>
        <p>
          生效 {dateLabel(Number(c.valid_from))}{#if c.retired_at.length}，退役 {dateLabel(
              Number(c.retired_at[0])
            )}{/if}{#if c.invalid_from.length}，自 {dateLabel(Number(c.invalid_from[0]))} 起不可信{/if}
        </p>
        {#if isCurrent(c)}
          <button class="secondary" disabled={session.busy} onclick={() => change('retire', c)}
            >退役</button
          >
        {/if}
        {#if !c.invalid_from.length}
          <button class="secondary" disabled={session.busy} onclick={() => change('compromise', c)}
            >标记泄露（自生效起）</button
          >
        {/if}
      </article>
    {/each}
    <h3>登记新的 controller</h3>
    <p>权限一经发布不可修改；扩大权限需登记新 key。受限 key 只能替换或撤销 scope 与依赖方都在其上限内的凭证。</p>
    <label
      ><input type="radio" bind:group={authorityKind} value="restricted" /> 受限（明确的 scope 与依赖方）</label
    >
    <label><input type="radio" bind:group={authorityKind} value="all" /> 全部授权（"*"）</label>
    {#if authorityKind === 'restricted'}
      <label>scope（空格或逗号分隔）<input bind:value={ceilingScopes} /></label>
      <label>依赖方 origin 或 Agent ID<input bind:value={ceilingAudiences} /></label>
    {/if}
    <label>名称（可选）<input bind:value={controllerName} maxlength="64" /></label>
    <button class="secondary" disabled={session.busy} onclick={previewController}>预览 key</button>
    {#if preview}
      <p>
        将登记 #{preview.generation}：<code>{preview.agentId}</code>。权限：{authorityKind === 'all'
          ? '全部（"*"）'
          : `${ceilingScopes} → ${ceilingAudiences}`}
      </p>
      <button class="primary" disabled={session.busy} onclick={register}>批准并登记</button>
    {/if}
  </section>

  {#if current.length}
    <section class="settings-section">
      <h2>签发授权</h2>
      <label
        >controller<select bind:value={signer}
          >{#each current as c (c.generation)}<option value={c.generation}
              >#{c.generation} {c.name[0] ?? ''}</option
            >{/each}</select
        ></label
      >
      <label>被授权 agent（did:agent）<input bind:value={subject} autocomplete="off" /></label>
      <label>scope（空格或逗号分隔）<input bind:value={scopes} /></label>
      <label>依赖方 origin 或 Agent ID<input bind:value={audiences} /></label>
      <label>关系（可选）<input bind:value={relationship} maxlength="64" /></label>
      <label
        >有效天数<input type="number" min="1" max={MAX_GRANT_DAYS} bind:value={days} /></label
      >
      <button
        class="primary"
        disabled={session.busy || !subject || !scopes || !audiences}
        onclick={() => (review = { kind: 'grant' })}>核对并签发</button
      >
    </section>
  {/if}

  <section class="settings-section">
    <h2>凭证</h2>
    {#if !client.cloud}<p>未配置云端服务，无法列出凭证。</p>{/if}
    {#each credentials as c (c.id)}
      <article class="history-message">
        <strong>{c.id}</strong>
        <p>状态：{c.status}；scope：{c.scopes.join(', ')}；依赖方：{c.audiences.join(', ')}</p>
        <p><code>{c.subject}</code></p>
        {#if c.expires_at}<p>到期 {dateLabel(c.expires_at)}</p>{/if}
        {#if c.status !== 'revoked' && current.length}
          <button
            class="secondary"
            disabled={session.busy}
            onclick={() => (review = { kind: 'revoke', id: c.id })}>撤销</button
          >
        {/if}
      </article>
    {/each}
    {#each jobs.filter((j) => j.stage === 'signed') as job (job.id)}
      <article class="history-message">
        <strong>待处理：{job.kind} {job.delegationId}</strong>
        <p>{job.stage}{job.error ? `（${job.error}）` : ''}</p>
        <button class="secondary" disabled={session.busy} onclick={() => resume(job)}>继续</button>
      </article>
    {/each}
  </section>
{/if}

{#if review}
  <Modal title={review.kind === 'grant' ? '确认签发授权' : '确认撤销'} onclose={() => (review = null)}>
    <dl class="evidence-list">
      <div>
        <dt>principal</dt>
        <dd><code>{principal?.principal_id}</code></dd>
      </div>
      <div>
        <dt>controller</dt>
        <dd>#{signer}</dd>
      </div>
      {#if review.kind === 'grant'}
        <div>
          <dt>被授权 agent</dt>
          <dd><code>{subject}</code>（agent 自述资料不代表身份证明）</dd>
        </div>
        <div>
          <dt>scope</dt>
          <dd>{split(scopes).join(', ')}</dd>
        </div>
        <div>
          <dt>依赖方</dt>
          <dd>{split(audiences).join(', ')}</dd>
        </div>
        <div>
          <dt>有效期</dt>
          <dd>{days} 天</dd>
        </div>
      {:else}
        <div>
          <dt>撤销凭证</dt>
          <dd>{review.id}</dd>
        </div>
      {/if}
    </dl>
    <p>事件由本机 vault 中的 controller key 签名并提交到 delegation 服务。撤销前第三方已完成的动作无法收回。</p>
    <button class="primary" disabled={session.busy} onclick={sign}>批准并签名</button>
  </Modal>
{/if}
