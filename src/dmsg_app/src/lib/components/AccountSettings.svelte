<script lang="ts">
  import { session, shortId, dateLabel, downloadBlob } from '../session.svelte'
  import { config } from '../config'
  import { connectAccount, loginOrigin } from '../connection'
  import type { AccountClient } from '../services/account'
  import { AccountRootClient, type RootJob } from '../services/account-root'
  import { CloudClient } from '../services/relay'
  import { createPrfCredential, prfSupported } from '../services/prf'
  import { b64, hex, unhex } from '../protocol/codec'
  import { Principal } from '@icp-sdk/core/principal'
  import type { _SERVICE as CoseService } from '../canisters/generated/cose'
  import Icon from './Icon.svelte'
  let client = $state.raw<AccountClient | null>(null),
    cose = $state.raw<CoseService | null>(null)
  let accountState = $state.raw<Awaited<ReturnType<AccountClient['refresh']>> | null>(null)
  let recovery = $state.raw<Awaited<ReturnType<AccountClient['recoveryStatus']>> | null>(null)
  let principal = $state(''),
    derivation = $state(loginOrigin())
  let job = $state<RootJob | null>(null)
  let packet = $state(''),
    incoming = $state(''),
    reviewed = $state('')
  let policyExecutions = $state(''),
    recoveryDays = $state('3')
  let purposes = $state<string[]>([])
  const account = $derived(session.meta?.account?.id ?? '')
  const approved = $derived(!!accountState?.device && !accountState.device.revoked_at.length)
  const admin = $derived(
    approved &&
      !!accountState?.device &&
      'Administrator' in accountState.device.input.role &&
      accountState.device.input.capabilities.some((c) => 'RootManage' in c)
  )
  const currentDigest = $derived(
    accountState?.info.current_root[0]
      ? hex(Uint8Array.from(accountState.info.current_root[0].bundle_digest))
      : ''
  )
  const labels = {
    reserve: '预留根代次',
    wrap: '封装新根',
    upload: '上传并回读验证',
    commit: '提交链上承诺',
    committed: '根承诺已核对'
  }
  function roots() {
    if (!client || !cose || !config.relayOrigin) throw new Error('请连接账户并配置密文服务。')
    return new AccountRootClient(
      client,
      new CloudClient({ origin: config.relayOrigin, environment: config.environment }),
      cose
    )
  }
  const progress = (stage: RootJob['stage']) => {
    session.progress = {
      stage: labels[stage],
      completed: Object.keys(labels).indexOf(stage),
      total: 4
    }
  }
  async function refresh() {
    if (!client || !account) return
    accountState = await client.refresh(account)
    purposes = accountState.info.sensitive_policy.allowed_purposes.map(
      (p) => Object.keys(p)[0]
    )
    recoveryDays = String(Number(accountState.info.recovery_delay_ms) / 86400000)
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
      const connection = await connectAccount(derivation, { fresh: true })
      principal = connection.identity.getPrincipal().toText()
      client = connection.account
      cose = connection.api.cose
      await refresh()
    }, '认证已连接。设备权限以已验证的链上状态为准。')
  }
  async function activate(next: RootJob) {
    session.activate(await roots().activate(account, next))
    await session.refresh()
    // The old client holds the old workspace metadata; reconnect before more control changes.
    client = null
    accountState = null
    job = null
  }
  async function rotate() {
    await session.run(async () => {
      await activate(await roots().rotate(account, progress))
    }, '新根已提交并启用。')
  }
  /** A change of the root's recipients pauses vault writes until a new root is committed. */
  async function rotateIfRequired() {
    if (!accountState || !('RekeyRequired' in accountState.info.vault_write_state)) return
    if (!config.relayOrigin) throw new Error('需要配置密文服务才能换根；换根前内容写入暂停。')
    await activate(await roots().rotate(account, progress))
  }
  async function openCurrent() {
    await session.run(async () => {
      await activate(await roots().openCurrent(account))
    }, '当前根和历史根封装已验证并启用。')
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
    }, '能力已登记。每次敏感动作仍需明确批准。')
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
  async function enablePrf() {
    await session.run(async () => {
      const { credentialId, output } = await createPrfCredential(session.meta!)
      await session.crypto.call('enablePrf', { credentialId, output })
      await session.refresh()
    }, '生物识别解锁已启用；每 7 天仍需登录解锁一次。')
  }
  async function disablePrf() {
    await session.run(async () => {
      await session.crypto.call('disablePrf')
      await session.refresh()
    }, '生物识别解锁已关闭。')
  }
</script>

<section class="settings-section">
  <div class="section-heading">
    <span class="item-icon"><Icon name="device" /></span>
    <div>
      <h2>账户与本机设备</h2>
      <p>认证登录、设备批准和内容解锁分别核对。</p>
    </div>
    <span class="pill">第 {session.meta?.rootGeneration} 代内容根</span>
  </div>
  <dl class="evidence-list">
    <div>
      <dt>本机设备</dt>
      <dd><code>{shortId(session.meta!.deviceId)}</code></dd>
    </div>
    <div>
      <dt>账户</dt>
      <dd><code>{account}</code> · {session.meta?.account?.homeUser}</dd>
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
  <button class="secondary" disabled={session.busy} onclick={connect}
    >连接 Internet Identity<Icon name="arrow-up-right" /></button
  >
  {#if principal}<p>认证 Principal：<code class="hash">{principal}</code></p>{/if}
  {#if client}
    <button
      class="secondary"
      disabled={session.busy}
      onclick={() =>
        session.run(async () => {
          await client!.resume()
          await refresh()
        })}>查询并继续未确认操作</button
    >
    <button class="text-button" disabled={session.busy} onclick={() => session.run(refresh)}
      >刷新认证状态</button
    >
  {/if}
  {#if accountState}<dl class="evidence-list">
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
<section class="settings-section">
  <h2>本机解锁</h2>
  <p>
    日常解锁由账户服务按登录身份发放解锁材料；启用生物识别后可离线快速解锁，但每 7 天仍要登录一次，设备撤销在下次联网时生效。
  </p>
  {#if session.meta?.prf}
    <p>已启用 · {dateLabel(session.meta.prf.enabledAt)}</p>
    <button class="secondary" disabled={session.busy} onclick={disablePrf}>关闭生物识别解锁</button>
  {:else}
    <button class="secondary" disabled={session.busy || !prfSupported()} onclick={enablePrf}
      >启用生物识别解锁</button
    >
    {#if !prfSupported()}<p class="caption">此浏览器没有可用的平台认证器。</p>{/if}
  {/if}
</section>
{#if client && account}
  {#if approved}
    <section class="settings-section">
      <h2>内容根</h2>
      <p>
        换根把新根封装给持有 VaultUnlock 的有效设备；这些设备增减后必须换根，内容写入才会恢复。
      </p>
      {#if currentDigest && session.meta?.account?.rootDigest !== currentDigest}
        <p role="alert">链上当前根比本机新。请读取当前根后再继续。</p>
        <button class="primary" disabled={session.busy || !config.relayOrigin} onclick={openCurrent}
          >读取并验证当前内容根</button
        >
      {/if}
      {#if admin}<button
          class="secondary"
          disabled={session.busy || !config.relayOrigin}
          onclick={rotate}>生成并提交新一代根</button
        >{/if}
      {#if !config.relayOrigin}<p class="caption">需配置密文服务以保存、回读根封装。</p>{/if}
      {#if job}<p role="status">进度：{labels[job.stage]} · 操作 {shortId(job.opId)}</p>{/if}
      {#if job && job.stage !== 'committed'}<button
          class="text-button"
          disabled={session.busy}
          onclick={() =>
            session.run(async () => {
              await activate(await roots().restartExpired(account))
            })}>核对过期状态并重新预留</button
        >{/if}
    </section>
    {#if admin && accountState}<section class="settings-section">
        <h2>认证绑定与敏感执行政策</h2>
        {#each accountState.info.auth_bindings as identity}<div class="settings-row">
            <code>{identity.toText()}</code><button
              class="secondary"
              disabled={session.busy || identity.toText() === principal}
              onclick={() =>
                session.run(async () => {
                  await client!.mutate(account, {
                    RemoveAuth: { principal: Principal.fromText(identity.toText()) }
                  })
                  await refresh()
                })}>移除此认证绑定</button
            >
          </div>{/each}
        <p>
          敏感执行：{accountState.info.sensitive_policy.frozen ? '已冻结' : '可批准'}；每日 {accountState
            .info.sensitive_policy.daily_executions} 次。
        </p>
        <button
          class="secondary"
          disabled={session.busy}
          onclick={() =>
            session.run(async () => {
              await client!.mutate(account, {
                SetPolicy: {
                  policy: {
                    ...accountState!.info.sensitive_policy,
                    frozen: !accountState!.info.sensitive_policy.frozen
                  }
                }
              })
              await refresh()
            })}
          >{accountState.info.sensitive_policy.frozen
            ? '明确解除执行冻结'
            : '冻结新的敏感执行'}</button
        >
        <label>每日执行次数<input bind:value={policyExecutions} inputmode="numeric" /></label>
        <button
          class="secondary"
          disabled={session.busy || !/^[0-9]+$/.test(policyExecutions)}
          onclick={() =>
            session.run(async () => {
              await client!.mutate(account, {
                SetPolicy: {
                  policy: {
                    ...accountState!.info.sensitive_policy,
                    daily_executions: Number(policyExecutions)
                  }
                }
              })
              await refresh()
            })}>批准上述执行预算</button
        >
        {#each [['FileAttestation', '文件摘要'], ['Statement', '文本与文件声明'], ['AppAction', '应用动作']] as [value, label]}<label
            ><input type="checkbox" {value} bind:group={purposes} />{label}</label
          >{/each}
        <button
          class="secondary"
          disabled={session.busy}
          onclick={() =>
            session.run(async () => {
              await client!.mutate(account, {
                SetPolicy: {
                  policy: {
                    ...accountState!.info.sensitive_policy,
                    allowed_purposes: purposes.map((name) => ({
                      [name]: null
                    })) as NonNullable<
                      typeof accountState
                    >['info']['sensitive_policy']['allowed_purposes']
                  }
                }
              })
              await refresh()
            })}>批准上述签名用途</button
        >
        <label>账户恢复等待天数（1–7）<input bind:value={recoveryDays} inputmode="numeric" /></label>
        <button
          class="secondary"
          disabled={session.busy || !/^[1-7]$/.test(recoveryDays)}
          onclick={() =>
            session.run(async () => {
              await client!.setRecoveryDelay(account, Number(recoveryDays))
              await refresh()
            })}>批准恢复等待期</button
        >
      </section>{/if}
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
        能力变更会使旧账户证据失效，但不需要换根。不会自动签名或收款。
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
                  await rotateIfRequired()
                }, '设备已撤销并已换根：它不再取得解锁材料，也读不到此后的新内容。')}
              >撤销设备</button
            >{/if}
        </div>{/each}
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
        <p>
          请与另一设备当面核对公钥、角色和能力。批准持有 VaultUnlock
          的设备后会立即换根，新设备随后即可读取当前内容根。
        </p>
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
              await rotateIfRequired()
            }, '已批准。新增设备时已同时换根，新设备现在可以读取当前内容根。')}
          >批准以上具体请求</button
        >{/if}
    {/if}
  </section>
  <section class="settings-section">
    <h2>账户恢复</h2>
    <p>
      所有设备丢失后，绑定过的登录身份可以在新设备申请恢复；等待期内任何有效设备都可以取消。
    </p>
    <button
      class="text-button"
      disabled={session.busy}
      onclick={() =>
        session.run(async () => {
          recovery = await client!.recoveryStatus(account)
        })}>查询恢复申请</button
    >
    {#if recovery?.pending}<p role="alert">
        有待处理的恢复申请：登录身份 {recovery.pending.request.new_auth.toText()}，最早完成时间 {dateLabel(
          Number(recovery.pending.execute_after)
        )}。
      </p>
      {#if approved}<button
          class="primary"
          disabled={session.busy}
          onclick={() =>
            session.run(async () => {
              recovery = await client!.disputeRecovery(account)
            }, '恢复申请已取消。')}>这不是我，取消此恢复</button
        >{/if}
    {:else if recovery}<p>当前没有待处理的恢复申请。</p>{/if}
  </section>
{/if}
