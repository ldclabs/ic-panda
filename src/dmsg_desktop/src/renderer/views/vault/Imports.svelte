<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { t, type Label } from '../../i18n/index.svelte'
  import { dismiss, jobs, type Stage } from './importing.svelte'

  /** Files on their way in: encrypt, verify, then held by the relay. */
  const stages: { stage: Stage; label: Label }[] = [
    { stage: 'encrypt', label: 'stageEncrypt' },
    { stage: 'verify', label: 'stageVerify' },
    { stage: 'sync', label: 'stageSync' }
  ]
  const order: Stage[] = ['encrypt', 'verify', 'sync', 'done']
</script>

{#if jobs.length}
  <div class="pointer-events-none absolute inset-x-2 bottom-2 z-10 grid gap-1.5">
    {#each jobs as job (job.key)}
      {@const at = order.indexOf(job.stage)}
      <div
        class="pointer-events-auto rounded-xl border border-line bg-surface px-3 py-2.5 shadow-pop animate-in fade-in-0 slide-in-from-bottom-2"
        data-testid="import-job"
        data-stage={job.error ? 'error' : job.stage}
        role="status"
      >
        <div class="flex items-center gap-2">
          <Icon name={job.stage === 'done' ? 'verified' : 'file'} class={cn('shrink-0', job.stage === 'done' ? 'text-ok' : job.error ? 'text-danger' : 'text-faint')} />
          <b class="min-w-0 flex-1 truncate text-[12.5px] font-medium">{job.name || t('typeFile')}</b>
          {#if job.error || job.stage === 'done' || job.waiting}
            <button class={buttonClass('ghost', 'xs', '-me-1 size-6 px-0')} aria-label={t('dismiss')} onclick={() => dismiss(job.key)}>
              <Icon name="close" />
            </button>
          {/if}
        </div>
        <ol class="mt-2 grid grid-cols-3 gap-1.5">
          {#each stages as { stage, label }, i (stage)}
            {@const phase = job.error && i === at ? 'failed' : i < at ? 'done' : i === at && !job.waiting ? 'active' : 'pending'}
            <li class="grid gap-1">
              <span
                class={cn(
                  'h-1 overflow-hidden rounded-full',
                  phase === 'done' ? 'bg-forest' : phase === 'failed' ? 'bg-danger' : 'bg-line',
                  phase === 'active' && 'progress-run'
                )}
              ></span>
              <span class={cn('text-[11px]', phase === 'pending' ? 'text-faint' : phase === 'failed' ? 'text-danger' : 'text-ink-2')}>{t(label)}</span>
            </li>
          {/each}
        </ol>
        {#if job.error}
          <p class="mt-1.5 text-[11.5px] text-danger">{t('error')}: {job.error}</p>
        {:else if job.waiting}
          <p class="mt-1.5 text-[11.5px] text-faint">{t('importWaiting')}</p>
        {:else if job.stage === 'done'}
          <p class="mt-1.5 text-[11.5px] text-ok">{t('importDone')}</p>
        {/if}
      </div>
    {/each}
  </div>
{/if}
