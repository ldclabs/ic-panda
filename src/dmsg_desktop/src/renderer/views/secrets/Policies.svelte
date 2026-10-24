<script lang="ts">
  import { onMount } from 'svelte'
  import Notice from '../../components/page/Notice.svelte'
  import { call, errorText, onChange, single, when } from '../../rpc'
  import { buttonClass } from '../../components/ui'
  import { stateLabel } from '../../i18n/index.svelte'
  import { secretText as s } from './text'
  let { open }: { open: (id: string) => void } = $props()
  let policies = $state<any[]>([]), items = $state<any[]>([]), agents = $state<any[]>([]), uses = $state<any[]>([])
  let editing = $state(false), selected = $state<any>(null), busy = $state(false), error = $state(''), loaded = $state(false)
  let name = $state(''), owner = $state(''), repo = $state(''), itemId = $state(''), fieldId = $state(''), grantTo = $state(''), reference = $state('')
  let alive = true
  const entry = $derived(items.find((i) => i.id === itemId))
  const fields = $derived((entry?.fields ?? []).filter((f: any) => f.type === 'concealed'))
  const inputClass = 'w-full rounded-lg border border-line bg-surface px-3 py-2 text-sm'

  const reload = single(async () => {
    try {
      const [p, v, a, u]: any[] = await Promise.all([call('secrets/policies/list'),call('vault/list'),call('agent/ids'),call('secrets/uses/list')])
      if (!alive) return
      policies = p.policies
      items = v.filter((e: any) => !e.deleted && e.item.type !== 'wallet' && !(e.item.tags ?? []).some((tag: string) => tag.startsWith('dmsg-')))
        .map((e: any) => ({ id: e.id, title: e.item.title, fields: e.item.fields.map((f: any) => ({ id: f.id, label: f.label, type: f.type })) }))
      agents = a.agents; uses = u.uses; loaded = true; error = ''
    } catch (e) { if (alive) error = errorText(e) }
  })

  onMount(() => { void reload(); const off = onChange(['approval/changed','agent/changed','vault/changed'],reload); return () => {alive = false;off()} })

  function edit(p: any = null) {
    selected=p;editing=true;name=p?.policy.name ?? '';owner=p?.policy.target.owner ?? '';repo=p?.policy.target.repo ?? ''
    itemId=p?.policy.credential.item_id ?? '';fieldId=p?.policy.credential.field_id ?? '';grantTo='';reference=''
  }

  async function action(run: () => Promise<any>, approval = false, refresh = true) {
    if (busy) return
    busy=true;error=''
    try {const r=await run();if(!alive)return;if(approval){editing=false;open(r.id)}if(refresh)await reload()}
    catch(e){if(alive)error=errorText(e)}finally{if(alive)busy=false}
  }

  async function save(event: SubmitEvent) {
    event.preventDefault()
    await action(async () => {
      const r: any=await call('secrets/ref',{item_id:itemId,field_id:fieldId})
      if(!alive)return
      return call('secrets/policies/save_request',{
        ...(selected?{id:selected.id,base_version:selected.version}:{}),
        ...(grantTo?{grant_to:grantTo}:{}),
        policy:{schema_version:1,name,enabled:true,adapter:'github.issues/1',credential:r.reference,
          target:{api_origin:'https://api.github.com',owner,repo},operations:['issue.create']}
      })
    },true)
  }

  async function showReference() {
    await action(async()=>{const r:any=await call('secrets/ref',{item_id:itemId,field_id:fieldId});if(alive)reference=r.uri})
  }

  async function older() {
    await action(async()=>{const r:any=await call('secrets/uses/list',{before:uses.at(-1)?.request_id});if(alive)uses=r.uses},false,false)
  }
</script>

<section class="space-y-4" data-testid="secret-policies">
  <div class="flex items-center gap-3"><div class="flex-1"><h2 class="font-semibold">{s('title')}</h2><p class="text-xs text-muted">{s('local')}</p></div><button class={buttonClass('secondary','sm')} disabled={busy} onclick={()=>reload()}>{s('refresh')}</button><button class={buttonClass('primary','sm')} disabled={busy} onclick={()=>edit()}>{s('new')}</button></div>
  {#if error}<Notice tone="danger">{error}</Notice>{/if}
  {#if editing}
    <form onsubmit={save} class="space-y-3 rounded-xl border border-line bg-paper p-4" data-testid="secret-policy-form">
      <label class="block text-xs">{s('name')}<input class={inputClass} bind:value={name} required maxlength="100" /></label>
      <div class="grid gap-3 sm:grid-cols-2"><label class="block text-xs">{s('owner')}<input class={inputClass} bind:value={owner} required maxlength="39" /></label><label class="block text-xs">{s('repo')}<input class={inputClass} bind:value={repo} required maxlength="100" /></label></div>
      <label class="block text-xs">{s('item')}<select class={inputClass} bind:value={itemId} onchange={()=>{fieldId='';reference=''}} required><option value="">{s('choose')}</option>{#each items as item (item.id)}<option value={item.id}>{item.title}</option>{/each}</select></label>
      <label class="block text-xs">{s('field')}<select class={inputClass} bind:value={fieldId} onchange={()=>reference=''} required><option value="">{s('choose')}</option>{#each fields as field (field.id)}<option value={field.id}>{field.label || field.id}</option>{/each}</select></label>
      <label class="block text-xs">{s('agent')}<select class={inputClass} bind:value={grantTo}><option value="">{s('none')}</option>{#each agents.filter((a)=>!a.paused) as agent (agent.agent_id)}<option value={agent.agent_id}>{agent.label} · {agent.agent_id}</option>{/each}</select></label>
      {#if grantTo}<p class="text-xs text-muted">{s('grantLifetime')}</p>{/if}
      <p class="text-xs text-muted">{s('scope')}</p>
      {#if reference}<code class="block break-all text-xs" data-testid="secret-reference">{reference}</code>{/if}
      <div class="flex flex-wrap gap-2"><button class={buttonClass('primary','sm')} disabled={busy || !fieldId}>{s('submit')}</button><button type="button" class={buttonClass('secondary','sm')} disabled={busy || !fieldId} onclick={showReference}>{s('reference')}</button><button type="button" class={buttonClass('secondary','sm')} disabled={busy} onclick={()=>editing=false}>{s('cancel')}</button></div>
    </form>
  {/if}
  {#each policies as p (p.id)}
    <article class="rounded-xl border border-line bg-paper p-4" data-testid="secret-policy">
      <h3 class="font-semibold">{p.policy.name} <span class="text-xs text-muted">{p.policy.enabled?s('enabled'):s('disabled')} · {s('version')} {p.version}</span></h3>
      <p class="my-1 break-all text-sm">{p.policy.target.owner}/{p.policy.target.repo} · issue.create</p><code class="text-xs break-all">secret.use:{p.id}</code>
      <div class="mt-3 flex flex-wrap gap-2"><button class={buttonClass('secondary','sm')} disabled={busy} onclick={()=>edit(p)}>{s('edit')}</button><button class={buttonClass('secondary','sm')} disabled={busy || !p.policy.enabled} onclick={()=>action(()=>call('secrets/policies/disable',{id:p.id}))}>{s('disable')}</button><button class={buttonClass('secondary','sm')} disabled={busy} onclick={()=>action(()=>call('secrets/policies/delete_request',{id:p.id,base_version:p.version}),true)}>{s('delete')}</button></div>
    </article>
  {:else}{#if loaded}<p class="text-sm text-muted">{s('empty')}</p>{/if}{/each}
  <p class="text-xs text-muted">{s('revokeHint')}</p>
  <h2 class="pt-4 font-semibold">{s('history')}</h2>
  {#each uses as use (use.request_id)}
    <article class="space-y-2 rounded-xl border border-line p-4 text-sm" data-testid="secret-use-record">
      <div class="flex justify-between gap-2"><strong>{use.approval?.detail?.policy_name}</strong><span>{stateLabel(use.state)}</span></div>
      <p class="break-all text-xs text-muted">{use.approval?.detail?.actor} · {when(use.created_at)}</p>
      <p class="whitespace-pre-wrap [overflow-wrap:anywhere]">{use.approval?.detail?.arguments?.title}</p>
      {#if use.result}<p class="break-all">#{use.result.number} · {use.result.html_url}</p>{/if}
      {#if use.error}<p class="font-mono text-xs">{use.error}</p>{/if}
      {#if use.state==='lost'}<p class="text-warn">{s('unknown')}</p>{/if}
      <div class="flex flex-wrap gap-2"><button class={buttonClass('secondary','sm')} onclick={()=>open(use.request_id)}>{s('review')}</button>{#if use.state==='awaiting_user'}<button class={buttonClass('secondary','sm')} disabled={busy} onclick={()=>action(()=>call('secrets/uses/cancel',{request_id:use.request_id}))}>{s('cancel')}</button>{/if}{#if use.state==='lost' && !use.reviewed_at}<button class={buttonClass('secondary','sm')} disabled={busy} onclick={()=>action(()=>call('secrets/uses/mark_reviewed',{request_id:use.request_id}))}>{s('mark')}</button>{/if}</div>
      {#if use.reviewed_at}<p class="text-xs text-muted">{s('reviewed')} · {when(use.reviewed_at)}</p>{/if}
    </article>
  {:else}{#if loaded}<p class="text-sm text-muted">{s('noHistory')}</p>{/if}{/each}
  {#if uses.length>=50}<button class={buttonClass('secondary','sm')} disabled={busy} onclick={older}>{s('more')}</button>{/if}
</section>
