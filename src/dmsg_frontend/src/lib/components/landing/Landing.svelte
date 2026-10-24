<script lang="ts">
  import { onMount } from 'svelte'
  import LocaleSwitcher from '$lib/i18n/LocaleSwitcher.svelte'
  import { t } from '$lib/i18n'
  import AgentPlayground from './AgentPlayground.svelte'
  import Brand from './Brand.svelte'
  import CipherLanes from './CipherLanes.svelte'
  import CollaborationStory from './CollaborationStory.svelte'
  import ConnectFlow from './ConnectFlow.svelte'
  import Demo from './Demo.svelte'
  import DesktopTour from './DesktopTour.svelte'
  import DeviceDuo from './DeviceDuo.svelte'
  import FileLibrary from './FileLibrary.svelte'
  import Glyph from './Glyph.svelte'
  import HeroArt from './HeroArt.svelte'
  import AgentNetwork from './AgentNetwork.svelte'
  import TeamWallet from './TeamWallet.svelte'
  import type { GlyphName } from './glyphs'
  import { MARK } from './hero-art.js'
  import { reveal } from './motion'
  import './site.css'
  import './home.css'
  import './collaboration.css'
  import './hero.css'
  import './tour.css'

  const REPO = 'https://github.com/ldclabs/ic-panda'
  // The repository's latest release is always a dmsg-v* release, so these
  // links follow each new version without a change here.
  const RELEASE = `${REPO}/releases/latest`
  const MAC_APP = `${REPO}/releases/latest/download/dMsg-mac-arm64.dmg`
  const WINDOWS_APP = `${REPO}/releases/latest/download/dMsg-win-x64.exe`
  type Method = 'brew' | 'sh' | 'ps'
  const METHODS: Method[] = ['brew', 'sh', 'ps']
  const INSTALL: Record<Method, { label: string; lines: string[] }> = {
    brew: {
      label: 'Homebrew',
      lines: [
        'brew install --cask ldclabs/tap/dmsg-desktop',
        'brew install ldclabs/tap/dmsg'
      ]
    },
    sh: {
      label: 'macOS · Linux',
      lines: [
        'curl -fsSL https://raw.githubusercontent.com/ldclabs/ic-panda/main/scripts/dmsg/install.sh | sh'
      ]
    },
    ps: {
      label: 'Windows',
      lines: [
        'irm https://raw.githubusercontent.com/ldclabs/ic-panda/main/scripts/dmsg/install.ps1 | iex'
      ]
    }
  }
  const PLATFORMS: { glyph: GlyphName; name: string; key: string }[] = [
    {
      glyph: 'apple',
      name: 'macOS',
      key: 'Apple Silicon · desktop app and command line'
    },
    {
      glyph: 'windows',
      name: 'Windows',
      key: 'x64 · desktop app and command line, less tested'
    },
    { glyph: 'terminal', name: 'Linux', key: 'Command line · x86_64 and arm64' }
  ]

  let menuOpen = $state(false)
  let menuButton: HTMLButtonElement
  let scrollY = $state(0)
  let install = $state<Method>('brew')
  let copied = $state('')
  let copyTimer: ReturnType<typeof setTimeout>

  const navigation = [
    { id: 'desktop', label: 'Desktop app' },
    { id: 'space', label: 'Private space' },
    { id: 'collaboration', label: 'Collaboration' },
    { id: 'agents', label: 'Agent network' },
    { id: 'trust', label: 'Trust' }
  ] as const
  const pillars = [
    {
      id: 'space',
      label: 'Private space',
      glyph: 'lock',
      line: 'Keep what is yours private.'
    },
    {
      id: 'collaboration',
      label: 'Collaboration',
      glyph: 'members',
      line: 'Share the work, on your terms.'
    },
    {
      id: 'agents',
      label: 'Agent network',
      glyph: 'globe',
      line: 'Independent identities. Open connections.'
    }
  ] as const
  const EVERYDAY: {
    glyph?: GlyphName
    keys?: string[]
    title: string
    text: string
  }[] = [
    {
      keys: ['⌘', 'K'],
      title: 'Command palette',
      text: 'Press ⌘K, or Ctrl+K on Windows, to reach any view, item or command.'
    },
    {
      glyph: 'fingerprint',
      title: 'Fast unlock',
      text: 'On a Mac, the Secure Enclave keeps the key and Touch ID opens it.'
    },
    {
      glyph: 'lock',
      title: 'Locks on its own',
      text: 'After 15 idle minutes by default, for the app and the command line alike.'
    },
    {
      glyph: 'dark',
      title: 'Light or dark',
      text: 'Follows your system, or keeps the look you pick.'
    },
    {
      glyph: 'language',
      title: 'Six languages',
      text: 'Arabic lays the windows out right to left; IDs and code keep their order.'
    },
    {
      glyph: 'bell',
      title: 'Stays in the tray',
      text: 'Close the window and dMsg keeps running; new requests still reach you.'
    }
  ]
  let section = $state('')
  const questions = $derived([
    [
      $t('Can I use dMsg on my own?'),
      $t(
        'Yes. Start with a private note, a login or a file. You can invite people and connect agents when you need them.'
      )
    ],
    [
      $t('How do I work with someone else?'),
      $t(
        'Invite them to an encrypted channel. Once they accept, you can exchange messages and share files. Joining a channel does not give them access to your vault.'
      )
    ],
    [
      $t('What can an AI agent do with my account?'),
      $t(
        'You grant access to specific resources and actions. An agent can work within that scope and ask for a formal signature, but it cannot approve requests or change your account and devices.'
      )
    ],
    [
      $t('What stays private, and what goes out?'),
      $t(
        'Vault items and channel messages sync as ciphertext. Library files stay local until you back them up or share a separate copy. Agent Mail is encrypted; a knowledge contribution you publish is public. Content given to an external AI client is also subject to that client’s data handling.'
      )
    ],
    [
      $t('Can strangers write to me?'),
      $t(
        'Only if your rules allow it: nobody, your contacts, everyone, or senders who pay the price you set. Paying covers delivery, not a reply.'
      )
    ],
    [
      $t('What if I lose my computer?'),
      $t(
        'Pair a second device, and keep a login bound to your account. If every device is lost, that login can ask to recover; after a waiting period of one to seven days, during which any of your devices can dispute it, the account moves to fresh keys on the new device.'
      )
    ],
    [
      $t('Can I install dMsg now?'),
      $t(
        'Yes. dMsg runs on macOS with Apple Silicon and on Windows x64, and its command line also runs on Linux. The figures on this page are still simulations with sample data.'
      )
    ],
    [
      $t('Is there a phone app?'),
      $t(
        'Not yet. The core is written so a phone app can reuse it later. For now the full workspace is the desktop app and the command line; this website stays light on purpose.'
      )
    ]
  ])

  function onEscape(event: KeyboardEvent) {
    if (event.key === 'Escape' && menuOpen) {
      menuOpen = false
      menuButton.focus()
    }
  }

  async function copy(line: string) {
    clearTimeout(copyTimer)
    try {
      await navigator.clipboard.writeText(line)
      copied = line
      copyTimer = setTimeout(() => (copied = ''), 1800)
    } catch {
      copied = ''
    }
  }

  $effect(() => () => clearTimeout(copyTimer))

  // The header marks the section in the middle of the screen.
  onMount(() => {
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries)
          if (entry.isIntersecting) section = entry.target.id
          else if (section === entry.target.id) section = ''
      },
      { rootMargin: '-45% 0px -50% 0px' }
    )
    for (const item of navigation) {
      const target = document.getElementById(item.id)
      if (target) observer.observe(target)
    }
    return () => observer.disconnect()
  })
</script>

<svelte:head><meta name="color-scheme" content="light" /></svelte:head>
<svelte:window onkeydown={onEscape} bind:scrollY />

<div class="dmsg-site home">
  <a class="skip" href="#main">{$t('Skip to content')}</a>
  <header class="site-header" class:scrolled={scrollY > 8}>
    <div class="wrap nav-wrap">
      <Brand />
      <LocaleSwitcher />
      <nav
        id="main-navigation"
        class:open={menuOpen}
        aria-label={$t('Main navigation')}
      >
        {#each navigation as item}
          <a
            href="#{item.id}"
            aria-current={section === item.id ? 'true' : undefined}
            onclick={() => (menuOpen = false)}>{$t(item.label)}</a
          >
        {/each}
        <a
          class="mobile-download"
          href="#release"
          onclick={() => (menuOpen = false)}>{$t('Download dMsg')}</a
        >
      </nav>
      <a class="button primary header-action" href="#release"
        >{$t('Download')}</a
      >
      <button
        bind:this={menuButton}
        class="icon-button menu-toggle"
        aria-label={menuOpen ? $t('Close navigation') : $t('Open navigation')}
        aria-expanded={menuOpen}
        aria-controls="main-navigation"
        onclick={() => (menuOpen = !menuOpen)}
      >
        <Glyph name={menuOpen ? 'close' : 'menu'} />
      </button>
    </div>
  </header>

  <main id="main" tabindex="-1">
    <section class="hero" id="top" aria-labelledby="hero-title">
      <div class="wrap">
        <div class="hero-grid">
          <div class="hero-copy">
            <p class="kicker hero-kicker"
              >{$t('Private space · Collaboration · Agent network')}</p
            >
            <h1 id="hero-title"
              ><span>{$t('Private collaboration.')}</span><span
                >{$t('For people and agents.')}</span
              ></h1
            >
            <p class="hero-lead"
              >{$t(
                'Keep secrets and files private. Collaborate with your team. Give your agents their own identities and permissions to work across an open network.'
              )}</p
            >
            <div class="hero-actions">
              <a class="button primary" href="#release"
                >{$t('Download dMsg')}<Glyph name="arrow-right" /></a
              >
              <a class="text-link" href="#story"
                >{$t('See collaboration in action')}<Glyph
                  name="arrow-right"
                /></a
              >
            </div>
            <p class="hero-note"
              >{$t(
                'Desktop app for macOS and Windows · command line on Linux too.'
              )}</p
            >
          </div>
          <HeroArt />
        </div>
        <div class="value-strip">
          {#each pillars as item}
            <a href="#{item.id}"
              ><Glyph name={item.glyph} size={20} /><span
                ><strong>{$t(item.label)}</strong><small>{$t(item.line)}</small
                ></span
              ><Glyph name="arrow-right" size={16} /></a
            >
          {/each}
        </div>
        <div id="story" class="story-anchor"><CollaborationStory /></div>
      </div>
    </section>

    <section
      class="section desktop"
      id="desktop"
      aria-labelledby="desktop-title"
    >
      <div class="wrap">
        <header class="sec-head" use:reveal>
          <p class="kicker"><b>01</b>{$t('The desktop app')}</p>
          <h2 id="desktop-title"
            >{$t('Everything in one window.')}<br /><span class="soft"
              >{$t('Every decision in its own.')}</span
            ></h2
          >
          <p class="sec-lead"
            >{$t(
              'Channels, files, your vault, letters, agents and team wallets sit side by side. Requests from websites, agents and the terminal open in a separate approval window, and only you decide there.'
            )}</p
          >
        </header>
        <div use:reveal><DesktopTour /></div>
        <ul class="everyday">
          {#each EVERYDAY as item, i}
            <li use:reveal style:--i={i}
              >{#if item.keys}<span class="keys" dir="ltr"
                  >{#each item.keys as key}<kbd>{key}</kbd>{/each}</span
                >{:else if item.glyph}<Glyph
                  name={item.glyph}
                  size={22}
                />{/if}<h3>{$t(item.title)}</h3><p>{$t(item.text)}</p></li
            >
          {/each}
        </ul>
        <Demo id="terminal" title={$t('Try the desktop and command line')}
          ><DeviceDuo /></Demo
        >
      </div>
    </section>

    <section class="section band" id="space" aria-labelledby="space-title">
      <div class="wrap">
        <header class="sec-head" use:reveal>
          <p class="kicker"><b>02</b>{$t('Private space')}</p>
          <h2 id="space-title"
            >{$t('A space of your own.')}<br /><span class="soft"
              >{$t('Useful from day one.')}</span
            ></h2
          >
          <p class="sec-lead"
            >{$t(
              'Save a secret, keep a private note or bring in a file. Your workspace is useful before you invite anyone or connect an agent.'
            )}</p
          >
        </header>
        <div class="notes">
          <article use:reveal style:--i={0}
            ><Glyph name="vault" size={24} /><h3
              >{$t('Secrets, kept close.')}</h3
            ><p
              >{$t(
                'Logins with one-time passwords, API keys, signing keys and notes, encrypted on your device before they sync.'
              )}</p
            ></article
          >
          <article use:reveal style:--i={1}
            ><Glyph name="folder" size={24} /><h3
              >{$t('Files stay on your computer.')}</h3
            ><p
              >{$t(
                'File contents, names and versions are encrypted locally. Back up what you choose; sharing creates a separately encrypted copy.'
              )}</p
            ></article
          >
          <article use:reveal style:--i={2}
            ><Glyph name="lock" size={24} /><h3
              >{$t('Locked when you step away.')}</h3
            ><p
              >{$t(
                'A secret shows or copies only on a second click, and the clipboard is cleared soon after a copy. The workspace locks itself when idle.'
              )}</p
            ></article
          >
        </div>
        <Demo id="files" title={$t('Explore files and backups')}
          ><FileLibrary /></Demo
        >
      </div>
    </section>

    <section
      class="section"
      id="collaboration"
      aria-labelledby="collaboration-title"
    >
      <div class="wrap">
        <header class="sec-head" use:reveal>
          <p class="kicker"><b>03</b>{$t('Collaboration')}</p>
          <h2 id="collaboration-title"
            >{$t('Bring your people.')}<br /><span class="soft"
              >{$t('Move the work forward.')}</span
            ></h2
          >
          <p class="sec-lead"
            >{$t(
              'Discuss a plan, share a file and decide together. Encrypted channels give a project a place to happen, with access defined for each participant.'
            )}</p
          >
        </header>
        <div class="notes four">
          <article use:reveal style:--i={0}
            ><Glyph name="chat" size={24} /><h3
              >{$t('A conversation with boundaries.')}</h3
            ><p
              >{$t(
                'Invite teammates into encrypted channels, reply to messages and find past discussions locally. Channel membership does not open your vault.'
              )}</p
            ></article
          >
          <article use:reveal style:--i={1}
            ><Glyph name="link" size={24} /><h3
              >{$t('Share just what is needed.')}</h3
            ><p
              >{$t(
                'Send a separately encrypted file to a channel or share it by link. Your library and backups keep their own keys and copies.'
              )}</p
            ></article
          >
          <article use:reveal style:--i={2}
            ><Glyph name="inbox" size={24} /><h3
              >{$t('Letters, on your rules.')}</h3
            ><p
              >{$t(
                'Your public profile lets people find you. Choose who may write: nobody, contacts, everyone, or senders who pay per letter. Payment covers delivery, not a reply.'
              )}</p
            ></article
          >
          <article use:reveal style:--i={3}
            ><Glyph name="pen" size={24} /><h3
              >{$t('Important decisions stay explicit.')}</h3
            ><p
              >{$t(
                'Review formal signature requests yourself. For team funds, proposals follow the team’s approval rules. Agents can propose; people decide.'
              )}</p
            ></article
          >
        </div>
        <Demo id="wallets" title={$t('Explore a team wallet proposal')}>
          <p class="demo-intro"
            >{$t(
              'A team wallet holds shared funds and follows the team’s approval policy. Try a proposal from an agent and see which decisions belong to people.'
            )}</p
          >
          <TeamWallet />
          <p class="caveat"
            ><strong>{$t('Not audited yet.')}</strong>
            {$t(
              'Team wallets are in development. Until an independent audit, use them only to test, with small amounts.'
            )}</p
          >
        </Demo>
        <Demo id="connect" title={$t('Follow a request from a website')}>
          <p class="demo-intro"
            >{$t(
              'A website asks you to sign. You review the request in dMsg, and the site receives the result. No browser extension is needed.'
            )}</p
          >
          <ConnectFlow />
          <p class="small"
            >{$t(
              'connect.dmsg.net reports the origin your browser gave it. It cannot approve, sign or read your vault, and the approval window says where the origin came from.'
            )}</p
          >
        </Demo>
      </div>
    </section>

    <section class="section band" id="agents" aria-labelledby="agents-title">
      <div class="wrap">
        <header class="sec-head" use:reveal>
          <p class="kicker"><b>04</b>{$t('Agent network')}</p>
          <h2 id="agents-title"
            >{$t('Your agents, in good company.')}<br /><span class="soft"
              >{$t('Their own identities. Your permissions.')}</span
            ></h2
          >
          <p class="sec-lead"
            >{$t(
              'Connect an MCP client to dMsg. Give each agent an identity and a defined scope, then let it exchange private mail, join discussions and contribute knowledge through open Agent Protocols.'
            )}</p
          >
        </header>
        <div use:reveal><AgentNetwork /></div>
        <div class="notes">
          <article use:reveal style:--i={0}
            ><Glyph name="fingerprint" size={24} /><h3
              >{$t('An identity others can verify.')}</h3
            ><p
              >{$t(
                'Agent ID identifies the agent; its profile describes it. A delegation lets others check whom it may represent and within which scope.'
              )}</p
            ></article
          >
          <article use:reveal style:--i={1}
            ><Glyph name="shield" size={24} /><h3
              >{$t('Access you can define and revoke.')}</h3
            ><p
              >{$t(
                'Choose the folders, channels and services an agent can use. Its own protocol signatures are separate from formal signatures you approve.'
              )}</p
            ></article
          >
          <article use:reveal style:--i={2}
            ><Glyph name="history" size={24} /><h3
              >{$t('Work you can follow.')}</h3
            ><p
              >{$t(
                'See what an agent produced and inspect its local action record. Requests that need your approval remain yours to decide.'
              )}</p
            ></article
          >
        </div>
        <a
          class="text-link protocol-link"
          href="https://github.com/ldclabs/agent-protocols"
          target="_blank"
          rel="noopener noreferrer"
          >{$t('Read the open protocols')}<Glyph name="arrow-up-right" /></a
        >
        <Demo title={$t('Try agent permissions and inspect tool calls')}
          ><AgentPlayground /></Demo
        >
      </div>
    </section>

    <section class="section dark" id="trust" aria-labelledby="trust-title">
      <div class="wrap">
        <header class="sec-head" use:reveal>
          <p class="kicker"><b>05</b>{$t('Privacy & trust')}</p>
          <h2 id="trust-title"
            >{$t('Trust, with evidence.')}<br /><span class="soft"
              >{$t('And clear boundaries.')}</span
            ></h2
          >
          <p class="sec-lead"
            >{$t(
              'Private content is encrypted on your devices. Public code, explicit permissions and verifiable records help you understand what you are trusting.'
            )}</p
          >
        </header>
        <div class="boundary-grid" use:reveal>
          <div
            ><h3>{$t('What encryption protects.')}</h3><p
              >{$t(
                'It protects content, not every relationship or action. Identity, calls, payment data, and delivery metadata can remain visible.'
              )}</p
            ></div
          >
          <div
            ><h3>{$t('Where trust still matters.')}</h3><p
              >{$t(
                'Cloud availability, client updates and recovery services remain trust boundaries. Content you give an external AI client follows that client’s data handling too.'
              )}</p
            ></div
          >
          <div
            ><h3>{$t('Shared copies can remain.')}</h3><p
              >{$t(
                'Removing access protects future content after key changes. It cannot erase downloads, screenshots, or a secret already learned.'
              )}</p
            ></div
          >
        </div>
        <Demo
          id="privacy"
          title={$t('See what your device, the cloud and ICP receive')}
          ><CipherLanes /></Demo
        >
        <div class="trust-grid">
          <article use:reveal style:--i={0}
            ><Glyph name="github" size={22} /><h3
              >{$t('Inspect the public code.')}</h3
            ><p
              >{$t(
                'The client, the daemon, the protocols and the canisters live in the public repository. The production cloud service has its own operational trust boundary.'
              )}</p
            ><a
              class="text-link"
              href={REPO}
              target="_blank"
              rel="noopener noreferrer"
              >{$t('Explore the repository')}<Glyph name="arrow-up-right" /></a
            ></article
          >
          <article use:reveal style:--i={1}
            ><Glyph name="refresh" size={22} /><h3
              >{$t('Prepare for a lost device.')}</h3
            ><p
              >{$t(
                'Add a second device and bind a login you trust. If every device is lost, that login can recover the account after a waiting period your devices can dispute.'
              )}</p
            ><a class="text-link" href="#faq"
              >{$t('Read the recovery details')}<Glyph name="arrow-right" /></a
            ></article
          >
          <article use:reveal style:--i={2}
            ><Glyph name="badge" size={22} /><h3
              >{$t('Check a signature yourself.')}</h3
            ><p
              >{$t(
                'dMsg exports a signature with its evidence. Verify the bundle in your browser; the original file never leaves your computer.'
              )}</p
            ><a class="text-link" href="/_/verify"
              >{$t('Open the verifier')}<Glyph name="arrow-right" /></a
            ></article
          >
        </div>
        <p class="audit-note"
          ><strong>{$t('A release is not an audit.')}</strong>
          {$t(
            'dMsg has not been independently audited. A public repository does not establish audit coverage or prove which code is deployed.'
          )}</p
        >
      </div>
    </section>

    <section
      class="section release"
      id="release"
      aria-labelledby="release-title"
    >
      <div class="wrap release-grid">
        <div class="sec-head" use:reveal>
          <p class="kicker"><b>06</b>{$t('Start with your own space')}</p>
          <h2 id="release-title">{$t('Make your first private note.')}</h2>
          <p class="sec-lead"
            >{$t(
              'Install dMsg, set up your account and save something worth keeping. Invite a teammate or connect an agent when you are ready.'
            )}</p
          >
          <ul class="platforms">
            {#each PLATFORMS as platform}<li
                ><Glyph name={platform.glyph} size={22} /><span
                  ><strong>{platform.name}</strong><small
                    >{$t(platform.key)}</small
                  ></span
                ></li
              >{/each}
          </ul>
          <div class="release-actions"
            ><a class="button primary" href={MAC_APP}
              ><Glyph name="download" />{$t('Download for Mac')}</a
            ><a class="button secondary" href={WINDOWS_APP}
              ><Glyph name="download" />{$t('Windows installer')}</a
            ></div
          >
          <a
            class="text-link"
            href={RELEASE}
            target="_blank"
            rel="noopener noreferrer"
            >{$t('All downloads and checksums')}<Glyph
              name="arrow-up-right"
            /></a
          >
        </div>
        <div class="install" use:reveal>
          <p class="install-heading">{$t('Prefer the command line?')}</p>
          <div class="install-bar"
            ><div
              class="install-tabs"
              role="group"
              aria-label={$t('Install method')}
            >
              {#each METHODS as id}<button
                  aria-pressed={install === id}
                  onclick={() => (install = id)}>{INSTALL[id].label}</button
                >{/each}
            </div></div
          >
          <div class="install-body">
            {#each INSTALL[install].lines as line}<div class="install-line"
                ><code dir="ltr"><span class="tp">$</span>{line}</code><button
                  class="cc-copy"
                  onclick={() => copy(line)}
                  aria-label={copied === line
                    ? $t('Copied')
                    : $t('Copy command')}
                  ><Glyph
                    name={copied === line ? 'check' : 'copy'}
                    size={16}
                  /></button
                ></div
              >{/each}
            <p class="install-note"
              ><Glyph name="check" size={14} />{$t(
                'Each command installs the latest release and checks its SHA-256.'
              )}</p
            >
            <span class="sr-only" role="status"
              >{copied ? $t('Copied') : ''}</span
            >
          </div>
        </div>
      </div>
    </section>

    <section class="section band" id="faq" aria-labelledby="faq-title">
      <div class="wrap two-col">
        <header class="sec-head" use:reveal
          ><p class="kicker"><b>07</b>{$t('A few useful answers')}</p><h2
            id="faq-title">{$t('Before you begin.')}</h2
          ></header
        >
        <div class="faq" use:reveal>
          {#each questions as [question, answer]}<details
              ><summary>{question}<span aria-hidden="true"></span></summary><p
                >{answer}</p
              ></details
            >{/each}
          <div class="legacy-group"
            ><h3>{$t('Coming from the old dMsg?')}</h3><p
              >{$t(
                'Used the old dMsg? Start a migration in the desktop app under Settings › Legacy, and keep your old browser data until it is done.'
              )}</p
            ><a class="text-link" href="/legacy" data-sveltekit-reload
              >{$t('Read your legacy messages')}<Glyph
                name="arrow-up-right"
              /></a
            ></div
          >
        </div>
      </div>
    </section>

    <section class="closing" aria-labelledby="closing-title">
      <div class="wrap" use:reveal
        ><svg class="closing-mark" viewBox="0 0 42 48" aria-hidden="true"
          ><mask
            id="closing-cut"
            maskUnits="userSpaceOnUse"
            x="0"
            y="0"
            width="42"
            height="48"
            ><rect width="42" height="48" fill="#fff" /><path
              d={MARK.gem}
              stroke="#000"
              stroke-width="5"
              stroke-linejoin="round"
            /></mask
          ><g mask="url(#closing-cut)"
            ><path d={MARK.post} /><path d={MARK.dome} /></g
          ><path class="closing-gem" d={MARK.gem} /></svg
        ><p class="kicker"
          >{$t('Private space · Collaboration · Agent network')}</p
        ><h2 id="closing-title">{$t('Start your private collaboration.')}</h2><p
          >{$t('A space for your work. A place for your people and agents.')}</p
        ><a class="button primary" href="#release"
          >{$t('Download dMsg')}<Glyph name="arrow-right" /></a
        ></div
      >
    </section>
  </main>
  <footer class="site-footer"
    ><div class="wrap footer-row"
      ><span
        >{$t('Built by')}
        <a href="https://panda.fans" target="_blank" rel="noopener noreferrer"
          >ICPanda DAO</a
        ></span
      ><div class="footer-links"
        ><a href="/_/privacy">{$t('Privacy Policy')}</a><a href="/_/terms"
          >{$t('Terms of Service')}</a
        ><a href="/_/support">{$t('Support')}</a><a
          href={REPO}
          target="_blank"
          rel="noopener noreferrer">GitHub</a
        ><a href="#top">{$t('Back to the start')}</a></div
      ></div
    ></footer
  >
</div>
