<script lang="ts">
  import Id from '../../components/page/Id.svelte'
  import { size, when } from '../../rpc'
  import { workflowText as w } from './workflow-text'
  import { ux, term } from './experience-text'
  let { request }: { request: any } = $props()
  const d = $derived(request.detail ?? {})
  const mounting = $derived(d.input ?? d.config?.input)
  const mountEntries = $derived(mounting?.entries ?? (d.entry ? [d.entry] : []))
  const selectedFields = $derived(
    request.type === 'secret_package_import'
      ? (d.preview?.fields ?? []).filter((f: any) =>
          d.selection?.fields?.includes(f.id)
        )
      : (d.fields ?? [])
  )
  const selectedAttachments = $derived(
    request.type === 'secret_package_import'
      ? (d.preview?.attachments ?? []).filter((_: any, i: number) =>
          d.selection?.attachments?.includes(i)
        )
      : (d.attachments ?? [])
  )
</script>

<dl
  class="grid gap-4 text-sm sm:grid-cols-2"
  data-testid="secret-review-summary"
>
  {#if d.account_id || d.device_id}<div class="min-w-0">
      <dt class="text-xs text-muted">{ux('accountDevice')}</dt>
      <dd class="mt-1 space-y-1">
        {#if d.account_id}<Id
            id={d.account_id}
            full
            wrap
          />{/if}{#if d.device_id}<Id id={d.device_id} wrap />{/if}
      </dd>
    </div>{/if}
  {#if request.type.startsWith('secret_mount')}
    <div class="min-w-0">
      <dt class="text-xs text-muted">{ux('destination')}</dt>
      <dd class="mt-1 break-all">
        {mounting?.project_label ?? d.entry?.relative_path}<br
        />{mounting?.project_directory ?? ''}<br />{mounting?.mount_point ?? ''}
      </dd>
    </div>
    <div>
      <dt class="text-xs text-muted">{ux('scope')}</dt>
      <dd class="mt-1">
        {w('window')}: {(d.scope?.window_ms ??
          d.mode?.window_ms ??
          d.grant_lifetime_ms ??
          0) / 1000}s · UID {d.scope?.uid ?? d.mode?.owner_uid ?? '—'}
      </dd>
    </div>
    {#each mountEntries as entry}{@const raw =
        entry.source?.source ?? entry.source}
      <div class="min-w-0">
        <dt class="text-xs text-muted">{entry.relative_path}</dt>
        <dd class="mt-1 break-all">
          <p>
            {raw?.resolved?.item_title ?? raw?.reference ?? raw?.file_id ?? ''}
          </p>
          {#if raw?.resolved}<p>{raw.resolved.field_label}</p>
            <Id id={raw.resolved.item_revision} full wrap />{/if}
          {#if raw?.version_id}<Id id={raw.version_id} full wrap />{/if}
          {#each entry.source?.variables ?? [] as variable}<p>
              {Array.isArray(variable) ? variable[0] : variable.name} · {Array.isArray(
                variable
              )
                ? `${variable[1].item_title} / ${variable[1].field_label}`
                : variable.reference?.field_id}
            </p>{/each}
        </dd>
      </div>{/each}
  {:else if request.type === 'secret_share' || request.type === 'secret_package_import'}
    <div>
      <dt class="text-xs text-muted">{ux('source')}</dt>
      <dd class="mt-1 font-medium">{d.title ?? d.preview?.title}</dd>
    </div>
    <div class="min-w-0">
      <dt class="text-xs text-muted">{ux('destination')}</dt>
      <dd class="mt-1 break-all">
        {request.type === 'secret_package_import'
          ? ux('openVault')
          : (d.channel?.name ?? ux('linkMode'))}{#if d.selection?.folder}<p>
            {w('destination')}: {d.selection.folder}
          </p>{/if}
      </dd>
    </div>
    <div>
      <dt class="text-xs text-muted">{ux('selected')}</dt>
      <dd class="mt-1 space-y-1">
        {#each selectedFields as field}<p>
            {field.label || field.id} · {term(field.type)}
          </p>{/each}{#if d.input?.include_notes || d.selection?.include_notes}<p
          >
            {w('notes')}
          </p>{/if}{#each selectedAttachments as attachment}<p>
            {attachment.name} · {size(attachment.size)}
          </p>{/each}
      </dd>
    </div>
    {#if request.type === 'secret_share' && !d.channel}<div>
        <dt class="text-xs text-muted">{ux('scope')}</dt>
        <dd class="mt-1">
          {when(d.expires_at)}<br />{w('sessions')}: {d.max_download_sessions}<br
          />{w('password')}: {d.password ? '✓' : '—'}
        </dd>
      </div>{/if}
  {:else if request.type === 'secret_discovery_import'}
    <div class="min-w-0">
      <dt class="text-xs text-muted">{ux('source')}</dt>
      <dd class="mt-1 break-all">{d.scan?.root}</dd>
    </div>
    <div>
      <dt class="text-xs text-muted">{ux('selected')}</dt>
      <dd class="mt-1 space-y-1">
        {#each d.scan?.report?.candidates?.filter( (c: any) => d.candidate_ids?.includes(c.id) ) ?? [] as candidate}<p
          >
            {candidate.relative_path} · {candidate.variables?.join(', ')}
          </p>{/each}
      </dd>
    </div>
  {:else if request.type.startsWith('secret_ssh')}
    <div>
      <dt class="text-xs text-muted">{ux('source')}</dt>
      <dd class="mt-1">
        {d.resolved?.item_title ?? d.binding?.resolved?.item_title}<br />{d
          .resolved?.field_label ?? d.binding?.resolved?.field_label}
      </dd>
    </div>
    <div class="min-w-0">
      <dt class="text-xs text-muted">{ux('fingerprint')}</dt>
      <dd class="mt-1">
        <Id id={d.fingerprint ?? d.binding?.fingerprint ?? ''} full wrap />
      </dd>
    </div>
    <div>
      <dt class="text-xs text-muted">{ux('scope')}</dt>
      <dd class="mt-1">
        {d.purpose
          ? term(d.purpose.kind)
          : (d.purposes ?? []).map(term).join(' · ')}{#if d.purpose?.username}<p
          >
            {d.purpose.username}
          </p>{/if}{#if d.purpose?.host_fingerprint}<Id
            id={d.purpose.host_fingerprint}
            full
            wrap
          />{/if}
      </dd>
    </div>
  {/if}
</dl>
