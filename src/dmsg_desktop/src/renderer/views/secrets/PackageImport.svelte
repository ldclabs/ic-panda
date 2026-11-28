<script lang="ts">
  import { snapshotFieldName } from '../vault/vault'
  import FolderPicker from '../files/FolderPicker.svelte'
  import Checkbox from '../../components/Checkbox.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import { buttonClass } from '../../components/ui'
  import { call, size } from '../../rpc'
  import { t } from '../../i18n/index.svelte'
  import { workflowText as w } from './workflow-text'
  import { ux } from './experience-text'
  import { panel } from './workflow-state.svelte'
  import { toggle, card } from './model'
  import { openVault, openFile } from './navigation.svelte'
  let { received, onNavigate }: { received: any; onNavigate?: () => void } = $props()
  let fields = $state<string[]>([]),
    attachments = $state<number[]>([]),
    notes = $state(false),
    folder = $state('root'),
    folders = $state<any[]>([]),
    picker = $state(false),
    ids = $state<string[]>([])
  let initializedPackage = ''
  $effect(() => {
    const id = received?.package_id ?? ''
    if (id && id !== initializedPackage) {
      initializedPackage = id
      fields = (received.preview?.fields ?? []).map((f: any) => f.id)
      attachments = (received.preview?.attachments ?? []).map((_: any, i: number) => i)
      notes = Boolean(received.preview?.notes)
      ids = received.already_imported ?? []
    } else if (received?.already_imported?.length) {
      ids = received.already_imported
    }
  })
  const model = panel(async () => {})
  async function submit() {
    await model.act(
      () =>
        call('secrets/packages/import', {
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
        if (r?.state === 'done') ids = r.ids ?? []
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
  {#if ids.length}<Notice tone="ok"
      >{ux('importDone')}</Notice
    >
    <div class="flex flex-wrap gap-2">
      {#each ids as id}<button
          class={buttonClass('primary')}
          onclick={() => { onNavigate?.(); openVault(id) }}>{ux('openVault')}</button
        >{/each}<button
        class={buttonClass('secondary')}
        onclick={() => { onNavigate?.(); openFile() }}>{ux('openFiles')}</button
      >
    </div>
  {:else}<fieldset
      class="space-y-2"
      disabled={model.state.busy}
    >
      <legend class="mb-2 text-callout text-muted">{ux('selected')}</legend
      >{#each received.preview.fields as field}<div>
          <Checkbox
            checked={fields.includes(field.id)}
            onCheckedChange={(checked) =>
              (fields = toggle(fields, field.id, checked))}
            >{snapshotFieldName(field, t)}</Checkbox
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
      <span class="text-callout"
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
    <button
      class={buttonClass('primary')}
      disabled={model.state.busy ||
        (!fields.length && !notes && !attachments.length)}
      onclick={submit}
      data-testid="package-import-submit">{w('secret_package_import')}</button
    >
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
