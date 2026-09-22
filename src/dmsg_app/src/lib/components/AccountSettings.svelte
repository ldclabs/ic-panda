<script lang="ts">
  import { session, shortId, dateLabel, downloadBlob } from '../session.svelte'
  import { config } from '../config'
  import { login, services } from '../services/ic'
  import { AccountClient } from '../services/account'
  import { AccountRootClient, type RootJob } from '../services/account-root'
  import { CloudClient } from '../services/relay'
  import { b64, hex, id, unb64, unhex } from '../protocol/codec'
  import type { RecoveryPolicy } from '../canisters/generated/user'
  import Icon from './Icon.svelte'
  let client = $state.raw<AccountClient | null>(null)
  let accountState = $state.raw<Awaited<ReturnType<AccountClient['refresh']>> | null>(null)
  let recovery = $state.raw<Awaited<ReturnType<AccountClient['recoveryStatus']>> | null>(null)
  let account = $state(''),
    principal = $state(''),
    derivation = $state(config.derivationOrigins[0])
  let recoveryCode = $state(''),
    checkCode = $state(''),
    saved = $state(false)
  let policy = $state.raw<RecoveryPolicy | null>(null),
    job = $state<RootJob | null>(null)
  let password = $state(''),
    packet = $state(''),
    incoming = $state(''),
    reviewed = $state('')
  let pairingRole = $state<'Member' | 'Administrator'>('Member')
  const approved = $derived(!!accountState?.device && !accountState.device.revoked_at.length)
  const admin = $derived(
    approved &&
      !!accountState?.device &&
      'Administrator' in accountState.device.input.role &&
      accountState.device.input.capabilities.some((c) => 'RootManage' in c)
  )
  const labels = {
    reserve: '预留根代次',
    derive: '验证在线派生密钥',
    wrap: '生成并保存根封装',
    upload: '上传并回读验证',
    commit: '提交链上承诺',
    committed: '根承诺已核对'
  }
  function roots() {
    if (!client || !config.relayOrigin) throw new Error('请连接账户并配置密文服务。')
    return new AccountRootClient(
      client,
      new CloudClient({ origin: config.relayOrigin, environment: config.environment })
    )
  }
  async function refresh() {
    if (!client || !account) return
    accountState = await client.refresh(account)
    policy = accountState.info.recovery[0] ?? policy
    if (config.relayOrigin) job = await roots().job(account)
    recovery = await client.recoveryStatus(account)
  }
  async function connect() {
    await session.run(async () => {
      client = null
      accountState = null
      principal = ''
      reviewed = ''
      packet = ''
      const identity = await login(session.crypto, session.meta!.transportPublic, derivation),
        api = await services(identity)
      principal = identity.getPrincipal().toText()
      client = new AccountClient(
        api.user!,
        api.agent,
        identity.getPrincipal(),
        session.crypto,
        session.meta!,
        config.canisters.user
      )
      account = (await client.connectedAccount()) ?? ''
      if (account) await refresh()
    }, '认证已连接。设备权限以已验证的链上状态为准。')
  }
  async function generateRecovery() {
    await session.run(async () => {
      const value = await session.crypto.call('accountRecovery', {
        account,
        generation: 1,
        action: 'generate'
      })
      recoveryCode = value.code
      saved = false
      policy = {
        generation: 1n,
        signing_pub: unb64(value.signingPublic),
        hpke_pub: unb64(value.hpkePublic),
        delay_ms: 86400000n
      }
    })
  }
  async function enroll() {
    const code = checkCode
    checkCode = ''
    await session.run(async () => {
      if (!client || !policy) throw new Error('请先生成账户恢复材料。')
      accountState = await client.enrollRecovery(account, code, policy)
      recoveryCode = ''
      await refresh()
    }, '账户恢复公钥已登记并验证；恢复私钥未常驻保存。')
  }
  async function initializeRoot() {
    await session.run(async () => {
      job = await roots().run(account, (stage) => {
        session.progress = {
          stage: labels[stage],
          completed: Object.keys(labels).indexOf(stage),
          total: 5
        }
      })
      await refresh()
    }, '根密文已上传、回读校验，并与链上承诺匹配。')
  }
  async function enableCapability(capability: 'FormalApprove' | 'PaymentOffer') {
    await session.run(async () => {
      if (!client || !accountState?.device) throw new Error('请先核对账户与设备。')
      const capabilities = accountState.device.input.capabilities
      if (capabilities.some((c) => capability in c)) return
      accountState = await client.mutate(account, {
        SetDeviceCapabilities: {
          device_id: unhex(session.meta!.deviceId),
          capabilities: [
            ...capabilities,
            { [capability]: null } as { FormalApprove: null } | { PaymentOffer: null }
          ]
        }
      })
      await refresh()
    }, '能力已登记。每次敏感动作仍需明确批准；请完成账户要求的换根。')
  }
  async function activate() {
    const pass = password
    password = ''
    await session.run(async () => {
      await refresh()
      if (!job?.context || !job.bytes || !accountState?.info.current_root[0])
        throw new Error('根初始化尚未完成。')
      const digest = hex(Uint8Array.from(accountState.info.current_root[0].bundle_digest))
      const meta = await session.crypto.call('activateAccountRoot', {
        context: job.context,
        digest,
        uploadId: String(job.plan!.upload_id),
        homeUser: config.canisters.user,
        issuer: accountState.info.issuer,
        password: pass
      })
      session.activate(meta)
      await session.refresh()
      // The old client holds the old workspace metadata; reconnect before more control changes.
      client = null
      accountState = null
      job = null
    }, '正式工作区已启用，原本地副本已保留。请立即导出新的账户恢复包。')
  }
  function reviewPacket() {
    if (!client) return
    reviewed = ''
    void session.run(async () => {
      const parsed = JSON.parse(incoming)
      if (parsed.format === 'dmsg-auth-request/1') {
        const request = client!.inspectBinding(incoming)
        if (request.account !== account) throw new Error('请求不属于此账户。')
        reviewed = `绑定认证身份 ${request.principal}。该身份仍需设备批准才能执行账户操作。`
      } else {
        const request = client!.inspectPairing(incoming)
        if (!('AddDevice' in request.command)) throw new Error('无效设备请求。')
        const d = request.command.AddDevice.device
        reviewed = `设备 ${hex(Uint8Array.from(d.device_id))}\n签名公钥 ${b64(Uint8Array.from(d.signing_pub))}\n接收公钥 ${b64(Uint8Array.from(d.hpke_pub))}\n角色 ${Object.keys(d.role)[0]}；能力 ${d.capabilities.map((c) => Object.keys(c)[0]).join('、')}`
      }
    })
  }
</script>

<section class="settings-section">
  <div class="section-heading">
    <span class="item-icon"><Icon name="device" /></span>
    <div>
      <h2>账户与本机设备</h2>
      <p>认证登录、设备批准和内容解锁分别核对。</p>
    </div>
    <span class="pill">{session.meta?.account ? '正式账户工作区' : '本地工作区'}</span>
  </div>
  <dl class="evidence-list">
    <div>
      <dt>本机设备</dt>
      <dd><code>{shortId(session.meta!.deviceId)}</code></dd>
    </div>
    <div>
      <dt>内容所属</dt>
      <dd>{session.meta?.account?.id ?? 'R0 本地内容；尚未转换到链上账户'}</dd>
    </div>
    <div>
      <dt>设备授权</dt>
      <dd>{client ? (approved ? '已批准' : '未批准') : '连接后核对当前权限'}</dd>
    </div>
  </dl>
  <label
    >认证派生来源<select bind:value={derivation}
      >{#each config.derivationOrigins as origin}<option value={origin}>{origin}</option
        >{/each}</select
    ></label
  >
  <button class="secondary" disabled={session.busy || !config.canisters.user} onclick={connect}
    >连接 Internet Identity<Icon name="arrow-up-right" /></button
  >
  {#if !config.canisters.user}<p class="caption">
      先在构建配置中设置新版用户服务。正式扩展 origin 还需加入原站点的 II 白名单。
    </p>{/if}
  {#if principal}<p>认证 Principal：<code class="hash">{principal}</code></p>{/if}
  {#if client && !account}<button
      class="primary"
      disabled={session.busy}
      onclick={() =>
        session.run(async () => {
          account = await client!.create()
          await refresh()
        })}>创建新版账户</button
    >{/if}
  {#if client}
    <button
      class="secondary"
      disabled={session.busy}
      onclick={() =>
        session.run(async () => {
          const result = await client!.resume()
          if (result) account = result
          await refresh()
        })}>查询并继续未确认操作</button
    >
    <label>账户 Xid<input bind:value={account} autocomplete="off" spellcheck="false" /></label>
    <button
      class="text-button"
      disabled={session.busy || !account}
      onclick={() => session.run(refresh)}>刷新认证状态</button
    >
  {/if}
  {#if accountState}<dl class="evidence-list">
      <div>
        <dt>账户</dt>
        <dd><code>{account}</code></dd>
      </div>
      <div>
        <dt>安全版本</dt>
        <dd>
          {String(accountState.info.security_epoch)} / 账户版本 {String(
            accountState.info.account_version
          )}
        </dd>
      </div>
      <div>
        <dt>内容根</dt>
        <dd>
          {accountState.info.current_root.length
            ? `第 ${accountState.info.current_root[0]!.generation} 代`
            : '尚未提交'} · {Object.keys(accountState.info.vault_write_state)[0]}
        </dd>
      </div>
    </dl>{/if}
</section>
{#if client && account}
  {#if admin}
    <section class="settings-section">
      <h2>账户恢复与内容根</h2>
      <p>
        账户恢复码绑定这个 Xid，与原本地工作区恢复码分开。账户接管默认等待 24
        小时；持码者可立即解密已有恢复包。
      </p>
      {#if !accountState?.info.recovery_checked}
        {#if !accountState?.info.recovery.length}<button
            class="secondary"
            disabled={session.busy}
            onclick={generateRecovery}>生成或继续账户恢复设置</button
          >{/if}
        {#if recoveryCode}<div class="recovery-code"><code>{recoveryCode}</code></div>
          <label class="check-label"
            ><input type="checkbox" bind:checked={saved} />已将账户恢复码单独保存</label
          >{/if}
        {#if policy}<label
            >重新输入账户恢复码<textarea
              rows="3"
              bind:value={checkCode}
              autocomplete="off"
              spellcheck="false"></textarea></label
          ><button
            class="primary"
            disabled={session.busy || !checkCode || (!!recoveryCode && !saved)}
            onclick={enroll}>验证并登记恢复公钥</button
          >{/if}
      {:else}
        <p>恢复公钥已验证 · 第 {String(accountState.info.recovery[0]!.generation)} 代</p>
        <button
          class="primary"
          disabled={session.busy || !config.relayOrigin}
          onclick={initializeRoot}>初始化或继续内容根提交</button
        >
        {#if !config.relayOrigin}<p class="caption">需配置密文服务以保存、回读根封装。</p>{/if}
        {#if job}<p role="status">进度：{labels[job.stage]} · 操作 {shortId(job.opId)}</p>{/if}
        {#if job && job.stage !== 'committed'}<button
            class="text-button"
            disabled={session.busy}
            onclick={() =>
              session.run(async () => {
                job = await roots().restartExpired(account)
                await refresh()
              })}>核对过期状态并重新预留</button
          >{/if}
      {/if}
    </section>
  {/if}
  {#if approved}
    <section class="settings-section">
      <h2>本机内容根</h2>
      <button
        class="secondary"
        disabled={session.busy ||
          !config.relayOrigin ||
          !accountState?.info.current_root.length}
        onclick={() =>
          session.run(async () => {
            job = await roots().openCurrent(account)
            await refresh()
          }, '当前根和历史根封装已验证；启用后可使用本机内容。')}>读取并验证当前内容根</button
      >
      {#if admin && session.meta?.account}<button
          class="secondary"
          disabled={session.busy || !config.relayOrigin}
          onclick={() =>
            session.run(async () => {
              job = await roots().rotate(account)
              await refresh()
            }, '新根已提交，请启用新代并更新恢复包。')}>生成并提交新一代根</button
        >{/if}
      {#if accountState && job?.stage === 'committed' && session.meta?.account?.rootDigest !== (accountState.info.current_root[0] ? hex(Uint8Array.from(accountState.info.current_root[0].bundle_digest)) : '')}
        <p>
          把已验证的本地内容写入新副本后启用正式工作区。原加密数据库保留，普通内容的云端同步仍待
          A2。
        </p>
        <label
          >本机口令<input
            type="password"
            bind:value={password}
            autocomplete="current-password"
          /></label
        >
        <button class="primary" disabled={session.busy || !password} onclick={activate}
          >验证副本并启用正式工作区</button
        >
      {/if}
    </section>
  {/if}
  <section class="settings-section">
    <h2>设备与认证批准</h2>
    {#if admin}<div class="settings-row">
        <button
          class="secondary"
          disabled={session.busy ||
            accountState?.device?.input.capabilities.some((c) => 'FormalApprove' in c)}
          onclick={() => enableCapability('FormalApprove')}>启用此设备的正式签名批准</button
        ><button
          class="secondary"
          disabled={session.busy ||
            accountState?.device?.input.capabilities.some((c) => 'PaymentOffer' in c)}
          onclick={() => enableCapability('PaymentOffer')}>启用此设备的来信收款授权</button
        >
      </div>
      <p class="caption">
        能力变更会使旧账户证据失效，并要求换根后继续内容写入。不会自动签名或收款。
      </p>{/if}
    {#if accountState}
      {#each accountState.info.devices as [key, device]}<div class="settings-row">
          <div>
            <strong><code>{shortId(hex(Uint8Array.from(key)))}</code></strong>
            <p>
              {Object.keys(device.input.role)[0]} · {device.revoked_at.length
                ? '已撤销'
                : '有效'}
            </p>
          </div>
          {#if admin && !device.revoked_at.length}<button
              class="secondary"
              disabled={session.busy || hex(Uint8Array.from(key)) === session.meta?.deviceId}
              onclick={() =>
                session.run(async () => {
                  accountState = await client!.mutate(account, {
                    RevokeDevice: { device_id: key }
                  })
                  await refresh()
                }, '设备已撤销。新内容写入须等待根换代。')}>撤销设备</button
            >{/if}
        </div>{/each}
      <label
        >这台设备申请的角色<select bind:value={pairingRole}
          ><option value="Member">成员：内容签名与解锁</option><option value="Administrator"
            >管理员：另含账户根管理</option
          ></select
        ></label
      >
      <button
        class="secondary"
        disabled={session.busy || approved}
        onclick={() =>
          session.run(async () => {
            packet = await client!.pairing(account, pairingRole)
          })}>生成本机设备批准请求</button
      >
    {/if}
    <button
      class="secondary"
      disabled={session.busy}
      onclick={() =>
        session.run(async () => {
          packet = await client!.beginBinding(account)
        })}>生成本次认证绑定请求</button
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
    {#if admin}
      <label
        >粘贴另一设备的请求<textarea
          rows="5"
          bind:value={incoming}
          oninput={() => (reviewed = '')}></textarea></label
      >
      <button class="secondary" disabled={session.busy || !incoming} onclick={reviewPacket}
        >核对待批准内容</button
      >
      {#if reviewed}<pre class="hash">{reviewed}</pre>
        <p>请与另一设备当面核对公钥和角色；批准后需要换根。</p>
        <button
          class="primary"
          disabled={session.busy}
          onclick={() =>
            session.run(async () => {
              if (JSON.parse(incoming).format === 'dmsg-auth-request/1')
                await client!.approveBinding(account, incoming)
              else await client!.approvePairing(account, incoming)
              incoming = ''
              reviewed = ''
              await refresh()
            })}>批准以上具体请求</button
        >{/if}
    {/if}
  </section>
  <section class="settings-section">
    <h2>所有设备丢失后的账户恢复</h2>
    <p>
      只恢复离线内容不会批准本机设备。此流程会在等待期结束后撤销原设备和原认证绑定；随后必须换根。
    </p>
    <label
      >账户恢复码<textarea
        rows="3"
        bind:value={checkCode}
        autocomplete="off"
        spellcheck="false"></textarea></label
    >
    <button
      class="secondary"
      disabled={session.busy || !checkCode}
      onclick={() => {
        const code = checkCode
        checkCode = ''
        void session.run(async () => {
          recovery = await client!.requestRecovery(account, code)
        })
      }}>申请延迟恢复到本机</button
    >
    <button
      class="text-button"
      disabled={session.busy}
      onclick={() =>
        session.run(async () => {
          recovery = await client!.recoveryStatus(account)
        })}>查询恢复进度</button
    >
    {#if recovery?.pending}<p>
        最早完成时间：{dateLabel(Number(recovery.pending.execute_after))}；{recovery.pending
          .dispute.length
          ? recovery.pending.reconfirmed
            ? '争议已重新确认'
            : '存在争议，敏感操作冻结'
          : '尚无争议'}
      </p>
      {#if approved}<button
          class="secondary"
          disabled={session.busy}
          onclick={() =>
            session.run(async () => {
              await client!.mutate(account, {
                DisputeRecovery: {
                  op_id: recovery!.pending!.request.op_id,
                  dispute: unhex(id())
                }
              })
              recovery = await client!.recoveryStatus(account)
            })}>对此恢复提出争议</button
        >{/if}
      {#if recovery.pending.dispute.length && !recovery.pending.reconfirmed}<button
          class="secondary"
          disabled={session.busy || !checkCode}
          onclick={() => {
            const code = checkCode
            checkCode = ''
            void session.run(async () => {
              recovery = await client!.reconfirmRecovery(account, code)
            })
          }}>用恢复码确认争议并重启等待期</button
        >{/if}
      <button
        class="primary"
        disabled={session.busy}
        onclick={() =>
          session.run(async () => {
            accountState = await client!.completeRecovery(account)
            await refresh()
          })}>检查等待期并完成恢复</button
      >
    {/if}
  </section>
{/if}
