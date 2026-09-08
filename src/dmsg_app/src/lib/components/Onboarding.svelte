<script lang="ts">
  import { session, downloadBlob, shortId } from '../session.svelte'
  import { config } from '../config'
  import Icon from './Icon.svelte'
  let screen = $state<'intro' | 'setup' | 'recovery' | 'check' | 'restore'>('intro')
  let password = $state(''),
    confirmPassword = $state(''),
    recoveryCode = $state(''),
    checkCode = $state('')
  let backupGenerated = $state(false),
    savedApart = $state(false),
    restoreFile = $state<File | null>(null)
  const needsSetup = $derived(!session.initialized)
  async function setup() {
    await session.run(async () => {
      if (password !== confirmPassword) throw new Error('两次口令不一致。')
      const result = await session.crypto.call('initialize', password)
      session.activate(result.meta)
      recoveryCode = result.recoveryCode
      screen = 'recovery'
      // Keep the password only through the initial export, then clear it.
      confirmPassword = ''
    })
  }
  async function initialBackup() {
    await session.run(async () => {
      const backup = await session.crypto.call('exportBackup', password)
      downloadBlob(backup.blob, backup.name)
      backupGenerated = true
      password = ''
    }, '恢复包已生成并开始下载。请确认文件已保存。')
  }
  async function verify() {
    await session.run(async () => {
      await session.crypto.call('verifyRecovery', checkCode)
      checkCode = ''
      recoveryCode = ''
      await session.refresh()
    }, '恢复码验证通过。现在可以保存第一个条目。')
  }
  async function unlock() {
    const value = password
    password = ''
    await session.run(async () => {
      await session.unlock(value)
      if (!session.meta?.recoveryChecked) {
        const pending = await session.crypto.call('pendingRecovery')
        recoveryCode = pending.recoveryCode
        backupGenerated = pending.backupGenerated
        savedApart = false
        password = value
        screen = 'recovery'
      }
    })
  }
  async function restore() {
    await session.run(async () => {
      if (!restoreFile) throw new Error('请选择加密恢复包。')
      if (password !== confirmPassword) throw new Error('两次口令不一致。')
      const result = await session.crypto.call('restore', {
        file: restoreFile,
        code: checkCode,
        password
      })
      password = ''
      confirmPassword = ''
      checkCode = ''
      restoreFile = null
      session.activate(result.meta)
      await session.refresh()
      session.message = result.missing.length
        ? `已恢复可验证内容；仍有 ${result.missing.length} 项缺口。`
        : `已验证并恢复 ${result.count} 个版本。本机尚未获得链上设备授权。`
    })
  }
</script>

<div class="welcome">
  <div class="welcome-copy">
    <span class="eyebrow">YOUR SPACE. YOUR SAY.</span>
    <h1>你的空间，<br />你来决定。</h1>
    <p class="lead">把秘密留给自己，<br />把信任交给你选定的人。</p>
    <div class="welcome-principles">
      <p><span>01</span> 私密内容，在本机加密</p>
      <p><span>02</span> 每一次授权，都由你确认</p>
      <p><span>03</span> 带着备份，独立恢复</p>
    </div>
    <p class="fine-print">Built by ICPanda DAO</p>
  </div>
  <section class="welcome-panel" aria-label="建立或解锁工作台">
    <div class="panel-symbol">
      <Icon name={screen === 'restore' ? 'refresh' : 'lock'} size={28} />
    </div>
    {#if screen === 'restore'}
      <span class="eyebrow">RECOVER YOUR SPACE</span>
      <h2>从备份回到这里。</h2>
      <p>需要完整的加密恢复包和分开保存的恢复码。恢复数据不会自动获得链上设备权限。</p>
      <form
        onsubmit={(event) => {
          event.preventDefault()
          void restore()
        }}
      >
        <label
          >加密恢复包<input
            type="file"
            accept=".dmsg,application/json"
            required
            onchange={(event) => (restoreFile = event.currentTarget.files?.[0] ?? null)}
          /></label
        >
        <label
          >恢复码<textarea
            bind:value={checkCode}
            autocomplete="off"
            spellcheck="false"
            required
            rows="3"
            placeholder="8 组，每组 8 位十六进制字符"></textarea></label
        >
        <label
          >设置本机新口令<input
            type="password"
            bind:value={password}
            required
            minlength="12"
            autocomplete="new-password"
          /></label
        >
        <label
          >确认新口令<input
            type="password"
            bind:value={confirmPassword}
            required
            minlength="12"
            autocomplete="new-password"
          /></label
        >
        <button class="primary wide" disabled={session.busy}
          >{session.busy ? '正在验证恢复包…' : '验证并恢复内容'}<Icon
            name="arrow-right"
          /></button
        >
      </form>
      <button
        class="text-button"
        onclick={() => {
          screen = 'intro'
          password = ''
          confirmPassword = ''
          checkCode = ''
          restoreFile = null
        }}>返回</button
      >
    {:else if screen === 'recovery'}
      <span class="eyebrow">02 / RECOVERY</span>
      <h2>给自己留一条归路。</h2>
      <p>恢复码能解密备份，请与恢复包分开保管。完成验证前，重新解锁仍可继续此步骤。</p>
      <div class="recovery-code"><code>{recoveryCode}</code></div>
      <p class="caption">恢复码持有人可立即解密已有备份。账户恢复延迟不会阻止离线解密。</p>
      <button
        class="secondary wide"
        onclick={initialBackup}
        disabled={session.busy || backupGenerated}
        ><Icon name="download" />{backupGenerated
          ? '初始恢复包已生成'
          : '下载初始恢复包'}</button
      >
      <label class="check-label"
        ><input
          type="checkbox"
          bind:checked={savedApart}
        />我已确认下载文件，并另行保存恢复码</label
      >
      <button
        class="primary wide"
        disabled={!savedApart || !backupGenerated}
        onclick={() => {
          recoveryCode = ''
          password = ''
          screen = 'check'
        }}>验证恢复码<Icon name="arrow-right" /></button
      >
    {:else if screen === 'check' || (session.unlocked && !session.meta?.recoveryChecked)}
      <span class="eyebrow">03 / VERIFY RECOVERY</span>
      <h2>确认你能找回内容。</h2>
      <p>从刚才保存的位置取回恢复码。验证通过后，再存入重要内容。</p>
      <form
        onsubmit={(event) => {
          event.preventDefault()
          void verify()
        }}
      >
        <label
          >输入已保存的恢复码<textarea
            bind:value={checkCode}
            required
            rows="3"
            autocomplete="off"
            spellcheck="false"></textarea></label
        >
        <button class="primary wide" disabled={session.busy}
          >验证并进入工作台<Icon name="arrow-right" /></button
        >
      </form>
    {:else if !needsSetup}
      <span class="eyebrow">WELCOME BACK</span>
      <h2>欢迎回到你的空间。</h2>
      <p>解锁本机内容。登录与正式签名仍需分别授权。</p>
      <form
        onsubmit={(event) => {
          event.preventDefault()
          void unlock()
        }}
      >
        <label
          >本机解锁口令<input
            type="password"
            bind:value={password}
            required
            autocomplete="current-password"
          /></label
        >
        <button class="primary wide" disabled={session.busy}
          >{session.busy ? '正在解锁…' : '解锁工作台'}<Icon name="arrow-right" /></button
        >
      </form>
      <p class="caption">为避免隐藏当前数据，请在空白 Chrome 配置中导入恢复包。</p>
      {#if session.meta}<div class="identity-line">
          <span>本机设备</span><code>{shortId(session.meta.deviceId)}</code>
        </div>{/if}
    {:else if screen === 'setup'}
      <span class="eyebrow">01 / YOUR DEVICE</span>
      <h2>从一个独立口令开始。</h2>
      <p>口令只用于本机解锁。无需购买名称，也无需持有代币。</p>
      <form
        onsubmit={(event) => {
          event.preventDefault()
          void setup()
        }}
      >
        <label
          >设置本机口令<input
            type="password"
            bind:value={password}
            required
            minlength="12"
            autocomplete="new-password"
            placeholder="至少 12 个字符"
          /></label
        >
        <label
          >再次输入口令<input
            type="password"
            bind:value={confirmPassword}
            required
            minlength="12"
            autocomplete="new-password"
          /></label
        >
        <button class="primary wide" disabled={session.busy}
          >{session.busy ? '正在建立工作台…' : '建立加密工作台'}<Icon
            name="arrow-right"
          /></button
        >
      </form>
      <button
        class="text-button"
        onclick={() => {
          screen = 'intro'
          password = ''
          confirmPassword = ''
        }}>返回</button
      >
    {:else}
      <span class="eyebrow">A PRIVATE WORKSPACE</span>
      <h2>从一件私密的事开始。</h2>
      <p>保存笔记、凭据和文件。即使离线，你的内容仍可在本机取用。</p>
      <div class="notice">
        <Icon name="info" />
        <p>当前为 R0 本地版本。云端同步、正式签名与付费来信尚未启用。</p>
      </div>
      <button class="primary wide" onclick={() => (screen = 'setup')}
        >创建我的工作台<Icon name="arrow-right" /></button
      >
      <button class="secondary wide" onclick={() => (screen = 'restore')}
        >从加密备份恢复</button
      >
      <p class="caption">本地数据库不是唯一备份。设置过程中会生成恢复材料。</p>
    {/if}
    {#if session.progress}<div class="progress-area" role="status">
        <span>{session.progress.stage}</span><progress
          max={session.progress.total || 1}
          value={session.progress.completed}
        ></progress>
      </div>{/if}
    {#if session.error}<p class="form-error" role="alert">{session.error}</p>{/if}
    {#if session.message}<p class="form-success" role="status">{session.message}</p>{/if}
    <div class="panel-foot">
      <span class="status-dot"></span>{config.environment === 'local'
        ? '本地加密 · 无云端交付'
        : `${config.environment} · 需通过服务验证`}
    </div>
  </section>
</div>
