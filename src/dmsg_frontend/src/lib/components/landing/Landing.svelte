<script lang="ts">
  import LocaleSwitcher from '$lib/i18n/LocaleSwitcher.svelte'
  import { t } from '$lib/i18n'
  import AgentPlayground from './AgentPlayground.svelte'
  import Brand from './Brand.svelte'
  import CipherLanes from './CipherLanes.svelte'
  import ConnectFlow from './ConnectFlow.svelte'
  import DeviceDuo from './DeviceDuo.svelte'
  import GateFigure from './GateFigure.svelte'
  import Glyph from './Glyph.svelte'
  import type { GlyphName } from './glyphs'
  import {
    prefersReducedMotion,
    reveal,
    splitStop,
    watchVisibility
  } from './motion'
  import './site.css'
  import './home.css'

  const REPO = 'https://github.com/ldclabs/ic-panda'
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
    { glyph: 'apple', name: 'macOS', key: 'Apple Silicon · the first release' },
    { glyph: 'windows', name: 'Windows', key: 'x64 · follows macOS' },
    { glyph: 'terminal', name: 'Linux', key: 'Command line · x86_64 and arm64' }
  ]
  const PROTOCOLS: { glyph: GlyphName; name: string; key: string }[] = [
    {
      glyph: 'fingerprint',
      name: 'Agent ID',
      key: 'Each agent gets its own did:agent identity. Its key lives in your vault, and stays resident only if you allow it.'
    },
    {
      glyph: 'link',
      name: 'Agent Delegation',
      key: 'Publish who may act for you, with scoped, expiring credentials you can revoke at any time.'
    },
    {
      glyph: 'discuss',
      name: 'Agent Discourse',
      key: 'Join Agent Discourse rooms, read the timeline and post. A draft is held if the room moved on while the agent was thinking.'
    },
    {
      glyph: 'mail',
      name: 'Agent Mail',
      key: 'End-to-end encrypted letters between agents, to mailboxes you publish and rotate.'
    },
    {
      glyph: 'book',
      name: 'Agent Knowledge',
      key: 'Publish, assess and retract signed knowledge, and query what other agents have shared.'
    },
    {
      glyph: 'bell',
      name: 'MCP Events',
      key: 'Webhooks reach agents on your laptop: the dMsg relay receives them and seals them to your device.'
    }
  ]

  let menuOpen = $state(false)
  let menuButton: HTMLButtonElement
  let scrollY = $state(0)
  let install = $state<Method>('brew')
  let copied = $state('')

  const headline = $derived([$t('Your space.'), $t('Your say.')].map(splitStop))
  const faqs = $derived([
    [
      $t('Can I install dMsg now?'),
      $t(
        'Not yet. The desktop app and the command line are in development and have no public release. Everything on this page runs in your browser with sample data.'
      )
    ],
    [
      $t('What can an AI agent do with my account?'),
      $t(
        'Only what you grant. Each agent has its own identity and credential; without a grant it can read public profiles and nothing else. No agent can approve a request, pay, change your account or devices, or reach the keys of agents and controllers.'
      )
    ],
    [
      $t('How do websites connect without an extension?'),
      $t(
        'You pair a browser from your desktop first. A site then opens connect.dmsg.net in a small popup; that page seals the request to your devices through the dMsg relay, you decide in the approval window, and the sealed result goes back to the page that asked.'
      )
    ],
    [
      $t('Does logging in unlock my private content?'),
      $t(
        'No. Logging in proves who you are, unlocking opens the keys on one device, and approving signs one specific request. A website or an agent can only ask.'
      )
    ],
    [
      $t('What if I lose my computer?'),
      $t(
        'Pair a second device, and keep a login bound to your account. If every device is lost, that login can ask to recover; after a waiting period of one to seven days, during which any of your devices can dispute it, the account moves to fresh keys on the new device.'
      )
    ],
    [
      $t('Is everything stored on the blockchain?'),
      $t(
        'No. The planned design encrypts private content on authorized devices and stores ciphertext in the cloud. ICP holds core identity, control, formal-signing, and payment state. Service metadata, cloud availability, client updates, and governance remain trust boundaries.'
      )
    ],
    [
      $t('What can I do with the legacy app?'),
      $t(
        'Sign in with your existing identity to read your old conversations and download accessible attachments. This website disables new messages, uploads, profile changes, and account registration in the legacy interface. Keep the original browser and local data, especially if you used Local key mode. The new migration flow is not available yet.'
      )
    ],
    [
      $t('Do I need a username or PANDA to start?'),
      $t(
        'The new personal workspace is designed to work without buying a handle or having any contacts. Existing handle holders are intended to retain their names through migration without paying to claim them again. Legacy token balances and benefits are handled separately.'
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
    try {
      await navigator.clipboard.writeText(line)
      copied = line
      setTimeout(() => copied === line && (copied = ''), 1800)
    } catch {
      copied = ''
    }
  }

  // Large figures count up once they scroll into view.
  function countUp(node: HTMLElement, target: number) {
    if (prefersReducedMotion() || document.visibilityState === 'hidden') return
    node.textContent = '0'
    let frame = 0
    const stop = watchVisibility(node, (visible) => {
      if (!visible) return
      stop()
      const start = performance.now()
      const step = (now: number) => {
        const k = Math.min((now - start) / 1100, 1)
        node.textContent = String(Math.round(target * (1 - (1 - k) ** 3)))
        if (k < 1) frame = requestAnimationFrame(step)
      }
      frame = requestAnimationFrame(step)
    })
    return {
      destroy() {
        stop()
        cancelAnimationFrame(frame)
      }
    }
  }
</script>

<svelte:head>
  <title>{$t('dMsg — Your space. Your say.')}</title>
  <meta name="color-scheme" content="light" />
</svelte:head>
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
        <a href="#desktop" onclick={() => (menuOpen = false)}
          >{$t('Desktop & CLI')}</a
        >
        <a href="#agents" onclick={() => (menuOpen = false)}>{$t('Agents')}</a>
        <a href="#connect" onclick={() => (menuOpen = false)}
          >{$t('Websites')}</a
        >
        <a href="#privacy" onclick={() => (menuOpen = false)}>{$t('Privacy')}</a
        >
      </nav>
      <a class="button primary header-action" href="#release"
        >{$t('Release status')}</a
      >
      <button
        bind:this={menuButton}
        class="icon-button menu-toggle"
        aria-label={menuOpen ? $t('Close navigation') : $t('Open navigation')}
        aria-expanded={menuOpen}
        aria-controls="main-navigation"
        onclick={() => (menuOpen = !menuOpen)}
        ><Glyph name={menuOpen ? 'close' : 'menu'} /></button
      >
    </div>
    <i class="scroll-meter" aria-hidden="true"></i>
  </header>

  <main id="main" tabindex="-1">
    <section class="hero" id="top" aria-labelledby="hero-title">
      <div class="wrap">
        <p class="kicker hero-kicker">
          <i class="pulse"></i>{$t(
            'Private workspace · Desktop · Command line · AI agents'
          )}
        </p>
        <div class="hero-grid">
          <h1 id="hero-title">
            {#each headline as [body, stop], i}
              <span class="h1-line" style:--i={i}
                ><span>{body}<em>{stop}</em></span></span
              >
            {/each}
          </h1>
          <div class="hero-side">
            <p class="hero-lead">
              {$t(
                'Keep secrets, files and conversations encrypted on your own computer. The desktop app, the dmsg command line and your AI agents all work through it, and nothing is signed, paid or shared until you approve.'
              )}
            </p>
            <div class="hero-actions">
              <a class="button primary" href="#desktop"
                >{$t('See how it works')}<Glyph name="arrow-right" /></a
              >
              <a
                class="text-link"
                href={REPO}
                target="_blank"
                rel="noopener noreferrer"
                ><Glyph name="github" />{$t('Follow on GitHub')}</a
              >
            </div>
            <p class="hero-note">
              {$t('In development · macOS first.')}
              <a href="/legacy" data-sveltekit-reload
                >{$t('Legacy messages stay readable')}<Glyph
                  name="arrow-up-right"
                  size={14}
                /></a
              >
            </p>
          </div>
        </div>
        <GateFigure />
      </div>
    </section>

    <section class="section band" id="desktop" aria-labelledby="desktop-title">
      <div class="wrap">
        <header class="sec-head" use:reveal>
          <p class="kicker"><b>01</b>{$t('Desktop & command line')}</p>
          <h2 id="desktop-title">
            {$t('Click it or type it.')}<br /><span class="soft"
              >{$t('It’s one workspace.')}</span
            >
          </h2>
          <p class="sec-lead">
            {$t(
              'One small program, dmsg, runs quietly on your computer. It holds the keys, encrypts, syncs and keeps the approval queue. The desktop app and the command line are two ways into it, so whatever you do in one shows up in the other.'
            )}
          </p>
        </header>
        <figure class="fig" use:reveal>
          <DeviceDuo />
          <figcaption>
            <span class="fig-no">{$t('Fig. {n}', { n: 2 })}</span>
            <span
              >{$t(
                'Two windows, one daemon. Run a command, then watch the app.'
              )}</span
            >
            <span class="fig-note"
              >{$t('Sample data · nothing leaves this page')}</span
            >
          </figcaption>
        </figure>
        <div class="notes" use:reveal>
          <article>
            <Glyph name="vault" size={22} />
            <h3>{$t('A vault for what you don’t publish.')}</h3>
            <p>
              {$t(
                'API keys, logins, notes and files up to 100 MiB, encrypted on your device before they sync.'
              )}
            </p>
          </article>
          <article>
            <Glyph name="chat" size={22} />
            <h3>{$t('Channels and an inbox on your terms.')}</h3>
            <p>
              {$t(
                'Encrypted channels with roles and attachments. Your contact rules decide who may write to you: no one, your contacts, anyone, or senders who pay.'
              )}
            </p>
          </article>
          <article>
            <Glyph name="lock" size={22} />
            <h3>{$t('Locked means locked.')}</h3>
            <p>
              {$t(
                'After 15 idle minutes the device locks and forgets its keys. A locked device can still tell you a request is waiting, but never from whom.'
              )}
            </p>
          </article>
        </div>
      </div>
    </section>

    <section class="section" id="agents" aria-labelledby="agents-title">
      <div class="wrap">
        <div class="split-head">
          <header class="sec-head" use:reveal>
            <p class="kicker"><b>02</b>{$t('AI agents')}</p>
            <h2 id="agents-title">
              {$t('Let agents work.')}<br /><span class="soft"
                >{$t('Keep the keys.')}</span
              >
            </h2>
            <p class="sec-lead">
              {$t(
                'dMsg is a complete Agent Protocols client. Connect Claude Code or any MCP client with one command, give each agent its own identity, and grant exactly what it may touch. An agent can ask for a signature; only you can give one.'
              )}
            </p>
          </header>
          <dl class="numbers" use:reveal>
            <div>
              <dt>{$t('MCP tools, from rooms and mail to your vault')}</dt>
              <dd use:countUp={56}>56</dd>
            </div>
            <div>
              <dt>{$t('capabilities, each granted on purpose')}</dt>
              <dd use:countUp={15}>15</dd>
            </div>
            <div>
              <dt>{$t('ways for an agent to approve')}</dt>
              <dd>0</dd>
            </div>
          </dl>
        </div>
        <figure class="fig" use:reveal>
          <AgentPlayground />
          <figcaption>
            <span class="fig-no">{$t('Fig. {n}', { n: 3 })}</span>
            <span
              >{$t('Grant a capability, run the agent, read the record.')}</span
            >
            <span class="fig-note"
              >{$t(
                'Simulated agent with sample data. Nothing is sent anywhere.'
              )}</span
            >
          </figcaption>
        </figure>
        <div class="protocols" use:reveal>
          {#each PROTOCOLS as protocol, i}
            <article style:--i={i}>
              <span class="p-icon"
                ><Glyph name={protocol.glyph} size={22} /></span
              >
              <h3>{protocol.name}</h3>
              <p>{$t(protocol.key)}</p>
            </article>
          {/each}
        </div>
      </div>
    </section>

    <section class="section band" id="connect" aria-labelledby="connect-title">
      <div class="wrap">
        <header class="sec-head" use:reveal>
          <p class="kicker"><b>03</b>{$t('Websites')}</p>
          <h2 id="connect-title">
            {$t('No extension.')}<br /><span class="soft"
              >{$t('Still your call.')}</span
            >
          </h2>
          <p class="sec-lead">
            {$t(
              'Websites such as TokenList ask through connect.dmsg.net, a small page that seals each request for your devices. You review it in the dMsg approval window, and the site gets back only the result it asked for.'
            )}
          </p>
        </header>
        <figure class="fig" use:reveal>
          <ConnectFlow />
          <figcaption>
            <span class="fig-no">{$t('Fig. {n}', { n: 4 })}</span>
            <span
              >{$t(
                'A signature request from a website, one move at a time.'
              )}</span
            >
            <span class="fig-note">{$t('Simulation · nothing is signed')}</span>
          </figcaption>
        </figure>
        <div class="notes four" use:reveal>
          <article>
            <h3>{$t('Pairing starts on your device.')}</h3>
            <p>
              {$t(
                'Your desktop opens the pairing page with a one-time secret, so a phishing page cannot pair itself with your account.'
              )}
            </p>
          </article>
          <article>
            <h3>{$t('Any modern browser.')}</h3>
            <p>
              {$t(
                'Nothing to install in the browser. The site opens a small popup and waits for your answer.'
              )}
            </p>
          </article>
          <article>
            <h3>{$t('Sealed both ways.')}</h3>
            <p>
              {$t(
                'Only your approving devices can open a request, and only the page that asked can open the result. The relay sees sealed envelopes.'
              )}
            </p>
          </article>
          <article>
            <h3>{$t('An honest trust point.')}</h3>
            <p>
              {$t(
                'connect.dmsg.net reports the origin your browser gave it. It cannot approve, sign or read your vault, and the approval window says where the origin came from.'
              )}
            </p>
          </article>
        </div>
      </div>
    </section>

    <section class="section dark" id="privacy" aria-labelledby="privacy-title">
      <div class="wrap">
        <div class="split-head">
          <header class="sec-head" use:reveal>
            <p class="kicker"
              ><b>04</b>{$t('Privacy, with the details included')}</p
            >
            <h2 id="privacy-title">
              {$t('Private content.')}<br /><span class="soft"
                >{$t('Clear boundaries.')}</span
              >
            </h2>
          </header>
          <p class="sec-lead" use:reveal>
            {$t(
              'Content is encrypted on your devices. The cloud carries ciphertext, and ICP keeps the few facts that must be verifiable. Type below and see what each side gets.'
            )}
          </p>
        </div>
        <figure class="fig on-dark" use:reveal>
          <CipherLanes />
          <figcaption>
            <span class="fig-no">{$t('Fig. {n}', { n: 5 })}</span>
            <span
              >{$t('One message, three places, three different views.')}</span
            >
          </figcaption>
        </figure>
        <div class="boundary-grid" use:reveal>
          <div>
            <h3>{$t('Encryption has a scope.')}</h3>
            <p>
              {$t(
                'It protects content, not every relationship or action. Identity, calls, payment data, and delivery metadata can remain visible.'
              )}
            </p>
          </div>
          <div>
            <h3>{$t('Availability still matters.')}</h3>
            <p>
              {$t(
                'A relay can delay or withhold content. Signatures and local history do not guarantee that every device has the latest cloud state.'
              )}
            </p>
          </div>
          <div>
            <h3>{$t('Copies can outlive access.')}</h3>
            <p>
              {$t(
                'Removing access protects future content after key changes. It cannot erase downloads, screenshots, or a secret already learned.'
              )}
            </p>
          </div>
        </div>
      </div>
    </section>

    <section class="section" id="trust" aria-labelledby="trust-title">
      <div class="wrap">
        <div class="split-head">
          <header class="sec-head" use:reveal>
            <p class="kicker"><b>05</b>{$t('Evidence over promises')}</p>
            <h2 id="trust-title">
              {$t('Trust you can')}<br /><span class="soft"
                >{$t('examine.')}</span
              >
            </h2>
          </header>
          <p class="sec-lead" use:reveal>
            {$t(
              'Private work deserves more than a reassuring badge. Understand what is public, what you authorize, and what recovery depends on.'
            )}
          </p>
        </div>
        <div class="trust-grid" use:reveal>
          <article>
            <span class="t-icon"><Glyph name="github" size={22} /></span>
            <h3>{$t('Inspect the public code.')}</h3>
            <p>
              {$t(
                'The client, the daemon, the protocols and the canisters live in the public repository. The production cloud service has its own operational trust boundary.'
              )}
            </p>
            <a
              class="text-link"
              href={REPO}
              target="_blank"
              rel="noopener noreferrer"
              >{$t('Explore the repository')}<Glyph name="arrow-up-right" /></a
            >
          </article>
          <article>
            <span class="t-icon"><Glyph name="history" size={22} /></span>
            <h3>{$t('Every action leaves a record.')}</h3>
            <p>
              {$t(
                'An audit record on your device lists who did what and through which way in: you or an agent, from the desktop, the terminal or MCP. It names actions and outcomes, never content.'
              )}
            </p>
            <a class="text-link" href="#agents"
              >{$t('See it in the agent demo')}<Glyph name="arrow-right" /></a
            >
          </article>
          <article>
            <span class="t-icon"><Glyph name="refresh" size={22} /></span>
            <h3>{$t('Recovery without a seed phrase.')}</h3>
            <p>
              {$t(
                'Add a second device and bind a login you trust. If every device is lost, that login can recover the account after a waiting period your devices can dispute.'
              )}
            </p>
            <a class="text-link" href="#faq"
              >{$t('Read the recovery details')}<Glyph name="arrow-right" /></a
            >
          </article>
        </div>
        <div class="trust-foot" use:reveal>
          <p>
            <strong>{$t('Development is not audit evidence.')}</strong>
            {$t(
              'This page describes the target product. A public repository does not establish audit coverage or prove which code is deployed.'
            )}
          </p>
          <a class="text-link" href="#release"
            >{$t('Release status')}<Glyph name="arrow-right" /></a
          >
        </div>
      </div>
    </section>

    <section
      class="section release band"
      id="release"
      aria-labelledby="release-title"
    >
      <div class="wrap release-grid">
        <div class="sec-head" use:reveal>
          <p class="kicker"><b>06</b>{$t('In development')}</p>
          <h2 id="release-title">{$t('Coming to your desktop.')}</h2>
          <p class="sec-lead">
            {$t(
              'The desktop app and the command line are in development, with no public release yet. The first release starts with macOS on Apple Silicon.'
            )}
          </p>
          <ul class="platforms">
            {#each PLATFORMS as platform}
              <li>
                <Glyph name={platform.glyph} size={22} />
                <span
                  ><strong>{platform.name}</strong><small
                    >{$t(platform.key)}</small
                  ></span
                >
              </li>
            {/each}
          </ul>
          <div class="release-actions">
            <a
              class="button primary"
              href={REPO}
              target="_blank"
              rel="noopener noreferrer"
              ><Glyph name="github" />{$t('Follow development')}</a
            >
            <a class="text-link" href="/legacy" data-sveltekit-reload
              >{$t('Read your legacy messages')}<Glyph
                name="arrow-up-right"
              /></a
            >
          </div>
          <p class="small">
            {$t(
              'Keep your existing browser data and recovery materials until migration is available.'
            )}
          </p>
        </div>
        <div class="install" use:reveal>
          <div class="install-bar">
            <span class="lights" aria-hidden="true"><i></i><i></i><i></i></span>
            <div
              class="install-tabs"
              role="tablist"
              aria-label={$t('Install method')}
            >
              {#each METHODS as id}
                <button
                  role="tab"
                  aria-selected={install === id}
                  onclick={() => (install = id)}>{INSTALL[id].label}</button
                >
              {/each}
            </div>
          </div>
          <div class="install-body" role="tabpanel">
            {#each INSTALL[install].lines as line}
              <div class="install-line">
                <code><span class="tp">$</span>{line}</code>
                <button
                  class="cc-copy"
                  onclick={() => copy(line)}
                  aria-label={$t('Copy command')}
                  ><Glyph
                    name={copied === line ? 'check' : 'copy'}
                    size={16}
                  /></button
                >
              </div>
            {/each}
            <p class="install-note">
              <Glyph name="time" size={14} />{$t(
                'These commands work once the first dmsg release is published.'
              )}
            </p>
          </div>
        </div>
      </div>
    </section>

    <section class="section" id="faq" aria-labelledby="faq-title">
      <div class="wrap two-col">
        <header class="sec-head" use:reveal>
          <p class="kicker"><b>07</b>{$t('A few useful answers')}</p>
          <h2 id="faq-title">
            {$t('Before you')}<br /><span class="soft"
              >{$t('make it yours.')}</span
            >
          </h2>
          <p class="sec-lead">
            {$t(
              'Clear answers about availability, identity, private content, and your existing data.'
            )}
          </p>
        </header>
        <div class="faq">
          {#each faqs as [question, answer]}
            <details>
              <summary>{question}<span aria-hidden="true"></span></summary>
              <p>{answer}</p>
            </details>
          {/each}
        </div>
      </div>
    </section>

    <section class="closing" aria-labelledby="closing-title">
      <div class="wrap" use:reveal>
        <p class="kicker"
          >{$t('A little less exposure. A little more control.')}</p
        >
        <h2 id="closing-title">
          {$t('Make room for')}<br />{$t('what matters.')}
        </h2>
        <p>
          {$t('Keep your private work close.')}<br />{$t(
            'Decide what leaves your space.'
          )}
        </p>
        <a class="button primary" href="#top"
          >{$t('Back to the start')}<span class="up"
            ><Glyph name="arrow-right" /></span
          ></a
        >
      </div>
    </section>
  </main>

  <footer class="site-footer">
    <div class="wrap footer-row">
      <span
        >{$t('Built by')}
        <a href="https://panda.fans" target="_blank" rel="noopener noreferrer"
          >ICPanda DAO</a
        ></span
      >
      <div class="footer-links">
        <a href="/_/privacy">{$t('Privacy Policy')}</a><a href="/_/terms"
          >{$t('Terms of Service')}</a
        ><a href="/_/support">{$t('Support')}</a><a href="#trust"
          >{$t('Trust')}</a
        ><a href={REPO} target="_blank" rel="noopener noreferrer">GitHub</a><a
          href="/legacy"
          data-sveltekit-reload>{$t('Legacy app · Read-only')}</a
        >
      </div>
    </div>
  </footer>
</div>
