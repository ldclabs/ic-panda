<script lang="ts">
  import { session, shortId } from '../session.svelte'
  import Icon from './Icon.svelte'
  let name = $state(session.data.profile?.name ?? ''),
    bio = $state(session.data.profile?.bio ?? ''),
    link = $state(session.data.profile?.link ?? ''),
    contact = $state<'closed' | 'invitation'>(session.data.profile?.contact ?? 'closed')
  let publishName = $state(session.data.profile?.publicFields.includes('name') ?? true),
    publishBio = $state(session.data.profile?.publicFields.includes('bio') ?? false),
    publishLink = $state(session.data.profile?.publicFields.includes('link') ?? false)
</script>

<div class="page-heading">
  <div>
    <span class="eyebrow">BE REACHABLE. ON YOUR TERMS.</span>
    <h1>身份</h1>
    <p>你的公开入口，由你决定展示什么。</p>
  </div>
  <span class="pill">本地资料草稿</span>
</div>
<div class="identity-layout">
  <section class="identity-form">
    <h2>公开资料</h2>
    <p>选中的字段将在你明确发布后公开。当前只保存到本机。</p>
    <form
      onsubmit={(event) => {
        event.preventDefault()
        void session.run(async () => {
          await session.crypto.call('saveProfile', {
            name,
            bio,
            link,
            contact,
            publicFields: [
              ...(publishName ? ['name'] : []),
              ...(publishBio ? ['bio'] : []),
              ...(publishLink ? ['link'] : [])
            ]
          })
          await session.refresh()
        }, '资料草稿已加密保存，尚未公开发布。')
      }}
    >
      <label
        >显示名<input
          bind:value={name}
          maxlength="100"
          required
          placeholder="你希望别人如何称呼你"
        /></label
      ><label class="check-label"
        ><input type="checkbox" bind:checked={publishName} />公开显示名</label
      >
      <label
        >简介<textarea bind:value={bio} rows="3" maxlength="500" placeholder="介绍你正在做的事"
        ></textarea></label
      ><label class="check-label"
        ><input type="checkbox" bind:checked={publishBio} />公开简介</label
      >
      <label
        >个人链接<input
          bind:value={link}
          type="url"
          maxlength="2048"
          placeholder="https://…"
        /></label
      ><label class="check-label"
        ><input type="checkbox" bind:checked={publishLink} />公开链接</label
      >
      <div class="section-divider"></div>
      <h2>来信规则</h2>
      <p>默认关闭陌生来信。支付不会获得阅读、回复或持续联系的承诺。</p>
      <label class="radio-card"
        ><input type="radio" value="closed" bind:group={contact} /><span
          ><strong>关闭陌生来信</strong><small>只在已有会话中交流</small></span
        ><Icon name="lock" /></label
      >
      <label class="radio-card"
        ><input type="radio" value="invitation" bind:group={contact} /><span
          ><strong>仅限邀请 / 联系人</strong><small>由你主动决定联系范围</small></span
        ><Icon name="user" /></label
      >
      <div class="disabled-option">
        <Icon name="key" />
        <div>
          <strong>付费来信</strong>
          <p>在 R1c 资金与退款流程验收后开放。</p>
        </div>
      </div>
      <button class="primary" disabled={session.busy}>保存资料草稿<Icon name="check" /></button
      >
    </form>
  </section>
  <aside class="profile-preview">
    <span class="eyebrow">访客预览 · 尚未发布</span>
    <div class="preview-avatar">
      {publishName && name ? name.slice(0, 1).toUpperCase() : 'D'}
    </div>
    <h2>{publishName && name ? name : '私密工作台用户'}</h2>
    {#if publishBio && bio}<p>{bio}</p>{/if}{#if publishLink && link}<p
        class="preview-link break-anywhere"
      >
        {link}
      </p>{/if}
    <div class="profile-rule">
      <Icon name={contact === 'closed' ? 'lock' : 'user'} /><span
        >{contact === 'closed' ? '陌生来信已关闭' : '通过邀请建立联系'}</span
      >
    </div>
    <div class="section-divider"></div>
    <span class="caption">稳定主体 · 本地标识</span><code class="hash"
      >{session.meta?.subjectId}</code
    >
    <p class="caption">名称可改变，主体与内容不随名称转移。</p>
    <span class="preview-brand">dMsg <span>你的空间，你来决定。</span></span>
  </aside>
</div>
