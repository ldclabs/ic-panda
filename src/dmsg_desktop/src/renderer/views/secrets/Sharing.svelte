<script lang="ts">
  import Collapsible from '../../components/Collapsible.svelte'
  import TextField from '../../components/TextField.svelte'
  import Dialog from '../../components/Dialog.svelte'
  import { powers } from '../channels/channels'
  import { snapshotFieldName } from '../vault/vault'
  import Progress from '../../components/Progress.svelte'
  import { onDestroy } from 'svelte'
  import Panel from './Panel.svelte'
  import SourcePicker from './SourcePicker.svelte'
  import Select from '../../components/Select.svelte'
  import SearchSelect from '../../components/SearchSelect.svelte'
  import Segmented from '../../components/Segmented.svelte'
  import Checkbox from '../../components/Checkbox.svelte'
  import SecretInput from '../../components/SecretInput.svelte'
  import Field from '../../components/page/Field.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import { panel, InputError } from './workflow-state.svelte'
  import {
    sourceItems,
    card,
    toggle,
    DraftVersion,
    type SourceItem
  } from './model'
  import { openVault } from './navigation.svelte'
  import { call, when } from '../../rpc'
  import { copyShareLink } from '../../components/clipboard'
  import { buttonClass } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { workflowText as w } from './workflow-text'
  import { ux, term } from './experience-text'
  import type { NearbyVaultSelection } from '../nearby'
  let { sourceId, showLinks, account, standalone = false, initialMode = 'link', onNearbySend }: { account: string; sourceId: string; showLinks: (id: string) => void; standalone?: boolean; initialMode?: 'link' | 'channel' | 'nearby'; onNearbySend?: (selection: NearbyVaultSelection, entryId: string) => void } = $props()
  let shares = $state<any[]>([]),
    items = $state<SourceItem[]>([]),
    channels = $state<any[]>([]),
    itemId = $state(''),
    fields = $state<string[]>([]),
    attachments = $state<string[]>([]),
    notes = $state(false),
    mode = $state<'link' | 'channel' | 'nearby'>('link'),
    modeInitialized = false,
    channelId = $state(''),
    hours = $state(24),
    sessions = $state('1'),
    password = $state(''),
    details = $state<Record<string, any>>({}),
    revokeTarget = $state<any>(null)
  $effect(() => {
    if (!modeInitialized) {
      mode = standalone ? 'nearby' : initialMode
      modeInitialized = true
    } else if (standalone && mode !== 'nearby') {
      mode = 'nearby'
    }
  })
  let prepared: { id: string; shape: string; at: number } | null = null
  let selectedRevision: string | undefined
  let attachmentsTouched = false
  const chosen = $derived(items.find((i) => i.id === itemId)),
    shareFields = $derived(chosen?.fields.filter((f) => f.populated) ?? []),
    draft = new DraftVersion()
  const hourLabel = $derived(new Intl.NumberFormat(locale(), { style: 'unit', unit: 'hour', unitDisplay: 'long' }).format(1))
  const canReview = $derived(chosen && !chosen.blocked && (fields.length || notes || attachments.length) &&
    (mode === 'nearby' ? Boolean(onNearbySend) : mode === 'channel' ? !!channelId : /^(?:[1-9]\d?|100)$/.test(sessions)))
  const model = panel(
    async () =>
      Promise.all([
        !standalone &&
          model.request(
            'shares',
            'secrets/shares/list',
            {},
            (r: any) => (shares = r.shares.filter((s: any) => !sourceId || s.plan.item_id === sourceId))
          ),
        model.request(
          'vault',
          'vault/list',
          {},
          (r: any[]) => {
            items = sourceItems(r)
            if (sourceId) itemId = sourceId
            if (selectedRevision !== chosen?.revision) void select()
          }
        ),
        !standalone && mode === 'channel' && loadChannels()
      ]),
    () =>
      shares.some((s) =>
        ['encrypting', 'uploading', 'publishing'].includes(s.state)
      )
  )
  function loadChannels() {
    return model.request('channels', 'channel/list', {}, (rows: any[]) => {
      channels = rows.filter((c) => !c.left && powers(c.ledger, account).writes)
      if (channelId && !channels.some((c) => c.id === channelId)) { channelId = ''; changed() }
    })
  }
  function changed() {
    draft.change()
    prepared = null
  }
  function shape() {
    return JSON.stringify([
      itemId,
      chosen?.revision,
      fields,
      notes,
      attachments.map((id) => [id, details[id]?.file?.current]),
      mode,
      channelId,
      hours,
      sessions
    ])
  }
  async function select() {
    changed()
    attachments = []
    attachmentsTouched = false
    details = {}
    const selected = chosen
    selectedRevision = selected?.revision
    fields = selected ? selected.fields.filter((f) => f.populated).map((f) => f.id) : []
    notes = Boolean(selected?.hasNotes)
    if (!selected) return
    await Promise.all(
      selected.files.map((id) =>
        model.read(
          `attachment-${id}`,
          () => call('files/show', { id }),
          (r: any) => {
            if (chosen?.id === selected.id && chosen.revision === selected.revision) {
              details = { ...details, [id]: r }
              const file = r?.file
              if (
                !attachmentsTouched &&
                file?.current &&
                file.local &&
                file.trashed_at == null &&
                !attachments.includes(id)
              ) {
                attachments = [...attachments, id]
              }
            }
          }
        )
      )
    )
  }
  async function prepare(event: SubmitEvent) {
    event.preventDefault()
    const source = chosen
    if (!source || !canReview) return
    if (mode === 'nearby') {
      onNearbySend?.(
        {
          item_id: source.id,
          revision: source.revision,
          field_ids: shareFields.filter((f) => fields.includes(f.id)).map((f) => f.id),
          include_notes: notes,
          attachments: attachments.map((id) => ({ file_id: id, version_id: details[id].file.current }))
        },
        source.id
      )
      return
    }
    const at = draft.capture(), saved = shape()
    const current = () => model.state.alive && draft.current(at) && shape() === saved && !chosen?.blocked
    await model.act(async () => {
      if (!prepared || prepared.at !== at || prepared.shape !== saved) {
        const result = await call<{ id: string }>(
          mode === 'channel' ? 'secrets/shares/channel_prepare' : 'secrets/shares/prepare',
          {
            ...(mode === 'channel' ? { channel_id: channelId } : {}),
            input: {
              item_id: source.id, revision: source.revision,
              field_ids: shareFields.filter((f) => fields.includes(f.id)).map((f) => f.id),
              include_notes: notes,
              attachments: attachments.map((id) => ({ file_id: id, version_id: details[id].file.current })),
              ...(mode === 'link' ? { expires_in_ms: hours * 3600000, max_download_sessions: Number(sessions) } : {})
            },
            ...(mode === 'link' && password ? { password } : {})
          }
        )
        if (!current()) return
        prepared = { id: result.id, at, shape: saved }
      }
      await call<{ id: string; state: string }>('secrets/shares/publish', { id: prepared.id })
      if (current()) {
        password = ''
        changed()
      }
    })
  }
  onDestroy(() => {
    draft.change()
    password = ''
    prepared = null
  })
</script>

<Panel {model} title={sourceId && chosen ? chosen.title : w('sharing')}>
  {#snippet actions()}
    {#if sourceId}<button class={buttonClass('ghost')} onclick={() => openVault(sourceId)}>{t('back')}</button>{/if}
    {#if !standalone}<button class={buttonClass('secondary')} onclick={() => showLinks(itemId)} data-testid="snapshot-links">{t('shareLinks')}</button>{/if}
  {/snippet}
  {#if !standalone}
    <Segmented
      bind:value={
        () => mode,
        (next) => {
          mode = next as 'link' | 'channel' | 'nearby'
          if (next === 'channel') void loadChannels()
          changed()
          password = ''
        }
      }
      options={[
        { value: 'link', label: ux('linkMode') },
        { value: 'channel', label: ux('channelMode') },
        { value: 'nearby', label: t('nearbyShare') }
      ]}
      label={ux('destination')}
      disabled={model.state.busy}
      testid="snapshot-mode"
    />
  {/if}
  <Notice tone={mode === 'nearby' ? 'info' : 'warn'}
    >{mode === 'nearby' ? t('nearbySendVaultHint') : mode === 'channel' ? w('channelWarning') : w('shareWarning')}</Notice
  >
  <form class={card} onsubmit={prepare} oninput={changed} onchange={changed}>
    {#if !sourceId}<SourcePicker
      {items}
      bind:itemId
      fields={false}
      sharing
      disabled={model.state.busy}
      onChange={select}
      testid="snapshot-source"
    />{:else if model.state.loaded && !chosen}<Notice tone="warn">{ux('noSources')}</Notice>{/if}
    {#if chosen}<fieldset class="space-y-2" disabled={model.state.busy || chosen.blocked}>
        <legend class="mb-2 text-callout font-medium text-muted"
          >{t('content')}</legend
        >{#each shareFields as field}<div><Checkbox
            checked={fields.includes(field.id)}
            onCheckedChange={(checked) => {
              changed()
              fields = toggle(fields, field.id, checked)
            }}>{snapshotFieldName(field, t)}</Checkbox></div>{/each}
        {#if chosen.hasNotes}<div>
          <Checkbox bind:checked={notes} onCheckedChange={changed}
            >{w('notes')}</Checkbox
          >
        </div>{/if}
        {#if !shareFields.length && !chosen.hasNotes && !chosen.files.length}<p class="text-body text-muted">{t('noItems')}</p>{/if}
        {#each chosen.files as id}
          {@const file = details[id]?.file}
          {@const available = file?.current && file.local && file.trashed_at == null}
          <div>
            <Checkbox
              checked={attachments.includes(id)}
              onCheckedChange={(checked) => {
                attachmentsTouched = true
                changed()
                attachments = toggle(attachments, id, checked)
              }}
              disabled={!available}
              >{file?.name ?? ux('file')}</Checkbox
            >{#if !available}<p class="text-callout text-warn">
                {file?.trashed_at ? t('fileMissing') : file ? ux('notLocal') : ux('unknown')}
              </p>{/if}
          </div>{/each}
      </fieldset>{/if}
    {#if mode === 'channel'}<Field label={t('channels')}
        ><SearchSelect
          bind:value={channelId}
          disabled={model.state.busy}
          options={channels.map((c) => ({
            value: c.id,
            label: c.name,
            hint: c.id.slice(0, 10)
          }))}
          label={t('channels')}
          placeholder={w('choose')}
          onChange={changed}
          testid="snapshot-channel"
        /></Field
      >
    {:else if mode === 'link'}<Collapsible testid="snapshot-limits">
        {#snippet title()}{t('linkDays')} · {hours === 24 ? t('days1') : hours === 168 ? t('days7') : hourLabel} · {w('sessions')} {sessions}{/snippet}
        <div class="grid gap-3 py-2 sm:grid-cols-2">
          <Field label={t('linkDays')}><Select
            value={String(hours)} onChange={(v) => { hours = Number(v); changed() }}
            options={[{ value: '1', label: hourLabel }, { value: '24', label: t('days1') }, { value: '168', label: t('days7') }]}
            disabled={model.state.busy} label={t('linkDays')} class="w-full" /></Field>
          <TextField label={w('sessions')} bind:value={sessions} hint="1–100" inputmode="numeric" pattern="[1-9][0-9]?|100" maxlength={3} required disabled={model.state.busy} />
          <SecretInput label={w('password')} bind:value={password} disabled={model.state.busy} />
        </div>
      </Collapsible>
    {/if}
    {#if chosen?.fields.some((f) => fields.includes(f.id) && f.type === 'otp')}<Notice
        tone="warn">{w('otpSeedWarning')}</Notice
      >{/if}{#if chosen?.type === 'ssh' && chosen.fields.some((f) => fields.includes(f.id) && f.type === 'concealed')}<Notice
        tone="warn">{w('privateKeyWarning')}</Notice
      >{/if}{#if chosen?.type === 'wallet' && (notes || chosen.fields.some((f) => fields.includes(f.id) && f.type === 'concealed'))}<Notice
        tone="warn">{w('walletSeedWarning')}</Notice
      >{/if}
    <button
      class={buttonClass('primary')}
      disabled={model.state.busy || !canReview}
      data-testid="snapshot-review">{mode === 'nearby' ? t('nearbyShare') : mode === 'channel' ? w('channelDelivery') : t('createLink')}</button>
    {#if !chosen}<button
        type="button"
        class={buttonClass('ghost')}
        onclick={() => openVault()}>{ux('openVault')}</button
      >{/if}
  </form>
  {#each shares as share}<article class={card}>
      <div class="flex flex-wrap justify-between gap-2">
        <h3 class="font-semibold">{share.plan.title}</h3>
        <span class="text-body">{term(share.state)}</span>
      </div>
      <p class="text-callout text-muted">
        {share.plan.channel?.name ?? when(share.plan.expires_at)}
      </p>
      {#if share.plan.channel}<p class="text-callout text-muted">
          {w('channelWarning')}
        </p>{/if}{#if share.total_chunks || ['uploading', 'publishing'].includes(share.state)}<Progress value={share.uploaded_chunks} max={share.total_chunks} label={term(share.state)} testid="share-upload-progress" />{/if}{#if share.error}<Notice tone="warn"
          >{ux('unknown')}</Notice
        >{/if}
      <div class="flex flex-wrap gap-2">
        {#if ['prepared', 'failed', 'needs_approval', 'awaiting_user'].includes(share.state)}<button
            class={buttonClass('secondary')}
            disabled={model.state.busy}
            onclick={() =>
              model.act(() => call('secrets/shares/publish', { id: share.id }))}
            >{share.state === 'failed'
              ? t('retry')
              : share.plan.channel
                ? w('channelDelivery')
                : t('createLink')}</button
          >{/if}{#if share.state === 'published' && !share.plan.channel}<button
            class={buttonClass('primary')}
            disabled={model.state.busy || share.cancel_requested}
            onclick={() =>
              model.act(
                async () => {
                  const r: any = await call('secrets/shares/get', {
                    id: share.id,
                    include_link: true
                  })
                  if (!model.state.alive) return
                  if (!r.link?.url) throw new InputError(w('linkUnavailable'))
                  await copyShareLink(r.link.url)
                },
                undefined,
                false
              )}>{w('copy')}</button
          >{/if}{#if ['unknown', 'revoke_pending', 'published'].includes(share.state)}<button
            class={buttonClass('secondary')}
            disabled={model.state.busy}
            onclick={() =>
              model.act(() =>
                call('secrets/shares/reconcile', { id: share.id })
              )}>{w('reconcile')}</button
          >{/if}<button
          class={buttonClass('danger')}
          disabled={model.state.busy ||
            ['cancelled', 'revoked'].includes(share.state) ||
            (!!share.plan.channel &&
              ['publishing', 'unknown', 'published'].includes(share.state))}
          onclick={() => (revokeTarget = share)}
          >{share.plan.channel ? t('cancel') : w('revokeShare')}</button
        >
      </div>
    </article>{/each}
</Panel>

<Dialog bind:open={() => !!revokeTarget, (next) => !next && (revokeTarget = null)} title={revokeTarget?.plan.channel ? t('cancel') : w('revokeShare')} tone="danger">
  <p class="break-words">{revokeTarget?.plan.title}</p>
  {#if !revokeTarget?.plan.channel}<p class="mt-2 text-body text-muted">{t('linkRevokeHint')}</p>{/if}
  {#if model.state.errors.action}<Notice tone="danger">{model.state.errors.action}</Notice>{/if}
  {#snippet footer()}
    <button class={buttonClass('ghost')} onclick={() => (revokeTarget = null)}>{t('back')}</button>
    <button class={buttonClass('danger')} disabled={model.state.busy} onclick={() => model.act(
      () => call('secrets/shares/revoke', { id: revokeTarget.id }), () => (revokeTarget = null)
    )}>{revokeTarget?.plan.channel ? t('cancel') : w('revokeShare')}</button>
  {/snippet}
</Dialog>
