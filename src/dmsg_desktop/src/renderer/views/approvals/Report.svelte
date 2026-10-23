<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Fact from '../../components/page/Fact.svelte'
  import Id from '../../components/page/Id.svelte'
  import { toast } from '../../components/toast.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { t, type Label } from '../../i18n/index.svelte'
  import { bridge, errorText, when } from '../../rpc'

  /**
   * A bundle's verification, check by check (PRD S05): content against the
   * file, the device's signature, the receipt's binding to the account and
   * key, the authorization evidence a user home certified, and the device's
   * present status, which never undoes a signature that verifies.
   */
  let { report, save }: { report: any; save?: () => unknown } = $props()

  const signerProfile = $derived(
    report.signature && report.evidence && report.binding
      ? `https://dmsg.net/${encodeURIComponent(report.account)}`
      : null
  )

  async function openProfile(event: MouseEvent) {
    event.preventDefault()
    const url = signerProfile
    if (!url) return
    try {
      await bridge(() => window.dmsg.openUrl(url))
    } catch (e) {
      toast(() => errorText(e))
    }
  }

  const contents: Record<string, Label> = {
    matches_file: 'contentMatches',
    mismatch: 'contentMismatch',
    digest_only: 'digestOnly',
    text: 'contentText',
    action: 'contentAction',
    unknown: 'contentUnknown'
  }
  const currents: Record<string, Label> = { active: 'currentActive', revoked: 'currentRevoked', unknown: 'currentUnknown' }
  const summary = (s: any) => (s?.profile === 'app_action' ? `${s.content.app_id} · ${s.content.command}` : (s?.content?.text ?? s?.content?.sha256 ?? ''))

  const checks = $derived<[Label, boolean | null, string][]>([
    ['checkContent', report.content === 'mismatch' ? false : report.content === 'unknown' ? null : true, t(contents[report.content] ?? 'contentUnknown')],
    ['checkSignature', report.signature, report.key_fingerprint ?? ''],
    ['checkBinding', report.binding, report.device_id ? `${t('device')} ${report.device_id.slice(0, 12)} · ${t('securityEpoch')} ${report.security_epoch}` : ''],
    ['checkEvidence', report.evidence, report.certified_at ? `${report.home} · ${when(report.certified_at)}` : report.home],
    ['checkCurrent', report.current === 'active' ? true : report.current === 'revoked' ? false : null, t(currents[report.current] ?? 'currentUnknown')]
  ])
</script>

<div class="mt-4 overflow-hidden rounded-xl border border-line" data-testid="verify-report">
  <div class={cn('flex items-center gap-3 px-4 py-3', report.valid ? 'bg-ok-pale' : 'bg-danger-pale')}>
    <span class={cn('grid size-9 place-items-center rounded-full text-paper', report.valid ? 'bg-ok' : 'bg-danger')}>
      <Icon name={report.valid ? 'check' : 'close'} />
    </span>
    <div class="min-w-0 flex-1">
      <b class="block text-[14px] font-semibold" data-testid="verify-result">{report.valid ? t('reportValid') : t('reportInvalid')}</b>
      {#if report.statement}<span class="block truncate text-[12px] text-muted">{summary(report.statement)}</span>{/if}
    </div>
    {#if save}<button class={buttonClass('ghost', 'sm')} onclick={save} data-testid="verify-save"><Icon name="download" />{t('saveReport')}</button>{/if}
  </div>
  <ul class="divide-y divide-line border-b border-line" data-testid="verify-checks">
    {#each checks as [label, passed, detail] (label)}
      <li class="flex items-start gap-2.5 px-4 py-2 text-[13px]">
        <span
          class={cn(
            'mt-0.5 grid size-4 shrink-0 place-items-center rounded-full',
            passed === true && 'bg-ok text-surface',
            passed === false && (label === 'checkCurrent' ? 'bg-warn text-surface' : 'bg-danger text-surface'),
            passed === null && 'border border-edge-soft text-faint'
          )}><Icon name={passed === true ? 'check' : passed === false ? 'close' : 'info'} class="size-2.5" /></span
        >
        <span class="w-36 shrink-0 text-ink-2">{t(label)}</span>
        <span class="min-w-0 flex-1 font-mono text-[11.5px] text-muted [overflow-wrap:anywhere]">{detail}</span>
      </li>
    {/each}
  </ul>
  <dl class="divide-y divide-line px-4">
    <Fact label={t('account')}><Id id={report.account} full /></Fact>
    {#if signerProfile}
      <Fact label={t('publicProfile')}>
        <a href={signerProfile} onclick={openProfile} class="inline-flex min-h-8 items-center gap-1.5 text-forest underline underline-offset-4 hover:text-ink" data-testid="verify-signer-profile"><bdi dir="ltr">{signerProfile}</bdi><Icon name="external" /></a>
      </Fact>
    {/if}
    {#if report.approved_at}<Fact label={t('approvedAt')}>{when(report.approved_at)}</Fact>{/if}
    {#if report.origin}<Fact label={t('origin')}><bdi dir="ltr" class="font-mono text-[12.5px]">{report.origin}</bdi></Fact>{/if}
    {#if report.statement}<Fact label={t('statement')}>{report.statement.profile}: {summary(report.statement)}</Fact>{/if}
  </dl>
</div>
