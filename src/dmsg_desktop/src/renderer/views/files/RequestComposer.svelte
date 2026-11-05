<script lang="ts">
  import { onDestroy } from 'svelte'
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import Select from '../../components/Select.svelte'
  import TextArea from '../../components/TextArea.svelte'
  import TextField from '../../components/TextField.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import { buttonClass } from '../../components/ui'
  import { copyText } from '../../components/clipboard'
  import { t } from '../../i18n/index.svelte'
  import { call, errorText } from '../../rpc'
  import { requestLabels, requestLimit, requestTotals, MAX_REQUEST_LABELS } from './requests'

  /** A link, or one for each name of a list; a failure keeps the names still to do. */
  let {
    open = $bindable(false),
    usage,
    onCreated
  }: {
    open?: boolean
    usage: any | null
    onCreated: () => void
  } = $props()

  const MIB = 1024 * 1024
  let title = $state('')
  let instructions = $state('')
  let names = $state('')
  let days = $state('14')
  let total = $state('')
  let working = $state(false)
  let progress = $state({ done: 0, total: 0 })
  let error = $state('')
  let created = $state<any[]>([])
  let alive = true
  onDestroy(() => { alive = false })

  const labels = $derived(requestLabels(names))
  const tooMany = $derived(labels.length > MAX_REQUEST_LABELS)
  const totals = $derived(requestTotals(usage))
  const dayOptions = $derived(
    [7, 14, 30, 90].map((n) => ({ value: String(n), label: t('requestDays', { n }) }))
  )
  const totalOptions = $derived(
    totals.options.map((mib) => ({ value: String(mib), label: requestLimit(mib * MIB) }))
  )
  const allLinks = $derived(
    created.map((link) => (link.label ? `${link.label},${link.url}` : link.url)).join('\n')
  )

  $effect(() => {
    if (open && !total) total = String(totals.preferred)
  })

  function start() {
    title = ''
    instructions = ''
    names = ''
    days = '14'
    total = String(totals.preferred)
    error = ''
    created = []
  }

  $effect(() => {
    if (!open && !working && created.length) start()
  })

  async function create() {
    if (working || !title.trim() || tooMany) return
    working = true
    error = ''
    const pending = labels.length ? [...labels] : ['']
    progress = { done: 0, total: pending.length }
    try {
      while (pending.length) {
        const link = await call<any>('files/request_create', {
          title: title.trim(),
          instructions: instructions.trim(),
          label: pending[0],
          expires_in_ms: Number(days) * 86_400_000,
          max_total_bytes: Number(total) * MIB
        })
        if (!alive) return
        created = [...created, link]
        pending.shift()
        progress.done++
      }
      names = ''
      onCreated()
    } catch (e) {
      if (!alive) return
      error = errorText(e)
      names = pending.join('\n')
      if (created.length) onCreated()
    } finally {
      working = false
    }
  }
</script>

<Dialog bind:open title={t('requestNew')} icon="inbox" wide dismissible={!working} testid="request-composer">
  {#if created.length && !error}
    <Notice tone="ok" testid="request-created">{t('requestCreated')}</Notice>
  {/if}
  {#if error}
    <Notice failure={error} tone="danger" testid="request-partial">
      {created.length ? t('requestPartial') : error}
    </Notice>
  {/if}
  {#if created.length}
    <ul class="mt-3 divide-y divide-line" data-testid="request-created-list">
      {#each created as link (link.request_id)}
        <li class="flex min-w-0 items-center gap-3 py-2">
          <p class="min-w-0 flex-1 break-words text-[13px]">
            <bdi dir="auto">{link.label || link.title}</bdi>
          </p>
          <button class={buttonClass('ghost', 'xs')} onclick={() => copyText(link.url)} data-testid="request-copy"
            >{t('linkCopy')}</button
          >
        </li>
      {/each}
    </ul>
    {#if created.length > 1}
      <button
        class={buttonClass('secondary', 'sm', 'mt-2')}
        onclick={() => copyText(allLinks)}
        data-testid="request-copy-all"><Icon name="link" />{t('requestCopyAll')}</button
      >
    {/if}
  {/if}
  {#if !created.length || error}
    <div class="mt-1 grid gap-3">
      <TextField
        label={t('requestTitle')}
        bind:value={title}
        maxlength={160}
        disabled={working}
        dir="auto"
        data-testid="request-title"
      />
      <TextArea
        label={t('requestInstructions')}
        hint={t('requestInstructionsHint')}
        bind:value={instructions}
        maxlength={1300}
        rows={3}
        disabled={working}
        dir="auto"
        data-testid="request-instructions"
      />
      <TextArea
        label={t('requestNames')}
        hint={t('requestNamesHint')}
        error={tooMany ? t('requestTooMany', { n: MAX_REQUEST_LABELS }) : undefined}
        bind:value={names}
        rows={3}
        disabled={working}
        dir="auto"
        data-testid="request-names"
      />
      <div class="grid gap-3 sm:grid-cols-2">
        <Select label={t('requestExpiry')} bind:value={days} options={dayOptions} disabled={working} testid="request-days" />
        <Select label={t('requestTotal')} bind:value={total} options={totalOptions} disabled={working} testid="request-total" />
      </div>
    </div>
  {/if}
  {#snippet footer()}
    <span class="flex-1"></span>
    {#if working}
      <p class="me-auto text-[12px] text-muted" role="status">
        {t('requestCreating', { done: progress.done, total: progress.total })}
      </p>
    {/if}
    {#if created.length && !error}
      <button class={buttonClass('primary')} onclick={() => (open = false)} data-testid="request-done">{t('done')}</button>
    {:else}
      <button class={buttonClass('ghost')} disabled={working} onclick={() => (open = false)}>{t('cancel')}</button>
      <button
        class={buttonClass('primary')}
        disabled={working || !title.trim() || tooMany}
        onclick={create}
        data-testid="request-create"
      >
        {labels.length > 1 ? t('requestCreateMany', { n: labels.length }) : t('requestCreate')}
      </button>
    {/if}
  {/snippet}
</Dialog>
