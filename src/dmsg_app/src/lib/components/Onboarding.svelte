<script lang="ts">
  import { onMount } from 'svelte'
  import { session, downloadBlob, shortId, dateLabel } from '../session.svelte'
  import { config } from '../config'
  import { connectAccount, loginOrigin, syncContent } from '../connection'
  import { agentFor, registrationHome, userActor } from '../services/ic'
  import type { AccountClient } from '../services/account'
  import { AccountRootClient } from '../services/account-root'
  import { readCloudSecurity } from '../services/cloud-security'
  import { CloudClient } from '../services/relay'
  import { evaluatePrf } from '../services/prf'
  import { removeWorkspaceDatabase, currentWorkspace, registry, workspaceState } from '../db'
  import { xidBytes } from '../protocol/identity'
  import { hex } from '../protocol/codec'
  import { errorText } from '../errors'
  import Icon from './Icon.svelte'
  let screen = $state<'intro' | 'bind' | 'pair' | 'recover'>('intro')
  let derivation = $state(loginOrigin())
  let elsewhere = $state(false)
  onMount(() => {
    void workspaceState().then((state) => (elsewhere = state.unlocked))
  })
  let account = $state(''),
    packet = $state(''),
    pairingRole = $state<'Member' | 'Administrator'>('Member')
  let client = $state.raw<AccountClient | null>(null),
    recovery = $state.raw<Awaited<ReturnType<AccountClient['recoveryStatus']>> | null>(null)
  const needsSetup = $derived(!session.initialized)
  const provisional = $derived(session.meta?.unlock === 'provisional')
  function roots(c: AccountClient, cose: NonNullable<Awaited<ReturnType<typeof connectAccount>>['api']['cose']>) {
    if (!config.relayOrigin) throw new Error('请先配置密文服务，才能保存内容根。')
    return new AccountRootClient(
      c,
      new CloudClient({ origin: config.relayOrigin, environment: config.environment }),
      cose
    )
  }
  async function setup() {
    await session.run(async () => {
      const meta = await session.crypto.call('initialize')
      session.activate(meta)
      screen = 'bind'
    })
  }
  async function unlockProvisional() {
    await session.run(async () => {
      await session.unlock(null)
      screen = 'bind'
    })
  }
  /** The cloud holds the only copy outside this device: sync after each login. */
  async function syncAfterLogin() {
    if (!session.bound || !config.relayOrigin) return
    const result = await session.run(async () => {
      session.progress = { stage: '正在同步云端内容', completed: 0, total: 1 }
      try {
        return await syncContent(derivation)
      } catch (error) {
        throw new Error(`自动同步未完成：${errorText(error)} 可在“设置 → 云端同步”重试。`)
      }
    })
    if (result)
      session.message = `已同步：核验 ${result.records} 个云端版本，提交 ${result.sent} 个本地版本。`
  }
  /** Login, fetch this device's unlock secret from the account's home, unlock. */
  async function unlockWithLogin() {
    await session.run(async () => {
      const meta = session.meta!
      const { account: c } = await connectAccount(derivation, { bound: false })
      // Binding switches to the login secret before the root opens; if that
      // was interrupted, the login's account names the secret to fetch.
      const id = meta.account?.id ?? (await c.connectedAccount())
      if (!id) throw new Error('此登录身份还没有账户。')
      const secret = await c.unlockSecret(id)
      await session.unlock({ secret })
    })
    await syncAfterLogin()
  }
  async function unlockWithPrf() {
    await session.run(async () => {
      const meta = session.meta!
      if (!meta.prf) throw new Error('此设备未启用生物识别解锁。')
      await session.unlock({ prf: await evaluatePrf(meta.prf.credentialId) })
      void checkDeviceStatus()
    })
  }
  /** After an offline unlock, a revoked device wipes its local copy as soon as
   * the certified device map says so. */
  async function checkDeviceStatus() {
    const meta = session.meta
    if (!meta?.account) return
    let revoked: boolean
    try {
      const agent = await agentFor(),
        { homeUser, id, issuer } = meta.account
      const { devices } = await readCloudSecurity(userActor(agent, homeUser), agent, {
        accountId: id,
        issuer,
        homeUser
      })
      revoked = devices.some(
        (d) => hex(Uint8Array.from(d.input.device_id)) === meta.deviceId && d.revoked_at.length
      )
    } catch {
      return // Offline or unverifiable: the next login unlock enforces the device state.
    }
    if (!revoked) return
    const name = await currentWorkspace()
    await session.lock()
    if (name) {
      await removeWorkspaceDatabase(name)
      const db = await registry()
      await db.delete('workspaces', name)
      db.close()
    }
    session.initialized = false
    session.meta = null
    session.error = '这台设备已被撤销，本机内容已清除。'
  }
  /** Bind a fresh workspace: this login's account, or a new one at a registration home. */
  async function createAccount() {
    await session.run(async () => {
      let connection: Awaited<ReturnType<typeof connectAccount>>
      try {
        connection = await connectAccount(derivation, { fresh: true, bound: false })
      } catch (error) {
        if (!(error instanceof Error) || !error.message.startsWith('AUTH_REQUIRED')) throw error
        connection = await connectAccount(derivation, { bound: false, home: await registrationHome() })
      }
      const c = connection.account
      client = c
      const cloud = config.relayOrigin
        ? new CloudClient({ origin: config.relayOrigin, environment: config.environment })
        : null
      account =
        (await c.connectedAccount()) ??
        (await c.create(
          cloud ? (home, principal) => cloud.accountAdmission(home, principal) : undefined
        ))
      await bindAndOpen(c, connection.api.cose, false)
    }, '账户已绑定到这台设备。')
    await syncAfterLogin()
  }
  async function bindAndOpen(
    c: AccountClient,
    cose: Awaited<ReturnType<typeof connectAccount>>['api']['cose'],
    recovered: boolean
  ) {
    const secret = await c.unlockSecret(account)
    await session.crypto.call('bindUnlockSecret', secret, derivation)
    if (!cose) throw new Error('未配置密钥服务。')
    const r = roots(c, cose)
    const state = await c.refresh(account)
    const job = recovered
      ? await r.recoverCurrent(account)
      : state.info.current_root[0]
        ? await r.openCurrent(account)
        : await r.run(account, (stage) => {
            session.progress = { stage: `内容根：${stage}`, completed: 0, total: 1 }
          })
    session.activate(await r.activate(account, job))
    if (recovered) {
      // The recovered root is known to the old devices; wrap a fresh one to this device only.
      session.activate(await r.activate(account, await r.rotate(account)))
    }
    await session.refresh()
  }
  async function pairingRequest() {
    await session.run(async () => {
      xidBytes(account)
      const { account: c } = await connectAccount(derivation, { fresh: true, bound: false })
      client = c
      packet = await c.pairing(account, pairingRole)
    })
  }
  async function openAfterApproval() {
    await session.run(async () => {
      xidBytes(account)
      const connection = await connectAccount(derivation, { bound: false })
      await bindAndOpen(connection.account, connection.api.cose, false)
    }, '已读取当前内容根，工作台可以使用。')
    await syncAfterLogin()
  }
  async function requestRecovery() {
    await session.run(async () => {
      xidBytes(account)
      const { account: c } = await connectAccount(derivation, { fresh: true, bound: false })
      client = c
      recovery = await c.requestRecovery(account)
    }, '恢复申请已提交；等待期内任何原设备都可取消。')
  }
  async function recoveryProgress() {
    await session.run(async () => {
      xidBytes(account)
      const { account: c } = await connectAccount(derivation, { bound: false })
      client = c
      recovery = await c.recoveryStatus(account)
    })
  }
  async function completeRecovery() {
    await session.run(async () => {
      const connection = await connectAccount(derivation, { bound: false })
      const c = connection.account
      const state = await c.refresh(account)
      if (!state.device) await c.completeRecovery(account)
      await bindAndOpen(c, connection.api.cose, true)
    }, '账户已恢复到这台设备，并已换到新的内容根。')
    await syncAfterLogin()
  }
</script>

<div class="welcome">
  <div class="welcome-copy">
    <span class="eyebrow">YOUR SPACE. YOUR SAY.</span>
    <h1>你的空间，<br />你来决定。</h1>
    <p class="lead">把秘密留给自己，<br />把信任交给你选定的人。</p>
    <div class="welcome-principles">
      <p><span>01</span> 私密内容，在本机加密</p>
      <p><span>02</span> 每一次授权，都由你确认</p>
      <p><span>03</span> 登录身份，就是你的归路</p>
    </div>
    <p class="fine-print">Built by ICPanda DAO</p>
  </div>
  <section class="welcome-panel" aria-label="建立或解锁工作台">
    <div class="panel-symbol">
      <Icon name={screen === 'recover' ? 'refresh' : 'lock'} size={28} />
    </div>
    {#if session.unlocked && !session.meta?.account}
      {#if screen === 'pair'}
        <span class="eyebrow">ADD THIS DEVICE</span>
        <h2>加入已有账户。</h2>
        <p>已有管理员设备批准这台设备并换根后，用同一登录身份读取当前内容根。</p>
        <label>账户 Xid<input bind:value={account} autocomplete="off" spellcheck="false" /></label>
        <label
          >申请的角色<select bind:value={pairingRole}
            ><option value="Member">成员：内容签名与解锁</option><option value="Administrator"
              >管理员：另含账户根管理</option
            ></select
          ></label
        >
        <button class="secondary wide" disabled={session.busy || !account} onclick={pairingRequest}
          >登录并生成设备批准请求</button
        >
        {#if packet}<label
            >交给已有管理员设备的请求<textarea rows="5" readonly value={packet}></textarea></label
          ><button
            class="text-button"
            onclick={() =>
              downloadBlob(
                new Blob([packet], { type: 'application/json' }),
                'dmsg-approval-request.json'
              )}>下载请求</button
          >{/if}
        <button class="primary wide" disabled={session.busy || !account} onclick={openAfterApproval}
          >已获批准，读取当前内容根<Icon name="arrow-right" /></button
        >
        <button class="text-button" onclick={() => (screen = 'bind')}>返回</button>
      {:else if screen === 'recover'}
        <span class="eyebrow">RECOVER YOUR ACCOUNT</span>
        <h2>所有设备都丢失时。</h2>
        <p>
          用绑定过的登录身份申请恢复。等待期（默认 3 天）内原设备可以取消；到期后这台设备替换全部旧设备和绑定，并通过链上密钥服务取回当前内容根。
        </p>
        <label>账户 Xid<input bind:value={account} autocomplete="off" spellcheck="false" /></label>
        <button class="secondary wide" disabled={session.busy || !account} onclick={requestRecovery}
          >登录并申请恢复到本机</button
        >
        <button class="text-button" disabled={session.busy || !account} onclick={recoveryProgress}
          >查询恢复进度</button
        >
        {#if recovery?.pending}<p>
            最早完成时间：{dateLabel(Number(recovery.pending.execute_after))}；申请设备
            <code>{shortId(recovery.pending.request.device.device_id instanceof Uint8Array ? Array.from(recovery.pending.request.device.device_id, (b) => b.toString(16).padStart(2, '0')).join('') : '')}</code>
          </p>{:else if recovery}<p>当前没有待处理的恢复申请。</p>{/if}
        <button class="primary wide" disabled={session.busy || !account} onclick={completeRecovery}
          >等待期已过，完成恢复并取回内容根<Icon name="arrow-right" /></button
        >
        <button class="text-button" onclick={() => (screen = 'bind')}>返回</button>
      {:else}
        <span class="eyebrow">01 / YOUR ACCOUNT</span>
        <h2>登录，绑定你的账户。</h2>
        <p>本机解锁材料由你的账户服务按登录身份发放，不再需要口令或恢复码。</p>
        <label
          >登录来源<select bind:value={derivation}
            >{#each config.derivationOrigins as origin}<option value={origin}>{origin}</option
              >{/each}</select
          ></label
        >
        <button class="primary wide" disabled={session.busy || !config.canisters.handle} onclick={createAccount}
          >连接 Internet Identity 并创建或绑定账户<Icon name="arrow-right" /></button
        >
        {#if !config.canisters.handle}<p class="caption">先在构建配置中设置名称注册表与用户服务。</p>{/if}
        <button class="secondary wide" onclick={() => (screen = 'pair')}>加入已有账户的新设备</button>
        <button class="secondary wide" onclick={() => (screen = 'recover')}>所有设备丢失后恢复</button>
        {#if session.meta}<div class="identity-line">
            <span>本机设备</span><code>{shortId(session.meta.deviceId)}</code>
          </div>{/if}
      {/if}
    {:else if !needsSetup}
      <span class="eyebrow">WELCOME BACK</span>
      <h2>欢迎回到你的空间。</h2>
      {#if provisional}
        <p>这台设备尚未绑定账户。继续完成设置。</p>
        {#if elsewhere}<p class="caption">
            设置正在另一窗口进行；在这里继续会锁定那个窗口。
          </p>{/if}
        <button class="primary wide" disabled={session.busy} onclick={unlockProvisional}
          >继续设置<Icon name="arrow-right" /></button
        >
      {:else}
        <p>登录后由账户服务发放本机解锁材料；撤销设备即刻生效。</p>
        {#if elsewhere}<p class="caption">
            工作台已在另一窗口解锁；在这里解锁会锁定那个窗口。
          </p>{/if}
        {#if session.meta?.prf}<button class="primary wide" disabled={session.busy} onclick={unlockWithPrf}
            >生物识别解锁<Icon name="arrow-right" /></button
          >{/if}
        <label
          >登录来源<select bind:value={derivation}
            >{#each config.derivationOrigins as origin}<option value={origin}>{origin}</option
              >{/each}</select
          ></label
        >
        <button class={session.meta?.prf ? 'secondary wide' : 'primary wide'} disabled={session.busy} onclick={unlockWithLogin}
          >{session.busy ? '正在解锁…' : '登录并解锁'}<Icon name="arrow-right" /></button
        >
      {/if}
      {#if session.meta}<div class="identity-line">
          <span>本机设备</span><code>{shortId(session.meta.deviceId)}</code>
        </div>{/if}
    {:else}
      <span class="eyebrow">A PRIVATE WORKSPACE</span>
      <h2>从一件私密的事开始。</h2>
      <p>保存笔记、凭据和文件。内容在本机加密，只有你批准的设备能读取。</p>
      <div class="notice">
        <Icon name="info" />
        <p>
          没有口令、恢复码或离线备份：登录身份加等待期是恢复途径，云端同步保存设备外的唯一副本。
        </p>
      </div>
      <button class="primary wide" disabled={session.busy} onclick={setup}
        >创建我的工作台<Icon name="arrow-right" /></button
      >
      <p class="caption">建立后需登录 Internet Identity 绑定账户；登录身份加等待期是唯一的恢复途径。</p>
    {/if}
    {#if session.progress}<div class="progress-area" role="status">
        <span>{session.progress.stage}</span><progress
          max={session.progress.total || 1}
          value={session.progress.completed}
        ></progress>
      </div>{/if}
    {#if session.error}<p class="form-error" role="alert">{session.error}</p>{/if}
    {#if session.message}<p class="form-success" role="status">{session.message}</p>{/if}
    <div class="panel-foot">
      <span class="status-dot"></span>{config.environment === 'production'
        ? '内容在本机加密'
        : `${config.environment} 环境 · 内容在本机加密`}
    </div>
  </section>
</div>
