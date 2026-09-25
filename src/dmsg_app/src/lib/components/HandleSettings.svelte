<script lang="ts">
  import { session } from '../session.svelte'
  import { config } from '../config'
  import { connectAccount, connectIdentity } from '../connection'
  import type { AccountClient } from '../services/account'
  import { HandleClient } from '../services/handle'
  import { xidText } from '../protocol/identity'
  let origin = $state(config.derivationOrigins[0]),
    oldOrigin = $state(config.derivationOrigins[0]),
    name = $state(''),
    status = $state('')
  let account = $state.raw<AccountClient | null>(null),
    client = $state.raw<HandleClient | null>(null)
  let preview = $state<Awaited<ReturnType<HandleClient['prepare']>> | null>(null)
  let job = $state<Awaited<ReturnType<HandleClient['job']>>>(null)
  async function connect(old: boolean) {
    await session.run(async () => {
      if (!session.meta?.account) throw new Error('请先建立正式工作区。')
      if (!old) {
        account = (await connectAccount(origin)).account
        client = null
        preview = null
        status = '已核对目标新账户，请另行登录旧名称权利人。'
      } else {
        // The legacy owner is a different person or II account; always prompt.
        const { identity, api } = await connectIdentity(oldOrigin)
        if (!account || !api.handle) throw new Error('先连接目标新账户与名称注册表。')
        client = new HandleClient(
          account,
          api.handle,
          config.canisters.handle,
          identity.getPrincipal()
        )
        job = await client.job()
        status = `旧身份：${identity.getPrincipal().toText()}`
      }
    })
  }
  async function inspect() {
    await session.run(async () => {
      if (!client) throw new Error('请先连接两端身份。')
      const owner = await client.ownership(name)
      if (owner) {
        status = `此名称当前归属 ${xidText(owner.owner_account)}，版本 ${owner.version}。`
        preview = null
        return
      }
      preview = await client.prepare(name)
      job = preview.job
      status = '名称尚未认领；冻结权利与目标账户已核对。'
    })
  }
  async function claim() {
    await session.run(async () => {
      if (!client || !job) throw new Error('请先核对认领预览。')
      try {
        await client.run()
        status = '免费认领已完成。名称映射已变更，原内容及权限分别保留。'
      } finally {
        job = await client.job()
        if (job?.phase === 'rejected') {
          preview = null
          status = ''
        }
      }
    })
  }
</script>

<section class="settings-section">
  <h2>旧名称认领</h2>
  <p>只有完整封存的名称可认领。普通委托不能代替冻结 owner 或管理员；隔离名称需先处理权属。</p>
  <p>目标新账户：<code>{session.meta?.account?.id ?? '尚未绑定'}</code></p>
  <label
    >新账户登录来源<select bind:value={origin}
      >{#each config.derivationOrigins as value}<option>{value}</option>{/each}</select
    ></label
  >
  <button class="secondary" disabled={session.busy} onclick={() => connect(false)}
    >连接目标新账户</button
  >
  <label
    >旧权利人原登录来源<select bind:value={oldOrigin}
      >{#each config.derivationOrigins as value}<option>{value}</option>{/each}</select
    ></label
  >
  <button class="secondary" disabled={!account || session.busy} onclick={() => connect(true)}
    >登录旧权利人</button
  >
  <label>旧名称<input bind:value={name} maxlength="20" autocomplete="off" /></label>
  <button class="secondary" disabled={!client || session.busy} onclick={inspect}
    >核对权属与认领预览</button
  >
  {#if preview}<dl class="evidence-list">
      <div>
        <dt>名称</dt>
        <dd>{preview.reservation.handle}</dd>
      </div>
      <div>
        <dt>冻结 owner</dt>
        <dd><code>{preview.reservation.legacy_owner.toText()}</code></dd>
      </div>
      <div>
        <dt>冻结版本</dt>
        <dd>{String(preview.snapshot.freeze_version)}</dd>
      </div>
      <div>
        <dt>来源服务</dt>
        <dd><code>{preview.snapshot.source_canister.toText()}</code></dd>
      </div>
    </dl>{/if}
  {#if job?.phase === 'rejected'}<p role="status">
      上次认领未通过链上核验，请重新核对权属与认领预览。
    </p>
  {:else if job && job.phase !== 'claimed'}<p>
      待认领 {job.name} → {job.account}；旧身份 {job.sourceOwner}。此批准只用于这一精确意图。
    </p>
    <button class="primary" disabled={!client || session.busy} onclick={claim}
      >批准并继续原认领</button
    >{/if}
  {#if status}<p role="status">{status}</p>{/if}
</section>
