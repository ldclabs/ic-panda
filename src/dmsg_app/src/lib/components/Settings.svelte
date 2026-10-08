<script lang="ts">
  import { session, formatBytes } from '../session.svelte'
  import { config, isExtension } from '../config'
  import { inspectRelay } from '../services/relay'
  import Icon from './Icon.svelte'
  let tab = $state('devices')
  let usage = $state<StorageEstimate | null>(null),
    serviceReport = $state('')
</script>

<div class="page-heading">
  <div>
    <span class="eyebrow">MAKE IT YOURS</span>
    <h1>设置</h1>
    <p>照看你的账户、设备和数据。</p>
  </div>
</div>
<div class="filter-bar" aria-label="设置分类">
  {#each [['devices', '账户与设备'], ['sync', '云端同步'], ['storage', '存储'], ['migration', '旧版迁移'], ['handles', '名称管理'], ['shared', '共享迁移'], ['commerce', '套餐与付款'], ['inbox', '来信与托管'], ['funds', '资金恢复'], ['agents', 'Agent 授权'], ['services', '服务连接']] as [value, label]}<button
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
  {#if tab === 'devices'}
    {#await import('./AccountSettings.svelte')}
      <p role="status">正在加载…</p>
    {:then component}
      <component.default />
    {:catch}
      <p role="alert">无法加载设置，请重新打开工作台。</p>
    {/await}
  {:else if tab === 'sync'}
    {#await import('./SyncSettings.svelte')}
      <p role="status">正在加载…</p>
    {:then component}
      <component.default />
    {:catch}
      <p role="alert">无法加载设置，请重新打开工作台。</p>
    {/await}
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
        {session.data.outbox.length} 个本地版本尚未同步。云端是设备外的唯一副本；卸载扩展或清除浏览器数据会删除未同步的内容。
      </p>
      <div class="settings-row">
        <div>
          <strong>保留存储空间</strong>
          <p>申请浏览器持久化存储，降低浏览器清理本机数据的可能。</p>
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
      <p>登录解锁后会自动同步；也可在“云端同步”手动同步。云端状态以已验证的提交回执为准。</p>
      {#if session.data.outbox.length}<div class="outbox-list">
          {#each session.data.outbox.slice(0, 10) as job}<div>
              <code>{job.id.slice(0, 16)}</code><span
                >{job.error === 'VERSION_CONFLICT'
                  ? '有编辑冲突'
                  : job.state === 'unknown'
                    ? '提交结果待确认'
                    : job.state === 'blocked'
                      ? '需要处理后再提交'
                      : '本地加密保存'}</span
              >
            </div>{/each}
        </div>{:else}<p class="caption">没有待同步条目。</p>{/if}
    </section>
  {:else if tab === 'agents'}{#await import('./AgentSettings.svelte')}
      <p role="status">正在加载…</p>
    {:then component}
      <component.default />
    {:catch}
      <p role="alert">无法加载设置，请重新打开工作台。</p>
    {/await}
  {:else if tab === 'inbox'}{#await import('./InboxSettings.svelte')}
      <p role="status">正在加载…</p>
    {:then component}
      <component.default />
    {:catch}
      <p role="alert">无法加载设置，请重新打开工作台。</p>
    {/await}
  {:else if tab === 'funds'}{#await import('./PaymentSettings.svelte')}<p>
        正在加载…
      </p>{:then component}<component.default />{:catch}<p role="alert">
        无法加载资金恢复。
      </p>{/await}
  {:else if tab === 'commerce'}{#await import('./CommerceSettings.svelte')}
      <p role="status">正在加载…</p>
    {:then component}
      <component.default />
    {:catch}
      <p role="alert">无法加载设置，请重新打开工作台。</p>
    {/await}
  {:else if tab === 'shared'}{#await import('./SharedSettings.svelte')}
      <p role="status">正在加载…</p>
    {:then component}
      <component.default />
    {:catch}
      <p role="alert">无法加载设置，请重新打开工作台。</p>
    {/await}
  {:else if tab === 'handles'}{#await import('./HandleSettings.svelte')}
      <p role="status">正在加载…</p>
    {:then component}
      <component.default />
    {:catch}
      <p role="alert">无法加载设置，请重新打开工作台。</p>
    {/await}
  {:else if tab === 'migration'}
    {#await import('./LegacySettings.svelte')}
      <p role="status">正在加载…</p>
    {:then component}
      <component.default />
    {:catch}
      <p role="alert">无法加载设置，请重新打开工作台。</p>
    {/await}
  {:else}
    <section class="settings-section">
      <h2>服务与发布状态</h2>
      <dl class="evidence-list">
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
            <dd><code>{[value].flat().join(', ') || '未配置'}</code></dd>
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
            serviceReport = `${result.protocol} · ${result.environment} · ${
              result.ready
                ? '配置自检通过；权限与证据仍逐次验证'
                : `配置不完整：${result.problems.join('、')}`
            }`
          })}>检查中继状态<Icon name="refresh" /></button
      >{#if serviceReport}<p role="status">{serviceReport}</p>{/if}
      <div class="notice">
        <Icon name="info" />
        <p>
          联网参数与权限固定在构建配置中，服务端不能远程放宽。中继的就绪状态只反映其配置自检，每次写入、支付和正式认证仍按账户证据与签名逐次验证。
        </p>
      </div>
    </section>
  {/if}
</div>
