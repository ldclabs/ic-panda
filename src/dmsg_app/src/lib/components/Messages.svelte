<script lang="ts">
  import { session, dateLabel, shortId } from '../session.svelte'
  import type { Channel } from '../models'
  import Icon from './Icon.svelte'
  import Modal from './Modal.svelte'
  let selected = $state<string | null>(null),
    composer = $state(''),
    create = $state(false),
    name = $state(''),
    recipient = $state(''),
    type = $state<Channel['type']>('direct')
  let draftSequence = 0
  const channels = $derived(session.data.channels)
  const current = $derived(channels.find((x) => x.record.id === selected) ?? null)
  const messages = $derived(
    session.data.messages
      .filter((x) => x.message.channelId === selected)
      .sort((a, b) => a.message.createdAt - b.message.createdAt)
  )
  async function select(channelId: string) {
    selected = channelId
    composer = ''
    const sequence = ++draftSequence
    const value = await session.crypto.call('getDraft', channelId)
    if (sequence === draftSequence && selected === channelId) composer = value
  }
  async function saveDraft() {
    session.touch()
    if (selected)
      await session.crypto.call('saveDraft', { channelId: selected, text: composer })
  }
  async function saveMessage() {
    if (!selected) return
    await session.run(async () => {
      await session.crypto.call('saveMessage', { channelId: selected!, text: composer })
      composer = ''
      await saveDraft()
      await session.refresh()
    }, '已保存加密消息草稿。尚未投递给对方。')
  }
</script>

<div class="page-heading">
  <div>
    <span class="eyebrow">SHARE WITH INTENT</span>
    <h1>消息</h1>
    <p>为一段对话，留一个私密的空间。</p>
  </div>
  <button class="primary" onclick={() => (create = true)}><Icon name="plus" />新建会话</button>
</div>
<div class="notice">
  <Icon name="cloud" />
  <p>当前未连接中继。会话与消息以加密草稿保存在本机，接入服务并确认成员资格后才能交付。</p>
</div>
<div class="messages-layout">
  <aside class="channel-list">
    <div class="list-label">会话 / {channels.length}</div>
    {#if !channels.length}<p class="channel-empty">
        还没有会话。<br />邀请一个人，从这里开始。
      </p>{/if}
    {#each channels as entry}<button
        class="channel-row"
        class:selected={selected === entry.record.id}
        onclick={() => session.run(() => select(entry.record.id))}
        ><span class="avatar"><Icon name="chat" /></span><span
          ><strong>{entry.channel.name}</strong><small
            >{entry.channel.type === 'direct'
              ? '两人会话'
              : entry.channel.type === 'collaboration'
                ? '协作频道'
                : '分发频道'} · 本地草稿</small
          ></span
        ></button
      >{/each}
  </aside>
  {#if current}
    <section class="conversation">
      <header>
        <div>
          <h2>{current.channel.name}</h2>
          <p>{current.channel.members.length} 位拟邀请成员 · 等待建立频道</p>
        </div>
        <span class="pill">本地草稿</span>
      </header>
      <details class="member-details">
        <summary>查看成员与历史范围</summary>{#each current.channel.members as member}<p>
            <code>{shortId(member.subject)}</code><span
              >{member.role === 'owner' ? '创建者' : '成员'} · {member.accepted
                ? '本机'
                : '等待接受邀请'}</span
            >
          </p>{/each}
        <p>新成员默认只能读取加入后的新 epoch。旧历史需要单独授权。</p>
      </details>
      <div class="message-timeline" aria-label="消息草稿">
        {#if !messages.length}<div class="conversation-empty">
            <Icon name="chat" size={36} />
            <h3>先写下你想说的话。</h3>
            <p>草稿不会发给对方，也不触发任何链上调用。</p>
          </div>{/if}
        {#each messages as entry}<div class="message-item">
            <div class="message-meta">
              <strong>我</strong><time>{dateLabel(entry.message.createdAt)}</time>
            </div>
            <p>{entry.message.text}</p>
            <span class="caption"><Icon name="device" size={14} />本地加密草稿 · 未投递</span>
          </div>{/each}
      </div>
      <form
        class="composer"
        onsubmit={(event) => {
          event.preventDefault()
          void saveMessage()
        }}
      >
        <label class="sr-only" for="composer">消息正文</label><textarea
          id="composer"
          bind:value={composer}
          maxlength="24000"
          rows="3"
          disabled={session.busy}
          placeholder="写一条私密消息…"
          oninput={() => {
            void saveDraft().catch((e) => (session.error = e.message))
          }}></textarea>
        <div>
          <span class="caption">草稿在本机加密保存</span><button
            class="primary"
            disabled={!composer.trim() || session.busy}
            >保存消息草稿<Icon name="arrow-right" /></button
          >
        </div>
      </form>
    </section>
  {:else}<section class="conversation empty-state">
      <Icon name="chat" size={44} />
      <h2>只和你选定的人交谈。</h2>
      <p>两人会话、协作频道与分发频道。<br />加入频道不会取得秘密库权限。</p>
      <button class="secondary" onclick={() => (create = true)}
        >创建第一段对话<Icon name="arrow-right" /></button
      >
    </section>{/if}
</div>
{#if create}<Modal title="新建会话草稿" onclose={() => (create = false)}
    ><form
      onsubmit={(event) => {
        event.preventDefault()
        void session.run(async () => {
          const record = await session.crypto.call('createChannel', { name, type, recipient })
          await session.refresh()
          create = false
          name = ''
          recipient = ''
          await select(record.id)
        }, '会话草稿已建立，邀请尚未发送。')
      }}
    >
      <label
        >会话类型<select bind:value={type}
          ><option value="direct">两人会话</option><option value="collaboration"
            >协作频道 · 成员均可发布</option
          ><option value="distribution">分发频道 · 指定成员发布</option></select
        ></label
      ><label
        >会话名称<input
          bind:value={name}
          required
          maxlength="100"
          placeholder="例如：项目资料交流"
        /></label
      ><label
        >对方稳定主体 ID<input
          class="mono-input"
          bind:value={recipient}
          pattern={'[0-9a-f]{64}'}
          placeholder="64 位十六进制 ID"
          required
        /></label
      >
      <p class="caption">
        名称和头像不能证明对方身份。邀请必须由对方主动接受，不包含旧历史授权。
      </p>
      {#if session.error}<p class="form-error" role="alert">{session.error}</p>{/if}
      <div class="modal-actions">
        <button class="secondary" type="button" onclick={() => (create = false)}>取消</button
        ><button class="primary" disabled={session.busy}>建立草稿</button>
      </div>
    </form></Modal
  >{/if}
