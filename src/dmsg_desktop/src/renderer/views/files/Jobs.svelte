<script lang="ts">
  import Progress from '../../components/Progress.svelte'
  import { size } from '../../rpc'
  import Icon from '../../components/Icon.svelte'
  import { iconButtonClass, buttonClass, cn } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { dismiss, jobs, type Job } from './adding.svelte'

  /**
   * Cards for what the library has under way: adding, backing up, fetching.
   * An import or a backup under way can be cancelled; at most the newest
   * few show, the rest as a count.
   */
  let { nameOf, onCancel, onRetry, onCancelImport }: { nameOf: (job: Job) => string; onCancel: (job: Job) => void; onRetry: (job: Job) => void; onCancelImport: () => void } = $props()

  const SHOWN = 4
  const shown = $derived(jobs.slice(-SHOWN))
  const verb = (job: Job) => (job.kind === 'add' ? t('adding') : job.kind === 'backup' ? t('statusBackingUp') : job.kind === 'fetch' ? t('fetchFile') : t('verifying'))
  const icon = (job: Job) => (job.kind === 'add' ? (job.folder ? 'folder' : 'add') : job.kind === 'backup' ? 'cloud-up' : job.kind === 'fetch' ? 'cloud-down' : 'verified')
  const count = (job: Job) => job.totalBytes ? `${size(job.bytes ?? 0)} / ${size(job.totalBytes)}${job.speed && job.stage === 'running' ? ` · ${size(job.speed)}/s` : ''}` : (job.folder ? t('filesAddedCount', { n: job.files ?? 0 }) : job.total ? `${job.done}/${job.total}` : '')
  const label = (job: Job) => job.stage === 'committing' ? t('transferCommitting') : job.stage === 'verifying' ? t('verifying') : job.stage === 'paused' ? t('transferLockPauses') : job.stage === 'retrying' ? t('retry') : job.stage === 'queued' ? t('waiting') : verb(job)
</script>

{#if jobs.length}
  <div class="pointer-events-none absolute inset-x-2 bottom-2 z-10 grid min-w-0 grid-cols-[minmax(0,1fr)] max-h-[calc(100%-1rem)] gap-1.5 overflow-x-hidden overflow-y-auto">
    {#if jobs.length > SHOWN}
      <p class="pointer-events-auto justify-self-center rounded-full border border-line bg-surface px-2.5 py-0.5 text-[11px] text-faint" data-testid="files-jobs-more">
        {t('moreJobs', { n: jobs.length - SHOWN })}
      </p>
    {/if}
    {#each shown as job (job.key)}
      <div
        class="pointer-events-auto min-w-0 rounded-xl border border-line bg-surface px-3 py-2.5 shadow-pop animate-in fade-in-0 slide-in-from-bottom-2"
        data-testid="files-job"
        data-kind={job.kind}
        data-stage={job.stage}
        role="status"
      >
        <div class="flex flex-wrap items-center gap-2">
          <Icon
            name={job.stage === 'done' ? 'verified' : job.stage === 'error' ? 'warning' : job.stage === 'cancelled' ? 'close' : icon(job)}
            class={cn('shrink-0', job.stage === 'done' ? 'text-ok' : job.stage === 'error' ? 'text-danger' : 'text-faint')}
          />
          <b class="min-w-0 flex-1 truncate text-[12.5px] font-medium" title={nameOf(job)} dir="auto">{nameOf(job)}</b>
          {#if (job.kind === 'backup' || job.upload) && ['error', 'retrying', 'paused'].includes(job.stage)}
            <button class={buttonClass('ghost', 'xs')} onclick={() => onRetry(job)}>{t('retry')}</button>
          {/if}
          {#if job.kind !== 'add' && job.stage !== 'done' && (job.version || job.upload)}
            <button class={buttonClass('ghost', 'xs', '-me-1 h-6 px-1.5')} onclick={() => onCancel(job)} data-testid="files-job-cancel">{t('cancel')}</button>
          {:else if job.kind === 'add' && job.stage === 'running' && job.cancellable}
            <button class={buttonClass('ghost', 'xs', '-me-1 h-6 px-1.5')} disabled={job.cancelling} onclick={onCancelImport} data-testid="files-job-cancel-import">{t('cancel')}</button>
          {:else if job.stage !== 'running'}
            <button class={iconButtonClass('ghost', 'lg')} aria-label={t('dismiss')} onclick={() => dismiss(job.key)}>
              <Icon name="close" />
            </button>
          {/if}
        </div>
        {#if job.stage !== 'done' && count(job)}<p class="mt-1.5 text-[11px] text-faint tabular-nums"><bdi>{count(job)}</bdi></p>{/if}
        {#if !['done', 'error', 'cancelled'].includes(job.stage)}
          <div class="mt-2"><Progress value={job.total && !job.folder && !['committing', 'verifying'].includes(job.stage) ? job.done : null} max={job.total || 1} label={label(job)} /></div>
        {/if}
        {#if ['done', 'error', 'cancelled'].includes(job.stage) || job.current}
        <p class={cn('mt-1.5 truncate text-[11.5px]', job.stage === 'error' ? 'text-danger' : job.stage === 'done' ? 'text-ok' : 'text-faint')}>
          {#if job.stage === 'error'}
            {job.kind === 'add' ? t('addFailed') : t('error')}: {job.error}
          {:else if job.stage === 'cancelled'}
            {t('importCancelled')}
          {:else if job.stage === 'done'}
            {job.kind === 'add' ? t('added') : job.kind === 'backup' ? t('statusBackedUp') : job.kind === 'fetch' ? t('fetched') : t('verified')}{job.note ? ` · ${job.note}` : ''}
          {:else}
            {label(job)}{job.current ? ` · ${job.current}` : ''}
          {/if}
        </p>
        {/if}
        {#if job.retryAt && job.stage === 'retrying'}<p class="mt-1 text-[11.5px] text-faint">{t('syncRetryAt', { time: new Date(job.retryAt).toLocaleTimeString() })}</p>{/if}
      </div>
    {/each}
  </div>
{/if}
