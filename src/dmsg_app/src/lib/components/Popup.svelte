<script lang="ts">
  import { onMount } from 'svelte'
  import { currentWorkspace, WorkspaceDB } from '../db'
  import { listRequests } from '../requests'
  import { isExtension } from '../config'
  import Icon from './Icon.svelte'
  let unlocked = $state(false),
    count = $state(0),
    exists = $state(false)
  onMount(() => {
    void (async () => {
      const name = await currentWorkspace()
      exists = !!name
      if (name) {
        const db = await WorkspaceDB.open(name)
        const lease = await db.db.get('meta', 'crypto-owner')
        unlocked = !!lease && lease.expiresAt > Date.now()
        db.db.close()
      }
      count = (await listRequests()).filter((r) => r.state === 'awaiting_user').length
    })()
  })
  function openWorkspace() {
    if (isExtension()) void chrome.tabs.create({ url: chrome.runtime.getURL('index.html') })
    else window.open('index.html')
    window.close()
  }
  async function sidePanel() {
    if (isExtension()) {
      const window = await chrome.windows.getCurrent()
      if (window.id !== undefined) await chrome.sidePanel.open({ windowId: window.id })
    } else window.open('sidepanel.html')
    window.close()
  }
  async function lock() {
    const name = await currentWorkspace()
    if (name) {
      const db = await WorkspaceDB.open(name)
      await db.invalidate()
      db.db.close()
    }
    const channel = new BroadcastChannel('dmsg-session')
    channel.postMessage('lock')
    channel.close()
    unlocked = false
  }
</script>

<main class="popup">
  <div class="brand"><img src="./assets/private-gate.png" alt="" /><strong>dMsg</strong></div>
  <div class="popup-status">
    <span class="item-icon"><Icon name="lock" size={24} /></span>
    <h1>{unlocked ? '工作台已解锁' : exists ? '你的空间已锁定' : '留一处私密空间'}</h1>
    <p>{unlocked ? '内容仅在解锁页面中可用。' : '打开工作台，开始一件私密的事。'}</p>
  </div>
  <div class="popup-count"><span>待确认的请求</span><strong>{count}</strong></div>
  <button class="primary wide" onclick={openWorkspace}
    >打开工作台<Icon name="arrow-up-right" /></button
  ><button class="secondary wide" onclick={sidePanel}>打开侧栏<Icon name="menu" /></button
  >{#if unlocked}<button class="text-button wide" onclick={lock}
      ><Icon name="lock" />立即锁定</button
    >{/if}
  <p class="caption popup-foot">
    本地版本 · {unlocked ? '15 分钟自动锁定' : '重新打开需要解锁'}
  </p>
</main>
