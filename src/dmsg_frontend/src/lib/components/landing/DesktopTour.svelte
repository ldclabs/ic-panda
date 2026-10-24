<script lang="ts">
  import { onMount } from 'svelte'
  import { fly } from 'svelte/transition'
  import { t } from '$lib/i18n'
  import Glyph from './Glyph.svelte'
  import type { GlyphName } from './glyphs'

  // The desktop's main window with sample data: the sidebar's groups and views
  // in the app's order, the gate rail at its foot, and a glance at each view.
  type View =
    | 'channels'
    | 'files'
    | 'vault'
    | 'inbox'
    | 'approvals'
    | 'agents'
    | 'wallets'
  const GROUPS: { label: string; views: View[] }[] = [
    { label: 'Work', views: ['channels', 'files', 'vault', 'inbox'] },
    { label: 'Authorization', views: ['approvals', 'agents'] },
    { label: 'Account', views: ['wallets'] }
  ]
  const VIEWS: Record<
    View,
    { glyph: GlyphName; label: string; detail: string; key: string }
  > = {
    channels: {
      glyph: 'chat',
      label: 'Channels',
      detail:
        'End-to-end encrypted conversations and files with people you choose',
      key: '1'
    },
    files: {
      glyph: 'folder',
      label: 'Files',
      detail:
        'Encrypted files on this device; nothing leaves unless you back it up or share it',
      key: '2'
    },
    vault: {
      glyph: 'vault',
      label: 'Vault',
      detail: 'Notes, logins and keys, encrypted on this device',
      key: '3'
    },
    inbox: {
      glyph: 'inbox',
      label: 'Inbox',
      detail: 'Letters, and who may write to you',
      key: '4'
    },
    approvals: {
      glyph: 'pen',
      label: 'Signatures & approvals',
      detail: 'Requests waiting for you, signatures and their records',
      key: '5'
    },
    agents: {
      glyph: 'robot',
      label: 'Agents',
      detail: 'Agent IDs, what they may do and what they did',
      key: '6'
    },
    wallets: {
      glyph: 'coins',
      label: 'Team wallets',
      detail:
        'Team funds on ICP, Bitcoin, EVM and Solana, managed together under wallet policy',
      key: '8'
    }
  }
  const ORDER = GROUPS.flatMap((group) => group.views)
  const CYCLE = 5600
  const MOD = '⌘'
  const AGENT = 'release-assistant'
  // Amounts keep their order inside right-to-left sentences.
  const ltr = (text: string) =>
    String.fromCharCode(0x2066) + text + String.fromCharCode(0x2069)
  const REVIEWER = 'review-assistant'
  const AGENT_ID = 'Agent ID'
  const CHANNELS = ['# release', '# design', 'Mei']
  const FILES: {
    name: string
    size: string
    state: 'cloud' | 'local' | 'up'
  }[] = [
    { name: 'release-plan.md', size: '12 KB', state: 'cloud' },
    { name: 'build-1.4.tar.gz', size: '48 MB', state: 'local' },
    { name: 'release-notes.md', size: '6 KB', state: 'up' }
  ]
  const ITEMS: { key: string; kind: string; glyph: GlyphName }[] = [
    { key: 'Deploy key', kind: 'API key', glyph: 'key' },
    { key: 'GitHub', kind: 'Login', glyph: 'shield' },
    { key: 'Release plan', kind: 'Note', glyph: 'file' },
    { key: 'Signing key', kind: 'Key', glyph: 'key' }
  ]
  const RULES = ['Nobody', 'Contacts only', 'Everyone', 'Paid letters']
  const TEAMMATE = 'Mei'
  const NOTES = 'release-notes.md'
  const FOLDER = 'release'
  const USER = 'alex'
  const PALETTE = `${MOD}K`
  const LETTERS = [
    {
      from: 'ada',
      subject: 'A question about the audit',
      time: '09:12',
      unread: true
    },
    {
      from: TEAMMATE,
      subject: 'Draft for the community post',
      time: '08:30',
      unread: false
    }
  ]
  const SIGNED: {
    title: string
    params: Record<string, string>
    via: string
    state: string
  }[] = [
    {
      title: 'Sign in to {site}',
      params: { site: 'example.com' },
      via: 'connect:example.com',
      state: 'Approved'
    },
    {
      title: 'Statement for {file}',
      params: { file: 'build-1.4.tar.gz' },
      via: 'cli',
      state: 'Signature generated'
    }
  ]
  const AGENTS = [
    {
      name: AGENT,
      caps: [
        'files.write:fld_r3l',
        'channel.publish:release',
        'mail.write:https://mail.dmsg.net'
      ],
      active: true
    },
    {
      name: REVIEWER,
      caps: ['discourse.read:https://discourse.dmsg.net'],
      active: false
    }
  ]
  const BALANCES = [
    { asset: 'BTC', amount: '1.20' },
    { asset: 'ICP', amount: '340.00' },
    { asset: 'USDC', amount: '2,500' }
  ]
  const AUDIT: { time: string; tool: string; result: string; ok: boolean }[] = [
    { time: '09:43', tool: 'dmsg_files_write', result: 'ok', ok: true },
    {
      time: '09:44',
      tool: 'agent_protocols_mail_send',
      result: 'ok',
      ok: true
    },
    { time: '09:45', tool: 'dmsg_vault_read', result: 'FORBIDDEN', ok: false }
  ]

  const id = $props.id()
  let root: HTMLElement
  let view = $state<View>('channels')
  let manual = $state(false)
  let locked = $state(false)
  let visible = $state(false)
  let paused = $state(false)
  let reduced = $state(true)
  let foreground = $state(true)
  const cycling = $derived(
    !manual && !locked && visible && !paused && foreground && !reduced
  )
  const current = $derived(VIEWS[view])

  $effect(() => {
    if (!cycling) return
    const next = ORDER[(ORDER.indexOf(view) + 1) % ORDER.length]!
    const timer = setTimeout(() => (view = next), CYCLE)
    return () => clearTimeout(timer)
  })

  function select(next: View) {
    manual = true
    view = next
  }

  onMount(() => {
    const motion = matchMedia('(prefers-reduced-motion: reduce)')
    const updateVisibility = () => (foreground = !document.hidden)
    reduced = motion.matches
    updateVisibility()
    document.addEventListener('visibilitychange', updateVisibility)
    const observer = new IntersectionObserver(
      ([entry]) => (visible = entry?.isIntersecting ?? false),
      { threshold: 0.35 }
    )
    observer.observe(root)
    return () => {
      observer.disconnect()
      document.removeEventListener('visibilitychange', updateVisibility)
    }
  })
</script>

<figure class="tour" bind:this={root}>
  <div
    class="tour-window"
    role="group"
    aria-label={$t('The dMsg desktop app')}
    onpointerenter={() => (paused = true)}
    onpointerleave={() => (paused = false)}
    onfocusin={() => (paused = true)}
    onfocusout={() => (paused = false)}
  >
    <aside class="tour-side">
      <span class="lights tour-lights" aria-hidden="true"
        ><i></i><i></i><i></i></span
      >
      <div class="tour-nav" role="group" aria-label={$t('Views')}>
        {#each GROUPS as group}
          <p class="tour-group">{$t(group.label)}</p>
          {#each group.views as item}
            <button
              class:current={view === item}
              aria-pressed={view === item}
              aria-controls="{id}-pane"
              onclick={() => select(item)}
              ><Glyph name={VIEWS[item].glyph} size={17} /><span
                >{$t(VIEWS[item].label)}</span
              ><kbd dir="ltr">{MOD}{VIEWS[item].key}</kbd
              >{#if view === item && cycling}{#key view}<i
                    class="tour-progress"
                    aria-hidden="true"
                  ></i>{/key}{/if}</button
            >
          {/each}
        {/each}
      </div>
      <div class="tour-rail" class:locked>
        <span
          ><Glyph name={locked ? 'lock' : 'unlock'} size={15} /><b
            >{locked ? $t('Locked') : $t('Unlocked')}</b
          ><small>{$t('Account synced')}</small></span
        >
        <button
          aria-label={locked ? $t('Unlock with Touch ID') : $t('Lock')}
          onclick={() => (locked = !locked)}
          ><Glyph name={locked ? 'fingerprint' : 'lock'} size={16} /></button
        >
      </div>
    </aside>
    <div class="tour-main">
      <header class="tour-bar">
        <b>{locked ? $t('Locked') : $t(current.label)}</b>
        <span class="tour-search" aria-hidden="true"
          ><Glyph name="search" size={14} /><span
            >{$t('Search or run a command')}</span
          ><kbd dir="ltr">{PALETTE}</kbd></span
        >
      </header>
      <div class="tour-pane" id="{id}-pane">
        {#key locked ? 'locked' : view}
          <div
            class="tour-view"
            in:fly={{ y: 10, duration: reduced ? 0 : 320, delay: 60 }}
          >
            {#if locked}
              <div class="tp-locked">
                <span class="tp-tile"><Glyph name="lock" size={24} /></span>
                <b>{$t('Locked')}</b>
                <p
                  >{$t(
                    'Nothing in your workspace shows until you unlock this device.'
                  )}</p
                >
                <button class="button primary" onclick={() => (locked = false)}
                  ><Glyph name="fingerprint" />{$t(
                    'Unlock with Touch ID'
                  )}</button
                >
              </div>
            {:else if view === 'channels'}
              <div class="tp-split">
                <ul class="tp-list">
                  {#each CHANNELS as channel, i}
                    <li class:on={i === 0}
                      ><Glyph
                        name={i < 2 ? 'chat' : 'members'}
                        size={15}
                      /><span dir="ltr">{channel}</span>{#if i === 0}<b>3</b
                        >{/if}</li
                    >
                  {/each}
                </ul>
                <div class="tp-thread">
                  <p class="tp-head"
                    ><b dir="ltr">{CHANNELS[0]}</b><span class="tp-chip"
                      ><Glyph name="lock" size={13} />{$t(
                        'Encrypted end to end'
                      )}</span
                    ></p
                  >
                  <div class="tp-msg">
                    <span class="tp-av">{TEAMMATE[0]}</span>
                    <div
                      ><p class="tp-who"><b>{TEAMMATE}</b><time>09:40</time></p
                      ><p>{$t('The notes read well. One fix in section 2.')}</p
                      ></div
                    >
                  </div>
                  <div class="tp-msg">
                    <span class="tp-av agent"
                      ><Glyph name="robot" size={15} /></span
                    >
                    <div
                      ><p class="tp-who"
                        ><b>{AGENT}</b><span class="tp-tag">{AGENT_ID}</span
                        ><time>09:43</time></p
                      ><p>{$t('Fixed. The release notes are ready.')}</p><span
                        class="tp-file"
                        ><Glyph name="file" size={14} /><bdi>{NOTES}</bdi></span
                      ></div
                    >
                  </div>
                  <div class="tp-msg">
                    <span class="tp-av you"
                      ><Glyph name="laptop" size={14} /></span
                    >
                    <div
                      ><p class="tp-who"><b>{$t('You')}</b><time>09:46</time></p
                      ><p>{$t('Thanks. We ship on Friday.')}</p></div
                    >
                  </div>
                </div>
              </div>
            {:else if view === 'files'}
              <div class="tp-files">
                <p class="tp-head"
                  ><b><Glyph name="folder" size={15} /><bdi>{FOLDER}</bdi></b
                  ><span class="tp-chip"
                    ><Glyph name="cloud" size={13} />{$t('{used} of {limit}', {
                      used: '60 MB',
                      limit: '10 GB'
                    })}</span
                  ></p
                >
                <ul>
                  {#each FILES as file}
                    <li
                      ><Glyph name="file" size={16} /><bdi>{file.name}</bdi
                      ><span class="tp-size" dir="ltr">{file.size}</span><span
                        class="tp-state {file.state}"
                        ><Glyph
                          name={file.state === 'local' ? 'laptop' : 'cloud-up'}
                          size={14}
                        />{file.state === 'cloud'
                          ? $t('Backed up')
                          : file.state === 'local'
                            ? $t('Only on this device')
                            : $t('Backing up…')}</span
                      >{#if file.state === 'up'}<i
                          class="tp-upload"
                          aria-hidden="true"
                        ></i>{/if}</li
                    >
                  {/each}
                </ul>
              </div>
            {:else if view === 'vault'}
              <div class="tp-split">
                <ul class="tp-list">
                  {#each ITEMS as item, i}
                    <li class:on={i === 1}
                      ><Glyph name={item.glyph} size={15} /><span
                        ><b>{$t(item.key)}</b><small>{$t(item.kind)}</small
                        ></span
                      ></li
                    >
                  {/each}
                </ul>
                <div class="tp-item">
                  <p class="tp-head"
                    ><b>GitHub</b><span class="tp-chip">{$t('Login')}</span></p
                  >
                  <dl>
                    <div><dt>{$t('Username')}</dt><dd dir="ltr">{USER}</dd></div
                    >
                    <div
                      ><dt>{$t('Password')}</dt><dd dir="ltr"
                        >••••••••••<Glyph name="eye" size={14} /></dd
                      ></div
                    >
                    <div
                      ><dt>{$t('One-time password')}</dt><dd dir="ltr"
                        >482 913<svg
                          class="tp-otp"
                          viewBox="0 0 16 16"
                          aria-hidden="true"
                          ><circle cx="8" cy="8" r="6" /><circle
                            class="run"
                            cx="8"
                            cy="8"
                            r="6"
                            pathLength="1"
                          /></svg
                        ></dd
                      ></div
                    >
                  </dl>
                </div>
              </div>
            {:else if view === 'inbox'}
              <div class="tp-inbox">
                <div class="tp-rule">
                  <span>{$t('Who may write to me')}</span>
                  <div class="tp-seg">
                    {#each RULES as rule}<span
                        class:on={rule === 'Contacts only'}>{$t(rule)}</span
                      >{/each}
                  </div>
                </div>
                <ul class="tp-letters">
                  {#each LETTERS as letter}
                    <li class:unread={letter.unread}
                      ><span class="tp-av">{letter.from[0]?.toUpperCase()}</span
                      ><span
                        ><b>{letter.from}</b><small>{$t(letter.subject)}</small
                        ></span
                      ><time>{letter.time}</time></li
                    >
                  {/each}
                </ul>
              </div>
            {:else if view === 'approvals'}
              <ul class="tp-requests">
                <li class="waiting"
                  ><span class="tp-gem" aria-hidden="true"></span><span
                    ><b>{$t('Sign the release statement')}</b><code dir="ltr"
                      >{`agent:${AGENT}`}</code
                    ></span
                  ><span class="tp-state wait"
                    >{$t('Waiting for your approval')}</span
                  ></li
                >
                {#each SIGNED as item}
                  <li
                    ><Glyph name="check" size={16} /><span
                      ><b>{$t(item.title, item.params)}</b><code dir="ltr"
                        >{item.via}</code
                      ></span
                    ><span class="tp-state">{$t(item.state)}</span></li
                  >
                {/each}
              </ul>
            {:else if view === 'agents'}
              <div class="tp-agents">
                <ul>
                  {#each AGENTS as agent}
                    <li
                      ><span class="tp-av agent"
                        ><Glyph name="robot" size={15} /></span
                      ><span
                        ><b>{agent.name}</b><span class="tp-caps" dir="ltr"
                          >{#each agent.caps as cap}<code>{cap}</code
                            >{/each}</span
                        ></span
                      ><span class="tp-state" class:ok={agent.active}
                        >{agent.active ? $t('Active') : $t('Paused')}</span
                      ></li
                    >
                  {/each}
                </ul>
                <ol class="tp-audit" dir="ltr">
                  {#each AUDIT as line}<li class:no={!line.ok}
                      ><time>{line.time}</time><span>{line.tool}</span><b
                        >{line.result}</b
                      ></li
                    >{/each}
                </ol>
              </div>
            {:else}
              <div class="tp-wallet">
                <p class="tp-head"
                  ><b>{$t('Team treasury')}</b><span class="tp-chip"
                    >{$t('1 of 2 approvals')}</span
                  ></p
                >
                <div class="tp-balances" dir="ltr">
                  {#each BALANCES as balance}
                    <span
                      ><small>{balance.asset}</small><b>{balance.amount}</b
                      ></span
                    >
                  {/each}
                </div>
                <div class="tp-proposal">
                  <p class="tp-who"
                    ><span class="tp-chip"
                      >{$t('Proposal #{n}', { n: 12 })}</span
                    ><span class="tp-state wait">{$t('Proposed')}</span></p
                  >
                  <b
                    >{$t('Pay {amount} from the team wallet', {
                      amount: ltr('0.4 BTC')
                    })}</b
                  >
                  <p>{$t('Proposed by')} <bdi>{AGENT}</bdi></p>
                  <span class="tp-votes" aria-hidden="true"><i></i><i></i></span
                  >
                </div>
              </div>
            {/if}
          </div>
        {/key}
      </div>
    </div>
  </div>
  <figcaption>
    {#key locked ? 'locked' : view}
      <span class="tour-caption" in:fly={{ y: 6, duration: reduced ? 0 : 280 }}
        ><Glyph name={locked ? 'lock' : current.glyph} size={18} /><b
          >{locked ? $t('Locked') : $t(current.label)}</b
        ><span
          >{locked
            ? $t('Locks after 15 idle minutes, or whenever you choose.')
            : $t(current.detail)}</span
        ></span
      >
    {/key}
    <small>{$t('Interactive example · sample data · nothing is sent')}</small>
  </figcaption>
</figure>
