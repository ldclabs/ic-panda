<script lang="ts">
  import Brand from '$lib/components/landing/Brand.svelte'
  import '$lib/components/landing/site.css'
  import '$lib/components/legal/legal.css'
  import LocaleSwitcher from '$lib/i18n/LocaleSwitcher.svelte'
  import { t } from '$lib/i18n'
  import {
    readBundle,
    verifyBundle,
    VerifyError,
    type BundleReport
  } from '$lib/verify'

  /**
   * Verify a dMsg attestation bundle (`dmsg-attestation/1`) without dMsg:
   * check by check, in this browser, against the deployment's user homes
   * and the Internet Computer's root key. A file to compare is hashed here
   * and never uploaded.
   */
  let bundleFile = $state<File | null>(null)
  let originalFile = $state<File | null>(null)
  let report = $state<BundleReport | null>(null)
  let error = $state('')
  let busy = $state(false)

  async function verify() {
    if (!bundleFile) return
    busy = true
    error = ''
    report = null
    try {
      const bundle = readBundle(await bundleFile.text())
      const file = originalFile
        ? new Uint8Array(await originalFile.arrayBuffer())
        : undefined
      report = await verifyBundle(bundle, file)
    } catch (e) {
      error =
        e instanceof VerifyError && e.reason === 'not_bundle'
          ? $t('This is not a dMsg attestation bundle.')
          : $t('Could not reach the Internet Computer. Try again.')
    } finally {
      busy = false
    }
  }

  const picked = (event: Event) =>
    (event.currentTarget as HTMLInputElement).files?.[0] ?? null

  const hex = (bytes?: Uint8Array) =>
    bytes
      ? Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('')
      : ''
  const time = (ms?: bigint) =>
    ms === undefined ? '' : new Date(Number(ms)).toLocaleString()

  const contentText = $derived(
    !report
      ? ''
      : {
          matches_file: $t('The file matches the signed digest'),
          mismatch: $t('The file does not match the signed digest'),
          text: $t('A text statement'),
          digest_only: $t(
            'Only the digest was signed; add the original file to compare it'
          ),
          action: $t('An application action'),
          unknown: $t('Unknown: the signature does not verify')
        }[report.content]
  )

  const checks = $derived(
    !report
      ? []
      : [
          {
            label: $t('Content'),
            ok:
              report.content === 'mismatch'
                ? false
                : report.content === 'unknown'
                  ? null
                  : true,
            detail: contentText
          },
          {
            label: $t('Signature'),
            ok: report.signature,
            detail: hex(report.keyFingerprint)
          },
          {
            label: $t('Key and account'),
            ok: report.binding,
            detail: report.account
          },
          {
            label: $t('Authorization evidence'),
            ok: report.evidence,
            detail: [report.home, time(report.certifiedAt)]
              .filter(Boolean)
              .join(' · ')
          }
        ]
  )

  const statement = $derived.by(() => {
    const content = report?.statement?.content
    if (!content) return ''
    if (content.kind === 'app_action')
      return `${content.action.app_id} · ${content.action.command.name}`
    return content.kind === 'digest' ? hex(content.sha256) : content.text
  })
</script>

<svelte:head>
  <title>{`${$t('Verify a dMsg signature')} — dMsg`}</title>
  <meta name="color-scheme" content="light" />
</svelte:head>

<div class="dmsg-site legal-site">
  <a class="skip" href="#main">{$t('Skip to content')}</a>
  <header class="site-header">
    <div class="wrap nav-wrap">
      <Brand />
      <LocaleSwitcher />
    </div>
  </header>

  <main id="main" tabindex="-1">
    <div class="wrap">
      <section class="legal-hero" aria-labelledby="verify-title">
        <h1 id="verify-title">{$t('Verify a dMsg signature')}</h1>
        <p class="small"
          >{$t(
            'Check an attestation bundle exported from dMsg. It is verified in this browser: a file you compare is hashed here and never uploaded.'
          )}</p
        >
      </section>

      <form
        class="verify-form"
        onsubmit={(event) => {
          event.preventDefault()
          void verify()
        }}
      >
        <label>
          <span>{$t('Attestation bundle')}</span>
          <input
            type="file"
            accept=".json,application/json"
            onchange={(event) => (bundleFile = picked(event))}
            data-testid="verify-bundle"
          />
        </label>
        <label>
          <span>{$t('Original file (optional)')}</span>
          <input
            type="file"
            onchange={(event) => (originalFile = picked(event))}
            data-testid="verify-original"
          />
        </label>
        <button
          class="button primary"
          type="submit"
          disabled={busy || !bundleFile}
          >{busy ? $t('Verifying…') : $t('Verify')}</button
        >
      </form>

      {#if error}<p class="verify-error" role="alert">{error}</p>{/if}

      {#if report}
        <section
          class="verify-report"
          class:valid={report.valid}
          aria-live="polite"
          data-testid="verify-report"
        >
          <h2>
            <span class="mark" aria-hidden="true"
              >{report.valid ? '✓' : '✕'}</span
            >
            {report.valid
              ? $t('The bundle verifies')
              : $t('The bundle does not verify')}
          </h2>
          <ul>
            {#each checks as check (check.label)}
              <li class:failed={check.ok === false}>
                <span class="mark" aria-hidden="true"
                  >{check.ok === false ? '✕' : check.ok ? '✓' : '·'}</span
                >
                <span class="check-label">{check.label}</span>
                <code dir="ltr">{check.detail}</code>
              </li>
            {/each}
          </ul>
          <dl>
            {#if statement}
              <div><dt>{$t('Statement')}</dt><dd>{statement}</dd></div>
            {/if}
            {#if report.deviceId}
              <div
                ><dt>{$t('Device')}</dt><dd
                  ><code dir="ltr">{hex(report.deviceId)}</code></dd
                ></div
              >
            {/if}
            {#if report.approvedAt !== undefined}
              <div
                ><dt>{$t('Approved')}</dt><dd>{time(report.approvedAt)}</dd
                ></div
              >
            {/if}
            {#if report.origin}
              <div
                ><dt>{$t('Website')}</dt><dd
                  ><code dir="ltr">{report.origin}</code></dd
                ></div
              >
            {/if}
          </dl>
          <p class="small"
            >{$t(
              'Whether the signing device is still active is not checked here. A device revoked later does not undo a signature that verifies.'
            )}</p
          >
        </section>
      {/if}
    </div>
  </main>
</div>

<style>
  .verify-form {
    display: grid;
    gap: 1rem;
    max-width: 36rem;
    margin-block: 1.5rem 2rem;
  }
  .verify-form label {
    display: grid;
    gap: 0.35rem;
    font-size: 0.9rem;
  }
  .verify-form button {
    justify-self: start;
  }
  .verify-error {
    color: #b91c1c;
  }
  .verify-report {
    max-width: 48rem;
    margin-bottom: 3rem;
    border: 1px solid #f0c4c4;
    border-radius: 1rem;
    padding: 1.25rem;
  }
  .verify-report.valid {
    border-color: #b9dcc9;
  }
  .verify-report h2 {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 1.15rem;
    margin: 0 0 1rem;
  }
  .verify-report ul {
    list-style: none;
    margin: 0 0 1rem;
    padding: 0;
    display: grid;
    gap: 0.5rem;
  }
  .verify-report li {
    display: grid;
    grid-template-columns: 1.25rem 11rem minmax(0, 1fr);
    gap: 0.5rem;
    align-items: start;
    color: #145c45;
  }
  .verify-report li.failed {
    color: #b91c1c;
  }
  .verify-report code {
    overflow-wrap: anywhere;
    font-size: 0.8rem;
  }
  .check-label {
    color: #10251f;
  }
  .verify-report dl div {
    display: grid;
    grid-template-columns: 11rem minmax(0, 1fr);
    gap: 0.5rem;
    padding-block: 0.25rem;
  }
  .verify-report dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  @media (max-width: 640px) {
    .verify-report li,
    .verify-report dl div {
      grid-template-columns: 1.25rem minmax(0, 1fr);
    }
    .verify-report dl div {
      grid-template-columns: minmax(0, 1fr);
    }
    .verify-report li code {
      grid-column: 2;
    }
  }
</style>
