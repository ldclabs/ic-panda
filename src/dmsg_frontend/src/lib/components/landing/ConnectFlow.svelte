<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '$lib/i18n'
  import Glyph from './Glyph.svelte'
  import type { GlyphName } from './glyphs'
  import { prefersReducedMotion, watchVisibility } from './motion'

  // dmsg-connect/1 in seven moves, between four parties.
  const ORIGIN = 'https://tokenlist.ing'
  const SOURCE = 'connect:https://tokenlist.ing'
  const APP = 'TokenList'
  const ACCOUNT = 'Alex'
  const FILE = 'release-brief.txt · v1'
  const DIGEST = '9f2c41b7e09a…d31e7c'
  const MOVES: [number, number, string][] = [
    [0, 1, 'signDocument'],
    [1, 2, 'HPKE'],
    [2, 3, 'hint'],
    [3, 3, ''],
    [3, 2, 'HPKE Auth'],
    [2, 1, ''],
    [1, 0, 'result']
  ]
  const DECISION = 3

  const lanes = $derived<{ glyph: GlyphName; name: string; host: string }[]>([
    { glyph: 'globe', name: $t('Website'), host: 'tokenlist.ing' },
    { glyph: 'link', name: $t('Bridge page'), host: 'connect.dmsg.net' },
    { glyph: 'cloud', name: $t('dMsg relay'), host: $t('sealed mailbox') },
    { glyph: 'laptop', name: $t('Your desktop'), host: $t('approval window') }
  ])

  let shown = $state(0)
  let decision = $state<'' | 'approved' | 'rejected'>('')
  let playing = $state(false)
  let left = $state(600)
  let root: HTMLDivElement
  let timer: ReturnType<typeof setTimeout> | undefined

  const labels = $derived([
    $t('Asks you to sign a file'),
    $t('Sealed so only your devices can open it'),
    $t('A hint wakes your desktop, no content'),
    $t('You review it and decide'),
    decision === 'rejected' ? $t('Sealed refusal') : $t('Sealed result'),
    $t('Opened only by the page that asked'),
    decision === 'rejected'
      ? $t('The site is told no; nothing is signed')
      : $t('The site verifies the signature')
  ])
  const waiting = $derived(shown === DECISION + 1 && !decision)
  const done = $derived(shown === MOVES.length)

  function schedule() {
    clearTimeout(timer)
    if (!playing || waiting || done) return
    timer = setTimeout(
      () => {
        shown++
        if (shown === MOVES.length) playing = false
        schedule()
      },
      shown ? 1500 : 500
    )
  }

  function play() {
    if (done) restart()
    playing = true
    schedule()
  }

  function pause() {
    playing = false
    clearTimeout(timer)
  }

  function next() {
    pause()
    if (!waiting && !done) shown++
  }

  function restart() {
    clearTimeout(timer)
    shown = 0
    decision = ''
    left = 600
    schedule()
  }

  function decide(approve: boolean) {
    decision = approve ? 'approved' : 'rejected'
    if (prefersReducedMotion()) {
      shown = MOVES.length
      return
    }
    playing = true
    shown++
    schedule()
  }

  onMount(() => {
    const tickTimer = setInterval(() => {
      if (waiting && left > 0) left--
    }, 1000)
    let started = false
    const stop = watchVisibility(
      root,
      (visible) => {
        if (!visible || started) return
        started = true
        if (prefersReducedMotion()) shown = DECISION + 1
        else play()
      },
      '-20% 0px'
    )
    return () => {
      clearInterval(tickTimer)
      clearTimeout(timer)
      stop()
    }
  })
</script>

<div class="connect-stage" bind:this={root}>
  <div class="seq" dir="ltr">
    {#each lanes as lane, i}
      <div class="seq-head" style:grid-column={i + 1}>
        <span class="sh-icon"><Glyph name={lane.glyph} size={18} /></span>
        <strong dir="auto">{lane.name}</strong>
        <code dir="auto">{lane.host}</code>
      </div>
      <i class="lifeline" style:grid-column={i + 1}></i>
    {/each}
    {#each MOVES as [from, to, code], i}
      {@const low = Math.min(from, to)}
      {@const span = Math.abs(to - from) + 1}
      <div
        class="seq-msg"
        class:self={from === to}
        class:rev={to < from}
        class:on={i < shown}
        class:now={i === shown - 1}
        class:hold={i === DECISION && waiting}
        style:grid-row={i + 2}
        style:grid-column="{low + 1} / span {span}"
        style:--span={span}
      >
        {#if from === to}
          <span class="self-mark"
            ><Glyph
              name={decision === 'rejected'
                ? 'close'
                : decision
                  ? 'check'
                  : 'pen'}
              size={14}
            /></span
          >
        {:else}
          <span class="track"
            ><i class="line"></i><i class="envelope"
              ><Glyph name="mail" size={12} /></i
            ></span
          >
        {/if}
        <span class="msg-label" dir="auto"
          ><b>{i + 1}</b>
          {labels[i]}{#if code}<code>{code}</code>{/if}</span
        >
      </div>
    {/each}
  </div>

  <div class="approval-window" class:awaiting={waiting}>
    <div class="win-bar">
      <span class="lights" aria-hidden="true"><i></i><i></i><i></i></span>
      <span class="win-title">{$t('dMsg · Approval')}</span>
    </div>
    <div class="aw-body">
      {#if shown <= DECISION}
        <div class="aw-empty">
          <Glyph name="time" size={22} />
          <p>
            {$t(
              'Nothing to review yet. The request appears here when it reaches this device.'
            )}
          </p>
        </div>
      {:else}
        <code class="aw-source">{SOURCE}</code>
        <h4>{$t('Sign a file')}</h4>
        <dl class="aw-rows">
          <div><dt>{$t('App')}</dt><dd>{APP}</dd></div>
          <div>
            <dt>{$t('Origin')}</dt>
            <dd>
              <code>{ORIGIN}</code><small
                >{$t(
                  'Reported by your browser through connect.dmsg.net'
                )}</small
              >
            </dd>
          </div>
          <div><dt>{$t('Browser')}</dt><dd>{$t('Chrome on this Mac')}</dd></div>
          <div><dt>{$t('Account')}</dt><dd>{ACCOUNT}</dd></div>
          <div><dt>{$t('File')}</dt><dd><code>{FILE}</code></dd></div>
          <div><dt>SHA-256</dt><dd><code>{DIGEST}</code></dd></div>
          {#if !decision}
            <div>
              <dt>{$t('Expires in')}</dt>
              <dd>
                <code
                  >{Math.floor(left / 60)}:{String(left % 60).padStart(
                    2,
                    '0'
                  )}</code
                >
              </dd>
            </div>
          {/if}
        </dl>
        {#if !decision}
          <div class="aw-actions">
            <button class="button secondary" onclick={() => decide(false)}
              >{$t('Reject')}</button
            >
            <button class="button primary" onclick={() => decide(true)}
              >{$t('Approve')}</button
            >
          </div>
        {:else if decision === 'approved'}
          <ol class="aw-progress" role="status">
            <li class="on"><Glyph name="check" size={14} />{$t('Approved')}</li>
            <li class:on={shown > DECISION + 1}>
              <Glyph name="check" size={14} />{$t('Signature generated')}
            </li>
            <li class:on={done}>
              <Glyph name="check" size={14} />{$t(
                'The site received the result'
              )}
            </li>
          </ol>
        {:else}
          <p class="aw-refused" role="status">
            <Glyph name="close" size={14} />{$t(
              'Rejected. The site is told no, and nothing is signed.'
            )}
          </p>
        {/if}
      {/if}
    </div>
  </div>
</div>

<div class="connect-controls">
  {#if playing}
    <button class="chip-btn" onclick={pause}
      ><Glyph name="pause" size={14} />{$t('Pause')}</button
    >
  {:else}
    <button class="chip-btn" onclick={play} disabled={waiting}
      ><Glyph name="play" size={14} />{done ? $t('Replay') : $t('Play')}</button
    >
  {/if}
  <button class="chip-btn" onclick={next} disabled={waiting || done}
    ><Glyph name="skip" size={14} />{$t('Next step')}</button
  >
  <button class="chip-btn" onclick={restart} disabled={!shown}
    ><Glyph name="refresh" size={14} />{$t('Start over')}</button
  >
  <span class="cc-status" aria-live="polite">
    {waiting ? $t('Your turn: approve or reject in the approval window.') : ''}
  </span>
</div>
