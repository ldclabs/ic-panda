<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Dialog from '../../components/Dialog.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import { toast } from '../../components/toast.svelte'
  import { iconButtonClass, buttonClass, cn } from '../../components/ui'
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
  let loading = $state(true)
  let listError = $state('')
  let detailError = $state('')
  let listRetry = $state(0)
  let detailRetry = $state(0)
  let review = $state<{ revision: string; head: string; title: string; at: number } | null>(null)
  let restoreError = $state('')

  $effect(() => {
    void head
    void listRetry
    let active = true
    listError = ''
    loading = true
    call<any[]>('vault/history', { id })
      .then(list => { if (active) versions = list })
      .catch(e => { if (active) listError = errorText(e) })
      .finally(() => { if (active) loading = false })
    return () => { active = false }
  })

  $effect(() => {
    const revision = open
    void detailRetry
    let active = true
    shown = null
    detailError = ''
    if (revision)
      call('vault/show', { id, revision })
        .then(entry => { if (active) shown = entry })
        .catch(e => { if (active) detailError = errorText(e) })
    return () => { active = false }
  })

  $effect(() => { if (review && review.head !== head) review = null })

  async function revert() {
    if (busy || !review || review.head !== head || !unlocked || !editable) return
    const { revision } = review
    restoreError = ''
    busy = true
    try {
      await call('vault/revert', { id, revision })
      toast(() => t('versionRestored'))
      review = null
      open = null
    } catch (e) {
      restoreError = errorText(e)
    } finally {
      busy = false
    }
  }
</script>

<section class="mt-4 rounded-xl border border-line bg-surface" data-testid="vault-history">
  <header class="flex items-center gap-2 border-b border-line px-4 py-2">
    <Icon name="history" class="text-faint" />
    <b class="min-w-0 flex-1 text-[12.5px] font-semibold">{t('history')}</b>
    <button class={iconButtonClass('ghost', 'lg')} aria-label={t('close')} onclick={onClose}><Icon name="close" /></button>
  </header>
  {#if listError}
    <Notice tone="warn" class="m-2" testid="vault-history-error">{listError}
      <button class={buttonClass('secondary', 'xs')} onclick={() => listRetry++}>{t('retry')}</button>
    </Notice>
  {:else if loading && !versions.length}
    <p class="p-3 text-sm text-muted">{t('loading')}</p>
  {:else if !versions.length}
    <p class="p-3 text-sm text-muted">{t('noItems')}</p>
  {/if}
  <ol class="max-h-72 divide-y divide-line overflow-y-auto">
    {#each versions as version (version.revision)}
      <li>
        <button
          class={cn('flex w-full items-center gap-2 px-4 py-2 text-start text-[12.5px] hover:bg-wash', open === version.revision && 'bg-pale')}
          onclick={() => (open = open === version.revision ? null : version.revision)}
          aria-expanded={open === version.revision}
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
          {#if !version.synced && version.save_state !== 'local_only'}<span class="size-1.5 rounded-full bg-warn" title={t('notSynced')}></span>{/if}
        </button>
        {#if open === version.revision && !shown}
          {#if detailError}
            <Notice tone="warn" class="m-2" testid="vault-version-error">{detailError}
              <button class={buttonClass('secondary', 'xs')} onclick={() => detailRetry++}>{t('retry')}</button>
            </Notice>
          {:else}<p class="px-4 py-2 text-sm text-muted">{t('loading')}</p>{/if}
        {:else if open === version.revision && shown}
          <div class="grid gap-2 bg-surface-2 px-4 py-3 text-[12.5px]" data-testid="vault-version-detail">
            <b class="font-semibold">{shown.item.title}</b>
            {#each sections(shown.item.fields.filter((f: any) => f.value)) as section (section.title)}
              {#if section.title}<div class="text-[11.5px] font-medium text-faint [overflow-wrap:anywhere]">{section.title}</div>{/if}
              {#each section.fields as field (field.id)}
                <div class="grid gap-x-3 @min-[440px]:grid-cols-[112px_1fr]">
                  <span class="text-faint">{fieldName(field, t)}</span>
                  {#if isHidden(field)}
                    {#if field.type === 'otp'}
                      <span class="text-faint">••••••</span>
                    {:else}
                      <SecretField {id} field={field.id} name={fieldName(field, t)} context={shown.item.title} revision={version.revision} {unlocked} copyable={false} />
                    {/if}
                  {:else}
                    <span class="break-all whitespace-pre-wrap">{field.value}</span>
                  {/if}
                </div>
              {/each}
            {/each}
            {#if shown.item.notes}<p class="whitespace-pre-wrap text-muted">{shown.item.notes}</p>{/if}
            {#if editable && unlocked && !version.current && !version.deleted}
              <button class={buttonClass('secondary', 'sm', 'justify-self-start')} disabled={busy} onclick={() => {
                restoreError = ''; review = { revision: version.revision, head, title: shown.item.title, at: version.at }
              }} data-testid="vault-revert">
                <Icon name="restore" />{t('restoreVersion')}
              </button>
            {/if}
          </div>
        {/if}
      </li>
    {/each}
  </ol>
</section>

<Dialog bind:open={() => !!review, value => { if (!value && !busy) review = null }} title={t('vaultRestoreReview')} icon="restore" dismissible={!busy} testid="vault-revert-dialog">
  {#if review}<p class="font-medium">{review.title}</p><p class="text-sm text-muted">{when(review.at)}</p>{/if}
  {#if restoreError}<p class="mt-2 text-sm text-danger" role="alert">{restoreError}</p>{/if}
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} disabled={busy} onclick={() => review = null}>{t('cancel')}</button>
    <button class={buttonClass('primary')} disabled={busy} onclick={revert} data-testid="vault-revert-confirm">{t('restoreVersion')}</button>
  {/snippet}
</Dialog>
