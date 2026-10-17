<script lang="ts">
  import { onMount } from 'svelte'
  import { t, tr } from '$lib/i18n'
  import Glyph from './Glyph.svelte'
  import { prefersReducedMotion } from './motion'

  // The rules of dmsg-wallet/1 with sample data: the amount picks the
  // approvals and the delay, only members decide, an agent only proposes,
  // the wallet signs what the proposal says and a member's device broadcasts
  // it. Nothing reaches a chain.
  type Stage =
    | 'open'
    | 'delayed'
    | 'ready'
    | 'signing'
    | 'signed'
    | 'broadcast'
    | 'settled'
    | 'rejected'
    | 'vetoed'
  type Decision = '' | 'approved' | 'rejected'
  type Entry = { seq: number; kind: string; actor: string; hash: string }

  const WALLET = 'w7x4q-ziaaa-aaaas-qd2pa-cai'
  const TO = 'bc1pq8h5mfzr6k2vx0fu4c9ydwl3gne7jsa8t5qhxz0rmp4wk6yv2dlsq9tc3e'
  const FEE = '0.00021 BTC'
  const AGENT = 'agent:bookkeeper'
  const CHAIN = 'Bitcoin'
  const MEI = 'Mei'
  const OMAR = 'Omar'
  const TXID = 'txid'
  const PROPOSAL = 12
  const DAY = 24 * 60 * 60
  const TIERS = [
    { amount: '0.02 BTC', approvals: 1, delay: false },
    { amount: '0.4 BTC', approvals: 2, delay: true }
  ]
  const ADDRESSES = [
    { chain: 'ICP', address: 'w7x4q-…-cai' },
    { chain: 'Bitcoin', address: 'bc1p8f…k2xz' },
    { chain: 'EVM', address: '0x3B7e…9c41' },
    { chain: 'Solana', address: '7HkQ…Pq2m' }
  ]
  const TRACK: Stage[] = [
    'open',
    'delayed',
    'ready',
    'signed',
    'broadcast',
    'settled'
  ]

  let tier = $state(1)
  let stage = $state<Stage>('open')
  let mine = $state<Decision>('')
  let mei = $state<Decision>('')
  let left = $state(DAY)
  let txid = $state('')
  let audit = $state<Entry[]>([])
  let timers: ReturnType<typeof setTimeout>[] = []

  const need = $derived(TIERS[tier]!)
  const count = $derived(
    Number(mine === 'approved') + Number(mei === 'approved')
  )
  const roles = $derived([
    { name: $t('You'), role: $t('Admin'), mark: $t('You').at(0) },
    { name: MEI, role: $t('Approver'), mark: MEI.at(0) },
    { name: OMAR, role: $t('Proposer'), mark: OMAR.at(0) },
    {
      name: 'bookkeeper',
      role: $t('Agent ID listed by {member}', { member: OMAR }),
      mark: ''
    }
  ])
  const stageLabel = $derived<Record<Stage, string>>({
    open: $t('Proposed'),
    delayed: $t('Delayed'),
    ready: $t('Ready'),
    signing: $t('Signing…'),
    signed: $t('Signed'),
    broadcast: $t('Broadcast'),
    settled: $t('Settled'),
    rejected: $t('Turned down'),
    vetoed: $t('Vetoed')
  })
  const story = $derived(
    {
      open:
        mine === 'rejected' && need.approvals === 1
          ? $t('Your no is recorded. Mei can still approve it.')
          : $t('Waiting for approvals. The agent cannot give one.'),
      delayed: $t(
        'Approved. Either approver can still veto it until the delay ends.'
      ),
      ready: $t('Ready. A member can execute it now.'),
      signing: $t(
        'The wallet builds the transaction from the proposal and signs it with its threshold key.'
      ),
      signed: $t(
        'Your device decodes the transaction, checks it against the proposal and broadcasts it.'
      ),
      broadcast: $t('Broadcast. Waiting for the network.'),
      settled: $t('Settled. Every step is in the audit chain.'),
      rejected: $t('Rejected. Nothing was signed.'),
      vetoed: $t('Vetoed. Nothing was signed.')
    }[stage]
  )
  const reached = (step: Stage) => {
    const at = stage === 'signing' ? 'ready' : stage
    return TRACK.indexOf(step) <= TRACK.indexOf(at)
  }
  // Amounts keep their order inside Arabic text.
  const ltr = (text: string) =>
    String.fromCharCode(0x2066) + text + String.fromCharCode(0x2069)
  const clock = (s: number) =>
    [s / 3600, (s % 3600) / 60, s % 60]
      .map((n) => String(Math.floor(n)).padStart(2, '0'))
      .join(':')

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

  // Each audit event hashes the one before it.
  function log(kind: string, actor: string) {
    const prev = audit[0]
    const seq = (prev?.seq ?? 39) + 1
    audit.unshift({
      seq,
      kind,
      actor,
      hash: hex(`${prev?.hash ?? ''}${seq}${kind}${actor}`, 8)
    })
  }

  function reset(next = tier) {
    timers.forEach(clearTimeout)
    timers = []
    tier = next
    stage = 'open'
    mine = ''
    mei = ''
    left = DAY
    txid = ''
    audit = []
    log('book.add Auditor', 'mei')
    log(`propose #${PROPOSAL}`, `omar · ${AGENT}`)
  }

  function settle() {
    if (count >= need.approvals) stage = need.delay ? 'delayed' : 'ready'
    // Rejected once too few approvers are left to reach the threshold.
    const open = 2 - Number(mine === 'rejected') - Number(mei === 'rejected')
    if (open < need.approvals) stage = 'rejected'
  }

  function decide(who: 'you' | 'mei', approve: boolean) {
    if (stage !== 'open') return
    if (who === 'you') mine = approve ? 'approved' : 'rejected'
    else mei = approve ? 'approved' : 'rejected'
    log(`${approve ? 'approve' : 'reject'} #${PROPOSAL}`, who)
    settle()
  }

  function veto() {
    stage = 'vetoed'
    log(`veto #${PROPOSAL}`, 'you')
  }

  function skip() {
    left = 0
    stage = 'ready'
  }

  function later(ms: number, run: () => void) {
    timers.push(setTimeout(run, prefersReducedMotion() ? 0 : ms))
  }

  function execute() {
    stage = 'signing'
    log(`execute #${PROPOSAL}`, 'you')
    later(1100, () => {
      txid = hex(`${PROPOSAL}${need.amount}`, 64)
      stage = 'signed'
      log(`signed #${PROPOSAL}`, 'wallet')
      later(1300, () => {
        stage = 'broadcast'
        log('report broadcast', 'you')
        later(1600, () => {
          stage = 'settled'
          log('report settled', 'you')
        })
      })
    })
  }

  reset()

  onMount(() => {
    const timer = setInterval(() => {
      if (stage === 'delayed' && left > 0) left = Math.max(left - 1, 0)
    }, 1000)
    return () => {
      clearInterval(timer)
      timers.forEach(clearTimeout)
    }
  })
</script>

<div class="wallet-lab">
  <aside class="lab-side">
    <header class="lab-agent">
      <span class="agent-orb"><Glyph name="coins" size={20} /></span>
      <span><strong>{$t('Team treasury')}</strong><code>{WALLET}</code></span>
    </header>
    <p class="lab-label">{$t('Addresses')}</p>
    <ul class="wl-addresses">
      {#each ADDRESSES as entry}
        <li>
          <span>{entry.chain}</span><code>{entry.address}</code><Glyph
            name="check"
            size={14}
          />
        </li>
      {/each}
    </ul>
    <p class="wl-small">{$t('Each checked against the wallet’s keys')}</p>
    <p class="lab-label">{$t('Policy for BTC')}</p>
    <ul class="wl-policy">
      <li class:hit={tier === 0}>
        <span
          >{$t('Up to {amount} in 24 hours', { amount: ltr('0.05 BTC') })}</span
        ><b>{$t('1 of 2 approvals')}</b>
      </li>
      <li class:hit={tier === 1}>
        <span>{$t('Up to {amount} in 24 hours', { amount: ltr('1 BTC') })}</span
        ><b>{$t('2 of 2, then 24 hours')}</b>
      </li>
      <li>
        <span>{$t('New address-book entries')}</span><b>{$t('Wait 24 hours')}</b
        >
      </li>
    </ul>
    <p class="lab-label">{$t('Members')}</p>
    <ul class="wl-members">
      {#each roles as member}
        <li>
          <span class="wl-orb" class:agent={!member.mark}
            >{#if member.mark}{member.mark}{:else}<Glyph
                name="robot"
                size={14}
              />{/if}</span
          >
          <span><strong>{member.name}</strong><small>{member.role}</small></span
          >
        </li>
      {/each}
    </ul>
  </aside>

  <div class="lab-run wl-run">
    <div class="wl-amount" role="radiogroup" aria-label={$t('Amount')}>
      <span>{$t('Amount')}</span>
      {#each TIERS as option, i}
        <button
          dir="ltr"
          role="radio"
          aria-checked={tier === i}
          onclick={() => tier !== i && reset(i)}>{option.amount}</button
        >
      {/each}
    </div>
    <article class="wl-ticket">
      <header>
        <strong>{$t('Proposal #{n}', { n: PROPOSAL })}</strong>
        <span class="wl-stage {stage}">{stageLabel[stage]}</span>
      </header>
      <dl class="aw-rows">
        <div>
          <dt>{$t('Send')}</dt>
          <dd><strong dir="ltr">{need.amount}</strong> · {CHAIN}</dd>
        </div>
        <div>
          <dt>{$t('To')}</dt>
          <dd>
            <code>{TO}</code><small class="wl-book"
              >{$t('Address book: {label} · past its 24-hour wait', {
                label: tr('Auditor')
              })}</small
            >
          </dd>
        </div>
        <div>
          <dt>{$t('Highest network fee')}</dt>
          <dd><code>{FEE}</code></dd>
        </div>
        <div>
          <dt>{$t('Proposed by')}</dt>
          <dd>
            <code>{AGENT}</code>
            <small class="wl-by"
              >{$t('An agent acting for {member}', { member: OMAR })}</small
            >
          </dd>
        </div>
      </dl>
      <p class="wl-need">
        {need.delay
          ? $t(
              'Needs 2 of 2 approvals, then 24 hours in which either approver can veto.'
            )
          : $t('Needs 1 of 2 approvals.')}
      </p>
      <ul class="wl-votes">
        <li>
          <span class="wl-orb">{$t('You').at(0)}</span>
          <span class="wl-voter"
            ><strong>{$t('You')}</strong><small
              >{$t('Your device signs your decision.')}</small
            ></span
          >
          {#if mine}
            <span class="wl-vote {mine}"
              >{mine === 'approved' ? $t('Approved') : $t('Declined')}</span
            >
          {:else if stage === 'open'}
            <span class="wl-buttons">
              <button
                class="button secondary"
                onclick={() => decide('you', false)}>{$t('Reject')}</button
              >
              <button class="button primary" onclick={() => decide('you', true)}
                >{$t('Approve')}</button
              >
            </span>
          {/if}
        </li>
        <li>
          <span class="wl-orb">{MEI.at(0)}</span>
          <span class="wl-voter"
            ><strong>{MEI}</strong><small>{$t('On another device')}</small
            ></span
          >
          {#if mei}
            <span class="wl-vote {mei}"
              >{mei === 'approved' ? $t('Approved') : $t('Declined')}</span
            >
          {:else if stage === 'open'}
            <button class="chip-btn" onclick={() => decide('mei', true)}
              >{$t('Approve as Mei')}</button
            >
          {/if}
        </li>
      </ul>
      <ol class="wl-track" aria-label={$t('Progress')}>
        {#each TRACK as step}
          <li
            class:on={reached(step)}
            class:skipped={step === 'delayed' && !need.delay}
          >
            {stageLabel[step]}
          </li>
        {/each}
      </ol>
      <p class="wl-story" aria-live="polite">
        {story}{#if stage === 'delayed'}
          <code>{clock(left)}</code>{/if}
      </p>
      {#if txid}
        <p class="wl-tx"><span>{TXID}</span><code>{txid}</code></p>
      {/if}
      <div class="wl-actions">
        {#if stage === 'delayed'}
          <button class="button secondary" onclick={veto}>{$t('Veto')}</button>
          <button class="button primary" onclick={skip}
            ><Glyph name="skip" size={16} />{$t('Skip the 24 hours')}</button
          >
        {:else if stage === 'ready'}
          <button class="button primary" onclick={execute}
            >{$t('Execute')}</button
          >
        {:else if ['settled', 'rejected', 'vetoed'].includes(stage)}
          <button class="button secondary" onclick={() => reset()}
            ><Glyph name="refresh" size={16} />{$t('Start over')}</button
          >
        {/if}
      </div>
    </article>
  </div>

  <aside class="lab-audit">
    <p class="lab-label"
      ><Glyph name="history" size={14} />{$t('Audit chain')}</p
    >
    <ol>
      {#each audit as entry (entry.seq)}
        <li>
          <time>#{entry.seq}</time>
          <span class="a-who">{entry.actor}</span>
          <b class="ok">{entry.hash}</b>
          <code>{entry.kind}</code>
        </li>
      {/each}
    </ol>
    <p class="lab-foot">
      {$t(
        'Each event hashes the one before it, so a member can check that none is missing.'
      )}
    </p>
  </aside>
</div>
