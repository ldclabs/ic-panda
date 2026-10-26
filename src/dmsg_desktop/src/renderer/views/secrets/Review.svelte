<script lang="ts">
  import { size as fileSize } from '../../rpc'
  import ReviewDetails from './ReviewDetails.svelte'
  import { ux } from './experience-text'
  import { secretText as s } from './text'
  import { workflowText as w } from './workflow-text'
  let { request }: { request: any } = $props()
  const d = $derived(request.detail ?? {})
  const policy = $derived(d.policy)
</script>

<div
  class="space-y-3 rounded-xl border border-line bg-paper p-4 text-sm"
  data-testid="secret-review"
>
  {#if ['secret_mount_config', 'secret_mount_read', 'secret_share', 'secret_package_import', 'secret_discovery_import'].includes(request.type)}
    <p class="font-semibold">{w(request.type)}</p>
    {#if request.type.startsWith('secret_mount')}
      {#if d.scope?.type === 'macos_file_window' || d.mode?.type === 'macos_file_window'}<p
          class="rounded-lg bg-warn-pale p-3 text-warn"
        >
          {w('sharedWindow')}
        </p>{/if}
      {#if d.scope?.type === 'linux_handle'}<p class="text-warn">
          {w('linuxHandleWarning')}
        </p>{/if}
      {#if d.grant_lifetime_ms}<p>
          {w('window')}: {d.grant_lifetime_ms / 1000}
        </p>{/if}
      <p class="text-xs text-muted">
        {w(
          request.type === 'secret_mount_config'
            ? 'exclude'
            : 'mountReadWarning'
        )}
      </p>
    {:else if request.type === 'secret_share'}<p class="text-warn">
        {w(d.channel ? 'channelWarning' : 'shareWarning')}
      </p>
      {#if d.fields?.some((f: any) => f.type === 'otp')}<p class="text-warn">
          {w('otpSeedWarning')}
        </p>{/if}
      {#if d.item_type === 'ssh' && d.fields?.some((f: any) => f.type === 'concealed')}<p
          class="text-warn"
        >
          {w('privateKeyWarning')}
        </p>{/if}
      {#if d.item_type === 'wallet' && (d.input?.include_notes || d.fields?.some((f: any) => f.type === 'concealed'))}<p
          class="text-warn"
        >
          {w('walletSeedWarning')}
        </p>{/if}
    {:else if request.type === 'secret_package_import'}<p class="text-warn">
        {w('localFiles')}
      </p>
      <p class="text-warn">{w('filesPermissions')}</p>
    {:else}<p class="text-warn">{w('scanHint')}</p>{/if}
    <ReviewDetails {request} />
    <details>
      <summary class="cursor-pointer text-xs text-forest"
        >{ux('details')}</summary
      >
      <pre
        class="max-h-96 overflow-auto whitespace-pre-wrap [overflow-wrap:anywhere]">{JSON.stringify(
          d,
          null,
          2
        )}</pre>
    </details>
    {#if request.delivery_report}<details>
        <summary class="cursor-pointer text-xs text-forest"
          >{ux('details')}</summary
        >
        <pre
          class="max-h-64 overflow-auto whitespace-pre-wrap break-all">{JSON.stringify(
            request.delivery_report,
            null,
            2
          )}</pre>
      </details>{/if}
  {:else if request.type === 'secret_ssh' || request.type === 'secret_ssh_binding'}
    <p class="text-warn">
      {w(
        request.type === 'secret_ssh_binding'
          ? 'sshEnableWarning'
          : 'sshSignWarning'
      )}
    </p>
    {#if d.purpose?.kind === 'git_sign'}<p class="text-warn">
        {w('gitWarning')}
      </p>{/if}
    {#if d.purpose?.kind === 'ssh_auth'}<p>
        {ux('fingerprint')}: {d.purpose.host_fingerprint} · {w('hostWarning')}
      </p>{/if}
    <ReviewDetails {request} />
    <details>
      <summary class="cursor-pointer text-xs text-forest"
        >{ux('details')}</summary
      >
      <pre
        class="max-h-96 overflow-auto whitespace-pre-wrap [overflow-wrap:anywhere]">{JSON.stringify(
          d,
          null,
          2
        )}</pre>
    </details>
  {:else if request.type === 'secret_process'}
    <p class="rounded-lg bg-warn-pale p-3 text-warn">{w('processWarning')}</p>
    <dl class="space-y-2 break-all">
      <div>
        <dt>{ux('accountDevice')}</dt>
        <dd>{d.account_id} / {d.device_id}</dd>
      </div>
      <div>
        <dt>{ux('program')}</dt>
        <dd>{d.executable?.path}</dd>
        <dd class="font-mono text-xs">{d.executable?.sha256}</dd>
      </div>
      <div>
        <dt>{ux('cwd')}</dt>
        <dd>{d.cwd}</dd>
      </div>
    </dl>
    <pre class="whitespace-pre-wrap [overflow-wrap:anywhere]">{JSON.stringify(
        d.argv,
        null,
        2
      )}</pre>
    <p>{ux('variable')}</p>
    {#each d.env ?? [] as [name, ref]}<p>
        {name}: {ref.item_title} / {ref.field_label} · {ref.item_revision}
      </p>{/each}
    <p>{ux('file')}</p>
    {#each d.files ?? [] as [name, source, bytes]}<div
        class="rounded-lg bg-wash p-3"
      >
        <p>{name} · {fileSize(bytes)}</p>
        <p class="break-all">
          {source.resolved
            ? `${source.resolved.item_title} / ${source.resolved.field_label}`
            : source.file_id}
        </p>
        <p class="break-all text-xs text-muted">
          {source.resolved?.item_revision ?? source.version_id}
        </p>
      </div>{/each}
    <p class="text-xs text-muted">{w('processReceipt')}</p>
  {:else if request.type === 'secret_use'}
    <p class="rounded-lg bg-warn-pale p-3 text-warn">{s('effect')}</p>
    <dl class="space-y-2 break-all">
      <div>
        <dt class="text-xs text-faint">{ux('accountDevice')}</dt>
        <dd>{d.account_id} / {d.device_id}</dd>
      </div>
      <div>
        <dt class="text-xs text-faint">Agent ID</dt>
        <dd>{d.actor}</dd>
      </div>
      <div>
        <dt class="text-xs text-faint">{s('name')}</dt>
        <dd>{d.policy_name} · {s('version')} {d.policy_version}</dd>
      </div>
      <div>
        <dt class="text-xs text-faint">{s('repo')}</dt>
        <dd data-testid="secret-review-target">
          {d.target?.api_origin}/repos/{d.target?.owner}/{d.target?.repo}/issues
        </dd>
      </div>
      <div>
        <dt class="text-xs text-faint">{s('field')}</dt>
        <dd>{d.resolved?.item_title} / {d.resolved?.field_label}</dd>
        <dd class="font-mono text-xs">
          {d.resolved?.reference?.item_id} / {d.resolved?.reference?.field_id} · {d
            .resolved?.item_revision}
        </dd>
      </div>
    </dl>
    {#if d.reason}<div>
        <p class="text-xs text-faint">{s('reason')}</p>
        <p
          class="whitespace-pre-wrap [overflow-wrap:anywhere]"
          data-testid="secret-review-reason"
        >
          {d.reason}
        </p>
      </div>{/if}
    <h3
      class="whitespace-pre-wrap font-semibold [overflow-wrap:anywhere]"
      data-testid="secret-review-title"
    >
      {d.arguments?.title}
    </h3>
    <div>
      <p class="mb-1 text-xs text-faint">{s('body')}</p>
      <pre
        class="max-h-80 overflow-auto whitespace-pre-wrap font-sans [overflow-wrap:anywhere]"
        data-testid="secret-review-body">{d.arguments?.body ?? ''}</pre>
    </div>
    <p class="text-xs text-muted">{s('single')}</p>
    {#if request.result}<p class="break-all" data-testid="secret-review-result">
        #{request.result.number} · {request.result.html_url}
      </p>{/if}
    {#if request.state === 'lost'}<p
        class="text-warn"
        data-testid="secret-review-unknown"
      >
        {s('unknown')}
      </p>{/if}
  {:else}
    <p class="text-warn">{s('config')}</p>
    <p class="break-all">{d.account_id} / {d.device_id}</p>
    <p class="font-mono text-xs">
      {d.id} · {s('version')}
      {d.base_version ?? 0}
    </p>
    {#if policy}
      <p class="font-semibold">
        {policy.name} · {policy.enabled ? s('enabled') : s('disabled')}
      </p>
      <p class="break-all">
        {policy.target?.api_origin}/repos/{policy.target?.owner}/{policy.target
          ?.repo}/issues · issue.create
      </p>
      <p class="font-mono text-xs break-all">
        {s('reference')}: dmsg-secret://{policy.credential?.account_id}/{policy
          .credential?.item_id}/{policy.credential?.field_id}
      </p>
      {#if d.grant_to}<p class="break-all">
          Agent ID: {d.grant_to} · secret.use:{d.id}
        </p>
        <p class="text-xs text-muted">{s('grantLifetime')}</p>{/if}
      <p class="text-xs text-muted">{s('scope')}</p>
    {:else}<p>{s('delete')}</p>{/if}
  {/if}
</div>
