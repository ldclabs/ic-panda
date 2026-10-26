<script lang="ts">
  import { onDestroy } from 'svelte'
  import Panel from './Panel.svelte'
  import SourcePicker from './SourcePicker.svelte'
  import SearchSelect from '../../components/SearchSelect.svelte'
  import Select from '../../components/Select.svelte'
  import Field from '../../components/page/Field.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Id from '../../components/page/Id.svelte'
  import { panel, InputError } from './workflow-state.svelte'
  import {
    sourceItems,
    card,
    DraftVersion,
    validPath,
    type SourceItem
  } from './model'
  import { call, bridge, size, errorText } from '../../rpc'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { workflowText as w } from './workflow-text'
  import { ux, term } from './experience-text'
  let { open }: { open: (id: string) => void } = $props()
  let mounts = $state<any[]>([]),
    statuses = $state<Record<string, any>>({}),
    supported = $state(false),
    ready = $state(true),
    items = $state<SourceItem[]>([]),
    files = $state<any[]>([]),
    versions = $state<any[]>([])
  let project = $state(''),
    path = $state('.env'),
    sourceType = $state('field_utf8'),
    itemId = $state(''),
    fieldId = $state(''),
    reference = $state(''),
    fileId = $state(''),
    versionId = $state(''),
    refs = $state(''),
    variable = $state('TOKEN'),
    alias = $state(''),
    windowSeconds = $state(30),
    mappings = $state<any[]>([]),
    preview = $state<any>(null)
  const draft = new DraftVersion()
  const model = panel(
    async () =>
      Promise.all([
        model.read(
          'mounts',
          async () => {
            const result: any = await call('secrets/mounts/list')
            if (model.state.alive) {
              mounts = result.mounts
              supported = result.supported
              ready = result.daemon_ready !== false
            }
            await Promise.all(
              result.mounts.map((m: any) =>
                model.request(
                  `mount-${m.id}`,
                  'secrets/mounts/status',
                  { id: m.id },
                  (s: any) => (statuses = { ...statuses, [m.id]: s })
                )
              )
            )
          },
          () => {}
        ),
        model.request(
          'vault',
          'vault/list',
          {},
          (v: any[]) => (items = sourceItems(v))
        ),
        model.request(
          'files',
          'files/search',
          { query: '' },
          (v: any[]) => (files = v)
        )
      ]),
    () => mounts.some((m) => statuses[m.id]?.state !== 'stopped')
  )
  function changed() {
    draft.change()
    preview = null
  }
  onDestroy(() => draft.change())
  async function selectFile() {
    changed()
    versions = []
    versionId = ''
    const selected = fileId
    await model.read(
      'versions',
      () => call('files/show', { id: selected }),
      (r: any) => {
        if (fileId === selected) {
          versions = r.versions
          versionId = r.file.current
        }
      }
    )
  }
  async function add() {
    const at = draft.capture()
    await model.act(
      async () => {
        if (
          !validPath(path) ||
          mappings.some((m) => m.relative_path === path) ||
          (alias && (!validPath(alias) || alias.includes('/')))
        )
          throw new InputError(ux('invalidMapping'))
        let source: any
        if (sourceType === 'library_file') {
          if (
            !fileId ||
            !versionId ||
            versions.find((v) => v.id === versionId)?.local === false
          )
            throw new InputError(ux('invalidMapping'))
          source = { type: sourceType, file_id: fileId, version_id: versionId }
        } else if (sourceType === 'refs_dotenv') {
          const names = new Set<string>()
          const variables = refs
            .split('\n')
            .filter((l) => l.trim() && !l.trim().startsWith('#'))
            .map((l) => {
              const i = l.indexOf('=')
              const name = l.slice(0, i).trim(),
                uri = l.slice(i + 1).trim()
              const m =
                /^dmsg-secret:\/\/([^/]+)\/([a-f0-9]{64})\/([A-Za-z0-9_-]+)$/.exec(
                  uri
                )
              if (
                i < 1 ||
                !m ||
                !/^[A-Z_][A-Z0-9_]{0,127}$/.test(name) ||
                names.has(name)
              )
                throw new InputError(ux('invalidMapping'))
              names.add(name)
              return {
                name,
                reference: {
                  schema_version: 1,
                  account_id: m[1],
                  item_id: m[2],
                  field_id: m[3]
                }
              }
            })
          if (
            !variables.length ||
            variables.length > 64 ||
            new TextEncoder().encode(refs).length > 65536
          )
            throw new InputError(ux('invalidMapping'))
          source = { type: sourceType, variables }
        } else {
          const r =
            reference.trim() ||
            (
              (await call('secrets/ref', {
                item_id: itemId,
                field_id: fieldId
              })) as any
            ).uri
          source = { type: 'field_utf8', reference: r }
        }
        return { relative_path: path, source, ...(alias ? { alias } : {}) }
      },
      (m: any) => {
        if (!draft.current(at)) return
        changed()
        mappings = [...mappings, m]
      },
      false
    )
  }
  async function prepare() {
    const at = draft.capture()
    // Native IPC cannot clone Svelte proxies. Freeze the exact metadata at click time.
    const input = JSON.parse(
      JSON.stringify({
        project_label: project,
        entries: mappings.map(({ alias, ...entry }) => entry),
        aliases: mappings
          .filter((m) => m.alias)
          .map((m) => ({ name: m.alias, entry: m.relative_path }))
      })
    )
    const windowMs = windowSeconds * 1000
    await model.act(
      () => bridge(() => window.dmsg.secretMountPrepare(input, windowMs)),
      (r) => {
        if (draft.current(at)) preview = r
      },
      false
    )
  }
  const settings = () =>
    model.act(
      () => bridge(() => window.dmsg.openExtensionSettings()),
      undefined,
      false
    )
  async function start(id: string) {
    await model.act(async () => {
      try {
        return await call('secrets/mounts/start', { id })
      } catch (e) {
        await model.refresh()
        if (!String(statuses[id]?.error ?? '').startsWith('FSKIT_')) throw e
      }
    })
  }
</script>

<Panel {model} title={w('mounts')} description={ux('mountSteps')}>
  {#if window.dmsg.platform === 'darwin'}<Notice tone="warn"
      >{w('sharedWindow')}</Notice
    >{#if supported && !ready}<Notice tone="warn"
        >{w('FSKIT_DAEMON_UNSIGNED')}</Notice
      >{:else if supported}<div class="flex flex-wrap items-center gap-3">
        <p class="max-w-2xl text-xs text-muted">{w('enableExtension')}</p>
        <button class={buttonClass('secondary')} onclick={settings}
          >{w('openSettings')}</button
        >
      </div>{/if}{/if}
  {#if model.state.loaded && !supported}<Notice>{w('unavailable')}</Notice
    >{:else if supported}<form
      class={card}
      onsubmit={(e) => {
        e.preventDefault()
        void prepare()
      }}
      oninput={changed}
      onchange={changed}
    >
      <h3 class="font-medium">{ux('setup')}</h3>
      <p class="text-xs leading-relaxed text-muted">{w('exclude')}</p>
      <Field label={w('project')}
        ><input bind:value={project} maxlength="100" required /></Field
      >
      <div class="grid gap-3 sm:grid-cols-2">
        <Field label={w('path')}
          ><input bind:value={path} placeholder=".env" /></Field
        ><Field label={w('source')}
          ><Select
            bind:value={sourceType}
            label={w('source')}
            options={['field_utf8', 'library_file', 'refs_dotenv'].map(
              (value, i) => ({
                value,
                label: w(['field', 'library', 'refs'][i])
              })
            )}
            onChange={changed}
            class="w-full"
          /></Field
        >
      </div>
      {#if sourceType === 'library_file'}<p class="text-xs text-muted">
          {w('filesPermissions')}
        </p>
        <div class="grid gap-3 sm:grid-cols-2">
          <Field label={ux('file')}
            ><SearchSelect
              bind:value={fileId}
              options={files.map((f) => ({
                value: f.id,
                label: f.name,
                hint: f.id.slice(0, 10),
                disabled: !!f.deleted
              }))}
              label={ux('file')}
              placeholder={ux('search')}
              empty={ux('noSources')}
              onChange={selectFile}
            /></Field
          ><Field label={ux('version')}
            ><SearchSelect
              bind:value={versionId}
              options={versions.map((v) => ({
                value: v.id,
                label: `${v.id.slice(0, 12)} · ${size(v.size)}`,
                hint: v.local === false ? ux('notLocal') : undefined,
                disabled: v.local === false
              }))}
              label={ux('version')}
              placeholder={ux('search')}
              onChange={changed}
            /></Field
          >
        </div>
      {:else}<SourcePicker
          {items}
          bind:itemId
          bind:fieldId
          onChange={() => {
            reference = ''
            changed()
          }}
        />
        {#if sourceType === 'refs_dotenv'}<div
            class="flex flex-wrap items-end gap-2"
          >
            <div class="min-w-40 flex-1">
              <Field label={ux('variable')}
                ><input bind:value={variable} placeholder="TOKEN" /></Field
              >
            </div>
            <button
              type="button"
              class={buttonClass('secondary')}
              disabled={!fieldId || model.state.busy}
              onclick={() =>
                model.act(
                  () =>
                    call('secrets/ref', { item_id: itemId, field_id: fieldId }),
                  (r: any) => {
                    changed()
                    refs += `${refs ? '\n' : ''}${variable}=${r.uri}`
                  },
                  false
                )}>{w('add')}</button
            >
          </div>
          <Field label=".env.refs"
            ><textarea
              bind:value={refs}
              spellcheck="false"
              placeholder="TOKEN=dmsg-secret://…"></textarea></Field
          >
        {:else}<details>
            <summary class="cursor-pointer text-xs text-forest"
              >{ux('advanced')}</summary
            ><Field label="dmsg-secret://"
              ><input
                bind:value={reference}
                placeholder="dmsg-secret://account/item/field"
                spellcheck="false"
              /></Field
            >
          </details>{/if}
      {/if}
      <Field label={w('alias')}
        ><input bind:value={alias} placeholder=".env" /></Field
      >
      <button
        type="button"
        class={buttonClass('secondary')}
        disabled={model.state.busy || mappings.length >= 64}
        onclick={add}>{w('add')}</button
      >
      {#each mappings as mapping, i}<div
          class="flex items-center justify-between gap-2 rounded-lg bg-wash p-2 text-sm"
        >
          <div class="min-w-0">
            <p class="break-all font-medium">
              {mapping.relative_path}{mapping.alias
                ? ` → ${mapping.alias}`
                : ''}
            </p>
            <p class="text-xs text-muted">
              {w(
                mapping.source.type === 'library_file'
                  ? 'library'
                  : mapping.source.type === 'refs_dotenv'
                    ? 'refs'
                    : 'field'
              )}
            </p>
          </div>
          <button
            type="button"
            class={buttonClass('ghost')}
            onclick={() => {
              changed()
              mappings = mappings.filter((_, n) => n !== i)
            }}>{t('remove')}</button
          >
        </div>{/each}
      {#if window.dmsg.platform === 'darwin'}<Field label={w('window')}
          ><input
            type="number"
            min="1"
            max="60"
            step="1"
            required
            bind:value={windowSeconds}
          /></Field
        >{/if}
      <button
        class={buttonClass('primary')}
        disabled={model.state.busy || !project || !mappings.length || !ready}
        >{w('prepare')}</button
      >
    </form>{/if}
  {#if preview}<div class={card} data-testid="mount-preview">
      <h3 class="font-medium">{ux('reviewSelection')}</h3>
      <p class="break-all text-sm">
        {preview.input.project_directory} → {preview.input.mount_point}
      </p>
      {#each preview.input.entries ?? mappings as entry}<p
          class="text-sm break-all"
        >
          {entry.relative_path}
        </p>{/each}
      <p class="text-xs text-muted">{w('window')}: {windowSeconds}s</p>
      <button
        class={buttonClass('primary')}
        disabled={model.state.busy}
        onclick={() =>
          model.act(
            () => bridge(() => window.dmsg.secretMountSave(preview.session)),
            (r: any) => {
              preview = null
              open(r.id)
            }
          )}>{w('review')}</button
      >
    </div>{/if}
  <Notice>{ux('mountSaved')}</Notice>
  {#each mounts as mount}<article class={card}>
      <div class="flex flex-wrap justify-between gap-2">
        <strong>{mount.input.project_label}</strong><span class="text-sm"
          >{term(statuses[mount.id]?.state ?? 'unknown')}</span
        >
      </div>
      <Id
        id={mount.input.mount_point}
        full
        wrap
        copyLabel={t('copy')}
      />{#if statuses[mount.id]?.error}{@const error = String(
          statuses[mount.id].error
        )}<Notice tone="warn"
          >{error.startsWith('FSKIT_')
            ? w(error)
            : errorText(Error(error))}</Notice
        >{#if error === 'FSKIT_EXTENSION_DISABLED'}<button
            class={buttonClass('secondary')}
            onclick={settings}>{w('openSettings')}</button
          >{/if}{/if}
      <div class="flex flex-wrap gap-2">
        <button
          class={buttonClass('primary')}
          disabled={model.state.busy ||
            statuses[mount.id]?.state !== 'stopped' ||
            !ready}
          onclick={() => start(mount.id)}>{w('start')}</button
        ><button
          class={buttonClass('secondary')}
          disabled={model.state.busy || statuses[mount.id]?.state === 'stopped'}
          onclick={() =>
            model.act(() =>
              call('secrets/mounts/revoke_reads', { id: mount.id })
            )}>{w('revoke')}</button
        ><button
          class={buttonClass('secondary')}
          disabled={model.state.busy || statuses[mount.id]?.state === 'stopped'}
          onclick={() =>
            model.act(() => call('secrets/mounts/stop', { id: mount.id }))}
          >{w('stop')}</button
        ><button
          class={buttonClass('danger')}
          disabled={model.state.busy}
          onclick={() =>
            model.act(() => call('secrets/mounts/remove', { id: mount.id }))}
          >{t('remove')}</button
        >
      </div>
      {#each statuses[mount.id]?.core?.grants ?? [] as grant}<p class="text-xs">
          {grant.entry} · {Math.ceil(grant.remaining_ms / 1000)}s · {size(
            grant.released_bytes
          )}
          <button
            class={buttonClass('ghost')}
            onclick={() => open(grant.request_id)}>{w('review')}</button
          >
        </p>{/each}
    </article>{/each}
</Panel>
