<script lang="ts">
  import { onDestroy } from 'svelte'
  import {
    collectArchive,
    encodeArchive,
    pack,
    pairingFingerprint,
    parseOffer,
    sealTransfer,
    type Inventory
  } from '@dmsg/legacy'
  import { createLegacyReader } from './reader'
  import { authStore } from '$lib/stores/auth'
  let mode = $state<'Local' | 'ECDH' | 'VetKey'>('Local')
  let password = $state(''),
    offerText = $state(''),
    confirmed = $state(false)
  let report = $state<Inventory | null>(null),
    error = $state(''),
    busy = $state(false)
  let objectCount = $state(0)
  let abort: AbortController | null = null
  const offer = $derived.by(() => {
    try {
      const value = JSON.parse(offerText)
      return parseOffer(value, location.origin, value.target)
    } catch {
      return null
    }
  })
  onDestroy(() => {
    abort?.abort()
    password = ''
    report = null
  })
  async function run(exporting: boolean) {
    if (busy) return
    busy = true
    error = ''
    abort = new AbortController()
    try {
      const { reader, checkpoint } = await createLegacyReader(mode)
      try {
        if (exporting) {
          if (!offer || !confirmed) throw new Error('先在两端核对配对指纹与目标扩展 ID。')
          const archive = await collectArchive(
            reader,
            {
              password,
              salt: $authStore.identity.getPrincipal().toText(),
              keyId: pack('PANDA'),
              contextVersion: 1,
              keyName: mode === 'VetKey' ? 'key_1' : 'test_key_1'
            },
            abort.signal
          )
          const encoded = encodeArchive(archive)
          try {
            const encrypted = await sealTransfer(encoded, offer, {
              origin: location.origin,
              target: offer.target,
              fingerprint: pairingFingerprint(offer)
            })
            const url = URL.createObjectURL(
              new Blob([encrypted], { type: 'application/octet-stream' })
            )
            const a = document.createElement('a')
            a.href = url
            a.download = 'legacy.dmsg-migration'
            a.click()
            setTimeout(() => URL.revokeObjectURL(url), 30000)
            // Do not retain secret-bearing source objects in a reactive UI tree.
            objectCount = reader.inventory.objects.length
            report = {
              ...reader.inventory,
              objects: [],
              calls: [],
              gaps: reader.inventory.gaps
            }
          } finally {
            encoded.fill(0)
            archive.keys.forEach((k) => k.coseKey.fill(0))
          }
        } else {
          report = await reader.collect(abort.signal)
          objectCount = report.objects.length
        }
      } finally {
        checkpoint.close()
      }
    } catch (cause) {
      error = cause instanceof Error ? cause.message : '清点未完成。'
    } finally {
      password = ''
      busy = false
      abort = null
    }
  }
</script>

<section class="migration">
  <h2>只读清点与加密迁移</h2>
  <p>
    使用当前旧身份，显式选择原来的密钥模式。读取不会初始化、换钥、确认交换或标记已读。当前结果是预迁移快照，不能确认最终切换。
  </p>
  <p>当前身份：<code>{$authStore.identity.getPrincipal().toText()}</code></p>
  <label
    >旧密钥模式 <select bind:value={mode} disabled={busy}
      ><option>Local</option><option>ECDH</option><option>VetKey</option></select
    ></label
  >
  <button
    class="button"
    disabled={busy || $authStore.identity.getPrincipal().isAnonymous()}
    onclick={() => run(false)}>清点可读取的历史</button
  >
  {#if report}<p role="status">
      已读取 {objectCount} 项记录；{report.gaps.length} 项缺口。查询结果不等同于认证冻结快照。
    </p>
    <ul>
      {#each report.gaps.slice(0, 30) as gap}<li>
          <code>{gap.source}</code>：{gap.code} · {gap.detail}
        </li>{/each}
    </ul>
  {/if}
  <p>
    已读取的消息页加密保存在此浏览器独立迁移缓存中；重新打开后可继续。原始数据库保持只读。旧服务变化会明确报告差异。
  </p>
  <button
    class="button"
    disabled={busy}
    onclick={async () => {
      const { checkpoint } = await createLegacyReader(mode)
      try {
        await checkpoint.clear()
        report = null
        objectCount = 0
      } finally {
        checkpoint.close()
      }
    }}>清除迁移缓存并重新清点</button
  >
  <label
    >扩展生成的配对请求<textarea
      bind:value={offerText}
      oninput={() => (confirmed = false)}
      rows="4"
      placeholder="从扩展设置的旧版迁移页复制配对请求"></textarea></label
  >
  {#if offer}<p>目标：<code>{offer.target}</code></p>
    <p>指纹：<code>{pairingFingerprint(offer)}</code></p>
    <label
      ><input type="checkbox" bind:checked={confirmed} /> 已与扩展显示的指纹、旧身份和目标标识核对一致</label
    >{/if}
  {#if mode !== 'VetKey'}<label
      >原来的解锁口令<input type="password" bind:value={password} autocomplete="off" /></label
    >{/if}
  <button
    class="button primary"
    disabled={busy || !offer || !confirmed || $authStore.identity.getPrincipal().isAnonymous()}
    onclick={() => run(true)}>解锁并生成加密迁移文件</button
  >
  {#if busy}<button class="button" onclick={() => abort?.abort()}>停止读取</button>{/if}
  {#if error}<p role="alert">{error}</p>{/if}
  <p>
    文件仅能由本次配对的扩展解密；配对有效期十分钟。保留原浏览器数据和旧服务，直到独立恢复验证完成。缺钥、权限与未完成上传会单独列出。
  </p>
</section>

<style>
  .migration {
    padding: 32px 0;
    border-top: 1px solid #cdd8cf;
    max-width: 780px;
  }
  label {
    display: block;
    margin: 16px 0;
  }
  textarea {
    display: block;
    width: 100%;
  }
  code {
    overflow-wrap: anywhere;
  }
  button {
    margin: 8px 12px 8px 0;
  }
</style>
