<script lang="ts">
  import LocaleSwitcher from '$lib/i18n/LocaleSwitcher.svelte'
  import { t } from '$lib/i18n'
  import Brand from './Brand.svelte'
  import Icon from './Icon.svelte'
  import WorkspacePreview from './WorkspacePreview.svelte'
  import './landing.css'

  let menuOpen = $state(false)
  let menuButton: HTMLButtonElement
  let dialog: HTMLDialogElement
  let signatureState = $state<'review' | 'approved' | 'rejected'>('review')
  let contactMode = $state('invite')
  const contactOptions = $derived([
    {
      id: 'invite',
      title: $t('Invitations only'),
      description: $t('People you invite can reach you.'),
      note: $t(
        'Invitations must be accepted. Joining a channel does not automatically make someone a contact.'
      )
    },
    {
      id: 'paid',
      title: $t('Paid requests'),
      description: $t('A delivery fee for new requests.'),
      note: $t(
        'Planned for a later release. Payment covers eligible delivery, not a reply, contact status, or signing permission. Fees and refund conditions must be shown before payment.'
      )
    },
    {
      id: 'closed',
      title: $t('Closed to new requests'),
      description: $t('No new contact from strangers.'),
      note: $t(
        'Closing stops new requests. Existing conversations keep their own permissions.'
      )
    }
  ])
  const activeContact = $derived(
    contactOptions.find((option) => option.id === contactMode)!
  )
  const faqs = $derived([
    [
      $t('Can I install dMsg for Chrome now?'),
      $t(
        'Not yet. The Chrome extension is in development and is not listed in the Chrome Web Store. The examples here explain the planned experience; they are not a live vault, messenger, or signing service.'
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
      $t('Does logging in unlock my private content?'),
      $t(
        'No. Login, device authorization, local unlock, and formal signing have separate permissions. Connecting a third-party app will not give it general access to your vault or automatic signing rights.'
      )
    ],
    [
      $t('What would I need to recover my data?'),
      $t(
        'The new recovery design requires a complete encrypted backup and its separately stored recovery code. A code alone cannot reconstruct missing ciphertext. Login recovery and threshold-signing control are separate. Until migration is available, preserve your old browser data and existing recovery materials.'
      )
    ],
    [
      $t('Is everything stored on the blockchain?'),
      $t(
        'No. The planned design encrypts private content on authorized devices and stores ciphertext in the cloud. ICP holds core identity, control, formal-signing, and payment state. Service metadata, cloud availability, client updates, and governance remain trust boundaries.'
      )
    ],
    [
      $t('Will the full workspace work on my phone?'),
      $t(
        'The initial full workspace is planned for desktop Chrome. This website provides product information and legacy access; a responsive page is not a promise of a full mobile vault or messenger.'
      )
    ]
  ])

  function openReview() {
    signatureState = 'review'
    dialog.showModal()
  }

  function onEscape(event: KeyboardEvent) {
    if (event.key === 'Escape' && menuOpen) {
      menuOpen = false
      menuButton.focus()
    }
  }
</script>

<svelte:head>
  <title>{$t('dMsg — Your space. Your say.')}</title>
  <meta
    name="description"
    content={$t(
      'A private workspace for your secrets, chosen collaborators, and explicit signatures. Chrome extension in development. Legacy app available in read-only mode.'
    )}
  />
  <meta name="color-scheme" content="light" />
</svelte:head>
<svelte:window onkeydown={onEscape} />

<div class="dmsg-site">
  <a class="skip" href="#main">{$t('Skip to content')}</a>
  <header class="site-header">
    <div class="wrap nav-wrap">
      <Brand />
      <LocaleSwitcher />
      <nav
        id="main-navigation"
        class:open={menuOpen}
        aria-label={$t('Main navigation')}
      >
        <a href="#workspace" onclick={() => (menuOpen = false)}
          >{$t('Workspace')}</a
        >
        <a href="#privacy" onclick={() => (menuOpen = false)}>{$t('Privacy')}</a
        >
        <a href="#developers" onclick={() => (menuOpen = false)}
          >{$t('Developers')}</a
        >
      </nav>
      <a class="button primary header-action" href="#release"
        >{$t('Coming to Chrome')}</a
      >
      <button
        bind:this={menuButton}
        class="icon-button menu-toggle"
        aria-label={menuOpen ? $t('Close navigation') : $t('Open navigation')}
        aria-expanded={menuOpen}
        aria-controls="main-navigation"
        onclick={() => (menuOpen = !menuOpen)}
        ><Icon name={menuOpen ? 'close' : 'menu'} /></button
      >
    </div>
  </header>

  <main id="main" tabindex="-1">
    <div class="wrap">
      <section class="hero" aria-labelledby="hero-title">
        <div class="eyebrow">{$t('Private workspace / Chrome extension')}</div>
        <h1 id="hero-title">{$t('Your space.')}<br />{$t('Your say.')}</h1>
        <div class="hero-bottom">
          <div>
            <div class="hero-actions"
              ><a class="button primary" href="#workspace"
                >{$t('Explore the workspace')}<Icon name="arrow-right" /></a
              ><a class="text-link" href="/legacy" data-sveltekit-reload
                >{$t('Open legacy app')}<Icon name="arrow-up-right" /></a
              ></div
            >
            <p class="hero-note"
              >{$t(
                'Chrome extension coming soon. Legacy app available read-only.'
              )}</p
            >
          </div>
          <p class="hero-copy"
            >{$t(
              'A private workspace for the secrets you keep, the people you choose, and the requests you decide to sign.'
            )}</p
          >
        </div>
      </section>
      <WorkspacePreview review={openReview} />
      <section class="section" id="contact">
        <div class="two-col">
          <div class="section-intro"
            ><div class="eyebrow section-label">{$t('Your public doorway')}</div
            ><h2>{$t('Be reachable.')}<br />{$t('On your terms.')}</h2><p
              >{$t(
                'A public profile helps people find you. Your contact rules decide how they approach — while your private work stays in its own space.'
              )}</p
            ><p class="small"
              >{$t(
                'Preview the planned contact modes. Paid requests will follow the personal workspace and private collaboration releases.'
              )}</p
            ></div
          >
          <div class="demo contact-demo">
            <div class="profile"
              ><span class="avatar-text">AL</span><h3>Alex Lin</h3><p
                class="profile-handle">{$t('@alex · Example profile')}</p
              ><p
                >{$t('Independent builder.')}<br />{$t(
                  'Thoughtful work, small teams.'
                )}</p
              ><div class="profile-status" aria-live="polite"
                ><strong>{activeContact.title}</strong><p
                  >{activeContact.description}</p
                ></div
              ><span class="demo-label">{$t('Visitor preview')}</span></div
            >
            <fieldset class="rules"
              ><legend>{$t('How people reach you')}</legend><p class="small"
                >{$t('Try a rule. This example is not saved.')}</p
              >{#each contactOptions as option}<label class="radio-option"
                  ><input
                    type="radio"
                    name="contact"
                    value={option.id}
                    bind:group={contactMode}
                  /><span
                    ><strong>{option.title}</strong><small
                      >{option.description}</small
                    ></span
                  ></label
                >{/each}<p class="contact-note" aria-live="polite"
                >{activeContact.note}</p
              ></fieldset
            >
          </div>
        </div>
      </section>
    </div>

    <section class="section dark" id="privacy">
      <div class="wrap">
        <div class="two-col">
          <div class="section-intro"
            ><div class="eyebrow section-label"
              >{$t('Privacy, with the details included')}</div
            ><h2
              >{$t('Private content.')}<br /><span
                >{$t('Clear boundaries.')}</span
              ></h2
            ><p
              >{$t(
                'The new dMsg is designed around encryption on your device, with distinct roles for the cloud and ICP.'
              )}</p
            ><a class="text-link" href="#trust"
              >{$t('Understand the trust model')}<Icon name="arrow-right" /></a
            ></div
          >
          <div>
            <article class="principle"
              ><Icon name="device" /><div
                ><h3>{$t('Your device handles private content.')}</h3><p
                  >{$t(
                    'Authorized, unlocked devices encrypt and decrypt your notes, messages, and files. A third-party connection does not open your vault.'
                  )}</p
                ></div
              ></article
            >
            <article class="principle"
              ><Icon name="cloud" /><div
                ><h3>{$t('The cloud stores and delivers ciphertext.')}</h3><p
                  >{$t(
                    'Cloudflare supports encrypted storage, delivery, and collaboration state. Operational metadata is visible where needed to run the service.'
                  )}</p
                ></div
              ></article
            >
            <article class="principle"
              ><Icon name="key" /><div
                ><h3>{$t('ICP anchors core control.')}</h3><p
                  >{$t(
                    'Identity ownership, device and recovery roots, formal signing, and final payment state have their own on-chain rules. Ordinary messages are not written on-chain one by one.'
                  )}</p
                ></div
              ></article
            >
          </div>
        </div>
        <div class="boundary-grid">
          <div
            ><h3>{$t('Encryption has a scope.')}</h3><p
              >{$t(
                'It protects content, not every relationship or action. Identity, calls, payment data, and delivery metadata can remain visible.'
              )}</p
            ></div
          >
          <div
            ><h3>{$t('Availability still matters.')}</h3><p
              >{$t(
                'A relay can delay or withhold content. Signatures and local history do not guarantee that every device has the latest cloud state.'
              )}</p
            ></div
          >
          <div
            ><h3>{$t('Copies can outlive access.')}</h3><p
              >{$t(
                'Removing access protects future content after key changes. It cannot erase downloads, screenshots, or a secret already learned.'
              )}</p
            ></div
          >
        </div>
      </div>
    </section>

    <div class="wrap">
      <section class="section" id="trust">
        <div class="eyebrow section-label">{$t('Evidence over promises')}</div>
        <div class="two-col"
          ><h2>{$t('Trust you can')}<br />{$t('examine.')}</h2><p
            >{$t(
              'Private work deserves more than a reassuring badge. Understand what is public, what you authorize, and what recovery depends on.'
            )}</p
          ></div
        >
        <div class="trust-grid">
          <article
            ><span class="icon-box"><Icon name="github" /></span><h3
              >{$t('Inspect the public code.')}</h3
            ><p
              >{$t(
                'Client and protocol work lives in the public repository. The production cloud service has its own operational trust boundary.'
              )}</p
            ><a
              class="text-link"
              href="https://github.com/ldclabs/ic-panda"
              target="_blank"
              rel="noopener noreferrer"
              >{$t('Explore the repository')}<Icon name="arrow-up-right" /></a
            ></article
          >
          <article
            ><span class="icon-box"><Icon name="key" /></span><h3
              >{$t('Keep permissions separate.')}</h3
            ><p
              >{$t(
                'Proving identity, reading shared content, approving a device, and signing a file each need their own scope. One does not imply the others.'
              )}</p
            ><a class="text-link" href="#developers"
              >{$t('See the request flow')}<Icon name="arrow-right" /></a
            ></article
          >
          <article
            ><span class="icon-box"><Icon name="download" /></span><h3
              >{$t('Plan for independent recovery.')}</h3
            ><p
              >{$t(
                'The recovery design pairs a complete encrypted backup with a separately stored code. Missing ciphertext cannot be recovered from a code alone.'
              )}</p
            ><a class="text-link" href="#faq"
              >{$t('Read the recovery details')}<Icon name="arrow-right" /></a
            ></article
          >
        </div>
        <div class="trust-foot"
          ><p
            ><strong>{$t('Development is not audit evidence.')}</strong>
            {$t(
              'This page describes the target product. A public repository does not establish audit coverage or prove which code is deployed.'
            )}</p
          ><a class="text-link" href="#release"
            >{$t('Release status')}<Icon name="arrow-right" /></a
          ></div
        >
      </section>

      <section class="section rule" id="developers">
        <div class="two-col">
          <div class="section-intro"
            ><div class="eyebrow section-label"
              >{$t('For connected applications')}</div
            ><h2>{$t('Your identity,')}<br />{$t('beyond one app.')}</h2><p
              >{$t(
                'Let an app ask for one specific action. Review its source, the content, and the consequence in a separate extension confirmation window.'
              )}</p
            ><button class="text-link" onclick={openReview}
              >{$t('Walk through a request')}<Icon name="arrow-right" /></button
            ></div
          >
          <div
            ><div class="flow"
              ><article
                ><span class="step-number">01</span><h3
                  >{$t('App requests.')}</h3
                ><p
                  >{$t(
                    'Bind a specific file version, purpose, identity, and requesting origin.'
                  )}</p
                ></article
              ><article
                ><span class="step-number">02</span><h3>{$t('You review.')}</h3
                ><p
                  >{$t(
                    'Inspect the actual request. Approve or reject it in the extension.'
                  )}</p
                ></article
              ><article
                ><span class="step-number">03</span><h3
                  >{$t('App verifies.')}</h3
                ><p
                  >{$t(
                    'Receive the result and verify content, signature, and authorization separately.'
                  )}</p
                ></article
              ></div
            ><div class="integrations"
              ><div
                ><strong>TokenList</strong>{$t(
                  'Planned file-signing pilot. Project permissions remain with TokenList.'
                )}</div
              ><div
                ><strong>alink</strong>{$t(
                  'Planned scoped controller integration. Its accounts and grants remain separate.'
                )}</div
              ></div
            ><p class="integration-note"
              >{$t(
                'Integrations are planned, not generally available. Connecting an app never means automatic signing.'
              )}</p
            ></div
          >
        </div>
      </section>

      <section class="section rule" id="faq"
        ><div class="two-col"
          ><div class="section-intro"
            ><div class="eyebrow section-label"
              >{$t('A few useful answers')}</div
            ><h2>{$t('Before you')}<br />{$t('make it yours.')}</h2><p
              >{$t(
                'Clear answers about availability, identity, private content, and your existing data.'
              )}</p
            ></div
          ><div class="faq"
            >{#each faqs as [question, answer]}<details
                ><summary>{question}<span aria-hidden="true">+</span></summary
                ><p>{answer}</p></details
              >{/each}</div
          ></div
        ></section
      >

      <section class="release rule" id="release" aria-labelledby="release-title"
        ><div
          ><div class="eyebrow section-label">{$t('In development')}</div><h2
            id="release-title">{$t('Coming to Chrome.')}</h2
          ><p
            >{$t(
              'The full workspace is not available to install yet. We’re building toward a personal vault, followed by private collaboration and scoped signing integrations.'
            )}</p
          ></div
        ><div class="release-actions"
          ><a
            class="button primary"
            href="https://github.com/ldclabs/ic-panda"
            target="_blank"
            rel="noopener noreferrer"
            ><Icon name="github" />{$t('Follow development')}</a
          ><a class="text-link" href="/legacy" data-sveltekit-reload
            >{$t('Read your legacy messages')}<Icon name="arrow-up-right" /></a
          ><p class="small"
            >{$t(
              'Keep your existing browser data and recovery materials until migration is available.'
            )}</p
          ></div
        ></section
      >
      <section class="closing rule"
        ><div class="eyebrow section-label"
          >{$t('A little less exposure. A little more control.')}</div
        ><h2>{$t('Make room for')}<br />{$t('what matters.')}</h2><p
          >{$t('Keep your private work close.')}<br />{$t(
            'Decide what leaves your space.'
          )}</p
        ><a class="button primary" href="#workspace"
          >{$t('Explore dMsg')}<Icon name="arrow-right" /></a
        ></section
      >
    </div>
  </main>
  <footer class="site-footer"
    ><div class="wrap footer-row"
      ><span
        >{$t('Built by')}
        <a href="https://panda.fans" target="_blank" rel="noopener noreferrer"
          >ICPanda DAO</a
        ></span
      ><div class="footer-links"
        ><a href="#privacy">{$t('Privacy')}</a><a href="#trust">{$t('Trust')}</a
        ><a
          href="https://github.com/ldclabs/ic-panda"
          target="_blank"
          rel="noopener noreferrer">GitHub</a
        ><a href="/legacy" data-sveltekit-reload
          >{$t('Legacy app · Read-only')}</a
        ></div
      ></div
    ></footer
  >

  <dialog bind:this={dialog} aria-labelledby="signature-title">
    <div class="modal-head"
      ><h2 id="signature-title">{$t('Review this file version')}</h2><button
        class="icon-button"
        aria-label={$t('Close signature example')}
        onclick={() => dialog.close()}><Icon name="close" /></button
      ></div
    >
    <div class="modal-body">
      <p class="eyebrow">{$t('Simulated request · No real signature')}</p>
      {#if signatureState === 'review'}
        <div class="data-row"
          ><span>{$t('Requesting app')}</span><strong
            >{$t('TokenList · Example')}</strong
          ></div
        ><div class="data-row"
          ><span>{$t('Identity')}</span><strong
            >{$t('Alex · Demo identity')}</strong
          ></div
        ><div class="data-row"
          ><span>{$t('File')}</span><strong>release-brief.txt · v1</strong></div
        ><div class="data-row"
          ><span>{$t('Purpose')}</span><strong
            >{$t('Confirm this example version')}</strong
          ></div
        >
        <div class="sample-content"
          ><strong>{$t('Example file content')}</strong><p
            >{$t('dMsg release brief')}<br />{$t(
              'Purpose: confirm this example file version.'
            )}<br />{$t('No real release or approval is represented.')}</p
          ></div
        >
        <p class="small"
          >{$t(
            'In the planned extension, approval authorizes only the reviewed payload and purpose. It does not grant access to your vault or approve future requests.'
          )}</p
        >
        <div class="demo-actions"
          ><button
            class="button secondary"
            onclick={() => (signatureState = 'rejected')}
            >{$t('Reject example')}</button
          ><button
            class="button primary"
            onclick={() => (signatureState = 'approved')}
            >{$t('Simulate approval')}</button
          ></div
        >
      {:else}
        <div role="status"
          ><h3
            >{signatureState === 'approved'
              ? $t('Example approved')
              : $t('Example rejected')}</h3
          ><p
            >{signatureState === 'approved'
              ? $t(
                  'The walkthrough ends here. No signature was generated, no identity was connected, and no result was sent to an app.'
                )
              : $t(
                  'You chose not to approve the sample request. Nothing was signed or sent.'
                )}</p
          ></div
        ><button class="button secondary" onclick={() => dialog.close()}
          >{$t('Close walkthrough')}</button
        >
      {/if}
    </div>
  </dialog>
</div>
