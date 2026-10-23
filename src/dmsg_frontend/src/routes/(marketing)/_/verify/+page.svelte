<script lang="ts">
  import Brand from '$lib/components/landing/Brand.svelte'
  import Icon from '$lib/components/landing/Icon.svelte'
  import IconCheckLine from '$lib/components/icons/IconCheckLine.svelte'
  import IconClose from '$lib/components/icons/IconClose.svelte'
  import IconInfo from '$lib/components/icons/IconInfo.svelte'
  import IconArticleLine from '$lib/components/icons/IconArticleLine.svelte'
  import FilePicker from '$lib/components/verify/FilePicker.svelte'
  import '$lib/components/landing/site.css'
  import LocaleSwitcher from '$lib/i18n/LocaleSwitcher.svelte'
  import { locale, t } from '$lib/i18n'
  import {
    readBundle,
    verifyBundle,
    VerifyError,
    type BundleReport
  } from '$lib/verify'

  let bundleFile = $state<File | null>(null)
  let originalFile = $state<File | null>(null)
  let report = $state<BundleReport | null>(null)
  let error = $state('')
  let busy = $state(false)

  function resetResult() {
    report = null
    error = ''
  }

  async function verify() {
    if (!bundleFile || busy) return
    const bundleInput = bundleFile
    const originalInput = originalFile
    busy = true
    resetResult()
    try {
      const bundle = readBundle(await bundleInput.text())
      const file = originalInput
        ? new Uint8Array(await originalInput.arrayBuffer())
        : undefined
      report = await verifyBundle(bundle, file)
    } catch (e) {
      error =
        e instanceof VerifyError && e.reason === 'not_bundle'
          ? 'This is not a dMsg attestation bundle.'
          : 'Could not reach the Internet Computer. Try again.'
    } finally {
      busy = false
    }
  }

  const hex = (bytes?: Uint8Array) =>
    bytes
      ? Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('')
      : ''
  const time = (ms: bigint, language: string) =>
    new Date(Number(ms)).toLocaleString(language)

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
                : ['unknown', 'digest_only'].includes(report.content)
                  ? null
                  : true,
            detail: contentText
          },
          { label: $t('Signature'), ok: report.signature },
          { label: $t('Key and account'), ok: report.binding },
          { label: $t('Authorization evidence'), ok: report.evidence }
        ]
  )

  // Account names in the input alone are not proof of the signer's identity.
  const signerVerified = $derived(
    report?.signature && report.evidence && report.binding
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
  <title>{`${$t('Verify a dMsg signature')} | dMsg`}</title>
  <meta name="color-scheme" content="light" />
</svelte:head>

<div class="dmsg-site verify-site">
  <a class="skip" href="#main">{$t('Skip to content')}</a>
  <header class="site-header">
    <div class="wrap nav-wrap"><Brand /><LocaleSwitcher /></div>
  </header>

  <main id="main" class="verify-wrap" tabindex="-1">
    <div class="verify-intro">
      <h1>{$t('Verify a dMsg signature')}</h1>
      <p
        >{$t(
          'Check who signed, what was signed, and whether the evidence verifies.'
        )}</p
      >
    </div>

    <div class="verify-layout">
      <form
        class="verify-form"
        aria-busy={busy}
        onsubmit={(event) => {
          event.preventDefault()
          void verify()
        }}
      >
        <FilePicker
          label={$t('Attestation bundle')}
          hint={$t('Choose the JSON bundle exported from dMsg.')}
          accept=".json,application/json"
          bind:file={bundleFile}
          disabled={busy}
          testId="verify-bundle"
          onchange={resetResult}
        />
        <FilePicker
          label={$t('Original file (optional)')}
          hint={$t('Add the original file to compare its signed digest.')}
          bind:file={originalFile}
          disabled={busy}
          optional
          testId="verify-original"
          onchange={resetResult}
        />
        <button
          class="button primary"
          type="submit"
          disabled={busy || !bundleFile}
        >
          {busy ? $t('Verifying…') : $t('Verify')}
          {#if !busy}<Icon name="arrow-right" />{/if}
        </button>
        <p class="privacy-note"
          >{$t(
            'Verification runs in your browser. Your files are never uploaded.'
          )}</p
        >
      </form>

      <div class="result-area" aria-live="polite" aria-busy={busy}>
        {#if error}
          <section class="result-placeholder error" role="alert">
            <span class="large-icon" aria-hidden="true"><IconClose /></span>
            <h2>{$t('Could not verify')}</h2>
            <p>{$t(error)}</p>
          </section>
        {:else if report}
          <section class="verify-report" data-testid="verify-report">
            <div class="result-heading" class:invalid={!report.valid}>
              <span class="result-icon" aria-hidden="true">
                {#if report.valid}<IconCheckLine />{:else}<IconClose />{/if}
              </span>
              <h2
                >{report.valid
                  ? $t('The bundle verifies')
                  : $t('The bundle does not verify')}</h2
              >
            </div>

            <div class="report-body">
              {#if signerVerified}
                <div class="signer">
                  <span class="field-label">{$t('Signer')}</span>
                  <a
                    class="signer-link"
                    href={`/${encodeURIComponent(report.account)}`}
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    <code dir="ltr">{report.account}</code>
                    <span class="profile-link"
                      >{$t('View profile')}<Icon name="arrow-up-right" /></span
                    >
                  </a>
                </div>
              {/if}

              <ul class="checks">
                {#each checks as check (check.label)}
                  <li
                    class:failed={check.ok === false}
                    class:pending={check.ok === null}
                  >
                    <span class="check-icon" aria-hidden="true">
                      {#if check.ok === false}<IconClose
                        />{:else if check.ok}<IconCheckLine />{:else}<IconInfo
                        />{/if}
                    </span>
                    <span class="check-label">{check.label}</span>
                    <span class="check-status"
                      >{check.ok === null
                        ? $t('Not checked')
                        : check.ok
                          ? $t('Passed')
                          : $t('Failed')}</span
                    >
                    {#if check.detail}<p class="check-detail">{check.detail}</p
                      >{/if}
                  </li>
                {/each}
              </ul>

              <dl class="statement-details">
                {#if statement}
                  <div class="statement"
                    ><dt>{$t('Statement')}</dt><dd dir="auto">{statement}</dd
                    ></div
                  >
                {/if}
                {#if report.approvedAt !== undefined}
                  <div
                    ><dt>{$t('Approved')}</dt><dd
                      >{time(report.approvedAt, $locale)}</dd
                    ></div
                  >
                {/if}
                {#if report.origin}
                  <div
                    ><dt>{$t('Website')}</dt><dd><bdi>{report.origin}</bdi></dd
                    ></div
                  >
                {/if}
              </dl>

              <details class="technical-details">
                <summary>{$t('Verification details')}</summary>
                <dl>
                  {#if !signerVerified}
                    <div
                      ><dt>{$t('Account in bundle')}</dt><dd
                        ><code dir="ltr">{report.account}</code></dd
                      ></div
                    >
                  {/if}
                  {#if report.keyFingerprint}
                    <div
                      ><dt>{$t('Signing key fingerprint')}</dt><dd
                        ><code dir="ltr">{hex(report.keyFingerprint)}</code></dd
                      ></div
                    >
                  {/if}
                  {#if report.deviceId}
                    <div
                      ><dt>{$t('Device')}</dt><dd
                        ><code dir="ltr">{hex(report.deviceId)}</code></dd
                      ></div
                    >
                  {/if}
                  <div
                    ><dt>{$t('User home')}</dt><dd
                      ><code dir="ltr">{report.home}</code></dd
                    ></div
                  >
                  {#if report.certifiedAt !== undefined}
                    <div
                      ><dt>{$t('Certified at')}</dt><dd
                        >{time(report.certifiedAt, $locale)}</dd
                      ></div
                    >
                  {/if}
                </dl>
              </details>
              <p class="verification-note"
                >{$t(
                  'Whether the signing device is still active is not checked here. A device revoked later does not undo a signature that verifies.'
                )}</p
              >
            </div>
          </section>
        {:else}
          <section class="result-placeholder" role="status">
            <span class="large-icon" aria-hidden="true"
              ><IconArticleLine /></span
            >
            <h2>{busy ? $t('Verifying…') : $t('Ready to verify')}</h2>
            <p
              >{busy
                ? $t('Checking the signature and authorization evidence…')
                : $t(
                    'Select an attestation bundle to see the signer and verification results here.'
                  )}</p
            >
          </section>
        {/if}
      </div>
    </div>
  </main>
</div>

<style>
  .verify-wrap {
    max-width: 1160px;
    margin: 0 auto;
    padding: 48px 40px 80px;
  }
  .verify-intro {
    margin-bottom: 32px;
  }
  .verify-intro h1 {
    font-size: clamp(28px, 3vw, 36px);
    font-weight: 650;
    line-height: 1.3;
    letter-spacing: -0.035em;
  }
  .verify-intro h1:lang(zh),
  .verify-intro h1:lang(ar) {
    letter-spacing: normal;
  }
  .verify-intro p {
    margin: 12px 0 0;
    max-width: 65ch;
    font-size: 15px;
  }
  .verify-layout {
    display: grid;
    grid-template-columns: 320px minmax(0, 1fr);
    align-items: start;
    gap: 32px;
  }
  .verify-form {
    display: grid;
    gap: 16px;
    min-width: 0;
  }
  .verify-form .button {
    width: 100%;
    min-height: 48px;
  }
  .verify-form .button:disabled {
    background: var(--line);
    color: var(--muted);
    cursor: not-allowed;
  }
  .verify-form .button:disabled :global(.icon) {
    filter: none;
    opacity: 0.6;
  }
  .verify-form .privacy-note {
    margin: 0;
    padding-inline: 4px;
    font-size: 12px;
    line-height: 1.7;
  }
  .result-area {
    min-width: 0;
  }
  .result-placeholder {
    display: flex;
    min-height: 416px;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 40px;
    border: 1px dashed #b8cdbd;
    border-radius: 16px;
    text-align: center;
  }
  .large-icon {
    display: grid;
    place-items: center;
    width: 56px;
    height: 56px;
    margin-bottom: 20px;
    border-radius: 16px;
    background: var(--pale);
    color: var(--action);
  }
  .verify-site h2 {
    font-size: 19px;
    line-height: 1.5;
    letter-spacing: -0.02em;
    font-weight: 650;
  }
  .result-placeholder p {
    max-width: 36ch;
    margin: 12px 0 0;
    font-size: 14px;
  }
  .result-placeholder.error {
    border-color: #e8b6b6;
    background: #fef2f2;
  }
  .error .large-icon,
  .invalid .result-icon {
    background: #fee2e2;
    color: #991b1b;
  }
  .verify-report {
    border: 1px solid var(--line);
    border-radius: 16px;
    background: #fff;
    overflow: hidden;
  }
  .result-heading {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 22px 28px;
    border-bottom: 1px solid var(--line);
    background: #f0f4ee;
  }
  .result-heading.invalid {
    background: #fef2f2;
    color: #991b1b;
    border-color: #e8b6b6;
  }
  .result-icon {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: 50%;
    flex-shrink: 0;
    background: #dceadd;
    color: var(--action);
  }
  .report-body {
    padding: 24px 28px;
  }
  .field-label,
  dt {
    color: var(--muted);
    font-size: 13px;
  }
  .signer {
    padding-bottom: 20px;
  }
  .verify-site .signer-link {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 4px 16px;
    min-height: 44px;
    color: var(--action);
  }
  .signer-link code {
    font-size: 15px;
    font-weight: 600;
  }
  .profile-link {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 13px;
    text-decoration: underline;
    text-underline-offset: 4px;
  }
  .checks {
    display: grid;
    gap: 14px;
    margin: 0;
    padding: 20px 0;
    border-block: 1px solid var(--line);
    list-style: none;
  }
  .checks li {
    display: grid;
    grid-template-columns: 20px minmax(0, 1fr) auto;
    align-items: center;
    gap: 2px 10px;
    color: var(--action);
    font-size: 13px;
  }
  .check-icon {
    display: flex;
  }
  .check-icon :global(svg) {
    width: 18px;
    height: 18px;
  }
  .check-label {
    color: var(--ink);
    font-size: 14px;
  }
  .check-status {
    font-size: 12px;
  }
  .checks .failed {
    color: #b91c1c;
  }
  .checks .pending {
    color: #92400e;
  }
  .checks .check-detail {
    grid-column: 2 / -1;
    margin: 4px 0 0;
    font-size: 13px;
    color: inherit;
  }
  dl {
    display: grid;
    gap: 14px;
    margin: 0;
  }
  dl > div {
    display: grid;
    grid-template-columns: 100px minmax(0, 1fr);
    gap: 6px 16px;
  }
  dd {
    margin: 0;
    font-size: 14px;
    overflow-wrap: anywhere;
  }
  code {
    font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
    font-size: 12px;
    overflow-wrap: anywhere;
  }
  .statement-details:not(:empty) {
    padding-top: 24px;
  }
  .statement-details .statement {
    grid-template-columns: minmax(0, 1fr);
  }
  .statement dd {
    padding: 12px 16px;
    border-inline-start: 2px solid #b8cdbd;
    background: var(--canvas);
    white-space: pre-wrap;
  }
  .technical-details {
    margin-top: 20px;
  }
  summary {
    padding-block: 12px;
    color: var(--muted);
    font-size: 13px;
    cursor: pointer;
    min-height: 44px;
  }
  .technical-details dl {
    margin-block: 8px 20px;
  }
  .report-body .verification-note {
    margin: 16px 0 0;
    padding-top: 16px;
    border-top: 1px solid var(--line);
    font-size: 12px;
    line-height: 1.7;
  }
  @media (max-width: 900px) {
    .verify-wrap {
      padding-inline: 24px;
    }
    .verify-layout {
      grid-template-columns: 280px minmax(0, 1fr);
      gap: 24px;
    }
    .report-body,
    .result-heading {
      padding: 20px;
    }
    dl > div {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  @media (max-width: 700px) {
    .verify-wrap {
      padding: 28px 20px 48px;
    }
    .verify-intro {
      margin-bottom: 24px;
    }
    .verify-layout {
      grid-template-columns: minmax(0, 1fr);
      gap: 28px;
    }
    .result-placeholder {
      min-height: 240px;
      padding: 28px 24px;
    }
  }
</style>
