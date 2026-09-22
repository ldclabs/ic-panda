<script lang="ts">
  import {
    collectFolderProof,
    pack,
    collectSnapshot,
    managerConsentDigest,
    memberClaimDigest,
    sharedProposalSchema,
    memberClaimSchema,
    type SnapshotProof
  } from '@dmsg/legacy'
  import { createLegacyReader } from './reader'
  import { dynAgent } from '$lib/utils/auth'
  import { authStore } from '$lib/stores/auth'
  const channels = [
    'nvdn4-5qaaa-aaaaj-qa4pq-cai',
    'zof5a-5yaaa-aaaai-acr2q-cai',
    '4jxyd-pqaaa-aaaah-qdqtq-cai'
  ]
  const identity = '2rgax-kyaaa-aaaap-anvba-cai'
  let source = $state(channels[0]!),
    channel = $state(1),
    requestText = $state(''),
    responseText = $state(''),
    status = $state(''),
    busy = $state(false),
    error = $state('')
  const toHex = (bytes: Uint8Array) =>
    Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('')
  const base64 = (bytes: Uint8Array) => {
    let text = ''
    for (let i = 0; i < bytes.length; i += 8192)
      text += String.fromCharCode(...bytes.subarray(i, i + 8192))
    return btoa(text).replaceAll('+', '-').replaceAll('/', '_').replace(/=+$/, '')
  }
  const wire = (p: SnapshotProof) => ({
    ...p,
    args: base64(p.args),
    nonce: base64(p.nonce),
    requestId: base64(p.requestId),
    certificate: base64(p.certificate),
    reply: base64(p.reply)
  })
  const request = $derived.by(() => {
    try {
      if (requestText.length > 20000) return null
      const value = JSON.parse(requestText),
        proposal = sharedProposalSchema.parse(value.proposal)
      if (
        value.format !== 'dmsg-legacy-approval-request/1' ||
        !channels.includes(proposal.source) ||
        !Number.isSafeInteger(value.expires_at) ||
        value.expires_at <= Date.now() ||
        value.expires_at > Date.now() + 7 * 86400000
      )
        return null
      const digest =
        value.kind === 'manager'
          ? managerConsentDigest(proposal, value.identity)
          : value.kind === 'member'
            ? memberClaimDigest(memberClaimSchema.parse(value.claim))
            : null
      if (
        !digest ||
        toHex(digest) !== value.digest ||
        (value.kind === 'member' && value.claim.member !== value.identity)
      )
        return null
      return { ...value, proposal, bytes: digest }
    } catch {
      return null
    }
  })
  async function run(task: () => Promise<void>) {
    if (busy) return
    busy = true
    error = ''
    try {
      await task()
    } catch (cause) {
      error = cause instanceof Error ? cause.message : '操作未完成'
    } finally {
      busy = false
    }
  }
  function download(value: unknown, name: string) {
    const url = URL.createObjectURL(
      new Blob([JSON.stringify(value, null, 2)], { type: 'application/json' })
    )
    const a = document.createElement('a')
    a.href = url
    a.download = name
    a.click()
    setTimeout(() => URL.revokeObjectURL(url), 30000)
  }
  async function inventory() {
    await run(async () => {
      const control = await collectSnapshot(
        dynAgent,
        source,
        { ChannelAuthority: channel },
        channels
      )
      const names = await collectSnapshot(dynAgent, identity, { Authorities: null }, [
        identity
      ])
      // The extension verifies the cutover against its reviewed configuration.
      const { verifySnapshotProof } = await import('@dmsg/legacy')
      const checked = await verifySnapshotProof(control.proofs[0]!, {
        rootKey: dynAgent.rootKey!,
        canister: source,
        caller: dynAgent.id.getPrincipal().toText(),
        scope: { ChannelAuthority: channel },
        after: null
      })
      download(
        {
          format: 'dmsg-legacy-shared-source/1',
          source,
          identity,
          cutover: toHex(checked.page.freeze.cutover_id[0]!),
          input: {
            channel,
            authority: wire(control.proofs[0]!),
            authorities: names.proofs.map(wire)
          }
        },
        'legacy-shared-source.json'
      )
      status = '冻结权限证明已导出。它包含成员关系，仅交给参与该频道迁移的人。'
    })
  }
  async function frozenHistory() {
    await run(async () => {
      const authority = await collectSnapshot(
        dynAgent,
        source,
        { ChannelAuthority: channel },
        channels
      )
      const messages = await collectSnapshot(dynAgent, source, { Messages: channel }, channels)
      const { reader, checkpoint } = await createLegacyReader('Local')
      let folder: Awaited<ReturnType<typeof collectFolderProof>> = []
      try {
        const info = await reader.call('channel', source, 'get_channel_if_update', [
          channel,
          0n
        ])
        if (info[0]?.files_state?.length) {
          const token = await reader.call('channel', source, 'download_files_token', [channel])
          folder = await collectFolderProof(
            dynAgent,
            token.storage[0].toText(),
            token.storage[1],
            new Uint8Array(token.access_token),
            ['532er-faaaa-aaaaj-qncpa-cai', 'sb6zj-3aaaa-aaaaj-qndla-cai']
          )
        }
      } finally {
        checkpoint.close()
      }
      const bytes = pack([
        {
          channel: `${source}/channel/${channel}`,
          authority: authority.proofs[0]!,
          messages: messages.proofs,
          folder
        }
      ])
      const url = URL.createObjectURL(
          new Blob([new Uint8Array(bytes)], { type: 'application/cbor' })
        ),
        a = document.createElement('a')
      a.href = url
      a.download = 'legacy-frozen-history.cbor'
      a.click()
      setTimeout(() => URL.revokeObjectURL(url), 30000)
      status = '已导出消息与附件的冻结对照证明。文件不含下载 token；请仅导入本人扩展。'
    })
  }
  async function approve() {
    await run(async () => {
      if (!request) throw new Error('请先核对完整且未过期的方案。')
      const expectedCaller = dynAgent.id.getPrincipal().toText()
      const result = await collectSnapshot(
        dynAgent,
        request.proposal.source,
        { Attestation: { digest: request.bytes, expires_at: BigInt(request.expires_at) } },
        channels
      )
      if (dynAgent.id.getPrincipal().toText() !== expectedCaller)
        throw new Error('旧身份已变化，请重新核对。')
      responseText = JSON.stringify(
        { proof: wire(result.proofs[0]!), expires_at: request.expires_at },
        null,
        2
      )
      status = '本版本的明确批准已生成。请返回新版扩展完成同一方案。'
    })
  }
</script>

<section class="migration">
  <h2>共享频道继承与成员认领</h2>
  <p>
    需要旧服务已完成冻结。全部冻结管理员对同一新负责人和初始频道记录的同意齐全后，才可建立唯一官方继承。
  </p>
  <label
    >来源频道服务<select bind:value={source}
      >{#each channels as value}<option>{value}</option>{/each}</select
    ></label
  >
  <label>旧频道编号<input type="number" min="0" max="4294967295" bind:value={channel} /></label>
  <button
    class="button"
    disabled={busy || $authStore.identity.getPrincipal().isAnonymous()}
    onclick={inventory}>导出冻结权限证明</button
  ><button
    class="button"
    disabled={busy || $authStore.identity.getPrincipal().isAnonymous()}
    onclick={frozenHistory}>导出此频道的最终历史对照证明</button
  >
  <label>新版扩展生成的批准请求<textarea bind:value={requestText} rows="5"></textarea></label>
  {#if request}<dl>
      <dt>批准类型</dt>
      <dd>{request.kind === 'manager' ? '管理员同意继承方案' : '旧成员认领'}</dd>
      <dt>旧频道</dt>
      <dd>{request.proposal.source} / {request.proposal.channel}</dd>
      <dt>新负责人</dt>
      <dd>{request.proposal.owner}</dd>
      <dt>新频道</dt>
      <dd>{request.proposal.channel_id}</dd>
      <dt>版本</dt>
      <dd>{request.proposal.version}</dd>
      <dt>拟代表的旧身份</dt>
      <dd>{request.identity}</dd>
      {#if request.kind === 'member'}<dt>目标账户 / 设备</dt>
        <dd>{request.claim.account} / {request.claim.device}</dd>{/if}
      <dt>到期时间</dt>
      <dd>{new Date(request.expires_at).toLocaleString()}</dd>
      <dt>意图摘要</dt>
      <dd>{request.digest}</dd>
    </dl>{/if}
  <p>
    当前签署身份：{$authStore.identity
      .getPrincipal()
      .toText()}。共享名称必须用冻结管理员的原始登录身份批准。
  </p>
  <button
    class="button primary"
    disabled={!request || busy || $authStore.identity.getPrincipal().isAnonymous()}
    onclick={approve}>我已核对，批准此版本的明确意图</button
  >
  {#if responseText}<label
      >返回新版扩展的批准结果<textarea readonly value={responseText} rows="5"
      ></textarea></label
    >{/if}
  {#if status}<p role="status">{status}</p>{/if}{#if error}<p role="alert">{error}</p>{/if}
</section>

<style>
  .migration {
    padding: 32px 0;
    border-top: 1px solid #cdd8cf;
    max-width: 780px;
  }
  label {
    display: block;
    margin: 16px 0;
  }
  textarea {
    display: block;
    width: 100%;
  }
  dd {
    overflow-wrap: anywhere;
    margin-bottom: 12px;
  }
  button {
    margin: 8px 12px 8px 0;
  }
</style>
