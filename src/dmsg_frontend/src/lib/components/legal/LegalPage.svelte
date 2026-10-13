<script lang="ts">
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
    }
  ])
  const page = $derived(pages.find((item) => item.kind === kind)!)
  const verifyCommand = `curl -s https://dmsg.net${currentTerms.path} | shasum -a 256`
  const earlierTerms = termsVersions.slice(0, -1).reverse()

  let doc = $state<LegalDocument | null>(null)
  let failed = $state(false)
  let copied = $state(false)
  let request = 0

  async function load(target: LegalKind, language: Locale) {
    // A slower earlier language must not replace the latest choice.
    const id = ++request
    failed = false
    try {
      const next = await loadLegalDocument(target, language)
      if (id === request) doc = next
    } catch {
      if (id === request) failed = true
    }
  }

  async function copyDigest() {
    try {
      await navigator.clipboard.writeText(currentTerms.sha256)
      copied = true
      setTimeout(() => (copied = false), 2000)
    } catch {
      // Clipboard blocked: the digest remains selectable.
    }
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
      <LocaleSwitcher />
      <nav class="legal-nav" aria-label={$t('Policies and help')}>
        {#each pages as item (item.kind)}
          <a
            href={item.href}
            aria-current={item.kind === kind ? 'page' : undefined}
            >{item.label}</a
          >
        {/each}
      </nav>
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
        {#if kind === 'terms'}
          <div class="terms-digest">
            <dl>
              <div>
                <dt>{$t('Version')}</dt>
                <dd>{currentTerms.version}</dd>
              </div>
              <div>
                <dt>SHA-256</dt>
                <dd class="digest-value">
                  <code dir="ltr">{currentTerms.sha256}</code>
                  <button
                    class="icon-button"
                    onclick={copyDigest}
                    aria-label={copied ? $t('Copied') : $t('Copy SHA-256')}
                    ><Icon name={copied ? 'check' : 'copy'} /></button
                  >
                </dd>
              </div>
            </dl>
            <p class="small"
              >{$t(
                'dMsg commerce records this digest with every purchase. To check it, download the English text and compute its SHA-256:'
              )}</p
            >
            <pre dir="ltr"><code>{verifyCommand}</code></pre>
            <div class="terms-links">
              <a
                class="text-link"
                href={currentTerms.path}
                target="_blank"
                rel="noopener"
                >{$t('Open the English text')}<Icon name="arrow-up-right" /></a
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
        {/if}
      </section>

      <div class="legal-layout">
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
