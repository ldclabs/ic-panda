<script lang="ts">
  import Panel from './Panel.svelte'
  import Dependencies from './Dependencies.svelte'
  import SourcePicker from './SourcePicker.svelte'
  import Select from '../../components/Select.svelte'
  import Checkbox from '../../components/Checkbox.svelte'
  import Field from '../../components/page/Field.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Id from '../../components/page/Id.svelte'
  import { panel } from './workflow-state.svelte'
  import {
    sourceItems,
    card,
    toggle,
    rotationAllowed,
    type SourceItem
  } from './model'
  import { openVault, openFile } from './navigation.svelte'
  import { call, bridge, when, size } from '../../rpc'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { workflowText as w } from './workflow-text'
  import { ux, term } from './experience-text'
  let {
    open,
    navigate
  }: {
    open: (id: string) => void
    navigate: (tab: string, id?: string) => void
  } = $props()
  let report = $state<any>(null),
    items = $state<SourceItem[]>([]),
    rotations = $state<any[]>([]),
    policies = $state<any[]>([]),
    ignored = $state(false),
    severity = $state('all'),
    query = $state('')
  let scan = $state<any>(null),
    picks = $state<string[]>([]),
    itemId = $state(''),
    fieldId = $state(''),
    index = $state<any>(null),
    dependencies = $state<Record<string, any>>({})
  const model = panel(
    async () =>
      Promise.all([
        model.request('health', 'secrets/health', {}, (r: any) => (report = r)),
        model.request(
          'rotations',
          'secrets/rotation/list',
          {},
          (r: any) => (rotations = r.rotations)
        ),
        model.request(
          'vault',
          'vault/list',
          {},
          (r: any[]) => (items = sourceItems(r))
        ),
        model.request(
          'policies',
          'secrets/policies/list',
          {},
          (r: any) => (policies = r.policies)
        ),
        ...(scan?.id
          ? [
              model.request(
                'scan',
                'secrets/discovery/status',
                { scan_id: scan.id },
                (r: any) => (scan = r)
              )
            ]
          : []),
        ...Object.keys(dependencies).map((id) =>
          model.request(
            `dependencies-${id}`,
            'secrets/rotation/dependencies',
            { id },
            (r: any) => (dependencies = { ...dependencies, [id]: r })
          )
        )
      ]),
    () => scan?.state === 'running'
  )
  const name = (f: any) =>
    items.find((i) => i.id === f.subject)?.title ??
    policies.find((p) => p.id === f.subject)?.policy.name ??
    ux(f.rule)
  const findings = $derived(
    (report?.findings ?? [])
      .filter(
        (f: any) =>
          (ignored || !f.dismissed) &&
          (severity === 'all' || f.severity === severity) &&
          (!query ||
            `${name(f)} ${ux(f.rule)}`
              .toLowerCase()
              .includes(query.toLowerCase()))
      )
      .sort(
        (a: any, b: any) =>
          (a.severity === 'high' ? 0 : a.severity === 'warning' ? 1 : 2) -
          (b.severity === 'high' ? 0 : b.severity === 'warning' ? 1 : 2)
      )
  )
  function remedy(f: any) {
    if (items.some((i) => i.id === f.subject)) {
      if (f.rule === 'attachment_backup_unconfirmed') openFile(f.field)
      else openVault(f.subject)
    } else if (
      f.rule.includes('policy') ||
      f.rule === 'not_observed_on_this_device'
    )
      navigate('policies', f.subject)
    else if (f.rule.includes('ssh')) navigate('ssh', f.subject)
    else if (f.rule.includes('mount')) navigate('mounts', f.subject)
    else location.hash = '/files/links'
  }
  const ref = () =>
    call<any>('secrets/ref', { item_id: itemId, field_id: fieldId })
  const revision = (r: any) =>
    items.find((i) => i.id === r.reference.item_id)?.revision
  const steps = ['created', 'tested', 'references_checked', 'upstream_revoked']
</script>

<Panel {model} title={w('health')} description={ux('healthScope')}>
  <nav
    class="flex flex-wrap gap-3 text-sm text-forest"
    aria-label={w('health')}
  >
    <a
      href="#health-findings"
      onclick={(e) => {
        e.preventDefault()
        document
          .getElementById('health-findings')
          ?.scrollIntoView({ block: 'start' })
      }}>{ux('findings')}</a
    ><a
      href="#health-scan"
      onclick={(e) => {
        e.preventDefault()
        document
          .getElementById('health-scan')
          ?.scrollIntoView({ block: 'start' })
      }}>{ux('scan')}</a
    ><a
      href="#health-rotation"
      onclick={(e) => {
        e.preventDefault()
        document
          .getElementById('health-rotation')
          ?.scrollIntoView({ block: 'start' })
      }}>{w('rotation')}</a
    >
  </nav>
  <section class={card} id="health-findings">
    <div class="flex flex-wrap items-center justify-between gap-2">
      <h3 class="font-semibold">
        {ux('findings')}
        {report?.findings?.filter((f: any) => !f.dismissed).length ?? '—'}
      </h3>
      {#if report?.observed_at}<span class="text-xs text-muted"
          >{when(report.observed_at)}</span
        >{/if}
    </div>
    <div class="grid gap-3 sm:grid-cols-2">
      <Field label={ux('search')}
        ><input type="search" bind:value={query} /></Field
      ><Field label={ux('severity')}
        ><Select
          bind:value={severity}
          options={['all', 'high', 'warning', 'info'].map((value) => ({
            value,
            label: ux(value)
          }))}
          label={ux('severity')}
          class="w-full"
        /></Field
      >
    </div>
    <Checkbox bind:checked={ignored}>{ux('ignored')}</Checkbox>
    {#if report && !model.state.errors.health && !report.findings.length}<Notice
        tone="ok">{ux('noFindings')}</Notice
      >{:else if !report}<Notice>{ux('unknown')}</Notice>{/if}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex (Scrollable results must be keyboard-scrollable.) -->
    <div
      class="max-h-[32rem] space-y-3 overflow-y-auto"
      tabindex="0"
      role="region"
      aria-label={ux('findings')}
    >
      {#each findings as finding}<article
          class="space-y-2 rounded-lg border border-line p-3"
        >
          <div class="flex flex-wrap justify-between gap-2">
            <strong class="text-sm">{ux(finding.rule)}</strong><span
              class="text-xs text-warn">{ux(finding.severity)}</span
            >
          </div>
          <p class="text-sm font-medium">{name(finding)}</p>
          <p class="text-xs text-muted">
            {term(finding.evidence)} · {when(finding.observed_at)}
          </p>
          <div class="flex flex-wrap gap-2">
            <button
              class={buttonClass('secondary')}
              onclick={() => remedy(finding)}>{ux('openSource')}</button
            ><button
              class={buttonClass('ghost')}
              disabled={model.state.busy || finding.dismissed}
              onclick={() =>
                model.act(() =>
                  call('secrets/health/dismiss', { id: finding.id })
                )}>{w('dismiss')}</button
            >
          </div>
          <details>
            <summary class="cursor-pointer text-xs text-muted"
              >{ux('details')}</summary
            ><Id id={finding.subject} full wrap /><Id
              id={finding.revision}
              full
              wrap
            />
            <p class="break-all text-xs">{finding.rule} · {finding.action}</p>
          </details>
        </article>
      {:else}{#if report?.findings.length}<p class="text-sm text-muted">
            {ux('noMatches')}
          </p>{/if}{/each}
    </div>
  </section>
  <section class={card} id="health-scan">
    <h3 class="font-semibold">{ux('scan')}</h3>
    <p class="text-sm text-muted">{w('scanHint')}</p>
    <button
      class={buttonClass('secondary')}
      disabled={model.state.busy || scan?.state === 'running'}
      onclick={() =>
        model.act(
          () => bridge(() => window.dmsg.secretScanDirectory()),
          (r: any) => {
            if (r) {
              scan = { id: r.scan_id, state: r.state }
              picks = []
            }
          }
        )}>{w('scan')}</button
    >
    {#if scan}<p class="break-all text-xs">
        {scan.root} · {term(scan.state)} · {scan.report?.files ?? 0}
        {ux('file')} · {size(scan.report?.bytes ?? 0)}
      </p>
      {#if scan.state === 'running'}<button
          class={buttonClass('secondary')}
          disabled={model.state.busy}
          onclick={() =>
            model.act(() =>
              call('secrets/discovery/cancel', { scan_id: scan.id })
            )}>{t('cancel')}</button
        >{/if}{#if scan.report && !scan.report.complete}<Notice tone="warn"
          >{w('partial')}</Notice
        >{/if}{#each scan.report?.candidates ?? [] as candidate}<div
          class="space-y-1"
        >
          <Checkbox
            checked={picks.includes(candidate.id)}
            onCheckedChange={(checked) =>
              (picks = toggle(picks, candidate.id, checked))}
            disabled={candidate.warnings.length > 0}
            >{candidate.relative_path} · {candidate.variables.join(
              ', '
            )}</Checkbox
          >{#if candidate.warnings.length}<p class="text-xs text-warn">
              {candidate.warnings.map(ux).join(' · ')}
            </p>{/if}
        </div>{/each}<button
        class={buttonClass('primary')}
        disabled={model.state.busy || !picks.length || scan.state === 'running'}
        onclick={() =>
          model.act(
            () =>
              call('secrets/discovery/import_request', {
                scan_id: scan.id,
                candidate_ids: picks
              }),
            (r: any) => open(r.id)
          )}>{w('import')}</button
      >{/if}
  </section>
  <section class={card} id="health-rotation">
    <h3 class="font-semibold">{w('rotation')}</h3>
    <p class="text-sm leading-relaxed text-muted">{ux('rotationHelp')}</p>
    <SourcePicker
      {items}
      bind:itemId
      bind:fieldId
      concealed
      onChange={() => (index = null)}
    />
    <div class="flex flex-wrap gap-2">
      <button
        class={buttonClass('primary')}
        disabled={model.state.busy || !fieldId}
        onclick={() =>
          model.act(async () =>
            call('secrets/rotation/create', {
              reference: (await ref()).reference
            })
          )}>{w('start')}</button
      ><button
        class={buttonClass('secondary')}
        disabled={model.state.busy || !fieldId}
        onclick={() =>
          model.act(
            async () =>
              call('secrets/references', {
                reference: (await ref()).reference
              }),
            (r: any) => (index = r),
            false
          )}>{w('references')}</button
      ><button
        class={buttonClass('ghost')}
        onclick={() => openVault(itemId || undefined)}>{ux('openVault')}</button
      >
    </div>
    <p class="text-xs text-muted">{w('referenceBoundary')}</p>
    {#if index}<Dependencies {index} {navigate} />{/if}
    {#each rotations as rotation}<article
        class="space-y-3 border-t border-line pt-4"
      >
        <div class="flex flex-wrap items-center justify-between gap-2">
          <h4 class="font-medium">
            {items.find((i) => i.id === rotation.reference.item_id)?.title ??
              ux('source')} · {rotation.reference.field_id}
          </h4>
          <button
            class={buttonClass('ghost')}
            onclick={() => openVault(rotation.reference.item_id)}
            >{ux('openSource')}</button
          >
        </div>
        {#if rotation.completed_at}<Notice tone="ok"
            >{ux('complete')} · {when(rotation.completed_at)}</Notice
          >{:else}<p class="text-xs text-muted">{ux('rotationBlocked')}</p>{/if}
        <button
          class={buttonClass('secondary')}
          disabled={model.state.busy}
          onclick={() =>
            model.act(
              () => call('secrets/rotation/dependencies', { id: rotation.id }),
              (r: any) =>
                (dependencies = { ...dependencies, [rotation.id]: r }),
              false
            )}>{w('dependencies')}</button
        >
        {#if dependencies[rotation.id]}<Dependencies
            index={dependencies[rotation.id]}
            {navigate}
          />{#if rotation.references_checked && rotation.references_digest !== dependencies[rotation.id].digest}<Notice
              tone="warn">{ux('sourceChanged')}</Notice
            >{/if}{/if}
        <ol class="grid gap-2 sm:grid-cols-2">
          {#each steps as step, i}<li>
              <button
                class={buttonClass(
                  'secondary',
                  'md',
                  'h-auto min-h-10 w-full justify-start whitespace-normal text-start'
                )}
                disabled={model.state.busy ||
                  !rotationAllowed(
                    rotation,
                    step,
                    revision(rotation),
                    dependencies[rotation.id]
                  )}
                onclick={() =>
                  model.act(() =>
                    call('secrets/rotation/update', {
                      id: rotation.id,
                      step,
                      dependencies_digest: dependencies[rotation.id]?.digest
                    })
                  )}
                ><span class="text-xs text-muted"
                  >{rotation[step] ? '✓' : i + 1}</span
                >{w(step)}</button
              >
            </li>{/each}
        </ol>
      </article>{/each}
  </section>
</Panel>
