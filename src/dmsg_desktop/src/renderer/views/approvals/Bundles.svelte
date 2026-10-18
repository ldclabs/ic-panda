<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Id from '../../components/page/Id.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import { toast } from '../../components/toast.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { t, type Label } from '../../i18n/index.svelte'
  import { bridge, errorText, when } from '../../rpc'
  import { reasonLabels } from '../../approval'
  import Report from './Report.svelte'

  /**
   * Evidence: a bundle verified from a file picked or dropped here, with its
   * original file; and the account's signatures, this device's and its
   * other devices', each verified in place or exported as a bundle.
   */
  let { history, known, unlocked, open }: { history: any[]; known: Set<string>; unlocked: boolean; open: (id: string) => unknown } = $props()

  let report = $state<any>(null)
  let busy = $state(false)
  let over = $state(false)

  async function attempt(work: () => Promise<unknown>) {
    busy = true
    try {
      await work()
    } catch (e) {
      toast(() => errorText(e))
    } finally {
      busy = false
    }
  }

  /** Files are picked in the main process's dialogs; the window sees only the outcome. */
  const verify = (withFile: boolean) =>
    attempt(async () => {
      report = (await bridge(() => window.dmsg.verifyBundle(withFile))) ?? report
    })

  /** A bundle dropped here, with the original file when two are dropped; the preload resolves their paths. */
  function drop(event: DragEvent) {
    event.preventDefault()
    over = false
    const files = [...(event.dataTransfer?.files ?? [])]
    const bundle = files.find((f) => f.name.toLowerCase().endsWith('.json')) ?? files[0]
    if (!bundle) return
    const original = files.find((f) => f !== bundle)
    void attempt(async () => {
      report = await bridge(() => window.dmsg.verifyDropped(bundle, original))
    })
  }

  const verifyEntry = (execution: string) =>
    attempt(async () => {
      report = await bridge(() => window.dmsg.verifyEntry(execution))
    })

  const exportBundle = (execution: string) =>
    attempt(async () => {
      const saved = await bridge<{ path: string } | null>(() => window.dmsg.exportBundle(execution))
      if (saved) toast(() => `${t('bundleSaved')}: ${saved.path}`, { testid: 'approvals-notice' })
    })

  const save = () =>
    attempt(async () => {
      // A `$state` object is a proxy, which the bridge cannot clone.
      const saved = await bridge<{ path: string } | null>(() => window.dmsg.saveReport($state.snapshot(report)))
      if (saved) toast(() => `${t('reportSaved')}: ${saved.path}`)
    })

  const stages: Record<string, Label> = { complete: 'stageComplete', unknown: 'stageUnknown', failed: 'stageFailed', expired: 'stageExpired', lost: 'stageLost' }
  const tone = (stage: string) => (stage === 'complete' ? 'ok' : stage === 'failed' ? 'danger' : stage === 'unknown' ? 'neutral' : 'warn')
  const summary = (s: any) => (s.profile === 'app_action' ? `${s.content.app_id} · ${s.content.command}` : (s.content.text ?? s.content.sha256))
</script>

<Section title={t('verifyBundle')} description={t('verifyBundleHint')} icon="verified" testid="verify-bundle">
  <div
    class={cn('grid justify-items-center gap-3 rounded-xl border border-dashed px-4 py-5 text-center transition-colors', over ? 'border-forest bg-pale' : 'border-line')}
    role="region"
    aria-label={t('dropBundle')}
    ondragover={(event) => (event.preventDefault(), (over = true))}
    ondragleave={() => (over = false)}
    ondrop={drop}
    data-testid="verify-drop"
  >
    <p class="text-[12.5px] text-muted">{t('dropBundle')}</p>
    <div class="flex flex-wrap justify-center gap-2">
      <button class={buttonClass('secondary')} disabled={busy} onclick={() => verify(false)}><Icon name="upload" />{t('verifyBundle')}</button>
      <button class={buttonClass('secondary')} disabled={busy} onclick={() => verify(true)}><Icon name="file" />{t('verifyWithFile')}</button>
    </div>
  </div>
  {#if report}<Report {report} {save} />{/if}
  <p class="mt-3 text-[12px] text-faint" data-testid="verify-online">{t('verifyOnline', { url: 'dmsg.net/_/verify' })}</p>
</Section>

{#if unlocked}
  <Section title={t('signed')} description={t('signedHint')} icon="signature" flush>
    <ul class="divide-y divide-line border-t border-line" data-testid="signed-list">
      {#each history as entry (entry.execution)}
        <li class="flex items-start gap-3 px-5 py-3">
          <div class="min-w-0 flex-1">
            <div class="flex flex-wrap items-center gap-1.5">
              <Pill tone={tone(entry.stage)}>{t(stages[entry.stage] ?? 'stageUnknown')}</Pill>
              <Pill>{entry.this_device ? t('thisDevice') : t('otherDevice')}</Pill>
              <span class="text-[11.5px] text-faint">{when(entry.created_at)}</span>
            </div>
            <p class="mt-1 text-[13px] [overflow-wrap:anywhere]">{summary(entry.statement)}</p>
            {#if entry.error}<p class="text-[12px] text-warn">{reasonLabels[entry.error] ? t(reasonLabels[entry.error]) : entry.error}</p>{/if}
            <div class="mt-1 flex flex-wrap items-center gap-1.5 text-[11.5px] text-faint">
              <bdi dir="ltr" class="font-mono">{entry.origin}</bdi><Id id={entry.execution} />
              {#if known.has(entry.approval)}<button class="text-forest hover:underline" onclick={() => open(entry.approval)}>{t('openRequest')}</button>{/if}
            </div>
          </div>
          {#if entry.stage === 'complete'}
            <div class="flex shrink-0 flex-col gap-1.5">
              <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => verifyEntry(entry.execution)} data-testid="signed-verify"><Icon name="verified" />{t('verifyHere')}</button>
              <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => exportBundle(entry.execution)}><Icon name="download" />{t('exportBundle')}</button>
            </div>
          {/if}
        </li>
      {:else}
        <li class="px-5 py-3 text-[13px] text-faint">-</li>
      {/each}
    </ul>
  </Section>
{/if}
