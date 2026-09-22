<script lang="ts">
  import { onMount } from 'svelte'
  import { session, downloadBlob } from '../session.svelte'
  import type { CryptoEngine } from '../crypto/engine'
  let origin = $state<'https://dmsg.net' | 'https://panda.fans'>('https://dmsg.net')
  let pairs = $state<Awaited<ReturnType<CryptoEngine['legacyPairs']>>>([])
  let archives = $state<Awaited<ReturnType<CryptoEngine['legacyList']>>>([])
  let jobs = $state<Awaited<ReturnType<CryptoEngine['legacyJobs']>>>([])
  let selected = $state(''),
    principal = $state(''),
    confirmed = $state(false)
  let report = $state<Awaited<ReturnType<CryptoEngine['legacyReport']>> | null>(null)
  let takeAvatar = $state(false),
    avatarUrl = $state('')
  $effect(() => {
    const blob = report?.avatar?.blob
    takeAvatar = false
    avatarUrl = blob ? URL.createObjectURL(blob) : ''
    return () => {
      if (avatarUrl) URL.revokeObjectURL(avatarUrl)
    }
  })
  let messageLimit = $state(100)
  let importing = $state(false)
  let takeName = $state(false),
    takeBio = $state(false),
    profileName = $state(''),
    profileBio = $state('')
  const states = {
    running: '进行中',
    paused: '已暂停',
    failed: '需要重试',
    cancelled: '已取消',
    complete: '本轮检查结束'
  }
  const stages = {
    inventoried: '已清点',
    copied: '已保存',
    ciphertext_verified: '已校验密文',
    keys_imported: '已封装恢复材料',
    content_verified: '已检查内容'
  }
  $effect(() => {
    if (report) {
      messageLimit = 100
      profileName = report.profile.name
      profileBio = report.profile.bio
      takeName = false
      takeBio = false
    }
  })
  async function importProfile() {
    await session.run(async () => {
      const current = session.data.profile ?? {
        name: '我的身份',
        bio: '',
        link: '',
        contact: 'closed' as const,
        publicFields: []
      }
      const avatarFile =
        takeAvatar && report?.avatar
          ? (
              await session.crypto.call(
                'importFile',
                new File([report.avatar.blob], report.avatar.name, {
                  type: report.avatar.blob.type
                })
              )
            ).key
          : current.avatarFile
      await session.crypto.call('saveProfile', {
        ...current,
        ...(avatarFile ? { avatarFile } : {}),
        name: takeName ? profileName : current.name,
        bio: takeBio ? profileBio : current.bio,
        publicFields: current.publicFields.filter(
          (field) => !(takeName && field === 'name') && !(takeBio && field === 'bio')
        )
      })
      await session.refresh()
    }, '选中的旧资料已保存到本机；公开发布需要单独确认。')
  }
  const pairing = $derived(pairs.find((p) => p.offer.nonce === selected))
  async function refresh() {
    pairs = await session.crypto.call('legacyPairs')
    archives = await session.crypto.call('legacyList')
    jobs = await session.crypto.call('legacyJobs')
  }
  onMount(() => {
    void session.run(refresh)
  })
  async function create() {
    await session.run(async () => {
      if (location.protocol !== 'chrome-extension:')
        throw new Error('请在已安装的扩展中生成配对。')
      const result = await session.crypto.call(
        'legacyPair',
        origin,
        `chrome-extension://${location.hostname}`
      )
      await refresh()
      selected = result.offer.nonce
      confirmed = false
    })
  }
  async function receive(file: File) {
    if (!pairing || !confirmed) return
    importing = true
    await session.run(async () => {
      report = await session.crypto.call('legacyImport', {
        file,
        nonce: pairing!.offer.nonce,
        fingerprint: pairing!.fingerprint,
        principal: principal.trim()
      })
      await refresh()
      await session.refresh()
      session.message = report.gaps.length
        ? '档案已保留；仍有缺口，未标记为完整迁移。'
        : '档案已在本机重新验证并加密保留，仍需空白设备恢复与最终切换核对。'
    })
    importing = false
    if (session.unlocked) await refresh()
  }
</script>

<section class="settings-section">
  <span class="eyebrow">BRING YOUR HISTORY</span>
  <h2>旧版迁移档案</h2>
  <p>目标：{session.meta?.account?.id ?? '当前本机工作台（尚未绑定链上账户）'}</p>
  <p>
    在原站使用旧身份只读导出，历史钥由本次配对加密交给扩展。迁入档案不授予新设备、名称或共享频道权限。
  </p>
  <label
    >原网站<select bind:value={origin}
      ><option>https://dmsg.net</option><option>https://panda.fans</option></select
    ></label
  >
  <button class="secondary" onclick={create} disabled={session.busy}>生成一次性配对</button>
  {#if pairs.length}<label
      >继续配对<select bind:value={selected} onchange={() => (confirmed = false)}
        ><option value="">请选择</option>{#each pairs as pair}<option value={pair.offer.nonce}
            >{pair.offer.origin} · {pair.offer.nonce.slice(0, 12)}{pair.started
              ? ' · 继续导入'
              : ''}</option
          >{/each}</select
      ></label
    >{/if}
  {#if pairing}
    <label
      >复制到原网站的配对请求<textarea readonly rows="4" value={JSON.stringify(pairing.offer)}
      ></textarea></label
    >
    <p>双方核对指纹：<code class="fingerprint">{pairing.fingerprint}</code></p>
    <a
      class="button-link secondary"
      href={`${pairing.offer.origin}/legacy`}
      target="_blank"
      rel="noreferrer">打开原站清点与加密导出</a
    >
    <label
      >原站显示的旧 Principal<input
        bind:value={principal}
        autocomplete="off"
        placeholder="逐字核对旧身份"
      /></label
    >
    <label
      ><input type="checkbox" bind:checked={confirmed} /> 已核对双方指纹、目标扩展 ID 和旧身份，接收此档案</label
    >
    <label
      >选择加密迁移文件<input
        type="file"
        accept=".dmsg-migration"
        disabled={!confirmed || !principal || session.busy}
        onchange={(event) => {
          const file = event.currentTarget.files?.[0]
          event.currentTarget.value = ''
          if (file) void receive(file)
        }}
      /></label
    >
  {/if}
  <p class="caption">
    新配对十分钟内有效。已经接收并持久化的同一任务可在重启后继续。最终恢复包仍限定为 256
    MiB；超限会明确失败。
  </p>
</section>
<section class="settings-section">
  <h2>迁移进度</h2>
  {#if importing}<button class="secondary" onclick={() => session.crypto.call('legacyPause')}
      >暂停迁移</button
    >{/if}
  {#each jobs as job}<div class="settings-row">
      <div>
        <strong>{job.mode} · {job.principal}</strong>
        <p>
          {states[job.state]} · {stages[job.stage]} · 已检查 {job.completed}/{job.count}{job.partial
            ? ' · 部分范围'
            : ''}
        </p>
      </div>
      {#if job.state !== 'complete'}<button
          class="secondary"
          disabled={session.busy}
          onclick={() =>
            session.run(async () => {
              await session.crypto.call('legacyCancel', job.id)
              await refresh()
            }, '任务已取消，来源和已保留内容未删除。')}>取消任务</button
        >{/if}
    </div>{/each}
  <button class="secondary" disabled={session.busy} onclick={() => session.run(refresh)}
    >刷新进度</button
  >
</section>
<section class="settings-section">
  <h2>已保留的历史</h2>
  {#each archives as archive}<div class="settings-row">
      <div>
        <strong>{archive.mode} · {archive.principal}</strong>
        <p>{archive.objects} 项来源记录 · {archive.gaps} 项缺口 · 预迁移快照</p>
      </div>
      <button
        class="secondary"
        disabled={session.busy}
        onclick={() =>
          session.run(async () => {
            report = await session.crypto.call('legacyReport', archive.key)
          })}>离线验证与阅读</button
      >
    </div>{:else}<p>尚未导入历史档案。</p>{/each}
  {#if report}<h3>{report.stage === 'partial' ? '部分档案，存在缺口' : '本机内容验证通过'}</h3>
    <p>
      来源记录与原作者的设备签名是不同证据；下方作者和时间来自旧服务查询。当前未确认最终切换。
    </p>
    {#if report.gaps.length}<ul>
        {#each report.gaps.slice(0, 30) as gap}<li>
            {gap.source}：{gap.code} · {gap.detail}
          </li>{/each}
      </ul>{/if}
    <p>
      独立恢复：{report.recovery === 'verified'
        ? '已在新工作区验证'
        : report.recovery === 'partial_verified'
          ? '新工作区已验证保留的部分内容'
          : '尚待新工作区恢复验证'}
    </p>
    <button
      class="secondary"
      onclick={() => {
        if (report)
          downloadBlob(
            new Blob(
              [
                JSON.stringify(
                  {
                    format: 'dmsg-legacy-progress-report/1',
                    objects: report.count,
                    result: report.stage,
                    snapshot: report.snapshot,
                    recovery: report.recovery,
                    gapCounts: report.gaps.reduce<Record<string, number>>(
                      (counts, gap) => ({
                        ...counts,
                        [gap.code]: (counts[gap.code] ?? 0) + 1
                      }),
                      {}
                    )
                  },
                  null,
                  2
                )
              ],
              { type: 'application/json' }
            ),
            'legacy-progress.json'
          )
      }}>导出脱敏进度报告</button
    >
    <h3>冻结后的增量核对</h3>
    <p>
      导入原站为同一身份导出的最终历史证明。频道、消息或附件不一致时保留旧档案，并列出需要重新收集的来源。
    </p>
    <input
      type="file"
      accept=".cbor"
      aria-label="冻结历史对照证明"
      disabled={session.busy}
      onchange={(event) => {
        const file = event.currentTarget.files?.[0]
        if (file && report)
          void session.run(async () => {
            report = await session.crypto.call('legacyCompareFrozen', report!.key, file)
          })
      }}
    />{#if report.frozen}<p>
        {report.frozen.matched
          ? '选定档案与已验证冻结源一致。生产切换仍按发布流程确认。'
          : `仍有 ${report.frozen.totalChanges} 项源数据差异。`}
      </p>
      <ul>
        {#each report.frozen.changes.slice(0, 30) as change}<li>
            {change.source} · {change.reason}
          </li>{/each}
      </ul>{/if}
    <h3>旧资料预览</h3>
    <label
      ><input type="checkbox" bind:checked={takeName} /> 导入展示名<input
        bind:value={profileName}
        maxlength="100"
      /></label
    >
    <label
      ><input type="checkbox" bind:checked={takeBio} /> 导入简介（最多 500 字）<textarea
        bind:value={profileBio}
        maxlength="500"></textarea></label
    >
    <button
      class="secondary"
      disabled={session.busy || !(takeName || takeBio || takeAvatar)}
      onclick={importProfile}>保存所选资料到本机</button
    >
    {#if report.profile.image}<p class="caption">
        旧头像地址已保留：{report.profile.image}。头像重新公开需要单独选择。
      </p>{/if}
    {#if avatarUrl}<img
        src={avatarUrl}
        alt="已保存的旧头像预览"
        width="96"
        height="96"
      /><label
        ><input type="checkbox" bind:checked={takeAvatar} /> 导入旧头像到本机私密资料</label
      >{/if}
    <h3>历史附件</h3>
    {#each report.files as file}<div class="settings-row">
        <span>{file.name}</span><button
          class="secondary"
          disabled={session.busy || !file.verified}
          onclick={() =>
            session.run(async () => {
              const data = await session.crypto.call('legacyFile', report!.key, file.source)
              downloadBlob(data.blob, data.name)
            })}>{file.verified ? '验证并下载' : '缺钥或校验未通过'}</button
        >
      </div>{/each}
    <h3>旧资源记录</h3>
    <p class="caption">
      以下为旧服务记录，尚未兑换或处置。PANDA/DMSG 余额、PoL 和未决付款需要独立清单与账本对账。
    </p>
    {#each report.resources as resource}<p class="caption">
        {resource.source} · 已付 PANDA 最小单位 {resource.paid} · 旧 gas 原始值 {resource.gas}
      </p>{/each}
    {#each report.entitlements as entry}<details>
        <summary>{entry.source} · 尚未处置</summary>
        <pre>{entry.value}</pre>
      </details>{/each}
    {#each report.messages.slice(0, messageLimit) as message}<article class="history-message">
        <p class="caption">
          {message.source} · {message.author} · {message.time}{message.system
            ? ' · 系统记录'
            : ''}
        </p>
        <p>{message.deleted ? '来源声明已删除' : message.text}</p>
      </article>{/each}
    {#if report.messages.length > messageLimit}<button
        class="secondary"
        onclick={() => (messageLimit += 100)}
        >再显示 100 条（共 {report.messages.length} 条）</button
      >{/if}
  {/if}
</section>

<style>
  .fingerprint {
    display: block;
    overflow-wrap: anywhere;
  }
  textarea {
    width: 100%;
  }
  .history-message {
    padding: 12px 0;
    border-bottom: 1px solid var(--border);
  }
  .history-message p {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
