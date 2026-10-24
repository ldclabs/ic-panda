<script lang="ts">
  import { secretText as s } from './text'
  let { request }: { request: any } = $props()
  const d = $derived(request.detail ?? {})
  const policy = $derived(d.policy)
</script>

<div class="space-y-3 rounded-xl border border-line bg-paper p-4 text-sm" data-testid="secret-review">
  {#if request.type === 'secret_use'}
    <p class="rounded-lg bg-warn-pale p-3 text-warn">{s('effect')}</p>
    <dl class="space-y-2 break-all">
      <div><dt class="text-xs text-faint">Account / Device</dt><dd>{d.account_id} / {d.device_id}</dd></div>
      <div><dt class="text-xs text-faint">Agent ID</dt><dd>{d.actor}</dd></div>
      <div><dt class="text-xs text-faint">{s('name')}</dt><dd>{d.policy_name} · {s('version')} {d.policy_version}</dd></div>
      <div><dt class="text-xs text-faint">{s('repo')}</dt><dd data-testid="secret-review-target">{d.target?.api_origin}/repos/{d.target?.owner}/{d.target?.repo}/issues</dd></div>
      <div><dt class="text-xs text-faint">{s('field')}</dt><dd>{d.resolved?.item_title} / {d.resolved?.field_label}</dd><dd class="font-mono text-xs">{d.resolved?.reference?.item_id} / {d.resolved?.reference?.field_id} · {d.resolved?.item_revision}</dd></div>
    </dl>
    {#if d.reason}<div><p class="text-xs text-faint">{s('reason')}</p><p class="whitespace-pre-wrap [overflow-wrap:anywhere]" data-testid="secret-review-reason">{d.reason}</p></div>{/if}
    <h3 class="whitespace-pre-wrap font-semibold [overflow-wrap:anywhere]" data-testid="secret-review-title">{d.arguments?.title}</h3>
    <div><p class="mb-1 text-xs text-faint">{s('body')}</p><pre class="max-h-80 overflow-auto whitespace-pre-wrap font-sans [overflow-wrap:anywhere]" data-testid="secret-review-body">{d.arguments?.body ?? ''}</pre></div>
    <p class="text-xs text-muted">{s('single')}</p>
    {#if request.result}<p class="break-all" data-testid="secret-review-result">#{request.result.number} · {request.result.html_url}</p>{/if}
    {#if request.state === 'lost'}<p class="text-warn" data-testid="secret-review-unknown">{s('unknown')}</p>{/if}
  {:else}
    <p class="text-warn">{s('config')}</p>
    <p class="break-all">{d.account_id} / {d.device_id}</p>
    <p class="font-mono text-xs">{d.id} · {s('version')} {d.base_version ?? 0}</p>
    {#if policy}
      <p class="font-semibold">{policy.name} · {policy.enabled ? s('enabled') : s('disabled')}</p>
      <p class="break-all">{policy.target?.api_origin}/repos/{policy.target?.owner}/{policy.target?.repo}/issues · issue.create</p>
      <p class="font-mono text-xs break-all">{s('reference')}: dmsg-secret://{policy.credential?.account_id}/{policy.credential?.item_id}/{policy.credential?.field_id}</p>
      {#if d.grant_to}<p class="break-all">Agent ID: {d.grant_to} · secret.use:{d.id}</p><p class="text-xs text-muted">{s('grantLifetime')}</p>{/if}
      <p class="text-xs text-muted">{s('scope')}</p>
    {:else}<p>{s('delete')}</p>{/if}
  {/if}
</div>
