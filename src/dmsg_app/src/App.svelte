<script lang="ts">
  import { onMount } from 'svelte'
  import { session } from './lib/session.svelte'
  import { isExtension } from './lib/config'
  import Icon from './lib/components/Icon.svelte'
  import Onboarding from './lib/components/Onboarding.svelte'
  import Vault from './lib/components/Vault.svelte'
  import Messages from './lib/components/Messages.svelte'
  import Signatures from './lib/components/Signatures.svelte'
  import Identity from './lib/components/Identity.svelte'
  import Settings from './lib/components/Settings.svelte'
  import Approval from './lib/components/Approval.svelte'
  const surface = document.body.dataset.surface ?? 'index'
  const navigation = [
    { id: 'vault', label: '秘密库', icon: 'lock', number: '01' },
    { id: 'messages', label: '消息', icon: 'chat', number: '02' },
    { id: 'signatures', label: '签名与授权', icon: 'signature', number: '03' },
    { id: 'identity', label: '身份', icon: 'user', number: '04' }
  ]
  let page = $state(surface === 'sidepanel' ? 'signatures' : 'vault')
  function navigate(id: string) {
    page = id
    location.hash = id
    session.message = ''
    session.error = ''
  }
  function openWorkspace() {
    if (isExtension())
      void chrome.tabs.create({ url: chrome.runtime.getURL(`index.html#${page}`) })
    else window.open(`index.html#${page}`)
  }
  onMount(() => {
    const syncHash = () => {
      const hash = location.hash.slice(1)
      if ([...navigation.map((n) => n.id), 'settings'].includes(hash)) page = hash
    }
    syncHash()
    window.addEventListener('hashchange', syncHash)
    void session.start().catch((error) => {
      session.error = error.message
      session.loaded = true
    })
    return () => window.removeEventListener('hashchange', syncHash)
  })
</script>

<svelte:head
  ><title>dMsg · {navigation.find((n) => n.id === page)?.label ?? '设置'}</title></svelte:head
>
<div
  class="app-shell"
  class:compact={surface === 'sidepanel'}
  class:unlocked={session.unlocked && session.meta?.recoveryChecked}
  class:approval-shell={surface === 'approve'}
>
  {#if session.unlocked && session.meta?.recoveryChecked && surface !== 'approve'}
    <aside class="sidebar">
      <a class="brand" href="#vault" onclick={() => navigate('vault')} aria-label="dMsg 秘密库"
        ><img src="./assets/private-gate.png" alt="" /><strong>dMsg</strong></a
      ><span class="sidebar-caption">你的私密工作台</span>
      <nav aria-label="主导航">
        {#each navigation as item}<button
            class:active={page === item.id}
            aria-current={page === item.id ? 'page' : undefined}
            aria-label={item.label}
            onclick={() => navigate(item.id)}
            ><Icon name={item.icon} /><span>{item.label}</span><small>{item.number}</small
            ></button
          >{/each}
      </nav>
      <div class="sidebar-bottom">
        <div class="sidebar-note">
          <Icon name="device" /><strong>内容留在你的掌握中。</strong>
          <p>本地版本<br />尚未连接云端服务</p>
        </div>
        <button
          class="settings-nav"
          class:active={page === 'settings'}
          onclick={() => navigate('settings')}
          ><Icon name="settings" /><span>设置</span></button
        >
        <div class="sidebar-user">
          <span class="avatar"
            >{session.data.profile?.name?.slice(0, 1).toUpperCase() || 'D'}</span
          >
          <div>
            <strong>{session.data.profile?.name || '我的空间'}</strong><span>本机已解锁</span>
          </div>
          <button class="icon-button" aria-label="锁定工作台" onclick={() => session.lock()}
            ><Icon name="lock" size={18} /></button
          >
        </div>
      </div>
    </aside>
  {/if}
  <div class="main-column">
    <header class="topbar">
      {#if !session.unlocked || !session.meta?.recoveryChecked || surface === 'approve'}<a
          href="index.html"
          class="brand"><img src="./assets/private-gate.png" alt="" /><strong>dMsg</strong></a
        >{:else}<div class="breadcrumb">
          <span>我的空间</span><span class="breadcrumb-slash">/</span><strong
            >{navigation.find((n) => n.id === page)?.label || '设置'}</strong
          >
        </div>{/if}
      <div class="topbar-right">
        <span class="connection-state"><span class="status-dot"></span>本机模式</span
        >{#if session.unlocked}<button
            class="icon-button"
            aria-label="立即锁定工作台"
            title="立即锁定"
            onclick={() => session.lock()}><Icon name="lock" /></button
          >{/if}{#if surface === 'sidepanel'}<button
            class="icon-button"
            aria-label="在全页打开工作台"
            onclick={openWorkspace}><Icon name="arrow-up-right" /></button
          >{/if}
      </div>
    </header>
    {#if !session.loaded}<main class="loading-screen">
        <div class="brand"><strong>dMsg</strong></div>
        <p role="status">正在打开本机工作台…</p>
      </main>
    {:else if !session.unlocked || !session.meta?.recoveryChecked}<main
        class="onboarding-main"
      >
        {#key session.lockEpoch}<Onboarding />{/key}
      </main>
    {:else if surface === 'approve'}<Approval />
    {:else}
      <main class="workspace-main" id="main-content">
        {#if session.busy}<div class="operation-progress" role="status">
            <span class="busy-dot"></span><span>{session.progress?.stage || '正在处理…'}</span
            >{#if session.progress && session.progress.total > 1}<progress
                value={session.progress.completed}
                max={session.progress.total}
              ></progress><span>{session.progress.completed} / {session.progress.total}</span
              >{/if}
          </div>{/if}
        {#if session.error}<div class="toast error" role="alert">
            <Icon name="info" /><span>{session.error}</span><button
              class="icon-button"
              aria-label="关闭错误提示"
              onclick={() => (session.error = '')}><Icon name="close" /></button
            >
          </div>{/if}
        {#if session.message}<div class="toast" role="status">
            <Icon name="check" /><span>{session.message}</span><button
              class="icon-button"
              aria-label="关闭提示"
              onclick={() => (session.message = '')}><Icon name="close" /></button
            >
          </div>{/if}
        {#if page === 'vault'}<Vault />{:else if page === 'messages'}<Messages
          />{:else if page === 'signatures'}<Signatures
          />{:else if page === 'identity'}<Identity />{:else}<Settings />{/if}
      </main>
    {/if}
  </div>
</div>
