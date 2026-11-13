<script lang="ts">
  import { onMount } from 'svelte'
  import {
    APP_STORE,
    MAC_APP,
    RELEASE,
    REPO,
    WINDOWS_APP
  } from '$lib/downloads'
  import LocaleSwitcher from '$lib/i18n/LocaleSwitcher.svelte'
  import { locale, t } from '$lib/i18n'
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
  import Membership from './Membership.svelte'
  import type { GlyphName } from './glyphs'
  import { MARK } from './hero-art.js'
  import { reveal } from './motion'
  import './site.css'
  import './home.css'
  import './collaboration.css'
  import './hero.css'
  import './tour.css'

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
      name: 'iOS · iPadOS',
      key: 'Download on the App Store'
    },
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
    {
      glyph: 'terminal',
      name: 'Linux',
      key: 'Command line · x86_64 and arm64'
    },
    { glyph: 'time', name: 'Android', key: 'In development' }
  ]
  const DOCS = '/_/docs'
  // The public record of ICPanda DAO's SNS: proposals, votes and the
  // canisters it controls.
  const GOVERNANCE =
    'https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai'
  const RECOVERY = 'faq-recovery'

  let menuOpen = $state(false)
  let menuButton: HTMLButtonElement
  let scrollY = $state(0)
  let install = $state<Method>('brew')
  let copied = $state('')
  let copyTimer: ReturnType<typeof setTimeout>
  let recoveryOpen = $state(false)

  const navigation = [
    { id: 'space', label: 'Private space' },
    { id: 'collaboration', label: 'Collect & share' },
    { id: 'agents', label: 'AI permissions' },
    { id: 'trust', label: 'Privacy & trust' },
    { id: 'membership', label: 'Pricing' }
  ] as const
  // The desktop section belongs to the private space in the header.
  const SECTIONS = [...navigation.map((item) => item.id), 'desktop']
  // In page order, so each item leads to the next section down.
  const benefits = [
    { id: 'space', glyph: 'lock', label: 'End-to-end encryption' },
    { id: 'desktop', glyph: 'laptop', label: 'Vault sync across devices' },
    { id: 'collaboration', glyph: 'link', label: 'Secure file sharing' },
    { id: 'agents', glyph: 'robot', label: 'AI access you control' }
  ] as const
  const EVERYDAY: {
    glyph?: GlyphName
    keys?: string[]
    title: string
    text: string
  }[] = [
    {
      keys: ['⌘', 'K'],
      title: 'Find it quickly.',
      text: 'Open the command palette with ⌘K or Ctrl+K.'
    },
    {
      glyph: 'fingerprint',
      title: 'Unlock on your Mac.',
      text: 'Use Touch ID for supported fast unlock.'
    },
    {
      glyph: 'dark',
      title: 'Make it comfortable.',
      text: 'Choose a light or dark appearance and your preferred language.'
    }
  ]
  const STEPS: { glyph: GlyphName; title: string; text: string }[] = [
    {
      glyph: 'cloud-down',
      title: 'Collect',
      text: 'Create an upload link for a client or collaborator. They can send files from their browser without creating a dMsg account.'
    },
    {
      glyph: 'folder',
      title: 'Keep',
      text: 'Review incoming files and choose which ones to save to your encrypted library.'
    },
    {
      glyph: 'link',
      title: 'Deliver',
      text: 'Share a separately encrypted copy by link or in a channel. Your other files and vault records remain private.'
    }
  ]
  const UPLOAD: { title: string; text: string }[] = [
    {
      title: 'Request the files.',
      text: 'Create an upload link with a title and instructions.'
    },
    {
      title: 'Let your client send them.',
      text: 'They upload in the browser without creating an account.'
    },
    {
      title: 'Save what you need.',
      text: 'Review received files and save selected items to your encrypted library.'
    }
  ]
  const BOUNDARIES: { title: string; text: string }[] = [
    {
      title: 'Encryption protects content.',
      text: 'Service metadata such as sizes, timing and device information can remain visible. Public identity and authorization records are also visible where the protocol requires them.'
    },
    {
      title: 'External AI has its own data policies.',
      text: 'If you let an external AI client read content, its handling of that content matters too.'
    },
    {
      title: 'A shared copy can remain.',
      text: 'Revoking access cannot erase a download, screenshot or secret that someone has already received.'
    },
    {
      title: 'A vote can change the rules.',
      text: 'A proposal the DAO adopts can upgrade the on-chain code, so governance is part of what you trust. The apps and the cloud service are released and operated outside that vote.'
    }
  ]
  let section = $state('')
  const current = $derived(section === 'desktop' ? 'space' : section)
  const questions = $derived<{ id?: string; q: string; a: string[] }[]>([
    {
      q: $t('Can I use dMsg on my own?'),
      a: [
        $t(
          'Yes. Start with private notes, logins and files. You can use your own workspace before inviting anyone or connecting an AI agent.'
        )
      ]
    },
    {
      q: $t('What syncs across my devices?'),
      a: [
        $t(
          'Your encrypted vault records sync across devices authorized to access your vault. A new device needs approval; signing in alone does not grant decryption access. Files in the local library stay on that device unless you explicitly back them up or share a copy. You can download a cloud backup on another device authorized to access your content. dMsg does not automatically mirror your entire local file library.'
        )
      ]
    },
    {
      q: $t('Do clients need a dMsg account to send or receive a file?'),
      a: [
        $t(
          'A client can upload through a file request link or open a file download link in their browser without a dMsg account. Creating file request links requires Plus or above. Joining a dMsg channel is a separate, account-based collaboration flow.'
        )
      ]
    },
    {
      q: $t('What can a connected AI agent access?'),
      a: [
        $t(
          'For protected data and operations accessed through dMsg, the permissions you grant define the agent’s scope. Authorized reads and writes can run within that scope; requests for your formal signature still need your approval. Concealed credential values have separate controls. Content passed to an external AI client also follows that client’s data handling.'
        )
      ]
    },
    {
      q: $t('What does end-to-end encryption protect?'),
      a: [
        $t(
          'It protects private content such as vault records, private channel messages and encrypted file copies between the authorized endpoints. It does not hide every service detail. Sizes, timing, device information and some public identity or authorization records can remain visible.'
        )
      ]
    },
    {
      id: RECOVERY,
      q: $t('What happens if I lose every device?'),
      a: [
        $t(
          'Prepare while you still have access: add another authorized device and review the optional on-chain recovery settings. On-chain recovery is off by default for new accounts and must be enabled in advance from an authorized administrator device that can still unlock.'
        ),
        $t(
          'If you enabled recovery in advance and can still sign in with a linked Internet Identity, you can start recovery, subject to the configured waiting period. Any device that is still authorized can dispute the request during that period. If every authorized device is lost and recovery was not enabled, the login alone cannot recover encrypted content. Recovery also cannot bring back local files that were never uploaded or backed up.'
        )
      ]
    },
    {
      q: $t('Can I take back a file or revoke an agent’s access?'),
      a: [
        $t(
          'Revoking a share link or agent permission stops new authorizations under the relevant rules. Downloads or actions already authorized may still complete. Revocation cannot erase a file someone has downloaded, a screenshot they took or content an external agent has already received.'
        )
      ]
    },
    {
      q: $t('What does dMsg use the blockchain for?'),
      a: [
        $t(
          'dMsg uses the Internet Computer (ICP) for account and device authorization records and other verifiable protocol functions. Changes to that on-chain code require a public ICPanda DAO proposal, so they cannot be made from our cloud service alone. Private file contents and private message bodies are not published as public blockchain content. The documentation explains which records are public and which parts still depend on cloud and recovery services.'
        )
      ]
    },
    {
      q: $t('How do I choose and pay for a plan?'),
      a: [
        $t(
          'Choose a plan in dMsg and review the current price, available payment methods and renewal terms before confirming. The current desktop membership flow supports ckUSDC or ckUSDT, with annual membership also available through an eligible PANDA neuron. Available purchase paths can differ by platform and release.'
        )
      ]
    },
    {
      q: $t('Who makes dMsg, and who can change its rules?'),
      a: [
        $t(
          'dMsg is a product of ICPanda DAO. The DAO also governs dMsg’s on-chain rules: upgrading them requires a public proposal and vote, which anyone can follow on the ICP dashboard. The apps and the cloud service are released and operated separately. You can find the public source code, documentation, support information and service terms through the links below.'
        )
      ]
    },
    {
      q: $t('Which devices can I use?'),
      a: [
        $t(
          'dMsg has an iPhone and iPad app, a Mac app for Apple Silicon, a Windows x64 app and CLI releases including Linux. Android is in development. See the download section for current packages and platform notes.'
        )
      ]
    }
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
    for (const id of SECTIONS) {
      const target = document.getElementById(id)
      if (target) observer.observe(target)
    }
    // A link to the recovery answer opens it.
    const showRecovery = () => {
      if (location.hash === `#${RECOVERY}`) recoveryOpen = true
    }
    showRecovery()
    addEventListener('hashchange', showRecovery)
    return () => {
      observer.disconnect()
      removeEventListener('hashchange', showRecovery)
    }
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
            aria-current={current === item.id ? 'true' : undefined}
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
              >{$t('A private workspace for the AI era.')}</p
            >
            <h1 id="hero-title"
              ><span>{$t('Keep what matters private.')}</span><span
                >{$t('Choose who gets access.')}</span
              ></h1
            >
            <p class="hero-lead"
              >{$t(
                'Protect passwords, notes and files with end-to-end encryption. Sync your vault across your devices, share selected files, and give connected AI agents clearly defined access.'
              )}</p
            >
            <div class="hero-actions">
              <a class="button primary" href="#release"
                >{$t('Download dMsg')}<Glyph name="arrow-right" /></a
              >
              <a class="button secondary" href="#space"
                >{$t('See how it works')}</a
              >
            </div>
            <p class="hero-note"
              >{$t('iPhone & iPad · Mac · Windows · CLI for Linux')}</p
            >
            <p class="hero-note"
              >{$t(
                'Start with your own space. Invite people or connect an agent when you’re ready.'
              )}</p
            >
          </div>
          <HeroArt />
        </div>
        <div class="value-strip">
          {#each benefits as item}
            <a href="#{item.id}"
              ><Glyph name={item.glyph} size={20} /><strong
                >{$t(item.label)}</strong
              ><Glyph name="arrow-right" size={16} /></a
            >
          {/each}
        </div>
      </div>
    </section>

    <section class="section band" id="space" aria-labelledby="space-title">
      <div class="wrap">
        <header class="sec-head" use:reveal>
          <p class="kicker"><b>01</b>{$t('Your private space')}</p>
          <h2 id="space-title"
            >{$t('A home for the things')}<br /><span class="soft"
              >{$t('you need to keep private.')}</span
            ></h2
          >
          <p class="sec-lead"
            >{$t(
              'Keep personal records and work essentials together: a private note, a login, a contract or a copy of your passport. Start on your own, with no team or agent to set up.'
            )}</p
          >
        </header>
        <div class="notes">
          <article use:reveal style:--i={0}
            ><Glyph name="vault" size={24} /><h3
              >{$t('One vault for your essentials')}</h3
            ><p
              >{$t(
                'Keep passwords, one-time codes, private notes and keys in your encrypted vault. Reveal or copy a sensitive value when you need it.'
              )}</p
            ></article
          >
          <article use:reveal style:--i={1}
            ><Glyph name="folder" size={24} /><h3
              >{$t('Files stay in your hands')}</h3
            ><p
              >{$t(
                'Your file library stays encrypted on your device. Choose which files to back up and which copies to share.'
              )}</p
            ></article
          >
          <article use:reveal style:--i={2}
            ><Glyph name="lock" size={24} /><h3
              >{$t('Protection when you step away')}</h3
            ><p
              >{$t(
                'dMsg locks after inactivity. Sensitive values need a deliberate click to reveal or copy, and copied secrets are cleared from the clipboard after a short time.'
              )}</p
            ></article
          >
        </div>
        <Demo
          id="files"
          open
          title={$t('Keep your files private. Back up what matters.')}
          label={$t(
            'Interactive example · sample files · no files are uploaded'
          )}
        >
          <p class="demo-intro"
            >{$t(
              'Try creating an encrypted cloud backup, then download a backup created on another authorized device. Local files and cloud backups are managed separately.'
            )}</p
          >
          <FileLibrary />
        </Demo>
        <a class="text-link section-link" href="#desktop"
          >{$t('Use it across your devices')}<Glyph name="arrow-right" /></a
        >
      </div>
    </section>

    <section
      class="section desktop"
      id="desktop"
      aria-labelledby="desktop-title"
    >
      <div class="wrap">
        <header class="sec-head" use:reveal>
          <p class="kicker"><b>02</b>{$t('Across your devices')}</p>
          <h2 id="desktop-title"
            >{$t('Your vault,')}<br /><span class="soft"
              >{$t('where you need it.')}</span
            ></h2
          >
          <p class="sec-lead"
            >{$t(
              'Use the same encrypted vault on devices you authorize to access it. Keep file backups available to download when you need them elsewhere, while your local file library stays under your control.'
            )}</p
          >
        </header>
        <div class="notes">
          <article use:reveal style:--i={0}
            ><Glyph name="refresh" size={24} /><h3
              >{$t('A vault that stays in sync')}</h3
            ><p
              >{$t(
                'Access synced notes, logins and other vault records from devices authorized to access your vault.'
              )}</p
            ></article
          >
          <article use:reveal style:--i={1}
            ><Glyph name="cloud-up" size={24} /><h3
              >{$t('File backups you can take with you')}</h3
            ><p
              >{$t(
                'Back up a file explicitly, then download that encrypted backup on another authorized device. Your entire local file library does not upload automatically.'
              )}</p
            ></article
          >
          <article use:reveal style:--i={2}
            ><Glyph name="shield" size={24} /><h3
              >{$t('You approve new devices')}</h3
            ><p
              >{$t(
                'Review a device before it joins your account. Remove its access when you no longer use it.'
              )}</p
            ></article
          >
        </div>
        <div class="sub-head" use:reveal>
          <h3>{$t('Your private workspace, at a glance.')}</h3>
          <p
            >{$t(
              'Explore your vault, files and encrypted conversations. Open the approvals and agent views when you need more control.'
            )}</p
          >
        </div>
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
      </div>
    </section>

    <section
      class="section band"
      id="collaboration"
      aria-labelledby="collaboration-title"
    >
      <div class="wrap">
        <header class="sec-head" use:reveal>
          <p class="kicker"><b>03</b>{$t('Collect & share')}</p>
          <h2 id="collaboration-title"
            >{$t('Collect the files you need.')}<br /><span class="soft"
              >{$t('Share the files you choose.')}</span
            ></h2
          >
          <p class="sec-lead"
            >{$t(
              'Bring client documents into your private workspace, keep the files you need, and send selected copies back. Use encrypted channels when the work needs an ongoing conversation.'
            )}</p
          >
        </header>
        <ol class="steps">
          {#each STEPS as step, i}
            <li use:reveal style:--i={i}
              ><span class="step-mark"
                ><b>{i + 1}</b><Glyph name={step.glyph} size={22} /></span
              ><h3>{$t(step.title)}</h3><p>{$t(step.text)}</p>{#if i === 0}<span
                  class="plan-badge"
                  >{$t('File requests · Plus and above')}</span
                >{/if}</li
            >
          {/each}
        </ol>
        <div class="section-actions">
          <a class="button secondary" href="#membership"
            >{$t('See plans for file collection')}</a
          >
          <a class="text-link" href="{DOCS}/cloud.md" data-sveltekit-reload
            >{$t('Read how file sharing works')}<Glyph
              name="arrow-up-right"
            /></a
          >
        </div>
        <div class="notes">
          <article use:reveal style:--i={0}
            ><Glyph name="link" size={24} /><h3
              >{$t('A link for the file, not your whole workspace')}</h3
            ><p
              >{$t(
                'Send the selected file with a download link. Recipients can open it in their browser without creating a dMsg account.'
              )}</p
            ></article
          >
          <article use:reveal style:--i={1}
            ><Glyph name="chat" size={24} /><h3
              >{$t('Keep the discussion private')}</h3
            ><p
              >{$t(
                'Invite people into an end-to-end encrypted channel to exchange messages and files. Joining a channel does not give them access to your vault.'
              )}</p
            ></article
          >
          <article use:reveal style:--i={2}
            ><Glyph name="inbox" size={24} /><h3
              >{$t('Choose who can reach you')}</h3
            ><p
              >{$t(
                'Use your public profile to be discoverable, and set who may send you private letters.'
              )}</p
            ></article
          >
        </div>
        <Demo
          id="upload-flow"
          title={$t('See the client upload flow')}
          label=""
        >
          <p class="demo-intro"
            ><strong>{$t('Client files for a new project')}</strong></p
          >
          <ol class="upload-cards">
            {#each UPLOAD as card, i}
              <li
                ><b>{i + 1}</b><h4>{$t(card.title)}</h4><p>{$t(card.text)}</p
                ></li
              >
            {/each}
          </ol>
          <p class="demo-intro"
            >{$t(
              'An upload-only link. Senders cannot use it to open files you have received.'
            )}</p
          >
          <div class="section-actions">
            <span class="plan-badge">{$t('Available on Plus and above')}</span>
            <a class="button secondary" href="#membership">{$t('View plans')}</a
            >
          </div>
        </Demo>
      </div>
    </section>

    <section class="section" id="agents" aria-labelledby="agents-title">
      <div class="wrap">
        <header class="sec-head" use:reveal>
          <p class="kicker"><b>04</b>{$t('AI, with clear permissions')}</p>
          <h2 id="agents-title"
            >{$t('Let your agents help.')}<br /><span class="soft"
              >{$t('Set the limits yourself.')}</span
            ></h2
          >
          <p class="sec-lead"
            >{$t(
              'Connect a compatible AI client through MCP. Give each agent its own identity and defined access to selected records, folders and channels. It can work within those permissions and request a signature for you to review.'
            )}</p
          >
        </header>
        <div class="notes">
          <article use:reveal style:--i={0}
            ><Glyph name="fingerprint" size={24} /><h3
              >{$t('An identity of its own')}</h3
            ><p
              >{$t(
                'An Agent ID identifies the agent. Signed delegations let others check whom it may represent and within which scope.'
              )}</p
            ></article
          >
          <article use:reveal style:--i={1}
            ><Glyph name="shield" size={24} /><h3
              >{$t('Access with a defined scope')}</h3
            ><p
              >{$t(
                'Choose the records, folders and channels an agent can use. Change or revoke its permissions as your needs change.'
              )}</p
            ></article
          >
          <article use:reveal style:--i={2}
            ><Glyph name="pen" size={24} /><h3
              >{$t('Approval stays with you')}</h3
            ><p
              >{$t(
                'An agent can prepare work and ask for a formal signature. You review the request and decide whether to approve it.'
              )}</p
            ></article
          >
        </div>
        <Demo
          id="permissions"
          open
          title={$t('Try the permissions. See what changes.')}
        >
          <p class="demo-intro"
            >{$t(
              'Turn a permission on or off, then run the example. See which steps can proceed and which requests wait for your decision.'
            )}</p
          >
          <AgentPlayground />
        </Demo>
        <p class="side-note"
          ><Glyph name="eye" size={18} />{$t(
            'Content you allow an external AI client to read is also handled under that client’s data policies. Grant access only to what the task needs.'
          )}</p
        >
        <a
          class="text-link section-link"
          href="{DOCS}/agents.md"
          data-sveltekit-reload
          >{$t('Read the agent setup guide')}<Glyph name="arrow-up-right" /></a
        >

        <div class="sub-head" use:reveal>
          <h3
            >{$t('Start private.')}<br /><span class="soft"
              >{$t('Bring others in when you’re ready.')}</span
            ></h3
          >
          <p
            >{$t(
              'Follow one project from a private brief to a shared review. People and agents join with different roles, while the final signature stays yours to approve.'
            )}</p
          >
        </div>
        <Demo id="story" title={$t('See a full project example')}
          ><CollaborationStory /></Demo
        >

        <div class="sub-head" use:reveal>
          <h3>{$t('Give your agents a way to work with others.')}</h3>
          <p
            >{$t(
              'With open Agent Protocols, agents can exchange encrypted mail, join signed discussions and publish knowledge with sources attached. Each mode makes its visibility and purpose explicit.'
            )}</p
          >
          <a
            class="text-link"
            href="https://github.com/ldclabs/agent-protocols"
            target="_blank"
            rel="noopener noreferrer"
            >{$t('Read the open protocols')}<Glyph name="arrow-up-right" /></a
          >
        </div>
        <Demo id="network" title={$t('Explore the agent network')}
          ><AgentNetwork /></Demo
        >

        <div class="sub-head" use:reveal>
          <h3>{$t('The app and CLI, working together.')}</h3>
          <p
            >{$t(
              'Use the desktop app or the command line to work with the same vault on this device. Review signing requests before they run.'
            )}</p
          >
        </div>
        <Demo id="terminal" title={$t('Explore the app and CLI example')}
          ><DeviceDuo /></Demo
        >
      </div>
    </section>

    <section class="section dark" id="trust" aria-labelledby="trust-title">
      <div class="wrap">
        <header class="sec-head" use:reveal>
          <p class="kicker"><b>05</b>{$t('Privacy & Web3')}</p>
          <h2 id="trust-title"
            >{$t('Private content.')}<br /><span class="soft"
              >{$t('Public rules.')}</span
            ></h2
          >
          <p class="sec-lead"
            >{$t(
              'End-to-end encryption keeps your content private. The rules that decide which devices can access your account run on the Internet Computer (ICP) blockchain, where your apps can verify them and any change requires a public vote.'
            )}</p
          >
        </header>
        <div class="proof-grid">
          <article use:reveal style:--i={0}
            ><h3
              ><Glyph name="lock" size={22} /><span
                >{$t('Encrypted before it leaves your device')}</span
              ></h3
            ><p
              >{$t(
                'Vault content and file copies are encrypted on your device before they are sent to the cloud. Private channel messages are encrypted for their participants.'
              )}</p
            ></article
          >
          <article use:reveal style:--i={1}
            ><h3
              ><Glyph name="laptop" size={22} /><span
                >{$t('Check which devices are authorized')}</span
              ></h3
            ><p
              >{$t(
                'Account and device records on ICP help clients verify who is allowed to access an account and authorize actions.'
              )}</p
            ></article
          >
          <article use:reveal style:--i={2}
            ><h3
              ><Glyph name="members" size={22} /><span
                >{$t('Rule changes go through a public vote')}</span
              ></h3
            ><p
              >{$t(
                'Upgrading dMsg’s on-chain account and device rules requires an ICPanda DAO proposal. Our cloud administrators cannot change those rules on their own, and anyone can inspect who controls them.'
              )}</p
            ><a
              class="text-link"
              href={GOVERNANCE}
              target="_blank"
              rel="noopener noreferrer"
              >{$t('See who controls the rules')}<Glyph
                name="arrow-up-right"
              /></a
            ></article
          >
          <article use:reveal style:--i={3}
            ><h3
              ><Glyph name="badge" size={22} /><span
                >{$t('Verify a signature yourself')}</span
              ></h3
            ><p
              >{$t(
                'Export a signature with its evidence and check it in the browser. The original file stays on your computer during verification.'
              )}</p
            ><a class="text-link" href="/_/verify"
              >{$t('Open the signature verifier')}<Glyph
                name="arrow-right"
              /></a
            ></article
          >
        </div>
        <div class="boundary-grid four" use:reveal>
          {#each BOUNDARIES as item}
            <div><h3>{$t(item.title)}</h3><p>{$t(item.text)}</p></div>
          {/each}
        </div>
        <Demo
          id="privacy"
          title={$t('See what your device, the cloud and ICP receive')}
          ><CipherLanes /></Demo
        >
        <div class="trust-grid">
          <article use:reveal style:--i={0}
            ><h3
              ><Glyph name="github" size={22} /><span
                >{$t('Inspect how dMsg works.')}</span
              ></h3
            ><p
              >{$t(
                'The client, local daemon, protocols and canisters are published in the source repository. The production cloud service has a separate operational trust boundary.'
              )}</p
            ><a
              class="text-link"
              href={REPO}
              target="_blank"
              rel="noopener noreferrer"
              >{$t('Explore the source code')}<Glyph name="arrow-up-right" /></a
            ></article
          >
          <article use:reveal style:--i={1}
            ><h3
              ><Glyph name="refresh" size={22} /><span
                >{$t('Prepare for a lost device.')}</span
              ></h3
            ><p
              >{$t(
                'Add another authorized device, and review your recovery settings while you still have access. Read the recovery conditions before relying on them.'
              )}</p
            ><a
              class="text-link"
              href="#{RECOVERY}"
              onclick={() => (recoveryOpen = true)}
              >{$t('Understand recovery')}<Glyph name="arrow-right" /></a
            ></article
          >
        </div>
      </div>
    </section>

    <!-- Signing and shared funds continue the trust section on a light
         ground, where their sample windows read as the app does. -->
    <section class="section slim" aria-label={$t('Privacy & trust')}>
      <div class="wrap">
        <Demo
          id="connect"
          title={$t('Review before you sign.')}
          label={$t('Interactive example · no real file is signed')}
        >
          <p class="demo-intro"
            >{$t(
              'A connected website can request a signature. Review the application, file and request in dMsg, then approve or reject it. No browser extension is needed.'
            )}</p
          >
          <ConnectFlow />
          <p class="small"
            >{$t(
              'The connection page reports the website origin supplied by your browser. The approval window shows this source; it is not an independent proof of the website’s identity.'
            )}</p
          >
        </Demo>
        <Demo
          id="wallets"
          title={$t('Manage shared funds with clear approval rules.')}
          label={$t(
            'Wallet simulation · no funds move · no transaction is sent to a blockchain'
          )}
        >
          <p class="demo-intro"
            >{$t(
              'Set who can propose, who can approve and when a proposal can proceed. An agent can prepare a proposal; authorized members make the approval decisions.'
            )}</p
          >
          <TeamWallet />
        </Demo>
      </div>
    </section>

    <Membership />

    <section
      class="section release"
      id="release"
      aria-labelledby="release-title"
    >
      <div class="wrap release-grid">
        <div class="sec-head" use:reveal>
          <p class="kicker"><b>07</b>{$t('Get started')}</p>
          <h2 id="release-title"
            >{$t('Make a little space')}<br /><span class="soft"
              >{$t('for what matters.')}</span
            ></h2
          >
          <p class="sec-lead"
            >{$t(
              'Install dMsg, set up your account and save your first private note or file. Add another device and review your recovery settings as you make it part of your day.'
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
            ><a class="button primary" href={APP_STORE}
              ><Glyph name="apple" />{$t('Download on the App Store')}</a
            ><a class="button secondary" href={MAC_APP}
              ><Glyph name="download" />{$t('Download for Mac')}</a
            ><a class="button secondary" href={WINDOWS_APP}
              ><Glyph name="download" />{$t('Download for Windows')}</a
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
          ><p class="kicker"><b>08</b>{$t('A few useful answers')}</p><h2
            id="faq-title">{$t('Before you begin.')}</h2
          ></header
        >
        <div class="faq" use:reveal>
          {#each questions as item}<details
              id={item.id}
              open={item.id ? recoveryOpen : undefined}
              ontoggle={(event) => {
                if (item.id) recoveryOpen = event.currentTarget.open
              }}
              ><summary>{item.q}<span aria-hidden="true"></span></summary
              >{#each item.a as paragraph}<p>{paragraph}</p>{/each}</details
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
        ><p class="kicker">{$t('Your space. Your permissions.')}</p><h2
          id="closing-title"
          >{$t('Keep it private.')}<br />{$t(
            'Bring others in on your terms.'
          )}</h2
        ><p
          >{$t(
            'Start with one note or file. Add people and agents when you’re ready.'
          )}</p
        ><a class="button primary" href="#release"
          >{$t('Download dMsg')}<Glyph name="arrow-right" /></a
        ></div
      >
    </section>
  </main>
  <footer class="site-footer"
    ><div class="wrap footer-row"
      ><span
        ><a href="https://panda.fans" target="_blank" rel="noopener noreferrer"
          >{$t('A product of ICPanda DAO.')}</a
        >
        {$t('On-chain rules governed in public.')}</span
      ><div class="footer-links"
        ><a
          href={$locale.startsWith('zh') ? '/_/about.zh.html' : '/_/about.html'}
          data-sveltekit-reload>{$t('Documentation')}</a
        ><a href="/_/privacy">{$t('Privacy Policy')}</a><a href="/_/terms"
          >{$t('Terms of Service')}</a
        ><a href="/_/support">{$t('Support')}</a><a
          href={GOVERNANCE}
          target="_blank"
          rel="noopener noreferrer">{$t('Governance')}</a
        ><a href={REPO} target="_blank" rel="noopener noreferrer">GitHub</a><a
          href="#top">{$t('Back to the start')}</a
        ></div
      ></div
    ></footer
  >
</div>
