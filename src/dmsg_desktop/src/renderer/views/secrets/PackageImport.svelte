<script lang="ts">
  import FolderPicker from '../files/FolderPicker.svelte'
  import Checkbox from '../../components/Checkbox.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import { buttonClass } from '../../components/ui'
  import { call, size } from '../../rpc'
  import { stateLabel, t } from '../../i18n/index.svelte'
  import { workflowText as w } from './workflow-text'
  import { ux, term } from './experience-text'
  import { panel } from './workflow-state.svelte'
  import { toggle, card } from './model'
  import { openVault, openFile } from './navigation.svelte'
  let { received, open }: { received: any; open: (id: string) => void } =
    $props()
  let fields = $state<string[]>([]),
    attachments = $state<number[]>([]),
    notes = $state(false),
    folder = $state('root'),
    folders = $state<any[]>([]),
    picker = $state(false),
    request = $state<any>(null),
    ids = $state<string[]>([])
  $effect(() => {
    if (received.already_imported) ids = received.already_imported
  })
  const model = panel(
    async () => {
      if (request?.id)
        await model.request(
          'import',
          'approval/show',
          { id: request.id },
          (r: any) => {
            request = r
            if (r.state === 'done')
              ids = r.delivery_report?.ids ?? r.result?.ids ?? ids
          }
        )
    },
    () => request?.id && ['executing', 'unknown'].includes(request.state)
  )
  async function submit() {
    await model.act(
      () =>
        call('secrets/packages/import_request', {
          selection: {
            package_id: received.package_id,
            digest: received.digest,
            fields,
            include_notes: notes,
            attachments,
            folder
          }
        }),
      (r: any) => {
        if (r.id) {
          request = r
          open(r.id)
        } else if (r.state === 'done') ids = r.ids
      },
      false
    )
  }
</script>

<div class={card} data-testid="channel-snapshot-import">
  <Notice>{w('localFiles')} {w('filesPermissions')}</Notice>
  {#each Object.values(model.state.errors) as error}<Notice tone="danger"
      >{error}</Notice
    >{/each}
  <h3 class="font-semibold">{received.preview.title}</h3>
  {#if ids.length || request?.state === 'done'}<Notice tone="ok"
      >{ux('importDone')}</Notice
    >
    <div class="flex flex-wrap gap-2">
      {#each ids as id}<button
          class={buttonClass('primary')}
          onclick={() => openVault(id)}>{ux('openVault')}</button
        >{/each}<button
        class={buttonClass('secondary')}
        onclick={() => openFile()}>{ux('openFiles')}</button
      >
    </div>
  {:else}<fieldset
      class="space-y-2"
      disabled={model.state.busy ||
        (!!request &&
          ['awaiting_user', 'executing', 'unknown'].includes(request.state))}
    >
      <legend class="mb-2 text-xs text-muted">{ux('selected')}</legend
      >{#each received.preview.fields as field}<div>
          <Checkbox
            checked={fields.includes(field.id)}
            onCheckedChange={(checked) =>
              (fields = toggle(fields, field.id, checked))}
            >{field.label || field.id} · {term(field.type)}</Checkbox
          >
        </div>{/each}{#if received.preview.notes}<Checkbox bind:checked={notes}
          >{w('notes')}</Checkbox
        >{/if}{#each received.preview.attachments as attachment, i}<div>
          <Checkbox
            checked={attachments.includes(i)}
            onCheckedChange={(checked) =>
              (attachments = toggle(attachments, i, checked))}
            >{attachment.name} · {size(attachment.size)}</Checkbox
          >
        </div>{/each}
    </fieldset>
    <div class="flex flex-wrap items-center gap-2">
      <span class="text-xs"
        >{w('destination')}: {folder === 'root'
          ? t('library')
          : (folders.find((f) => f.id === folder)?.name ?? folder)}</span
      ><button
        class={buttonClass('secondary')}
        disabled={model.state.busy}
        onclick={() =>
          model.act(
            () => call('files/tree'),
            (tree: any) => {
              folders = tree
              picker = true
            },
            false
          )}>{w('choose')}</button
      >
    </div>
    {#if request}<Notice>{stateLabel(request.state ?? 'awaiting_user')}</Notice
      ><button class={buttonClass('secondary')} onclick={() => open(request.id)}
        >{w('review')}</button
      >{#if ['rejected', 'cancelled', 'failed', 'expired'].includes(request.state)}<button
          class={buttonClass('primary')}
          onclick={() => (request = null)}>{ux('retry')}</button
        >{/if}{:else}<button
        class={buttonClass('primary')}
        disabled={model.state.busy ||
          (!fields.length && !notes && !attachments.length)}
        onclick={submit}>{w('import')}</button
      >{/if}
  {/if}
</div>
<FolderPicker
  bind:open={picker}
  {folders}
  title={w('destination')}
  description={w('filesPermissions')}
  action={w('choose')}
  onPick={(id) => (folder = id)}
/>
