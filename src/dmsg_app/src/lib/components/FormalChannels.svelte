<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { session, dateLabel, downloadBlob } from '../session.svelte'
  import { config } from '../config'
  import { id } from '../protocol/codec'
  import { login, services } from '../services/ic'
  import { AccountClient } from '../services/account'
  import { CloudClient } from '../services/relay'
  import { ChannelClient, type ChannelInvitation } from '../services/channel'
  import type { ChannelLedger } from '../protocol/channel'
  import type { CryptoEngine } from '../crypto/engine'
  let client = $state.raw<ChannelClient | null>(null),
    socket: WebSocket | null = null
  let selectedFile = $state(''),
    fileSendId = $state(id())
  let channels = $state<Awaited<ReturnType<CryptoEngine['channelList']>>>([]),
    selected = $state('')
  let messages = $state<any[]>([]),
    pending = $state<Awaited<ReturnType<CryptoEngine['channelPending']>>>([])
  let origin = $state(config.derivationOrigins[0]),
    name = $state(''),
    kind = $state<ChannelLedger['type']>('collaboration'),
    createId = $state(id())
  let invitation = $state(''),
    inviteAccount = $state(''),
    inviteRole = $state<'member' | 'publisher'>('member'),
    invitationResult = $state('')
  let historyTarget = $state(''),
    historyFrom = $state(1),
    historyTo = $state(1),
    historyId = $state(id())
  let sponsor = $state(''),
    sponsorPacket = $state(''),
    transition = $state(id())
  let transferTarget = $state(''),
    transferPacket = $state('')
  let text = $state(''),
    sendId = $state(id()),
    status = $state(''),
    available = $state(false)
  const current = $derived(channels.find((c) => c.id === selected)),
    ledger = $derived(current?.ledger),
    me = $derived(ledger?.members[session.meta?.account?.id ?? ''])
  const labels = { active: '可发送', rotation_required: '等待换代', archived: '已归档' }
  async function local() {
    channels = await session.crypto.call('channelList')
    if (!selected && channels.length) selected = channels[0].id
    if (selected) {
      messages = await session.crypto.call('channelMessages', selected)
      pending = await session.crypto.call('channelPending', selected)
    }
  }
  onMount(() => {
    void session.run(local)
  })
  onDestroy(() => socket?.close())
  async function connect() {
    await session.run(async () => {
      if (!session.meta?.account || !config.relayOrigin)
        throw new Error('请先建立正式工作区并配置云端服务。')
      const identity = await login(session.crypto, session.meta.transportPublic, origin),
        api = await services(identity)
      const account = new AccountClient(
        api.user!,
        api.agent,
        identity.getPrincipal(),
        session.crypto,
        session.meta,
        config.canisters.user
      )
      if ((await account.connectedAccount()) !== session.meta.account.id)
        throw new Error('登录身份与当前工作区不一致。')
      client = new ChannelClient(
        account,
        new CloudClient({ origin: config.relayOrigin, environment: config.environment }),
        session.meta.account.id
      )
      await client.refresh()
      await local()
      status = '账户证据已核验。'
    })
  }
  async function sync() {
    await session.run(async () => {
      if (!client || !selected) throw new Error('请先连接并选择频道。')
      await client.sync(selected)
      await local()
      available = false
      status = '消息与控制记录已校验并保存。'
    })
  }
  async function rotate() {
    await session.run(async () => {
      if (!client) throw new Error('请先连接账户。')
      await client.rotate(selected)
      await local()
      status = '当前频道密钥已核验并保存。'
    })
  }
  async function create() {
    await session.run(async () => {
      if (!client) throw new Error('请先连接账户。')
      selected = await client.create(name, kind, createId)
      await client.rotate(selected)
      createId = id()
      name = ''
      await local()
      status = '频道已创建并完成第一代密钥分发。'
    })
  }
  async function join() {
    await session.run(async () => {
      if (!client) throw new Error('请先连接账户。')
      const packet = JSON.parse(invitation) as ChannelInvitation
      await client.join(packet)
      selected = packet.channel
      await local()
      status = '已接受邀请，等待新一代密钥；加入前的历史需要单独授权。'
    })
  }
  async function invite() {
    await session.run(async () => {
      if (!client) throw new Error('请先连接账户。')
      invitationResult = JSON.stringify(
        await client.invite(selected, inviteAccount, inviteRole),
        null,
        2
      )
      await local()
    })
  }
  async function sendFile() {
    await session.run(async () => {
      if (!client) throw new Error('请先连接账户。')
      await client.sendFile(selected, selectedFile, fileSendId)
      fileSendId = id()
      selectedFile = ''
      await local()
      status = '文件版本已加密投递。'
    })
  }
  async function send() {
    await session.run(async () => {
      if (!client) throw new Error('请先连接账户。')
      await client.send(selected, text, sendId)
      sendId = id()
      text = ''
      await local()
      status = '中继已持久保存，消息已核验。'
    })
  }
  async function transfer(action: 'prepare' | 'accept' | 'commit') {
    await session.run(async () => {
      if (!client) throw new Error('请先连接账户。')
      const result =
        action === 'prepare'
          ? await client.prepareOwnerTransfer(selected, transferTarget)
          : action === 'accept'
            ? await client.acceptOwnerTransfer(JSON.parse(transferPacket))
            : await client.commitOwnerTransfer(JSON.parse(transferPacket))
      if (action !== 'commit') transferPacket = JSON.stringify(result, null, 2)
      else {
        transferPacket = ''
        await local()
        status = '双方批准的 owner 转移已核验。'
      }
    })
  }
  async function change(action: Parameters<ChannelClient['control']>[1]) {
    await session.run(async () => {
      if (!client) throw new Error('请先连接账户。')
      await client.control(selected, action)
      await local()
      status = '控制变更已核验；涉及成员变化时需完成换代。'
    })
  }
</script>

<div class="page-heading">
  <div>
    <span class="eyebrow">VERIFIED CHANNELS</span>
    <h1>正式频道</h1>
    <p>邀请、成员变更、历史授权与密钥换代分别核对。</p>
  </div>
</div>
<section class="settings-section">
  <label
    >账户登录来源<select bind:value={origin}
      >{#each config.derivationOrigins as value}<option>{value}</option>{/each}</select
    ></label
  >
  <button class="secondary" disabled={session.busy || !session.meta?.account} onclick={connect}
    >连接当前账户</button
  >
  <p class="caption">离线可阅读本机已保存历史。锁定后停止新发送，未知结果从原任务继续。</p>
  <details>
    <summary>创建频道</summary><label
      >本机频道名称<input bind:value={name} maxlength="100" /></label
    ><label
      >频道类型<select bind:value={kind}
        ><option value="collaboration">协作频道</option><option value="direct">双人会话</option
        ><option value="distribution">发布频道</option></select
      ></label
    ><button
      class="primary"
      disabled={!client || session.busy || !name.trim()}
      onclick={create}>创建并分发第一代密钥</button
    >
  </details>
  <details>
    <summary>接受邀请</summary>
    <p>请通过可信渠道核对邀请中的频道 ID、目标账户和控制头。</p>
    <textarea bind:value={invitation} rows="4" aria-label="频道邀请"></textarea><button
      class="primary"
      disabled={!client || session.busy || !invitation}
      onclick={join}>核对并接受邀请</button
    >
  </details>
</section>
{#if channels.length}<section class="settings-section">
    <label
      >已保存频道<select
        bind:value={selected}
        onchange={() => {
          socket?.close()
          void session.run(local)
        }}
        >{#each channels as channel}<option value={channel.id}
            >{channel.name} · {channel.ledger
              ? labels[channel.ledger.status]
              : '待完成创建'}</option
          >{/each}</select
      ></label
    >
    <code class="hash">{selected}</code>
    <p>{ledger ? `第 ${ledger.epoch} 代 · ${labels[ledger.status]}` : '创建任务尚未闭合'}</p>
    <div class="settings-row">
      <button class="secondary" disabled={!client || session.busy} onclick={sync}
        >补拉与核验</button
      ><button
        class="secondary"
        disabled={!client ||
          session.busy ||
          !me ||
          (ledger?.status === 'rotation_required' && !me.can_rotate)}
        onclick={rotate}
        >{ledger?.status === 'rotation_required' ? '继续密钥换代' : '核验当前密钥'}</button
      ><button
        class="secondary"
        disabled={!client || session.busy}
        onclick={() =>
          session.run(async () => {
            socket?.close()
            socket = await client!.hints(selected, () => (available = true))
            status = '已连接临时消息提示；提示不会自动授权内容。'
          })}>接收新消息提示</button
      >
    </div>
    {#if available}<p role="status">有新的频道活动，请补拉核验。</p>{/if}
    {#if ledger?.status === 'archived' && me?.role === 'owner'}<button
        class="secondary"
        disabled={!client || session.busy}
        onclick={() =>
          session.run(async () => {
            await client!.reactivate(selected)
            await local()
          })}>重新预留名额并恢复频道</button
      >{/if}
    {#if ledger}<details>
        <summary>成员与邀请</summary>
        {#each Object.values(ledger.members) as member}<div class="settings-row">
            <span
              ><code>{member.account}</code> · {member.role} · 从第 {member.joined_epoch} 代加入</span
            >{#if me?.role === 'owner' && member.role !== 'owner'}<button
                class="secondary"
                disabled={session.busy || !client}
                onclick={() =>
                  change({
                    type: 'role',
                    account: member.account,
                    role: member.role === 'admin' ? 'member' : 'admin',
                    can_rotate: member.role !== 'admin'
                  })}>{member.role === 'admin' ? '设为成员' : '设为管理员'}</button
              >{/if}{#if (me?.role === 'owner' || (me?.role === 'admin' && member.role !== 'admin')) && member.account !== session.meta?.account?.id && member.role !== 'owner'}<button
                class="secondary"
                disabled={session.busy || !client}
                onclick={() => change({ type: 'remove', account: member.account })}
                >移除成员</button
              >{/if}
          </div>{/each}
        {#if me?.role === 'owner' || me?.role === 'admin'}<label
            >邀请目标账户 Xid<input bind:value={inviteAccount} maxlength="20" /></label
          ><label
            >初始角色<select bind:value={inviteRole}
              ><option value="member">成员</option><option value="publisher">发布者</option
              ></select
            ></label
          ><button
            class="primary"
            disabled={!client || session.busy || !inviteAccount}
            onclick={invite}>创建精确邀请</button
          >{#if invitationResult}<textarea
              readonly
              value={invitationResult}
              rows="5"
              aria-label="发给目标成员的邀请"></textarea>{/if}{/if}
        {#if me?.role === 'owner'}<button
            class="secondary"
            disabled={!client || session.busy}
            onclick={() => change({ type: 'archive' })}>归档频道</button
          >{:else if me}<button
            class="secondary"
            disabled={!client || session.busy}
            onclick={() => change({ type: 'leave' })}>退出频道</button
          >{/if}
      </details>{/if}
    <details>
      <summary>独立历史授权</summary>
      <p>
        仅将选择的历史代封装给目标账户当前有效设备及恢复公钥。新成员不会自动获得加入前的历史。
      </p>
      {#if me?.role === 'owner' || me?.role === 'admin'}<label
          >目标成员账户<input bind:value={historyTarget} maxlength="20" /></label
        ><label>开始代<input type="number" min="1" bind:value={historyFrom} /></label><label
          >结束代<input type="number" min="1" bind:value={historyTo} /></label
        ><button
          class="primary"
          disabled={!client || session.busy || !historyTarget}
          onclick={() =>
            session.run(async () => {
              await client!.grantHistory(
                selected,
                historyTarget,
                historyFrom,
                historyTo,
                historyId
              )
              historyId = id()
              await local()
              status = '所选历史范围已单独授权。'
            })}>批准所选历史范围</button
        >{/if}<button
        class="secondary"
        disabled={!client || session.busy}
        onclick={() =>
          session.run(async () => {
            await client!.sync(selected, true)
            await local()
          })}>核验并补拉已授权历史</button
      >
    </details>
    <details>
      <summary>频道费用承担方</summary>
      <p>
        承接只变更容量和名额的付款方，内容权限仍由成员和历史授权决定。准备期间普通写入暂停。
      </p>
      <label>新 sponsor 账户<input bind:value={sponsor} maxlength="20" /></label><button
        class="secondary"
        disabled={!client || session.busy || me?.role !== 'owner' || !sponsor}
        onclick={() =>
          session.run(async () => {
            sponsorPacket = JSON.stringify(
              await client!.prepareSponsor(selected, sponsor, transition),
              null,
              2
            )
          })}>冻结并核对承接清单</button
      ><textarea bind:value={sponsorPacket} rows="5" aria-label="费用承接的精确双边提案"
      ></textarea><button
        class="secondary"
        disabled={!client || session.busy || !sponsorPacket}
        onclick={() =>
          session.run(async () => {
            sponsorPacket = JSON.stringify(
              await client!.acceptSponsor(JSON.parse(sponsorPacket)),
              null,
              2
            )
          })}>作为新 sponsor 明确接受清单</button
      ><button
        class="primary"
        disabled={!client || session.busy || !sponsorPacket || me?.role !== 'owner'}
        onclick={() =>
          session.run(async () => {
            await client!.commitSponsor(JSON.parse(sponsorPacket))
            transition = id()
            sponsorPacket = ''
            status = '费用承接已提交并对账。'
          })}>提交双方精确批准</button
      ><button
        class="secondary"
        disabled={!client || session.busy || me?.role !== 'owner'}
        onclick={() =>
          session.run(async () => {
            await client!.cancelSponsor(selected, transition)
            transition = id()
            sponsorPacket = ''
            status = '未提交的承接已取消。'
          })}>取消此未提交承接</button
      >
    </details>
    <details>
      <summary>双方批准的 owner 转移</summary>
      <p>
        原 owner 发起精确提案，目标账户在有效期内接受，再由原 owner
        提交。中途控制头或身份变化会使提案失效。
      </p>
      {#if me?.role === 'owner'}<label
          >新 owner 账户<input bind:value={transferTarget} maxlength="20" /></label
        ><button
          class="secondary"
          disabled={!client || session.busy || !transferTarget}
          onclick={() => transfer('prepare')}>生成转移提案</button
        >{/if}<textarea
        bind:value={transferPacket}
        rows="5"
        aria-label="owner 转移提案或接受结果"></textarea><button
        class="secondary"
        disabled={!client || session.busy || !transferPacket}
        onclick={() => transfer('accept')}>作为目标核对并接受</button
      >{#if me?.role === 'owner'}<button
          class="primary"
          disabled={!client || session.busy || !transferPacket}
          onclick={() => transfer('commit')}>提交双方批准的转移</button
        >{/if}
    </details>
    {#if pending.length}<details open>
        <summary>待对账的原任务</summary>{#each pending as job}<div class="settings-row">
            <span>{job.action}<br /><code>{job.requestId}</code></span><button
              class="secondary"
              disabled={!client || session.busy}
              onclick={() =>
                session.run(async () => {
                  await client!.resume(selected, job.key)
                  await local()
                })}>核对并继续原任务</button
            >
          </div>{/each}
      </details>{/if}
    <div class="message-history">
      {#each messages as message}<article class="history-message">
          <p class="caption">
            {message.account} · 第 {message.value.epoch} 代 · {dateLabel(
              message.value.client_created_at
            )} · 来源声明时间
          </p>
          <p>{message.text}</p>
          {#if message.file}<p>{message.file.name} · {message.file.size} 字节</p>
            <button
              class="secondary"
              disabled={!client || session.busy}
              onclick={() =>
                session.run(async () => {
                  const file = await client!.downloadFile(selected, message.seq)
                  downloadBlob(file.blob, file.name)
                  await session.refresh()
                })}>核验、保存并下载附件</button
            >{/if}
          <span class="caption">已持久保存 · 设备签名与内容校验通过</span>
        </article>{/each}
    </div>
    <details>
      <summary>发送已保存的文件版本</summary><label
        >文件<select bind:value={selectedFile}
          ><option value="">先在保险库保存文件</option
          >{#each session.data.entries.filter((e) => e.item.file) as entry}<option
              value={entry.record.key}>{entry.item.title}</option
            >{/each}</select
        ></label
      ><button
        class="secondary"
        disabled={!client || session.busy || !selectedFile || ledger?.status !== 'active'}
        onclick={sendFile}>发送所选文件版本</button
      >
    </details>
    <form
      onsubmit={(event) => {
        event.preventDefault()
        void send()
      }}
    >
      <label>消息<textarea bind:value={text} maxlength="16000" rows="3"></textarea></label
      ><button
        class="primary"
        disabled={!client || session.busy || !text || ledger?.status !== 'active' || !me}
        >发送加密消息</button
      >
    </form>
  </section>{:else}<p class="empty-state">
    尚未保存正式频道。可以创建频道或导入发给此账户的邀请。
  </p>{/if}
{#if status}<p role="status">{status}</p>{/if}
