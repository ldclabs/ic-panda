<script lang="ts">
  import { onMount, tick } from 'svelte'
  import { t, tr } from '$lib/i18n'
  import Glyph from './Glyph.svelte'
  import type { GlyphName } from './glyphs'
  import { prefersReducedMotion, watchVisibility } from './motion'

  // One simulated daemon behind both panes, as on a real device.
  type Kind = 'api' | 'login' | 'note' | 'key'
  type Item = {
    id: string
    key?: string
    title?: string
    kind: Kind
    tag: string
    user?: string
    secret?: string
    fresh?: boolean
  }
  type Line = {
    kind: 'cmd' | 'out' | 'ok' | 'err' | 'dim' | 'ask'
    text: string
  }
  type Approval = {
    id: string
    file: string
    text: string
    hash: string
    state: 'waiting' | 'approved' | 'rejected'
    left: number
  }

  const SECRET = 'dmsg_demo_key_not_a_secret'
  const PROMPT = '❯'
  const HELP = 'dmsg help'
  const TITLE = 'zsh — dmsg'
  const NAME = 'dMsg'
  const CLI = 'cli'
  const DAEMON = 'dmsg daemon'
  const ACCOUNT = 'acc_5q2w…h7'
  const SUGGESTIONS = [
    'dmsg status',
    'dmsg vault add --title "Stripe key" --type api --tag billing',
    'dmsg attest file build-1.4.tar.gz --text "v1.4 release"',
    'dmsg lock',
    'dmsg unlock --fast'
  ]
  const KIND_GLYPH: Record<Kind, GlyphName> = {
    api: 'key',
    login: 'shield',
    note: 'file',
    key: 'key'
  }
  const NAV: { glyph: GlyphName; key: string }[] = [
    { glyph: 'vault', key: 'Vault' },
    { glyph: 'folder', key: 'Files' },
    { glyph: 'chat', key: 'Channels' },
    { glyph: 'inbox', key: 'Inbox' },
    { glyph: 'pen', key: 'Signatures' },
    { glyph: 'robot', key: 'Agents' },
    { glyph: 'coins', key: 'Team wallets' },
    { glyph: 'settings', key: 'Settings' }
  ]

  let locked = $state(false)
  let items = $state<Item[]>([
    {
      id: 'itm_7c1',
      key: 'Deploy key',
      kind: 'api',
      tag: 'deploy',
      secret: SECRET
    },
    {
      id: 'itm_3a9',
      key: 'Server login',
      kind: 'login',
      tag: 'ops',
      user: 'alex',
      secret: 'correct-horse-demo'
    },
    { id: 'itm_b02', key: 'Release plan', kind: 'note', tag: 'release' },
    {
      id: 'itm_e44',
      key: 'Signing key',
      kind: 'key',
      tag: 'release',
      secret: 'dmsg_demo_signing_key'
    }
  ])
  let selected = $state('itm_7c1')
  let approval = $state<Approval | null>(null)
  let armed = $state<'' | 'show' | 'copy'>('')
  let shownFor = $state(0)
  let copiedFor = $state(0)
  let lines = $state<Line[]>([])
  let input = $state('')
  let typing = $state(false)
  let asking = $state(false)
  let flow = $state<'' | 'to-window' | 'to-terminal'>('')
  let stacked = $state(false)
  let geometry = $state({ win: 0, term: 0 })

  let root: HTMLDivElement
  let winPane: HTMLDivElement
  let termPane: HTMLDivElement
  let screen: HTMLDivElement
  let history: string[] = []
  let recall = -1
  let flowTimer: ReturnType<typeof setTimeout> | undefined
  let armTimer: ReturnType<typeof setTimeout> | undefined

  const current = $derived(items.find((item) => item.id === selected))
  const pending = $derived(items.some((item) => item.fresh))
  const kindLabel = $derived<Record<Kind, string>>({
    api: $t('API key'),
    login: $t('Login'),
    note: $t('Note'),
    key: $t('Key')
  })
  const titleOf = (item: Item) => (item.key ? tr(item.key) : item.title!)
  const clock = (seconds: number) =>
    `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`
  const sleep = (ms: number) =>
    new Promise((resolve) => setTimeout(resolve, ms))

  function hex(seed: string, size: number): string {
    let out = ''
    let h = 2166136261
    for (let round = 0; out.length < size; round++) {
      for (const ch of seed + round)
        h = Math.imul(h ^ ch.charCodeAt(0), 16777619)
      out += (h >>> 0).toString(16).padStart(8, '0')
    }
    return out.slice(0, size)
  }

  function signal(direction: 'to-window' | 'to-terminal') {
    flow = ''
    clearTimeout(flowTimer)
    requestAnimationFrame(() => {
      flow = direction
      flowTimer = setTimeout(() => (flow = ''), 900)
    })
  }

  function print(kind: Line['kind'], text = '') {
    lines.push({ kind, text })
    if (lines.length > 120) lines.splice(0, lines.length - 120)
    tick().then(() => screen && (screen.scrollTop = screen.scrollHeight))
  }

  function table(rows: string[][]) {
    const widths: number[] = []
    for (const row of rows)
      row.forEach(
        (cell, c) => (widths[c] = Math.max(widths[c] ?? 0, cell.length))
      )
    for (const [i, row] of rows.entries())
      print(
        i ? 'out' : 'dim',
        row
          .map((cell, c) => cell.padEnd((widths[c] ?? 0) + 2))
          .join('')
          .trimEnd()
      )
  }

  function words(command: string): string[] {
    return [...command.matchAll(/"([^"]*)"|'([^']*)'|(\S+)/g)].map(
      (m) => m[1] ?? m[2] ?? m[3] ?? ''
    )
  }

  function option(args: string[], name: string): string | undefined {
    const i = args.indexOf(name)
    return i >= 0 ? args[i + 1] : undefined
  }

  function lockedError(): boolean {
    if (!locked) return false
    print('err', 'error: the workspace is locked (LOCKED)')
    print('dim', 'unlock it with: dmsg unlock --fast')
    return true
  }

  function setLocked(value: boolean, from: 'window' | 'terminal') {
    locked = value
    armed = ''
    shownFor = 0
    signal(from === 'window' ? 'to-terminal' : 'to-window')
  }

  function settle(approved: boolean, from: 'window' | 'terminal') {
    if (!approval || approval.state !== 'waiting') return
    approval.state = approved ? 'approved' : 'rejected'
    asking = false
    if (from === 'window') {
      signal('to-terminal')
      print(
        'dim',
        approved
          ? 'approved in the approval window'
          : 'rejected in the approval window'
      )
    }
    if (approved) {
      print('ok', `✓ approved · signature generated`)
      print('out', `  execution  exe_${hex(approval.id, 6)}`)
      print('out', `  receipt    certified by ICP`)
    } else print('err', '✗ rejected · nothing was signed')
    const id = approval.id
    setTimeout(() => {
      if (approval?.id === id) approval = null
    }, 2600)
  }

  async function run(command: string) {
    const text = command.trim()
    print('cmd', text)
    if (!text) return
    history.unshift(text)
    recall = -1
    if (asking) {
      settle(/^y(es)?$/i.test(text), 'terminal')
      if (approval) signal('to-window')
      return
    }
    const args = words(text)
    if (args[0] === 'clear') {
      lines = []
      return
    }
    if (args[0] !== 'dmsg') {
      if (args[0] === 'sudo' || args[0] === 'rm')
        print('err', 'this terminal only knows dmsg')
      else print('err', `zsh: command not found: ${args[0]}`)
      return
    }
    const [, verb, sub] = args
    if (!verb || verb === 'help' || verb === '--help') {
      print('out', 'dmsg — the dMsg command line (a few commands of this demo)')
      const help: [string, string][] = [
        ['status', 'the daemon and this workspace'],
        ['vault list', 'items in the vault'],
        ['vault show <id> [--reveal]', 'one item'],
        ['vault add --title T [--type api|login|note|key] [--tag T]', ''],
        ['lock | unlock [--fast]', 'forget or open the keys'],
        ['attest file <path> --text T', 'ask for your approval to sign'],
        ['approvals list', 'requests waiting for a person'],
        ['mcp', 'serve AI agents over stdio']
      ]
      for (const [command, about] of help)
        print('out', `  ${command.padEnd(28)}${about}`)
      return
    }
    if (verb === 'status') {
      const waiting = approval?.state === 'waiting' ? 1 : 0
      print('out', 'daemon     running · 127.0.0.1:52817')
      print('out', 'account    acc_5q2w…h7 · Alex')
      print(
        locked ? 'err' : 'ok',
        `session    ${locked ? 'locked' : 'unlocked · locks after 15 idle minutes'}`
      )
      print(
        'out',
        `vault      ${items.length} items · ${items.filter((i) => i.fresh).length} waiting to sync`
      )
      print('out', `approvals  ${waiting} waiting`)
      return
    }
    if (verb === 'lock') {
      if (!locked) setLocked(true, 'terminal')
      print('ok', 'locked · keys cleared from memory')
      return
    }
    if (verb === 'unlock') {
      if (!locked) {
        print('dim', 'already unlocked')
        return
      }
      if (args.includes('--fast')) {
        print('dim', 'Touch ID…')
        await sleep(650)
      } else {
        print('dim', 'opening https://dmsg.net/_/login in your browser…')
        await sleep(900)
        print('dim', 'signed in with Internet Identity')
      }
      setLocked(false, 'terminal')
      print('ok', 'unlocked · synced')
      return
    }
    if (verb === 'vault' && sub === 'list') {
      if (lockedError()) return
      table([
        ['ID', 'TYPE', 'TAG', 'TITLE'],
        ...items.map((item) => [item.id, item.kind, item.tag, titleOf(item)])
      ])
      return
    }
    if (verb === 'vault' && sub === 'show') {
      if (lockedError()) return
      const item = items.find((i) => i.id === args[3])
      if (!item) {
        print('err', `error: no item ${args[3] ?? '<id>'} (NOT_FOUND)`)
        return
      }
      selected = item.id
      signal('to-window')
      print('out', `title   ${titleOf(item)}`)
      print('out', `type    ${item.kind} · tag ${item.tag}`)
      if (item.user) print('out', `user    ${item.user}`)
      if (item.secret)
        print(
          'out',
          `secret  ${args.includes('--reveal') ? item.secret : '********  (add --reveal)'}`
        )
      return
    }
    if (verb === 'vault' && sub === 'add') {
      if (lockedError()) return
      const title = option(args, '--title')
      const kind = (option(args, '--type') ?? 'note') as Kind
      if (!title) {
        print('err', 'error: --title is required (INVALID_INPUT)')
        return
      }
      if (!['api', 'login', 'note', 'key'].includes(kind)) {
        print('err', `error: unknown type ${kind} (INVALID_INPUT)`)
        return
      }
      const id = `itm_${hex(title + items.length, 3)}`
      items.unshift({
        id,
        title,
        kind,
        tag: option(args, '--tag') ?? 'personal',
        ...(kind === 'note' ? {} : { secret: SECRET }),
        fresh: true
      })
      selected = id
      signal('to-window')
      print('ok', `saved ${id} · ${kind} · "${title}"`)
      print('dim', 'encrypted on this device · waits for the next sync')
      return
    }
    if (verb === 'attest' && sub === 'file') {
      if (lockedError()) return
      const file = args[3]
      const statement = option(args, '--text')
      if (!file || !statement) {
        print('err', 'usage: dmsg attest file <path> --text T')
        return
      }
      const id = `apr_${hex(file + Date.now(), 3)}`
      const hash = hex(file + statement, 64)
      approval = {
        id,
        file,
        text: statement,
        hash,
        state: 'waiting',
        left: 600
      }
      signal('to-window')
      print('out', `request  ${id} · sign file ${file}`)
      print('out', `sha-256  ${hash.slice(0, 12)}…${hash.slice(-6)}`)
      print('dim', 'review it in the approval window, or answer here')
      print('ask', 'Approve? [y/N]')
      asking = true
      return
    }
    if (verb === 'approvals') {
      if (approval?.state === 'waiting')
        print(
          'out',
          `${approval.id}  cli  sign file ${approval.file}  expires ${clock(approval.left)}`
        )
      else print('dim', 'no requests waiting')
      return
    }
    if (verb === 'mcp') {
      print('dim', 'dmsg mcp serves AI agents over stdio; see the next section')
      return
    }
    print(
      'err',
      `error: unknown command "${args.slice(1).join(' ')}" · try ${HELP}`
    )
  }

  async function type(command: string) {
    if (typing) return
    typing = true
    input = ''
    const fast = prefersReducedMotion()
    if (!fast)
      for (const ch of command) {
        input += ch
        await sleep(14 + Math.random() * 30)
      }
    input = ''
    if (!fast) await sleep(140)
    typing = false
    await run(command)
  }

  function onKey(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault()
      const value = input
      input = ''
      run(value)
    } else if (event.key === 'ArrowUp' && history.length) {
      event.preventDefault()
      recall = Math.min(recall + 1, history.length - 1)
      input = history[recall] ?? ''
    } else if (event.key === 'ArrowDown') {
      event.preventDefault()
      recall = Math.max(recall - 1, -1)
      input = history[recall] ?? ''
    } else if (event.key === 'l' && event.ctrlKey) {
      event.preventDefault()
      lines = []
    }
  }

  // A secret is shown or copied only after a second, deliberate click.
  function arm(action: 'show' | 'copy') {
    if (armed !== action) {
      armed = action
      clearTimeout(armTimer)
      armTimer = setTimeout(() => (armed = ''), 3000)
      return
    }
    armed = ''
    if (action === 'show') shownFor = 20
    else {
      navigator.clipboard?.writeText(current?.secret ?? '').catch(() => {})
      copiedFor = 30
    }
  }

  function select(id: string) {
    selected = id
    armed = ''
    shownFor = 0
  }

  onMount(() => {
    const timer = setInterval(() => {
      if (shownFor > 0) shownFor--
      if (copiedFor > 0) copiedFor--
      if (approval?.state === 'waiting' && approval.left > 0) approval.left--
    }, 1000)
    const measure = () => {
      stacked = root.clientWidth < 900
      const base = root.getBoundingClientRect().left
      const center = (el: HTMLElement) => {
        const r = el.getBoundingClientRect()
        return r.left - base + r.width / 2
      }
      geometry = { win: center(winPane), term: center(termPane) }
    }
    const resize = new ResizeObserver(measure)
    resize.observe(root)
    let started = false
    const stop = watchVisibility(
      root,
      async (visible) => {
        if (!visible || started) return
        started = true
        await sleep(500)
        await type('dmsg status')
        await sleep(700)
        await type('dmsg vault list')
      },
      '-20% 0px'
    )
    return () => {
      clearInterval(timer)
      clearTimeout(flowTimer)
      clearTimeout(armTimer)
      resize.disconnect()
      stop()
    }
  })
</script>

<div
  class="duo"
  class:stacked
  bind:this={root}
  style:--win="{geometry.win}px"
  style:--term="{geometry.term}px"
>
  <div class="duo-pane app-window" bind:this={winPane}>
    <div class="win-bar">
      <span class="lights" aria-hidden="true"><i></i><i></i><i></i></span>
      <span class="win-title">{NAME}</span>
      <span class="lock-state" class:locked
        ><Glyph name={locked ? 'lock' : 'unlock'} size={14} />{locked
          ? $t('Locked')
          : $t('Unlocked')}</span
      >
      <button
        class="win-lock"
        onclick={() => setLocked(!locked, 'window')}
        aria-label={locked ? $t('Unlock the example') : $t('Lock the example')}
        ><Glyph name={locked ? 'unlock' : 'lock'} size={16} /></button
      >
    </div>
    <div class="win-body">
      <ul class="win-nav" aria-hidden="true">
        {#each NAV as entry, i}
          <li class:active={i === 0}>
            <Glyph name={entry.glyph} size={16} /><span>{$t(entry.key)}</span>
            {#if i === 4 && approval?.state === 'waiting'}<b class="badge">1</b
              >{/if}
          </li>
        {/each}
      </ul>
      <div class="win-main">
        {#if locked}
          <div class="win-locked">
            <span class="lock-orb"><Glyph name="lock" size={22} /></span>
            <h4>{$t('Vault locked')}</h4>
            <p>
              {$t(
                'Titles, notes and secrets stay encrypted until you unlock this device.'
              )}
            </p>
            <button
              class="button primary"
              onclick={() => setLocked(false, 'window')}>{$t('Unlock')}</button
            >
          </div>
        {:else}
          <div class="vault-head">
            <h4>{$t('Vault')}</h4>
            <span>{$t('{count} items', { count: items.length })}</span>
          </div>
          <div class="vault-grid">
            <ul class="vault-list">
              {#each items as item (item.id)}
                <li>
                  <button
                    class:active={item.id === selected}
                    class:fresh={item.fresh}
                    aria-pressed={item.id === selected}
                    onclick={() => select(item.id)}
                  >
                    <span class="vi-icon"
                      ><Glyph name={KIND_GLYPH[item.kind]} size={16} /></span
                    >
                    <span class="vi-text">
                      <strong>{item.key ? $t(item.key) : item.title}</strong>
                      <small>{kindLabel[item.kind]} · {item.tag}</small>
                    </span>
                  </button>
                </li>
              {/each}
            </ul>
            {#if current}
              <div class="vault-detail">
                <h5>{current.key ? $t(current.key) : current.title}</h5>
                <p class="vd-meta">
                  <span>{kindLabel[current.kind]}</span><span
                    >#{current.tag}</span
                  >
                </p>
                {#if current.user}
                  <div class="vd-row">
                    <span>{$t('Username')}</span><code>{current.user}</code>
                  </div>
                {/if}
                {#if current.secret}
                  <div class="vd-row">
                    <span>{$t('Secret')}</span>
                    <code class="secret-value"
                      >{shownFor ? current.secret : '••••••••••••'}</code
                    >
                  </div>
                  <div class="vd-actions">
                    <button
                      class="chip-btn"
                      class:armed={armed === 'show'}
                      onclick={() => (shownFor ? (shownFor = 0) : arm('show'))}
                      ><Glyph name="eye" size={14} />{shownFor
                        ? $t('Hide')
                        : armed === 'show'
                          ? $t('Click again to show')
                          : $t('Show')}</button
                    >
                    <button
                      class="chip-btn"
                      class:armed={armed === 'copy'}
                      onclick={() => arm('copy')}
                      ><Glyph name="copy" size={14} />{armed === 'copy'
                        ? $t('Click again to copy')
                        : $t('Copy')}</button
                    >
                  </div>
                  <p class="vd-note" aria-live="polite">
                    {#if copiedFor}{$t(
                        'Sample copied. The app clears the clipboard after {seconds} s.',
                        { seconds: copiedFor }
                      )}{:else if shownFor}{$t('Hides again in {seconds} s.', {
                        seconds: shownFor
                      })}{/if}
                  </p>
                {:else if current.kind === 'note'}
                  <p class="vd-body">
                    {$t(
                      'Ship v1.4 on Friday. The brief is in the release channel.'
                    )}
                  </p>
                {/if}
                <p class="vd-sync" class:pending={current.fresh}>
                  <Glyph
                    name={current.fresh ? 'time' : 'check'}
                    size={14}
                  />{current.fresh
                    ? $t('Saved on this device · waits for sync')
                    : $t('Synced')}
                </p>
              </div>
            {/if}
          </div>
        {/if}

        {#if approval}
          <div class="approval-sheet" role="dialog" aria-label={$t('Approval')}>
            <div class="as-card">
              <div class="as-head">
                <strong>{$t('Approval')}</strong>
                <code>{CLI}</code>
              </div>
              <p class="as-source">{$t('From an interactive terminal')}</p>
              <h5>{$t('Sign a file')}</h5>
              <div class="vd-row">
                <span>{$t('File')}</span><code>{approval.file}</code>
              </div>
              <div class="vd-row">
                <span>{$t('Statement')}</span><code>{approval.text}</code>
              </div>
              <div class="vd-row">
                <span>SHA-256</span><code
                  >{approval.hash.slice(0, 12)}…{approval.hash.slice(-6)}</code
                >
              </div>
              {#if approval.state === 'waiting'}
                <div class="vd-row">
                  <span>{$t('Expires in')}</span><code
                    >{clock(approval.left)}</code
                  >
                </div>
                {#if locked}
                  <p class="as-note">{$t('Unlock to review this request.')}</p>
                  <div class="as-actions">
                    <button
                      class="button primary"
                      onclick={() => setLocked(false, 'window')}
                      >{$t('Unlock')}</button
                    >
                  </div>
                {:else}
                  <div class="as-actions">
                    <button
                      class="button secondary"
                      onclick={() => settle(false, 'window')}
                      >{$t('Reject')}</button
                    >
                    <button
                      class="button primary"
                      onclick={() => settle(true, 'window')}
                      >{$t('Approve')}</button
                    >
                  </div>
                {/if}
              {:else}
                <p
                  class="as-result"
                  class:ok={approval.state === 'approved'}
                  role="status"
                >
                  <Glyph
                    name={approval.state === 'approved' ? 'check' : 'close'}
                    size={16}
                  />{approval.state === 'approved'
                    ? $t('Approved. Signature generated.')
                    : $t('Rejected. Nothing was signed.')}
                </p>
              {/if}
            </div>
          </div>
        {/if}
      </div>
    </div>
    <div class="win-status">
      <span class:pending={pending || locked}
        ><Glyph
          name={locked ? 'lock' : pending ? 'time' : 'check'}
          size={13}
        />{locked
          ? $t('Locked')
          : pending
            ? $t('Saved on this device · waits for sync')
            : $t('Synced')}</span
      >
      <code>{ACCOUNT}</code>
    </div>
  </div>

  <div class="duo-pane terminal" bind:this={termPane} dir="ltr">
    <div class="win-bar dark">
      <span class="lights" aria-hidden="true"><i></i><i></i><i></i></span>
      <span class="win-title">{TITLE}</span>
    </div>
    <div class="term-screen" bind:this={screen}>
      {#each lines as line, i (i)}
        <div class="tl {line.kind}">
          {#if line.kind === 'cmd'}<span class="tp">{PROMPT}</span
            >{/if}{line.text}
        </div>
      {/each}
      <label class="term-input">
        <span class="tp">{asking ? '?' : PROMPT}</span>
        <input
          bind:value={input}
          onkeydown={onKey}
          readonly={typing}
          placeholder={typing || asking ? '' : HELP}
          aria-label={$t('Type a dmsg command')}
          autocomplete="off"
          autocapitalize="off"
          spellcheck="false"
        />
      </label>
    </div>
    <div class="term-chips">
      {#each SUGGESTIONS as command}
        <button disabled={typing} onclick={() => type(command)}
          >{command}</button
        >
      {/each}
    </div>
  </div>

  <div class="duo-bus" class:flowing={flow} data-flow={flow} aria-hidden="true">
    <span class="bus-line"></span>
    <span class="bus-pulse"></span>
    <span class="bus-label"
      ><Glyph name="shield" size={14} /><code>{DAEMON}</code></span
    >
  </div>
</div>
