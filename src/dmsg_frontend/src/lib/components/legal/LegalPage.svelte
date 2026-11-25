<script lang="ts">
  import { onDestroy } from 'svelte'
  import Brand from '$lib/components/landing/Brand.svelte'
  import Icon from '$lib/components/landing/Icon.svelte'
  import '$lib/components/landing/site.css'
  import LocaleSwitcher from '$lib/i18n/LocaleSwitcher.svelte'
  import { locale, localeDir, t, type Locale } from '$lib/i18n'
  import {
    currentTerms,
    loadLegalDocument,
    termsVersions,
    type LegalDocument,
    type LegalKind
  } from '$lib/legal'
  import './legal.css'

  let { kind }: { kind: LegalKind } = $props()

  const pages = $derived([
    {
      kind: 'privacy',
      href: '/_/privacy',
      label: $t('Privacy Policy'),
      description: $t(
        'How dMsg handles your content, metadata, payments and device data, and the choices you have.'
      )
    },
    {
      kind: 'terms',
      href: '/_/terms',
      label: $t('Terms of Service'),
      description: $t(
        'The terms for using dMsg, including plans, payments and refunds, with the published digest of each version.'
      )
    },
    {
      kind: 'support',
      href: '/_/support',
      label: $t('Support'),
      description: $t(
        'Where to get help with dMsg, what to include in a report and how to report a security issue.'
      )
    },
    {
      kind: 'delete-account',
      href: '/_/delete-account',
      label: $t('Delete account'),
      description: $t(
        'How to permanently delete your dMsg cloud service account and data, including after uninstalling the app.'
      )
    }
  ])
  const page = $derived(pages.find((item) => item.kind === kind)!)
  const verifyCommand = `curl -s https://dmsg.net${currentTerms.path} | shasum -a 256`
  const earlierTerms = termsVersions.slice(0, -1).reverse()
  const supportEmail = 'hi@yiwen.ai'
  const deleteSubject = 'dMsg Account Deletion Request'
  const deleteMailto = `mailto:${supportEmail}?subject=${encodeURIComponent(deleteSubject)}&body=${encodeURIComponent(
    'dMsg Account ID or public handle (if known):\nPlease permanently delete my dMsg cloud service account.\n\nDo not include private keys, recovery materials, passwords, one-time codes or login sessions.\n'
  )}`

  let doc = $state<LegalDocument | null>(null)
  let failed = $state(false)
  let digestCopyState = $state<'idle' | 'copied' | 'failed'>('idle')
  let digestCopyStatus = $state('')
  let lastDigestAnnouncement = ''
  let digestCopyTimer: ReturnType<typeof setTimeout> | undefined
  let digestCopyRequest = 0
  let emailCopyState = $state<'idle' | 'copied' | 'failed'>('idle')
  let emailCopyStatus = $state('')
  let lastAnnouncement = ''
  let emailCopyTimer: ReturnType<typeof setTimeout> | undefined
  let emailCopyRequest = 0
  let request = 0

  onDestroy(() => {
    ++digestCopyRequest
    clearTimeout(digestCopyTimer)
    ++emailCopyRequest
    clearTimeout(emailCopyTimer)
  })

  async function load(target: LegalKind, language: Locale) {
    // A slower earlier language must not replace the latest choice.
    const id = ++request
    doc = null
    failed = false
    try {
      const next = await loadLegalDocument(target, language)
      if (id === request) doc = next
    } catch {
      if (id === request) failed = true
    }
  }

  async function copyDigest() {
    const id = ++digestCopyRequest
    clearTimeout(digestCopyTimer)
    let ok = false
    try {
      await navigator.clipboard.writeText(currentTerms.sha256)
      ok = true
    } catch {
      ok = false
    }
    if (id !== digestCopyRequest) return
    digestCopyState = ok ? 'copied' : 'failed'
    let message = ok ? $t('Copied') : $t('Copy failed')
    if (message === lastDigestAnnouncement) message += '\u00A0'
    lastDigestAnnouncement = message
    digestCopyStatus = ''
    requestAnimationFrame(() => {
      if (id === digestCopyRequest) digestCopyStatus = message
    })
    digestCopyTimer = setTimeout(() => {
      digestCopyState = 'idle'
      digestCopyStatus = ''
    }, 2000)
  }

  async function copySupportEmail() {
    const id = ++emailCopyRequest
    clearTimeout(emailCopyTimer)
    let ok = false
    try {
      await navigator.clipboard.writeText(supportEmail)
      ok = true
    } catch {
      ok = false
    }
    if (id !== emailCopyRequest) return
    emailCopyState = ok ? 'copied' : 'failed'
    let message = ok ? $t('Copied') : $t('Copy failed')
    if (message === lastAnnouncement) message += '\u00A0'
    lastAnnouncement = message
    emailCopyStatus = ''
    requestAnimationFrame(() => {
      if (id === emailCopyRequest) emailCopyStatus = message
    })
    emailCopyTimer = setTimeout(() => {
      emailCopyState = 'idle'
      emailCopyStatus = ''
    }, 2000)
  }

  $effect(() => {
    void load(kind, $locale)
  })

  // The root layout localizes the shared description; this page refines it.
  $effect(() => {
    document
      .querySelector('meta[name="description"]')
      ?.setAttribute('content', page.description)
  })
</script>

<svelte:head>
  <title>{`${page.label} — dMsg`}</title>
  <meta name="color-scheme" content="light" />
</svelte:head>

<div class="dmsg-site legal-site">
  <a class="skip" href="#main">{$t('Skip to content')}</a>
  <header class="site-header">
    <div class="wrap nav-wrap">
      <Brand />
      <nav class="legal-nav" aria-label={$t('Policies and help')}>
        {#each pages as item (item.kind)}
          <a
            href={item.href}
            aria-current={item.kind === kind ? 'page' : undefined}
            >{item.label}</a
          >
        {/each}
      </nav>
      <LocaleSwitcher />
    </div>
  </header>

  <main id="main" tabindex="-1">
    <div class="wrap">
      <section class="legal-hero" aria-labelledby="legal-title">
        <div class="eyebrow">{$t('Policies and help')}</div>
        <h1
          id="legal-title"
          lang={doc?.locale}
          dir={doc ? localeDir(doc.locale) : undefined}
          >{doc?.title || page.label}</h1
        >
        <p class="legal-description">{page.description}</p>
        {#if kind === 'terms'}
          <details class="terms-digest">
            <summary
              ><span>{$t('Version')} {currentTerms.version}</span>{$t(
                'Verification details'
              )}</summary
            >
            <div class="digest-body">
              <dl>
                <div>
                  <dt>{$t('Version')}</dt>
                  <dd>{currentTerms.version}</dd>
                </div>
                <div>
                  <dt>SHA-256</dt>
                  <dd class="digest-value">
                    <code dir="ltr">{currentTerms.sha256}</code>
                    <span class="copy-control">
                      <button
                        type="button"
                        class="icon-button"
                        onclick={copyDigest}
                        aria-label={$t('Copy SHA-256')}
                        data-copied={digestCopyState === 'copied'
                          ? ''
                          : undefined}
                        data-failed={digestCopyState === 'failed'
                          ? ''
                          : undefined}
                        ><Icon
                          name={digestCopyState === 'copied' ? 'check' : 'copy'}
                        /></button
                      >
                      <span
                        class="copy-status"
                        class:visible={digestCopyStatus !== ''}
                        class:failed={digestCopyState === 'failed'}
                        role="status">{digestCopyStatus}</span
                      >
                    </span>
                  </dd>
                </div>
              </dl>
              <p class="small"
                >{$t(
                  'To verify this version, download the English text and compute its SHA-256:'
                )}</p
              >
              <!-- Scrollable code block needs keyboard focus access (WCAG 2.1.1). -->
              <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
              <pre dir="ltr" tabindex="0"><code>{verifyCommand}</code></pre>
              <div class="terms-links">
                <a
                  class="text-link"
                  href={currentTerms.path}
                  target="_blank"
                  rel="noopener"
                  >{$t('Open the English text')}<Icon
                    name="arrow-up-right"
                  /></a
                >
                {#if earlierTerms.length > 0}
                  <span class="small">{$t('Earlier versions')}</span>
                  {#each earlierTerms as version (version.version)}
                    <a class="text-link" href={version.path} target="_blank"
                      >{`v${version.version}`}</a
                    >
                  {/each}
                {/if}
              </div>
            </div>
          </details>
        {:else if kind === 'delete-account'}
          <div class="deletion-card">
            <dl>
              <div>
                <dt>{$t('Support email')}</dt>
                <dd class="digest-value">
                  <code id="delete-support-email" dir="ltr">{supportEmail}</code>
                  <span class="copy-control">
                    <button
                      type="button"
                      class="icon-button"
                      onclick={copySupportEmail}
                      aria-label={$t('Copy support email')}
                      data-copied={emailCopyState === 'copied' ? '' : undefined}
                      data-failed={emailCopyState === 'failed' ? '' : undefined}
                      ><Icon
                        name={emailCopyState === 'copied' ? 'check' : 'copy'}
                      /></button
                    >
                    <span
                      class="copy-status"
                      class:visible={emailCopyStatus !== ''}
                      class:failed={emailCopyState === 'failed'}
                      role="status">{emailCopyStatus}</span
                    >
                  </span>
                </dd>
              </div>
              <div>
                <dt>{$t('Email subject')}</dt>
                <dd><code dir="ltr">{deleteSubject}</code></dd>
              </div>
            </dl>
            <div class="deletion-actions">
              <a class="button primary" href={deleteMailto}
                >{$t('Email deletion request')}</a
              >
            </div>
          </div>
        {/if}
      </section>

      <div class="legal-layout">
        {#if doc && doc.sections.length > 0}
          <details class="legal-mobile-toc">
            <summary>{$t('On this page')}</summary>
            <nav aria-label={$t('On this page')}>
              {#each doc.sections as section (section.id)}
                <a href={`#${section.id}`}>{section.title}</a>
              {/each}
            </nav>
          </details>
        {/if}
        <article
          class="legal-doc"
          lang={doc?.locale}
          dir={doc ? localeDir(doc.locale) : undefined}
          aria-busy={!doc && !failed}
        >
          {#if kind === 'terms' && doc && doc.locale !== 'en'}
            <p class="legal-note"
              >{$t(
                'This translation is provided for convenience. The English text of version {version} is binding.',
                { version: currentTerms.version }
              )}</p
            >
          {/if}
          {#if doc}
            {@html doc.html}
          {:else if failed}
            <p role="alert">{$t('Could not load this page.')}</p>
            <button
              type="button"
              class="button secondary"
              onclick={() => void load(kind, $locale)}>{$t('Try again')}</button
            >
          {:else}
            <p>{$t('Loading…')}</p>
          {/if}
        </article>
        {#if doc && doc.sections.length > 0}
          <nav class="legal-toc" aria-labelledby="legal-toc-title">
            <p id="legal-toc-title" class="eyebrow">{$t('On this page')}</p>
            <ol>
              {#each doc.sections as section (section.id)}
                <li><a href={`#${section.id}`}>{section.title}</a></li>
              {/each}
            </ol>
          </nav>
        {/if}
      </div>
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
        >{#each pages as item (item.kind)}<a href={item.href}>{item.label}</a
          >{/each}<a
          href="https://github.com/ldclabs/ic-panda"
          target="_blank"
          rel="noopener noreferrer">GitHub</a
        ><a href="/legacy" data-sveltekit-reload
          >{$t('Legacy app · Read-only')}</a
        ></div
      ></div
    ></footer
  >
</div>
