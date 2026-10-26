<script lang="ts">
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
  import { call, when, size } from '../../rpc'
  import { toast } from '../../components/toast.svelte'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { workflowText as w } from './workflow-text'
  import { ux, term } from './experience-text'
  let { open }: { open: (id: string) => void } = $props()
  let shares = $state<any[]>([]),
    items = $state<SourceItem[]>([]),
    channels = $state<any[]>([]),
    itemId = $state(''),
    fields = $state<string[]>([]),
    attachments = $state<string[]>([]),
    notes = $state(false),
    mode = $state('link'),
    channelId = $state(''),
    hours = $state(24),
    sessions = $state(1),
    password = $state(''),
    details = $state<Record<string, any>>({}),
    preview = $state<any>(null)
  const chosen = $derived(items.find((i) => i.id === itemId)),
    draft = new DraftVersion()
  const model = panel(
    async () =>
      Promise.all([
        model.request(
          'shares',
          'secrets/shares/list',
          {},
          (r: any) => (shares = r.shares)
        ),
        model.request(
          'vault',
          'vault/list',
          {},
          (r: any[]) => (items = sourceItems(r))
        ),
        model.request(
          'channels',
          'channel/list',
          {},
          (r: any[]) => (channels = r)
        )
      ]),
    () =>
      shares.some((s) =>
        ['encrypting', 'uploading', 'publishing'].includes(s.state)
      )
  )
  function changed() {
    draft.change()
    preview = null
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
  const previewCurrent = $derived(
    preview && preview.shape === shape() && draft.current(preview.at)
  )
  async function select() {
    changed()
    fields = []
    attachments = []
    notes = false
    details = {}
    const selected = chosen
    if (!selected) return
    await Promise.all(
      selected.files.map((id) =>
        model.read(
          `attachment-${id}`,
          () => call('files/show', { id }),
          (r: any) => {
            if (itemId === selected.id) details = { ...details, [id]: r }
          }
        )
      )
    )
  }
  async function prepare(event: SubmitEvent) {
    event.preventDefault()
    if (!chosen) return
    const at = draft.capture(),
      saved = shape(),
      secret = password
    await model.act(
      () =>
        call(
          mode === 'channel'
            ? 'secrets/shares/channel_prepare'
            : 'secrets/shares/prepare',
          {
            ...(mode === 'channel' ? { channel_id: channelId } : {}),
            input: {
              item_id: chosen!.id,
              revision: chosen!.revision,
              field_ids: fields,
              include_notes: notes,
              attachments: attachments.map((id) => ({
                file_id: id,
                version_id: details[id]?.file?.current
              })),
              ...(mode === 'channel'
                ? {}
                : {
                    expires_in_ms: hours * 3600000,
                    max_download_sessions: sessions
                  })
            },
            ...(mode === 'link' && secret ? { password: secret } : {})
          }
        ),
      (r: any) => {
        if (draft.current(at) && shape() === saved) {
          password = ''
          preview = { ...r, at, shape: saved }
        }
      },
      false
    )
  }
  onDestroy(() => {
    draft.change()
    password = ''
    preview = null
  })
</script>

<Panel {model} title={w('sharing')} description={ux('reviewSelection')}>
  <Segmented
    bind:value={
      () => mode,
      (next) => {
        mode = next
        changed()
        password = ''
      }
    }
    options={[
      { value: 'link', label: ux('linkMode') },
      { value: 'channel', label: ux('channelMode') }
    ]}
    label={ux('destination')}
    testid="snapshot-mode"
  />
  <Notice tone="warn"
    >{mode === 'channel' ? w('channelWarning') : w('shareWarning')}</Notice
  >
  <form class={card} onsubmit={prepare} oninput={changed} onchange={changed}>
    <SourcePicker
      {items}
      bind:itemId
      fields={false}
      sharing
      onChange={select}
      testid="snapshot-source"
    />
    {#if chosen}<fieldset class="space-y-2">
        <legend class="mb-2 text-xs font-medium text-muted"
          >{ux('selected')}</legend
        >{#each chosen.fields as field}<Checkbox
            checked={fields.includes(field.id)}
            onCheckedChange={(checked) => {
              changed()
              fields = toggle(fields, field.id, checked)
            }}>{field.label || field.id} · {term(field.type)}</Checkbox
          >{/each}
        <div>
          <Checkbox bind:checked={notes} onCheckedChange={changed}
            >{w('notes')}</Checkbox
          >
        </div>
        {#each chosen.files as id}<div>
            <Checkbox
              checked={attachments.includes(id)}
              onCheckedChange={(checked) => {
                changed()
                attachments = toggle(attachments, id, checked)
              }}
              disabled={!details[id]?.file?.current}
              >{details[id]?.file?.name ?? ux('file')}</Checkbox
            >{#if !details[id]?.file?.current}<p class="text-xs text-warn">
                {ux('unknown')}
              </p>{/if}
          </div>{/each}
      </fieldset>{/if}
    {#if mode === 'channel'}<Field label={t('channels')}
        ><SearchSelect
          bind:value={channelId}
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
    {:else}<div class="grid gap-3 sm:grid-cols-2">
        <Field label={w('hours')}
          ><Select
            value={String(hours)}
            onChange={(v) => {
              hours = Number(v)
              changed()
            }}
            options={[1, 24, 168].map((h) => ({
              value: String(h),
              label: `${h} ${w('hours')}`
            }))}
            label={w('hours')}
            class="w-full"
          /></Field
        ><Field label={w('sessions')}
          ><input
            type="number"
            min="1"
            max="100"
            step="1"
            required
            bind:value={sessions}
          /></Field
        >
      </div>
      <Field label={w('password')}><SecretInput bind:value={password} /></Field
      >{/if}
    {#if chosen?.fields.some((f) => fields.includes(f.id) && f.type === 'otp')}<Notice
        tone="warn">{w('otpSeedWarning')}</Notice
      >{/if}{#if chosen?.type === 'ssh' && chosen.fields.some((f) => fields.includes(f.id) && f.type === 'concealed')}<Notice
        tone="warn">{w('privateKeyWarning')}</Notice
      >{/if}{#if chosen?.type === 'wallet' && (notes || chosen.fields.some((f) => fields.includes(f.id) && f.type === 'concealed'))}<Notice
        tone="warn">{w('walletSeedWarning')}</Notice
      >{/if}
    <button
      class={buttonClass('primary')}
      disabled={model.state.busy ||
        !chosen ||
        chosen.blocked ||
        (mode === 'channel' && !channelId) ||
        (!fields.length && !notes && !attachments.length)}
      >{w('prepareSnapshot')}</button
    >
    {#if !chosen}<button
        type="button"
        class={buttonClass('ghost')}
        onclick={() => openVault()}>{ux('openVault')}</button
      >{/if}
  </form>
  {#if previewCurrent}<div class={card} data-testid="snapshot-preview">
      <h3 class="font-semibold">{chosen?.title}</h3>
      <p class="text-sm">
        {fields
          .map((id) => chosen?.fields.find((f) => f.id === id)?.label || id)
          .join(', ')}{notes ? ` · ${w('notes')}` : ''}
      </p>
      {#each attachments as id}<p class="text-sm">
          {details[id]?.file?.name}
        </p>{/each}
      <p class="text-xs text-muted">
        {size(preview.plan.plain_bytes)} · {mode === 'channel'
          ? preview.plan.channel?.name
          : when(preview.plan.expires_at)}
      </p>
      <button
        class={buttonClass('primary')}
        disabled={model.state.busy}
        onclick={() => {
          if (previewCurrent)
            void model.act(
              () => call('secrets/shares/request', { id: preview.id }),
              (r: any) => open(r.id)
            )
        }}>{w('review')}</button
      >
    </div>{/if}
  {#each shares as share}<article class={card}>
      <div class="flex flex-wrap justify-between gap-2">
        <h3 class="font-semibold">{share.plan.title}</h3>
        <span class="text-sm">{term(share.state)}</span>
      </div>
      <p class="text-xs text-muted">
        {share.plan.channel?.name ?? when(share.plan.expires_at)}
      </p>
      {#if share.plan.channel}<p class="text-xs text-muted">
          {w('channelWarning')}
        </p>{/if}{#if share.total_chunks}<progress
          class="w-full accent-forest"
          value={share.uploaded_chunks}
          max={share.total_chunks}
          aria-label={term(share.state)}
        ></progress>{/if}{#if share.error}<Notice tone="warn"
          >{ux('unknown')}</Notice
        >{/if}
      <div class="flex flex-wrap gap-2">
        {#if ['prepared', 'needs_approval', 'awaiting_user'].includes(share.state)}<button
            class={buttonClass('secondary')}
            disabled={model.state.busy}
            onclick={() =>
              model.act(
                () => call('secrets/shares/request', { id: share.id }),
                (r: any) => open(r.id)
              )}>{w('review')}</button
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
                  await navigator.clipboard.writeText(r.link.url)
                  toast(() => t('copiedText'))
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
          onclick={() =>
            model.act(() => call('secrets/shares/revoke', { id: share.id }))}
          >{share.plan.channel ? t('cancel') : w('revokeShare')}</button
        >
      </div>
    </article>{/each}
</Panel>
