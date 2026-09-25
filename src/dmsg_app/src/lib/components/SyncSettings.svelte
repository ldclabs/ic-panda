<script lang="ts">
  import { session, downloadBlob, formatBytes } from '../session.svelte'
  import { config } from '../config'
  import { connectAccount } from '../connection'
  import { ContentClient } from '../services/content'
  import { CloudClient } from '../services/relay'
  import Modal from './Modal.svelte'
  let notices = $state<any[]>([]),
    lifecycle = $state<any>(null)
  let client = $state.raw<ContentClient | null>(null)
  let derivation = $state(config.derivationOrigins[0]),
    status = $state(''),
    usage = $state<{ committed: number; reserved: number } | null>(null)
  let name = $state(''),
    bio = $state(''),
    link = $state(''),
    publishedLinks = $state<string[]>([]),
    originalLink = $state(''),
    observedProfileHash = $state<string | null>(null),
    publishName = $state(false),
    publishBio = $state(false),
    publishLink = $state(false)
  let exportModal = $state(false),
    password = $state('')
  async function connect() {
    await session.run(async () => {
      if (!session.meta?.account || !config.relayOrigin)
        throw new Error('先在设备与认证完成正式工作区绑定，并配置云端服务。')
      const { account } = await connectAccount(derivation)
      const connected = new ContentClient(
        account,
        new CloudClient({ origin: config.relayOrigin, environment: config.environment }),
        session.meta.account.id
      )
      await connected.refresh()
      client = connected
      const quota = await connected.quota()
      usage = { committed: quota.committed, reserved: quota.reserved }
      lifecycle = quota.lifecycle
      notices = quota.lifecycle?.notice ? [quota.lifecycle.notice] : []
      const published = await connected.profile()
      observedProfileHash = published?.value.hash ?? null
      publishedLinks = published?.profile.links ?? []
      originalLink = publishedLinks[0] ?? ''
      publishName = Boolean(published?.profile.display_name)
      publishBio = Boolean(published?.profile.bio)
      publishLink = publishedLinks.length > 0
      name = published?.profile.display_name || session.data.profile?.name || ''
      bio = published?.profile.bio || session.data.profile?.bio || ''
      link = originalLink || session.data.profile?.link || ''
      status = '账户与设备证据已核对。'
    })
  }
  async function sync(write: boolean) {
    await session.run(async () => {
      if (!client) throw new Error('请先连接账户。')
      let pulled = await client.pull()
      const sent = write ? await client.pushPending() : 0
      if (sent) pulled = await client.pull()
      await session.refresh()
      const quota = await client.quota()
      usage = { committed: quota.committed, reserved: quota.reserved }
      lifecycle = quota.lifecycle
      notices = quota.lifecycle?.notice ? [quota.lifecycle.notice] : []
      status = `已验证 ${pulled.records} 个云端版本、${pulled.files} 个文件；本次提交 ${sent} 个本地版本。`
    })
  }
  async function publish() {
    await session.run(async () => {
      if (!client) throw new Error('请先连接账户。')
      const previous = await client.profile()
      if ((previous?.value.hash ?? null) !== observedProfileHash)
        throw new Error('公开资料已由其他设备修改，请重新连接并核对后发布。')
      const result = await client.publishProfile({
        version: (previous?.profile.version ?? 0) + 1,
        prev_hash: previous?.value.hash ?? null,
        display_name: publishName ? name : '',
        bio: publishBio ? bio : '',
        links:
          publishLink && link
            ? link === originalLink
              ? publishedLinks
              : [link, ...publishedLinks.slice(1)]
            : [],
        avatar_upload: previous?.profile.avatar_upload ?? null
      })
      observedProfileHash = result.value.hash
      publishedLinks = result.profile.links
      originalLink = publishedLinks[0] ?? ''
      status = '已发布明确选择的公开字段。'
    })
  }
  async function backup() {
    await session.run(async () => {
      if (!client) throw new Error('请先连接账户。')
      await client.pull()
      const result = await session.crypto.call('exportBackup', password)
      downloadBlob(result.blob, result.name)
      password = ''
      exportModal = false
      await session.refresh()
      status = result.missing.length
        ? `恢复包已生成，仍有 ${result.missing.length} 项本地任务缺口。`
        : '已生成包含已验证云端快照和本机内容的恢复包。'
    })
  }
</script>

<section class="settings-section">
  <h2>云端同步与导出</h2>
  <p>
    内容在扩展中加密和验证。后台仅能提交已固定的密文版本，短期批准到期后暂停；重新解锁后核对原操作。
  </p>
  <label
    >原登录来源<select bind:value={derivation}
      >{#each config.derivationOrigins as origin}<option>{origin}</option>{/each}</select
    ></label
  >
  <button class="secondary" onclick={connect} disabled={session.busy || !session.meta?.account}
    >连接当前账户</button
  >
  {#if client}<div class="settings-row">
      <button class="secondary" onclick={() => sync(false)} disabled={session.busy}
        >读取与验证云端内容</button
      ><button class="primary" onclick={() => sync(true)} disabled={session.busy}
        >同步待提交内容</button
      ><button
        class="secondary"
        disabled={session.busy}
        onclick={() =>
          session.run(async () => {
            const count = await client!.prepareBackground()
            status = `已准备 ${count} 个固定密文版本；授权到期后需解锁对账。`
          })}>准备短期后台提交</button
      ><button class="secondary" onclick={() => (exportModal = true)} disabled={session.busy}
        >完整导出云端与本机内容</button
      >
    </div>{/if}
  {#if usage}<p>
      中继报告的密文用量：已提交 {formatBytes(usage.committed)}，预留 {formatBytes(
        usage.reserved
      )}。写权限和额度以有效商业证据为准。
    </p>{/if}
  <p class="caption">
    冲突保留双方版本；未知提交先查询原请求。恢复包单文件上限 256
    MiB，包含全部编码与封装开销，超限明确失败。读取和导出不要求重新购买套餐。
  </p>
  {#if lifecycle}<details>
      <summary>资源生命周期与退出窗口</summary>
      <pre>{JSON.stringify(lifecycle, null, 2)}</pre>
      {#each notices as notice}<button
          class="secondary"
          disabled={!client || session.busy}
          onclick={() =>
            session.run(async () => {
              await client!.acknowledgeNotice(notice.id)
              status = '通知已签收；原定导出窗口会保留。'
            })}>明确签收通知 {notice.id}</button
        >{/each}
    </details>{/if}
  {#if status}<p role="status">{status}</p>{/if}
</section>
<section class="settings-section">
  <h2>选择公开资料</h2>
  <p>仅在点击发布时公开选中的字段。取消字段选择并发布，会从当前公开资料中移除该字段。</p>
  <label
    ><input type="checkbox" bind:checked={publishName} /> 公开展示名<input
      bind:value={name}
      maxlength="80"
    /></label
  >
  <label
    ><input type="checkbox" bind:checked={publishBio} /> 公开简介<textarea
      bind:value={bio}
      maxlength="2000"></textarea></label
  >
  <label
    ><input type="checkbox" bind:checked={publishLink} /> 公开链接<input
      type="url"
      bind:value={link}
    /></label
  >
  <button class="primary" onclick={publish} disabled={!client || session.busy}
    >发布选中的字段</button
  >
  <button
    class="secondary"
    disabled={!client || session.busy}
    onclick={() =>
      session.run(async () => {
        await client!.resumeProfile()
        status = '公开资料原请求已对账。'
      })}>对账上次资料修改</button
  >
</section>
{#if exportModal}<Modal
    title="导出云端与本机恢复包"
    onclose={() => {
      exportModal = false
      password = ''
    }}
    ><form
      onsubmit={(event) => {
        event.preventDefault()
        void backup()
      }}
    >
      <p>先固定并验证云端快照，再合并本机内容。文件不包含设备或认证私钥。</p>
      <label
        >当前本机口令<input
          type="password"
          bind:value={password}
          autocomplete="current-password"
          required
        /></label
      >{#if session.error}<p role="alert">{session.error}</p>{/if}<button
        class="primary"
        disabled={session.busy}>验证并导出</button
      >
    </form></Modal
  >{/if}
