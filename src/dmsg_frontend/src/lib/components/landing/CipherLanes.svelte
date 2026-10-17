<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '$lib/i18n'
  import Glyph from './Glyph.svelte'
  import { prefersReducedMotion } from './motion'

  const ACCOUNT = 'acc_5q2w…h7'
  const DEVICE = 'dev_9c1e…40'
  const ALPHABET =
    'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_'

  const id = $props.id()
  let edited = $state<string | null>(null)
  const text = $derived(
    edited ?? $t('Ship v1.4 on Friday. The deploy key is in the vault.')
  )
  let shown = $state('')
  let bytes = $state(0)
  let stamp = $state('')
  let key: CryptoKey | null = null
  let frame = 0
  let revision = 0

  function encode(data: Uint8Array): string {
    let out = ''
    for (const byte of data) out += String.fromCharCode(byte)
    return btoa(out).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
  }

  // Real AES-GCM under a throwaway key, so the length you see is honest.
  async function seal(value: string): Promise<string> {
    const plain = new TextEncoder().encode(value)
    const iv = crypto.getRandomValues(new Uint8Array(12))
    let body: Uint8Array
    try {
      key ??= await crypto.subtle.generateKey(
        { name: 'AES-GCM', length: 256 },
        false,
        ['encrypt']
      )
      body = new Uint8Array(
        await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, key, plain)
      )
    } catch {
      body = crypto.getRandomValues(new Uint8Array(plain.length + 16))
    }
    const sealed = new Uint8Array(iv.length + body.length)
    sealed.set(iv)
    sealed.set(body, iv.length)
    bytes = sealed.length
    return encode(sealed)
  }

  function scramble(target: string) {
    cancelAnimationFrame(frame)
    if (prefersReducedMotion() || document.visibilityState === 'hidden') {
      shown = target
      return
    }
    const start = performance.now()
    const step = (now: number) => {
      const done = Math.min((now - start) / 420, 1)
      let out = ''
      for (let i = 0; i < target.length; i++)
        out +=
          Math.random() < done
            ? target[i]
            : ALPHABET[(Math.random() * ALPHABET.length) | 0]
      shown = out
      if (done < 1) frame = requestAnimationFrame(step)
    }
    frame = requestAnimationFrame(step)
  }

  $effect(() => {
    const value = text
    const mine = ++revision
    const timer = setTimeout(async () => {
      const sealed = await seal(value)
      if (mine !== revision) return
      stamp = new Date().toTimeString().slice(0, 8)
      scramble(sealed)
    }, 90)
    return () => clearTimeout(timer)
  })

  onMount(() => () => cancelAnimationFrame(frame))
</script>

<div class="lanes">
  <div class="lane lane-device">
    <label class="lane-head" for={id}
      ><Glyph name="laptop" size={18} /><strong>{$t('On your device')}</strong
      ></label
    >
    <small>{$t('Readable, and only while unlocked')}</small>
    <textarea
      {id}
      value={text}
      oninput={(event) => (edited = event.currentTarget.value)}
      maxlength="160"
      rows="4"
      spellcheck="false"></textarea>
    <span class="lane-hint"
      >{$t('Type anything. It never leaves this page.')}</span
    >
  </div>

  <div class="lane lane-cloud">
    <span class="lane-head"
      ><Glyph name="cloud" size={18} /><strong>{$t('In the dMsg cloud')}</strong
      ></span
    >
    <small>{$t('Ciphertext, plus what it takes to deliver it')}</small>
    <code class="cipher" aria-label={$t('Ciphertext')}>{shown}</code>
    <ul class="meta">
      <li><span>{$t('account')}</span><code>{ACCOUNT}</code></li>
      <li><span>{$t('device')}</span><code>{DEVICE}</code></li>
      <li class="size">
        <span>{$t('size')}</span><code>{$t('{bytes} bytes', { bytes })}</code>
      </li>
      <li><span>{$t('time')}</span><code>{stamp}</code></li>
    </ul>
  </div>

  <div class="lane lane-icp">
    <span class="lane-head"
      ><Glyph name="badge" size={18} /><strong>{$t('On ICP')}</strong></span
    >
    <small>{$t('Never your messages or files')}</small>
    <ul class="ledger">
      <li><Glyph name="key" size={14} />{$t('Account, devices and logins')}</li>
      <li
        ><Glyph name="lock" size={14} />{$t(
          'A commitment to your content root'
        )}</li
      >
      <li
        ><Glyph name="pen" size={14} />{$t(
          'Signature receipts, after you approve'
        )}</li
      >
      <li><Glyph name="vault" size={14} />{$t('Settled payments and names')}</li
      >
      <li
        ><Glyph name="coins" size={14} />{$t(
          'Team wallets, their proposals and approvals'
        )}</li
      >
    </ul>
  </div>
</div>
<p class="lanes-note">
  {$t(
    'Encrypted in your browser with a throwaway AES-GCM key, for illustration; dMsg’s own format differs. Notice the size: encryption hides content, not length or timing.'
  )}
</p>
