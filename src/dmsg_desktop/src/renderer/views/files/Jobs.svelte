<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { dismiss, jobs, type Job } from './adding.svelte'

  /** Cards for what the library has under way: adding, backing up, fetching. A backup under way can be cancelled. */
  let { names, onCancel }: { names: Record<string, string>; onCancel: (job: Job) => void } = $props()

  const label = (job: Job) => (job.kind === 'add' ? job.name : names[job.name] ?? job.name)
  const verb = (job: Job) => (job.kind === 'add' ? t('adding') : job.kind === 'backup' ? t('statusBackingUp') : job.kind === 'fetch' ? t('fetchFile') : t('verifying'))
  const icon = (job: Job) => (job.kind === 'add' ? 'add' : job.kind === 'backup' ? 'cloud-up' : job.kind === 'fetch' ? 'cloud-down' : 'verified')
</script>

{#if jobs.length}
  <div class="pointer-events-none absolute inset-x-2 bottom-2 z-10 grid gap-1.5">
    {#each jobs as job (job.key)}
      <div
        class="pointer-events-auto rounded-xl border border-line bg-surface px-3 py-2.5 shadow-pop animate-in fade-in-0 slide-in-from-bottom-2"
        data-testid="files-job"
        data-kind={job.kind}
        data-stage={job.stage}
        role="status"
      >
        <div class="flex items-center gap-2">
          <Icon name={job.stage === 'done' ? 'verified' : job.stage === 'error' ? 'warning' : icon(job)} class={cn('shrink-0', job.stage === 'done' ? 'text-ok' : job.stage === 'error' ? 'text-danger' : 'text-faint')} />
          <b class="min-w-0 flex-1 truncate text-[12.5px] font-medium">{label(job)}</b>
          <span class="text-[11px] text-faint tabular-nums">{job.total ? `${job.done}/${job.total}` : ''}</span>
          {#if job.kind === 'backup' && job.stage !== 'done' && job.version}
            <button class={buttonClass('ghost', 'xs', '-me-1 h-6 px-1.5')} onclick={() => onCancel(job)} data-testid="files-job-cancel">{t('cancelBackup')}</button>
          {:else if job.stage !== 'running'}
            <button class={buttonClass('ghost', 'xs', '-me-1 size-6 px-0')} aria-label={t('dismiss')} onclick={() => dismiss(job.key)}>
              <Icon name="close" />
            </button>
          {/if}
        </div>
        <span class={cn('mt-2 block h-1 overflow-hidden rounded-full', job.stage === 'error' ? 'bg-danger' : job.stage === 'done' ? 'bg-forest' : 'bg-line')}>
          {#if job.stage === 'running'}
            {#if job.total}
              <span class="block h-full rounded-full bg-forest transition-[width] duration-300" style:width={`${Math.round((100 * job.done) / job.total)}%`}></span>
            {:else}
              <span class="progress-run block h-full"></span>
            {/if}
          {/if}
        </span>
        <p class={cn('mt-1.5 text-[11.5px]', job.stage === 'error' ? 'text-danger' : job.stage === 'done' ? 'text-ok' : 'text-faint')}>
          {#if job.stage === 'error'}
            {job.kind === 'add' ? t('addFailed') : t('error')}: {job.error}
          {:else if job.stage === 'done'}
            {job.kind === 'add' ? t('added') : job.kind === 'backup' ? t('statusBackedUp') : job.kind === 'fetch' ? t('fetched') : t('verified')}{job.note ? ` · ${job.note}` : ''}
          {:else}
            {verb(job)}
          {/if}
        </p>
      </div>
    {/each}
  </div>
{/if}
