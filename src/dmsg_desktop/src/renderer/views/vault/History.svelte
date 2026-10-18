<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { toast } from '../../components/toast.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { call, errorText, when } from '../../rpc'
  import SecretField from './SecretField.svelte'
  import { fieldName, isHidden, sections } from './vault'

  /**
   * Every version of an item, newest first: who wrote it and when, and
   * whether it was a deletion or a conflict copy. A version opens below
   * the list as it was, hidden values on request; restoring one makes its
   * content current again as a new version, so nothing is overwritten.
   */
  let {
    id,
    head,
    device,
    unlocked,
    editable,
    onClose
  }: { id: string; head: string; device: string | null; unlocked: boolean; editable: boolean; onClose: () => void } = $props()

  let versions = $state<any[]>([])
  let open = $state<string | null>(null)
  let shown = $state<any>(null)
  let busy = $state(false)

  $effect(() => {
    void head
    call<any[]>('vault/history', { id })
      .then((list) => (versions = list))
      .catch((e) => toast(() => errorText(e)))
  })

  $effect(() => {
    const revision = open
    shown = null
    if (revision)
      call('vault/show', { id, revision })
        .then((entry) => open === revision && (shown = entry))
        .catch((e) => toast(() => errorText(e)))
  })

  async function revert(revision: string) {
    busy = true
    try {
      await call('vault/revert', { id, revision })
      toast(() => t('versionRestored'))
      open = null
    } catch (e) {
      toast(() => errorText(e))
    } finally {
      busy = false
    }
  }
</script>

<section class="mt-4 rounded-xl border border-line bg-surface shadow-xs" data-testid="vault-history">
  <header class="flex items-center gap-2 border-b border-line px-4 py-2">
    <Icon name="history" class="text-faint" />
    <b class="min-w-0 flex-1 text-[12.5px] font-semibold">{t('history')}</b>
    <button class={buttonClass('ghost', 'xs', 'size-6 px-0')} aria-label={t('close')} onclick={onClose}><Icon name="close" /></button>
  </header>
  <ol class="max-h-72 divide-y divide-line overflow-y-auto">
    {#each versions as version (version.revision)}
      <li>
        <button
          class={cn('flex w-full items-center gap-2 px-4 py-2 text-start text-[12.5px] hover:bg-wash', open === version.revision && 'bg-pale')}
          onclick={() => (open = open === version.revision ? null : version.revision)}
          data-testid="vault-version"
        >
          <span class="min-w-0 flex-1">
            <span class="block text-ink">{when(version.at)}</span>
            <span class="block text-[11.5px] text-faint">
              {version.device === device ? t('thisDevice') : `${t('device')} ${version.device.slice(0, 8)}`}
            </span>
          </span>
          {#if version.current}<span class="rounded bg-pale px-1.5 py-px text-[11px] text-forest">{t('currentVersion')}</span>{/if}
          {#if version.deleted}<span class="rounded bg-wash px-1.5 py-px text-[11px] text-muted">{t('deletedVersion')}</span>{/if}
          {#if version.conflict}<span class="rounded bg-warn-pale px-1.5 py-px text-[11px] text-warn">{t('conflict')}</span>{/if}
          {#if !version.synced}<span class="size-1.5 rounded-full bg-warn" title={t('notSynced')}></span>{/if}
        </button>
        {#if open === version.revision && shown}
          <div class="grid gap-2 bg-surface-2 px-4 py-3 text-[12.5px]" data-testid="vault-version-detail">
            <b class="font-semibold">{shown.item.title}</b>
            {#each sections(shown.item.fields.filter((f: any) => f.value)) as section (section.title)}
              {#if section.title}<div class="text-[11.5px] font-medium text-faint uppercase">{section.title}</div>{/if}
              {#each section.fields as field (field.id)}
                <div class="grid gap-x-3 @min-[440px]:grid-cols-[112px_1fr]">
                  <span class="text-faint">{fieldName(field, t)}</span>
                  {#if isHidden(field)}
                    {#if field.type === 'otp'}
                      <span class="text-faint">••••••</span>
                    {:else}
                      <SecretField {id} field={field.id} revision={version.revision} {unlocked} copyable={false} />
                    {/if}
                  {:else}
                    <span class="break-all whitespace-pre-wrap">{field.value}</span>
                  {/if}
                </div>
              {/each}
            {/each}
            {#if shown.item.notes}<p class="whitespace-pre-wrap text-muted">{shown.item.notes}</p>{/if}
            {#if editable && unlocked && !version.current && !version.deleted}
              <button class={buttonClass('secondary', 'sm', 'justify-self-start')} disabled={busy} onclick={() => revert(version.revision)} data-testid="vault-revert">
                <Icon name="restore" />{t('restoreVersion')}
              </button>
            {/if}
          </div>
        {/if}
      </li>
    {/each}
  </ol>
</section>
