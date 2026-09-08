<script lang="ts">
  import { session, dateLabel, downloadBlob, formatBytes, shortId } from '../session.svelte'
  import { config, isExtension } from '../config'
  import { inspectRelay } from '../services/relay'
  import { login, services } from '../services/ic'
  import { hex } from '../protocol/codec'
  import Modal from './Modal.svelte'
  import Icon from './Icon.svelte'
  let tab = $state('recovery'),
    modal = $state<'backup' | 'password' | null>(null),
    password = $state(''),
    next = $state(''),
    confirm = $state('')
  let usage = $state<StorageEstimate | null>(null),
    principal = $state(''),
    linkedSubject = $state(''),
    serviceReport = $state(''),
    derivation = $state(config.derivationOrigins[0])
  let legacyInput = $state<HTMLInputElement>()
  function close() {
    modal = null
    password = ''
    next = ''
    confirm = ''
  }
  async function backup() {
    await session.run(async () => {
      const result = await session.crypto.call('exportBackup', password)
      downloadBlob(result.blob, result.name)
      close()
      await session.refresh()
      session.message = result.missing.length
        ? `已生成部分备份，清单列出 ${result.missing.length} 项缺口。`
        : `已生成含本机内容的恢复包，共 ${result.count} 个版本。请确认下载文件。`
    })
  }
  async function changePassword() {
    await session.run(async () => {
      if (next !== confirm) throw new Error('两次新口令不一致。')
      await session.crypto.call('changePassword', { current: password, next })
      close()
    }, '本机口令已更新，内容密钥保持不变。')
  }
  async function signIn() {
    await session.run(async () => {
      const identity = await login(session.crypto, session.meta!.transportPublic, derivation),
        api = await services(identity)
      principal = identity.getPrincipal().toText()
      const subject = await api.user!.my_subject()
      linkedSubject = subject.length ? hex(Uint8Array.from(subject[0]!)) : ''
    }, '认证完成。登录不会自动批准设备或解锁其他主体的内容。')
  }
</script>

<div class="page-heading">
  <div>
    <span class="eyebrow">MAKE IT YOURS</span>
    <h1>设置</h1>
    <p>照看你的设备、恢复材料和数据。</p>
  </div>
</div>
<div class="filter-bar" aria-label="设置分类">
  {#each [['recovery', '恢复与备份'], ['devices', '设备与认证'], ['storage', '存储'], ['migration', '旧版迁移'], ['services', '服务连接']] as [value, label]}<button
      class:active={tab === value}
      aria-pressed={tab === value}
      onclick={() => {
        tab = value
        if (value === 'storage')
          void navigator.storage.estimate().then((result) => (usage = result))
      }}>{label}</button
    >{/each}
</div>
<div class="settings-content">
  {#if tab === 'recovery'}
    <section class="settings-section">
      <div class="section-heading">
        <span class="item-icon"><Icon name="key" /></span>
        <div>
          <h2>恢复材料</h2>
          <p>登录恢复与内容恢复，分别保管。</p>
        </div>
        <span class="pill">{session.meta?.recoveryChecked ? '恢复码已验证' : '尚未验证'}</span>
      </div>
      <div class="settings-row">
        <div>
          <strong>最近生成的恢复包</strong>
          <p>
            {session.meta?.lastBackupAt
              ? `${dateLabel(session.meta.lastBackupAt)} · ${session.meta.lastBackupCount} 个版本`
              : '尚未生成'}
          </p>
        </div>
        <button class="primary" onclick={() => (modal = 'backup')}
          ><Icon name="download" />导出加密备份</button
        >
      </div>
      <div class="notice">
        <Icon name="info" />
        <p>
          恢复需要加密包和分开保存的恢复码。新内容不会自动出现在旧备份中；卸载扩展可能删除本地数据。
        </p>
      </div>
      <dl class="evidence-list">
        <div>
          <dt>登录认证</dt>
          <dd>使用 Internet Identity 的恢复方式</dd>
        </div>
        <div>
          <dt>本机内容</dt>
          <dd>完整加密包 + 恢复码，可离线恢复</dd>
        </div>
        <div>
          <dt>新设备授权</dt>
          <dd>已有设备批准或预登记的链上恢复流程</dd>
        </div>
        <div>
          <dt>正式签名密钥</dt>
          <dd>由原密钥服务控制，不能导出为助记词</dd>
        </div>
      </dl>
    </section>
    <section class="settings-section">
      <div class="settings-row">
        <div>
          <h2>本机解锁</h2>
          <p>默认 15 分钟无敏感操作后锁定；关闭解锁页面后需重新解锁。</p>
        </div>
        <button class="secondary" onclick={() => (modal = 'password')}>修改口令</button>
      </div>
    </section>
  {:else if tab === 'devices'}
    <section class="settings-section">
      <div class="section-heading">
        <span class="item-icon"><Icon name="device" /></span>
        <div>
          <h2>这台设备</h2>
          <p>独立的签名密钥与接收密钥</p>
        </div>
        <span class="pill">仅本机 · 未登记</span>
      </div>
      <dl class="evidence-list">
        <div>
          <dt>设备 ID</dt>
          <dd><code>{shortId(session.meta!.deviceId)}</code></dd>
        </div>
        <div>
          <dt>创建时间</dt>
          <dd>{dateLabel(session.meta!.createdAt)}</dd>
        </div>
        <div>
          <dt>签名公钥</dt>
          <dd><code class="hash">{session.meta?.signingPublic}</code></dd>
        </div>
        <div>
          <dt>本机权限</dt>
          <dd>解锁本机内容、编辑加密草稿</dd>
        </div>
        <div>
          <dt>链上设备权限</dt>
          <dd>尚未批准</dd>
        </div>
      </dl>
    </section>
    <section class="settings-section">
      <h2>Internet Identity</h2>
      <p>选择原应用的派生来源，以保留该来源下的 Principal。不同来源不会自动合并。</p>
      <label
        >派生来源<select bind:value={derivation}
          >{#each config.derivationOrigins as origin}<option value={origin}
              >{origin}{origin === 'https://panda.fans' ? ' · 旧版来源' : ''}</option
            >{/each}</select
        ></label
      ><button
        class="secondary"
        onclick={signIn}
        disabled={session.busy || !config.canisters.user}
        >连接 Internet Identity<Icon name="arrow-up-right" /></button
      >{#if !config.canisters.user}<p class="caption">
          需先配置新版 dmsg_user canister；正式扩展 origin 还须由原派生站点列入 II 白名单。
        </p>{/if}{#if principal}<div class="notice">
          <Icon name="user" />
          <div>
            <strong>本次认证 Principal</strong><code class="hash">{principal}</code>
            <p>
              {linkedSubject
                ? `该认证绑定的主体：${linkedSubject}`
                : '此身份尚未建立新版主体。'} 本地主体和链上主体不会因名称相同而合并。
            </p>
          </div>
        </div>{/if}
    </section>
  {:else if tab === 'storage'}
    <section class="settings-section">
      <h2>本机存储</h2>
      <div class="storage-total">
        <strong>{formatBytes(usage?.usage ?? 0)}</strong><span
          >/ 浏览器配额 {formatBytes(usage?.quota ?? 0)}</span
        >
      </div>
      <progress max={usage?.quota || 1} value={usage?.usage || 0}></progress>
      <p>
        {session.data.outbox.filter((j) => j.state !== 'stored').length} 个本地版本尚未同步。草稿、恢复封装、冲突记录不会自动淘汰。
      </p>
      <div class="settings-row">
        <div>
          <strong>保留存储空间</strong>
          <p>申请浏览器持久化存储，仍需独立备份。</p>
        </div>
        <button
          class="secondary"
          onclick={() =>
            session.run(async () => {
              if (isExtension()) {
                if (!(await chrome.permissions.request({ permissions: ['unlimitedStorage'] })))
                  throw new Error('存储权限未授予。')
              } else if (!(await navigator.storage.persist()))
                throw new Error('浏览器没有授予持久化存储。')
            }, '浏览器已授予存储请求。')}>申请保留空间</button
        >
      </div>
    </section>
    <section class="settings-section">
      <h2>待同步内容</h2>
      <p>当前服务未启用，以下数据均只保存在本机。</p>
      {#if session.data.outbox.length}<div class="outbox-list">
          {#each session.data.outbox.slice(-10).reverse() as job}<div>
              <code>{job.id.slice(0, 16)}</code><span
                >{job.error === 'VERSION_CONFLICT' ? '有编辑冲突' : '本地加密保存'}</span
              >
            </div>{/each}
        </div>{:else}<p class="caption">没有待同步条目。</p>{/if}
    </section>
  {:else if tab === 'migration'}
    <section class="settings-section">
      <span class="eyebrow">BRING YOUR HISTORY</span>
      <h2>先保留，再迁移。</h2>
      <p>
        旧网站与扩展使用不同的存储空间。请在原浏览器打开旧版只读入口，核对身份、旧密钥模式和可读取的附件。
      </p>
      <a
        class="secondary button-link"
        href="https://dmsg.net/legacy"
        target="_blank"
        rel="noreferrer">打开旧版只读入口<Icon name="arrow-up-right" /></a
      >
      <ol class="migration-steps">
        <li>
          <strong>核对旧身份与数据清单</strong>
          <p>区分旧 Principal、名称委托角色、频道与个人档案。</p>
        </li>
        <li>
          <strong>保留原密文和密钥恢复材料</strong>
          <p>Local 模式依赖原浏览器材料；读取失败不会生成新钥来替代。</p>
        </li>
        <li>
          <strong>在本机保存授权取得的档案</strong>
          <p>
            可先将旧版导出的文件作为私密文件加密保存。当前不自动解码旧 MK /
            KEK，也不认定共享频道所有权。
          </p>
        </li>
        <li>
          <strong>验证内容与空白设备恢复</strong>
          <p>密文复制、内容解密与独立恢复是不同阶段。未验证的项目保持未完成。</p>
        </li>
      </ol>
      <input
        class="sr-only"
        bind:this={legacyInput!}
        aria-label="选择旧版档案文件"
        type="file"
        onchange={(event) => {
          const file = event.currentTarget.files?.[0]
          event.currentTarget.value = ''
          if (file)
            void session.run(async () => {
              await session.crypto.call('importFile', file)
              await session.refresh()
            }, '旧档案文件已在秘密库加密保存；旧格式内容尚未解密验证。')
        }}
      /><button class="secondary" onclick={() => legacyInput?.click()} disabled={session.busy}
        ><Icon name="download" />保管旧版导出文件</button
      >
    </section>
  {:else}
    <section class="settings-section">
      <h2>服务与发布状态</h2>
      <dl class="evidence-list">
        <div>
          <dt>构建阶段</dt>
          <dd>R0 · 本地实现</dd>
        </div>
        <div>
          <dt>环境</dt>
          <dd>{config.environment}</dd>
        </div>
        <div>
          <dt>中继 API</dt>
          <dd>{config.relayOrigin || '未配置'}</dd>
        </div>
        {#each Object.entries(config.canisters) as [name, value]}<div>
            <dt>dmsg_{name}</dt>
            <dd><code>{value || '未配置'}</code></dd>
          </div>{/each}
        <div>
          <dt>外部应用白名单</dt>
          <dd>
            {config.externalOrigins.length ? config.externalOrigins.join(', ') : '尚未开放'}
          </dd>
        </div>
      </dl>
      <button
        class="secondary"
        disabled={!config.relayOrigin || session.busy}
        onclick={() =>
          session.run(async () => {
            const result = await inspectRelay()
            serviceReport = `${result.protocol} · ${result.ready ? '服务报告就绪，仍需验证权限和证据' : '尚未就绪'}；待通过：${result.gates.join('、') || '以实际协议证据为准'}`
          })}>检查中继状态<Icon name="refresh" /></button
      >{#if serviceReport}<p role="status">{serviceReport}</p>{/if}
      <div class="notice">
        <Icon name="info" />
        <p>
          联网参数固定在构建配置中。当前候选云端协议与链上安全证据尚未互通，不会通过远端开关自动开放生产写入、支付或正式签名。
        </p>
      </div>
    </section>
  {/if}
</div>
{#if modal}<Modal
    title={modal === 'backup' ? '重新验证后导出' : '修改本机口令'}
    onclose={close}
    ><form
      onsubmit={(event) => {
        event.preventDefault()
        void (modal === 'backup' ? backup() : changePassword())
      }}
    >
      <p>
        {modal === 'backup'
          ? '导出包含本机未同步内容与可恢复的历史版本。文件中不包含设备私钥，也不会导出阈值签名私钥。'
          : '仅重新封装本机数据密钥，不改变已有内容密钥或恢复码。'}
      </p>
      <label
        >当前口令<input
          type="password"
          bind:value={password}
          autocomplete="current-password"
          required
        /></label
      >
      {#if modal === 'password'}<label
          >新口令<input
            type="password"
            bind:value={next}
            minlength="12"
            required
            autocomplete="new-password"
          /></label
        ><label
          >再次输入新口令<input
            type="password"
            bind:value={confirm}
            minlength="12"
            required
            autocomplete="new-password"
          /></label
        >{/if}
      {#if session.error}<p class="form-error" role="alert">{session.error}</p>{/if}
      <div class="modal-actions">
        <button type="button" class="secondary" onclick={close}>取消</button><button
          class="primary"
          disabled={session.busy}
          >{session.busy
            ? '正在验证…'
            : modal === 'backup'
              ? '生成加密备份'
              : '更新口令'}</button
        >
      </div>
    </form></Modal
  >{/if}
