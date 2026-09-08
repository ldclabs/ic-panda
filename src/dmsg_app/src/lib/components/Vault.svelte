<script lang="ts">
  import { session, dateLabel, downloadBlob, formatBytes } from '../session.svelte'
  import { isExtension } from '../config'
  import type { Item, ItemKind, VaultEntry } from '../models'
  import Icon from './Icon.svelte'
  import Modal from './Modal.svelte'
  let query = $state(''),
    filter = $state('all'),
    selected = $state<string | null>(null)
  let editor = $state(false),
    editing = $state<VaultEntry | null>(null),
    title = $state(''),
    kind = $state<ItemKind>('note')
  let body = $state(''),
    username = $state(''),
    secret = $state(''),
    url = $state(''),
    tags = $state(''),
    favorite = $state(false)
  let action = $state<'delete' | 'reveal' | 'copy' | null>(null),
    revealed = $state(false)
  let resumeId = $state<string | undefined>()
  let fileInput = $state<HTMLInputElement>()
  const names: Record<ItemKind, string> = {
    note: '私密笔记',
    login: '登录凭据',
    api: 'API 凭据',
    key: '密钥材料',
    file: '私密文件'
  }
  const icons: Record<ItemKind, string> = {
    note: 'file',
    login: 'lock',
    api: 'key',
    key: 'key',
    file: 'file'
  }
  const entries = $derived(
    session.data.entries
      .filter((entry) => {
        if (filter === 'trash' ? !entry.record.tombstone : entry.record.tombstone) return false
        if (filter === 'favorites' && !entry.item.favorite) return false
        if (
          ['note', 'login', 'api', 'key', 'file'].includes(filter) &&
          entry.item.type !== filter
        )
          return false
        const q = query.trim().toLowerCase()
        return (
          !q ||
          `${entry.item.title} ${entry.item.tags.join(' ')} ${entry.item.body}`
            .toLowerCase()
            .includes(q)
        )
      })
      .sort((a, b) => b.item.updatedAt - a.item.updatedAt)
  )
  const current = $derived(entries.find((entry) => entry.record.id === selected) ?? null)
  const count = $derived(session.data.entries.filter((x) => !x.record.tombstone).length)
  function edit(entry?: VaultEntry) {
    editing = entry ?? null
    kind = entry?.item.type ?? 'note'
    title = entry?.item.title ?? ''
    body = entry?.item.body ?? ''
    username = entry?.item.username ?? ''
    secret = entry?.item.secret ?? ''
    url = entry?.item.url ?? ''
    tags = entry?.item.tags.join(', ') ?? ''
    favorite = entry?.item.favorite ?? false
    editor = true
    session.touch()
  }
  function closeEditor() {
    editor = false
    title = ''
    body = ''
    username = ''
    secret = ''
    url = ''
    tags = ''
    editing = null
  }
  async function save() {
    await session.run(async () => {
      const item: Item = {
        type: kind,
        title,
        body,
        username,
        secret,
        url,
        tags: tags
          .split(/[,，]/)
          .map((s) => s.trim())
          .filter(Boolean),
        favorite,
        createdAt: editing?.item.createdAt ?? Date.now(),
        updatedAt: Date.now()
      }
      const result = await session.crypto.call('saveItem', {
        item,
        id: editing?.record.id,
        base: editing?.record.revision
      })
      if (result.conflict) session.message = '检测到编辑冲突，已保留两个版本。'
      else {
        selected = result.id
        filter = 'all'
        session.message = '已加密保存在本机。'
      }
      closeEditor()
      await session.refresh()
    })
  }
  async function importFile(event: Event) {
    const input = event.currentTarget as HTMLInputElement,
      file = input.files?.[0]
    input.value = ''
    if (!file) return
    const resume = resumeId
    resumeId = undefined
    await session.run(async () => {
      const record = await session.crypto.call('importFile', file, resume)
      selected = record.id
      filter = 'file'
      await session.refresh()
    }, '文件已分块加密保存在本机。')
  }
  async function performAction() {
    if (!current) return
    const entry = current,
      operation = action
    action = null
    if (operation === 'reveal') {
      revealed = true
      session.touch()
      return
    }
    await session.run(async () => {
      if (operation === 'copy') {
        if (
          isExtension() &&
          !(await chrome.permissions.request({ permissions: ['clipboardWrite'] }))
        )
          throw new Error('未授予剪贴板写入权限。')
        await navigator.clipboard.writeText(entry.item.secret)
        session.message = '已复制到系统剪贴板，外部副本不受 dMsg 锁定保护。'
      } else if (operation === 'delete') {
        await session.crypto.call('deleteItem', {
          id: entry.record.id,
          base: entry.record.revision
        })
        selected = null
        await session.refresh()
        session.message = '已移入回收站，历史版本仍保留。'
      }
    })
  }
  async function download(entry: VaultEntry) {
    await session.run(async () => {
      const result = await session.crypto.call('downloadFile', entry.record.key)
      downloadBlob(result.blob, result.name)
    }, '完整性校验通过，已开始下载。')
  }
</script>

<div class="page-heading">
  <div>
    <span class="eyebrow">YOUR PRIVATE SPACE</span>
    <h1>秘密库<span class="heading-count">{count}</span></h1>
    <p>值得保留的内容，只在你解锁后出现。</p>
  </div>
  <div class="heading-actions">
    <button class="secondary" onclick={() => fileInput?.click()} disabled={session.busy}
      ><Icon name="download" />导入文件</button
    ><button class="primary" onclick={() => edit()} disabled={session.busy}
      ><Icon name="plus" />新建条目</button
    >
  </div>
</div>
<input
  class="sr-only"
  aria-label="选择私密文件"
  bind:this={fileInput!}
  type="file"
  onchange={importFile}
/>
{#if session.data.imports.length}<div class="notice warning">
    <Icon name="info" />
    <div>
      <strong>有中断的文件任务</strong>{#each session.data.imports as job}<div
          class="conflict-row"
        >
          <span>{job.name} · {job.completed} / {job.total} 块</span><button
            class="text-button"
            onclick={() => {
              resumeId = job.id
              fileInput?.click()
            }}>重选原文件继续</button
          >
        </div>{/each}
      <p>已加密的块保持原字节，继续前会检查所选文件的一致性。</p>
    </div>
  </div>{/if}
<div class="vault-toolbar">
  <label class="search-field"
    ><Icon name="search" /><input
      type="search"
      aria-label="在本机搜索秘密库"
      bind:value={query}
      oninput={() => session.touch()}
      placeholder="在你的秘密库中搜索…"
    /><kbd>/</kbd></label
  ><span class="caption local-search"><Icon name="device" size={16} />仅搜索本机</span>
</div>
<div class="filter-bar" aria-label="筛选条目">
  {#each [['all', '全部条目'], ['favorites', '已收藏'], ['note', '笔记'], ['login', '登录'], ['api', 'API 凭据'], ['key', '密钥'], ['file', '文件'], ['trash', '回收站']] as [value, label]}
    <button
      class:active={filter === value}
      aria-pressed={filter === value}
      onclick={() => {
        filter = value
        selected = null
        revealed = false
      }}>{label}</button
    >
  {/each}
</div>
{#if session.data.conflicts.length}
  <div class="notice warning">
    <Icon name="info" />
    <div>
      <strong>{session.data.conflicts.length} 个冲突版本需要处理</strong>
      <p>原版本继续保留，选择后会生成一个新版本。</p>
      {#each session.data.conflicts as conflict}<div class="conflict-row">
          <span>{conflict.item.title} · {dateLabel(conflict.item.updatedAt)}</span><button
            class="text-button"
            onclick={() =>
              session.run(async () => {
                const head = session.data.entries.find(
                  (x) => x.record.id === conflict.record.id
                )
                if (!head) throw new Error('找不到当前版本。')
                await session.crypto.call('resolveConflict', {
                  key: conflict.record.key,
                  base: head.record.revision
                })
                await session.refresh()
              }, '冲突已解决，两个原始版本仍保留。')}>采用此版本</button
          >
        </div>{/each}
    </div>
  </div>
{/if}
{#if entries.length === 0}
  <div class="empty-vault">
    <div class="empty-art">
      <Icon name={filter === 'trash' ? 'trash' : 'file'} size={44} /><span
        class="empty-art-corner"><Icon name="lock" size={18} /></span
      >
    </div>
    <span class="eyebrow">{query ? 'LOCAL SEARCH' : 'A LITTLE SPACE, JUST FOR YOU'}</span>
    <h2>
      {query
        ? '没有找到匹配的内容。'
        : filter === 'trash'
          ? '回收站是空的。'
          : count
            ? '这里还没有条目。'
            : '从一件值得保留的事开始。'}
    </h2>
    <p>
      {query
        ? '换一个标题、标签或正文关键词试试。'
        : filter === 'trash'
          ? '删除会留下墓碑，旧版本不会在同步时悄悄复活。'
          : '一段未公开的想法、一枚 API 凭据，或只想留给自己的文件。'}
    </p>
    {#if !query && filter !== 'trash'}<button class="primary" onclick={() => edit()}
        ><Icon name="plus" />保存第一个条目</button
      >{/if}
    {#if !count && !query}<div class="starter-types">
        {#each ['note', 'login', 'api', 'key'] as type}<button
            onclick={() => {
              edit()
              kind = type as ItemKind
            }}><Icon name={icons[type as ItemKind]} />{names[type as ItemKind]}</button
          >{/each}
      </div>{/if}
  </div>
{:else}
  <div class="vault-grid" class:has-selection={!!current}>
    <section class="item-list" aria-label="秘密条目">
      <div class="list-label"><span>条目 / {entries.length}</span><span>最近修改</span></div>
      {#each entries as entry (entry.record.key)}
        <button
          class="item-row"
          class:selected={selected === entry.record.id}
          aria-pressed={selected === entry.record.id}
          onclick={() => {
            selected = entry.record.id
            revealed = false
            session.touch()
          }}
        >
          <span class="item-icon"><Icon name={icons[entry.item.type]} /></span><span
            class="item-summary"
            ><strong>{entry.item.title}</strong><span
              >{names[entry.item.type]}{#if entry.item.tags.length}<span class="middle-dot"
                  >·</span
                >{entry.item.tags.join(' / ')}{/if}</span
            ></span
          >
          {#if entry.item.favorite}<Icon name="star" size={16} />{/if}<span class="item-date"
            >{dateLabel(entry.item.updatedAt)}</span
          >
        </button>
      {/each}
    </section>
    {#if current}
      <aside class="item-detail" aria-label="条目详情">
        <div class="detail-top">
          <span class="eyebrow">{names[current.item.type]}</span><button
            class="icon-button"
            aria-label="关闭详情"
            onclick={() => {
              selected = null
              revealed = false
            }}><Icon name="close" /></button
          >
        </div>
        <h2>{current.item.title}</h2>
        <div class="tag-list">
          {#each current.item.tags as tag}<span>{tag}</span>{/each}
        </div>
        <div class="detail-fields">
          {#if current.item.username}<div>
              <span class="field-label">用户名 / 标识</span>
              <p>{current.item.username}</p>
            </div>{/if}
          {#if current.item.secret}<div>
              <span class="field-label">私密字段</span>
              <div class="secret-field">
                <code>{revealed ? current.item.secret : '••••••••••••••••'}</code><button
                  class="icon-button"
                  aria-label={revealed ? '隐藏私密字段' : '显示私密字段'}
                  onclick={() => {
                    if (revealed) revealed = false
                    else action = 'reveal'
                  }}><Icon name="eye" /></button
                ><button
                  class="icon-button"
                  aria-label="复制私密字段"
                  onclick={() => (action = 'copy')}><Icon name="copy" /></button
                >
              </div>
            </div>{/if}
          {#if current.item.url}<div>
              <span class="field-label">关联地址</span>
              <p class="break-anywhere">{current.item.url}</p>
            </div>{/if}
          {#if current.item.body}<div>
              <span class="field-label">{current.item.type === 'note' ? '笔记' : '备注'}</span>
              <p class="preserve-lines">{current.item.body}</p>
            </div>{/if}
          {#if current.item.file}<div class="file-summary">
              <Icon name="file" size={32} />
              <div>
                <strong>{formatBytes(current.item.file.size)}</strong><span
                  >{current.item.file.chunks.length} 个加密块</span
                >
              </div>
            </div>
            <div>
              <span class="field-label">原文件 SHA-256</span><code class="hash"
                >{current.item.file.sha256}</code
              >
            </div>
            <button
              class="primary wide"
              disabled={session.busy}
              onclick={() => download(current!)}><Icon name="download" />验证并下载文件</button
            >{/if}
        </div>
        <div class="detail-meta">
          <span>独立加密版本</span><code>{current.record.revision.slice(0, 12)}</code><span
            >保存状态</span
          ><span>已加密保存在本机</span>
        </div>
        <div class="detail-actions">
          {#if current.record.tombstone}<button
              class="secondary"
              onclick={() =>
                session.run(async () => {
                  await session.crypto.call('deleteItem', {
                    id: current!.record.id,
                    base: current!.record.revision,
                    restore: true
                  })
                  selected = null
                  await session.refresh()
                }, '已作为新版本恢复。')}><Icon name="refresh" />恢复条目</button
            >{:else}{#if current.item.type !== 'file'}<button
                class="secondary"
                onclick={() => edit(current!)}><Icon name="edit" />编辑</button
              >{/if}<button class="text-button danger" onclick={() => (action = 'delete')}
              ><Icon name="trash" />移入回收站</button
            >{/if}
        </div>
      </aside>
    {:else}<aside class="detail-placeholder">
        <Icon name="lock" size={32} />
        <p>选一个条目，慢慢查看。</p>
        <span>内容只在已解锁的工作台中显示。</span>
      </aside>{/if}
  </div>
{/if}
<div class="workspace-note">
  <Icon name="lock" size={16} /><span
    >本机加密保存。{session.meta?.lastBackupAt
      ? `最近生成备份：${dateLabel(session.meta.lastBackupAt)}`
      : '新增内容需要另行导出备份。'}</span
  >
</div>
{#if editor}
  <Modal title={editing ? '编辑私密条目' : '新建私密条目'} onclose={closeEditor}>
    <form
      onsubmit={(event) => {
        event.preventDefault()
        void save()
      }}
      oninput={() => session.touch()}
    >
      <div class="form-row">
        <label
          >类型<select aria-label="类型" bind:value={kind}
            >{#each ['note', 'login', 'api', 'key'] as type}<option value={type}
                >{names[type as ItemKind]}</option
              >{/each}</select
          ></label
        ><label
          >标题<input
            bind:value={title}
            required
            maxlength="200"
            placeholder="例如：项目部署凭据"
          /></label
        >
      </div>
      {#if kind !== 'note'}<label
          >{kind === 'login' ? '用户名' : '标识 / 服务名称'}<input
            bind:value={username}
            maxlength="512"
            autocomplete="off"
          /></label
        ><label
          >{kind === 'login'
            ? '密码'
            : kind === 'api'
              ? 'API Secret / Token'
              : '原始密钥材料'}<textarea
            class="mono-input"
            bind:value={secret}
            rows="3"
            autocomplete="off"
            spellcheck="false"
            maxlength="10000"></textarea></label
        ><label
          >关联地址<input bind:value={url} maxlength="2048" placeholder="https://…" /></label
        >{/if}
      <label
        >{kind === 'note' ? '笔记内容' : '备注'}<textarea
          bind:value={body}
          rows={kind === 'note' ? 8 : 3}
          maxlength="20000"
          placeholder="只有解锁之后，内容才会出现在这里。"></textarea></label
      >
      <label
        >标签<span class="label-hint">以逗号分隔</span><input
          bind:value={tags}
          maxlength="820"
          placeholder="工作, 私人"
        /></label
      >
      <label class="check-label"
        ><input type="checkbox" bind:checked={favorite} />加入收藏</label
      >
      {#if session.error}<p class="form-error" role="alert">{session.error}</p>{/if}
      <div class="modal-actions">
        <button type="button" class="secondary" onclick={closeEditor}>取消</button><button
          class="primary"
          disabled={session.busy}
          >{session.busy ? '正在加密…' : '加密保存'}<Icon name="lock" /></button
        >
      </div>
    </form>
  </Modal>
{/if}
{#if action}
  <Modal
    title={action === 'delete'
      ? '移入回收站？'
      : action === 'copy'
        ? '复制私密字段？'
        : '显示私密字段？'}
    onclose={() => (action = null)}
  >
    <p>
      {action === 'delete'
        ? '保存删除墓碑并保留历史版本。你可以在回收站中恢复条目。'
        : action === 'copy'
          ? '内容将写入系统剪贴板。锁定或撤销授权不能删除其他程序已取得的副本。'
          : '私密字段将显示在屏幕上，请确认周围环境。'}
    </p>
    <div class="modal-actions">
      <button class="secondary" onclick={() => (action = null)}>取消</button><button
        class="primary"
        onclick={performAction}
        >{action === 'delete'
          ? '移入回收站'
          : action === 'copy'
            ? '复制到剪贴板'
            : '显示内容'}</button
      >
    </div>
  </Modal>
{/if}
