<script lang="ts">
  import { sha256 } from '@noble/hashes/sha2.js'
  import { hex } from '../protocol/codec'
  import { MAX_FILE, CHUNK_SIZE } from '../config'
  import { formatBytes } from '../session.svelte'
  import Icon from './Icon.svelte'
  let expected = $state(''),
    digest = $state(''),
    name = $state(''),
    size = $state(0),
    busy = $state(false),
    error = $state('')
  async function choose(event: Event) {
    const file = (event.currentTarget as HTMLInputElement).files?.[0]
    if (!file) return
    digest = ''
    error = ''
    name = ''
    busy = true
    const state = sha256.create()
    try {
      if (file.size > MAX_FILE) throw new Error('文件上限为 100 MiB。')
      for (let offset = 0; offset < file.size; offset += CHUNK_SIZE)
        state.update(
          new Uint8Array(await file.slice(offset, offset + CHUNK_SIZE).arrayBuffer())
        )
      digest = hex(state.digest())
      name = file.name
      size = file.size
    } catch (e) {
      error = e instanceof Error ? e.message : '文件读取失败。'
    } finally {
      state.destroy()
      busy = false
    }
  }
</script>

<section class="verify-panel">
  <div class="section-heading">
    <Icon name="file" size={24} />
    <div>
      <h2>核对文件指纹</h2>
      <p>在本机计算，不上传原文件。</p>
    </div>
  </div>
  <label>选择本地文件<input type="file" onchange={choose} disabled={busy} /></label><label
    >待核对的 SHA-256<span class="label-hint">可选</span><input
      class="mono-input"
      bind:value={expected}
      maxlength="64"
      placeholder="粘贴 64 位十六进制摘要"
    /></label
  >
  {#if busy}<p role="status">正在读取文件并计算摘要…</p>{/if}{#if error}<p
      class="form-error"
      role="alert"
    >
      {error}
    </p>{/if}
  {#if digest}<div class="verification-result">
      <strong>{name}</strong><span>{formatBytes(size)}</span><code class="hash">{digest}</code>
      <dl class="evidence-list">
        <div>
          <dt>内容一致性</dt>
          <dd>
            {expected.trim()
              ? expected.trim().toLowerCase() === digest
                ? '摘要匹配'
                : '摘要不匹配'
              : '已计算，未提供待比对摘要'}
          </dd>
        </div>
        <div>
          <dt>数学签名</dt>
          <dd>未提供签名</dd>
        </div>
        <div>
          <dt>主体绑定</dt>
          <dd>未知</dd>
        </div>
        <div>
          <dt>签署时授权</dt>
          <dd>未知</dd>
        </div>
        <div>
          <dt>当前授权</dt>
          <dd>未知</dd>
        </div>
      </dl>
    </div>{/if}
  <p class="caption">指纹一致只说明文件字节一致，不能证明作者身份、项目权限或文件可信。</p>
</section>
