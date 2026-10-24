<script lang="ts">
  import { onMount } from 'svelte'
  import Policies from './Policies.svelte'
  import FolderPicker from '../files/FolderPicker.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import { call, bridge, errorText, single, when } from '../../rpc'
  import { buttonClass } from '../../components/ui'
  import { stateLabel, t } from '../../i18n/index.svelte'
  import { workflowText as w } from './workflow-text'
  let { open }: { open: (id: string) => void } = $props()
  const tabs = ['policies','mounts','ssh','health','sharing','receive','activity']
  let tab = $state('policies'), busy = $state(false), error = $state('')
  let mounts = $state<any[]>([]), statuses = $state<Record<string,any>>({}), supported = $state(false)
  let keys = $state<any[]>([]), bindings = $state<any[]>([]), ssh = $state<any>({})
  let findings = $state<any[]>([]), rotations = $state<any[]>([]), items = $state<any[]>([]), shares = $state<any[]>([]), activity = $state<any[]>([])
  let project = $state(''), path = $state('.env'), sourceType = $state('field_utf8'), reference = $state(''), fileId = $state(''), versionId = $state(''), refs = $state(''), windowSeconds = $state(30)
  let aliasName = $state('')
  let mappings = $state<any[]>([]), mountPreview = $state<any>(null)
  let scan = $state<any>(null), scanPicks = $state<string[]>([])
  let itemId = $state(''), fields = $state<string[]>([]), attachments = $state<string[]>([]), notes = $state(false), hours = $state(24), sessions = $state(1), password = $state('')
  let details = $state<Record<string,any>>({}), sharePreview = $state<any>(null)
  let link = $state(''), receivePassword = $state(''), received = $state<any>(null), receiveFields = $state<string[]>([]), receiveAttachments = $state<number[]>([]), receiveNotes = $state(false)
  let folderPicker=$state(false), receiveFolder=$state('root'), folders=$state<any[]>([])
  let rotationItem = $state(''), rotationField = $state('')
  let alive = true
  let selectionGeneration = 0
  const chosen = $derived(items.find((e)=>e.id===itemId))
  const input = 'w-full rounded-lg border border-line bg-surface px-3 py-2 text-sm'
  const card = 'space-y-3 rounded-xl border border-line bg-paper p-4'
  const button = buttonClass('secondary','sm')
  const primary = buttonClass('primary','sm')
  const reload = single(async () => {
    const selectedTab = tab
    try {
      if(selectedTab==='mounts') {
        const result:any = await call('secrets/mounts/list')
        const state = await Promise.all(result.mounts.map(async(m:any)=>[m.id,await call('secrets/mounts/status',{id:m.id})]))
        if(!alive || tab!==selectedTab)return
        mounts=result.mounts;supported=result.supported;statuses=Object.fromEntries(state)
      } else if(selectedTab==='ssh') {
        const [result,status]:any[]=await Promise.all([call('secrets/ssh/keys'),call('secrets/ssh/status')])
        if(!alive || tab!==selectedTab)return
        keys=result.keys;bindings=result.bindings;ssh=status
      } else if(selectedTab==='health') {
        const [health,rotation,vault]:any[]=await Promise.all([call('secrets/health'),call('secrets/rotation/list'),call('vault/list')])
        if(!alive || tab!==selectedTab)return
        findings=health.findings;rotations=rotation.rotations;items=eligible(vault)
        if(scan?.id){const result=await call('secrets/discovery/status',{scan_id:scan.id});if(alive && tab===selectedTab)scan=result}
      } else if(selectedTab==='sharing') {
        const [result,vault]:any[]=await Promise.all([call('secrets/shares/list'),call('vault/list')])
        if(!alive || tab!==selectedTab)return
        shares=result.shares;items=eligible(vault,true)
      } else if(selectedTab==='activity') {
        const result:any[]=await call('approval/list',{all:true})
        if(alive && tab===selectedTab)activity=result.filter((r)=>String(r.type).startsWith('secret_'))
      }
    }catch(e){if(alive)error=errorText(e)}
  })
  function eligible(vault:any[],sharing=false){return vault.filter((e)=>!e.deleted && (sharing || e.item.type!=='wallet') && !(e.item.tags??[]).some((tag:string)=>tag.startsWith('dmsg-'))).map((e)=>({id:e.id,revision:e.revision,type:e.item.type,title:e.item.title,fields:e.item.fields.map((f:any)=>({id:f.id,label:f.label,type:f.type})),files:e.item.files??[]}))}
  async function act(fn:()=>Promise<any>,approval=false){if(busy)return;busy=true;error='';try{const result=await fn();if(!alive)return;if(approval && result?.id)open(result.id);await reload()}catch(e){if(alive)error=errorText(e)}finally{if(alive)busy=false}}
  function changeTab(next:string){tab=next;error='';password='';receivePassword='';link='';void reload()}
  onMount(()=>{void reload();const timer=setInterval(()=>{if(!busy && (tab==='mounts'||(tab==='health' && scan?.state==='running')||(tab==='sharing' && shares.some((s)=>['encrypting','uploading','publishing'].includes(s.state)))))void reload()},1000);const off=window.dmsg.onEvent((event)=>{if(['approval/changed','vault/changed','resync'].includes(event.type))void reload()});return()=>{alive=false;selectionGeneration++;clearInterval(timer);off();password='';receivePassword='';link='';received=null;sharePreview=null}})
  function addMapping(){
    try{
      const source=sourceType==='field_utf8'?{type:sourceType,reference}:sourceType==='library_file'?{type:sourceType,file_id:fileId,version_id:versionId}:{type:sourceType,variables:refs.split('\n').filter((line)=>line.trim() && !line.trim().startsWith('#')).map((line)=>{const index=line.indexOf('=');if(index<=0)throw Error(w('invalidRefs'));const uri=line.slice(index+1).trim();const match=/^dmsg-secret:\/\/([^/]+)\/([a-f0-9]{64})\/([A-Za-z0-9_-]+)$/.exec(uri);if(!match)throw Error(w('invalidRefs'));return {name:line.slice(0,index).trim(),reference:{schema_version:1,account_id:match[1],item_id:match[2],field_id:match[3]}}})}
      mappings=[...mappings,{relative_path:path,source,...(aliasName?{alias:aliasName}:{})}];mountPreview=null
    }catch(e){error=errorText(e)}
  }
  async function selectItem(){
    const at=++selectionGeneration;fields=[];attachments=[];notes=false;sharePreview=null;details={};const selected=chosen
    if(!selected)return
    await act(async()=>{const entries=await Promise.all(selected.files.map(async(id:string)=>[id,await call('files/show',{id})]));if(alive && at===selectionGeneration)details=Object.fromEntries(entries)})
  }
  async function prepareShare(){await act(async()=>{if(!chosen)return;const secret=password;password='';const result=await call('secrets/shares/prepare',{input:{item_id:chosen.id,revision:chosen.revision,field_ids:fields,include_notes:notes,attachments:attachments.map((id)=>({file_id:id,version_id:details[id]?.file?.current})),expires_in_ms:hours*3600000,max_download_sessions:sessions},...(secret?{password:secret}:{})});if(alive)sharePreview=result})}
</script>

<section class="space-y-4" data-testid="secret-workflows">
  <h2 class="font-semibold">{w('workflows')}</h2>
  <nav class="flex flex-wrap gap-2" aria-label={w('workflows')}>{#each tabs as value}<button class={tab===value?primary:button} aria-current={tab===value?'page':undefined} onclick={()=>changeTab(value)}>{w(value)}</button>{/each}<button class={button} disabled={busy} onclick={()=>reload()}>{w('refresh')}</button></nav>
  {#if error}<Notice tone="danger">{error}</Notice>{/if}
  {#if tab==='policies'}<Policies {open} />
  {:else if tab==='mounts'}
    <p class="text-sm text-muted">{w('exclude')}</p>
    {#if window.dmsg.platform==='darwin'}<Notice tone="warn">{w('sharedWindow')}</Notice>{/if}
    {#if !supported}<Notice>{w('unavailable')}</Notice>{:else}
      <div class={card}>
        <label class="block text-xs">{w('project')}<input class={input} bind:value={project} maxlength="100" /></label>
        <label class="block text-xs">{w('path')}<input class={input} bind:value={path} /></label>
        <label class="block text-xs">{w('source')}<select class={input} bind:value={sourceType}><option value="field_utf8">{w('field')}</option><option value="library_file">{w('library')}</option><option value="refs_dotenv">{w('refs')}</option></select></label>
        {#if sourceType==='field_utf8'}<label class="block text-xs">dmsg-secret://<input class={input} bind:value={reference} placeholder="dmsg-secret://account/item/field" /></label>{:else if sourceType==='library_file'}<p class="text-xs text-warn">{w('filesPermissions')}</p><label class="block text-xs">File ID<input class={input} bind:value={fileId} /></label><label class="block text-xs">Version ID<input class={input} bind:value={versionId} /></label>{:else}<label class="block text-xs">.env.refs<textarea class={input} bind:value={refs} placeholder="TOKEN=dmsg-secret://account/item/field"></textarea></label>{/if}
        <label class="block text-xs">{w('alias')}<input class={input} bind:value={aliasName} placeholder=".env" /></label>
        <button class={button} disabled={busy||mappings.length>=64} onclick={addMapping}>{w('add')}</button>
        {#each mappings as mapping,i}<div class="flex items-center justify-between gap-2"><code class="break-all text-xs">{mapping.relative_path} · {mapping.source.type} {mapping.alias?`→ ${mapping.alias}`:''}</code><button class={button} disabled={busy} onclick={()=>{mappings=mappings.filter((_,n)=>n!==i);mountPreview=null}}>{t('remove')}</button></div>{/each}
        {#if window.dmsg.platform==='darwin'}<label class="block text-xs">{w('window')}<input class={input} type="number" min="1" max="60" bind:value={windowSeconds} /></label>{/if}
        <button class={primary} disabled={busy||!project||!mappings.length} onclick={()=>act(async()=>{const result=await bridge(()=>window.dmsg.secretMountPrepare({project_label:project,entries:mappings.map(({alias,...entry})=>entry),aliases:mappings.filter((m)=>m.alias).map((m)=>({name:m.alias,entry:m.relative_path}))},windowSeconds*1000));if(alive)mountPreview=result})}>{w('prepare')}</button>
        {#if mountPreview}<p class="break-all text-sm">{mountPreview.input.project_directory} → {mountPreview.input.mount_point}</p><p class="text-xs text-muted">{w('local')}</p><button class={primary} disabled={busy} onclick={()=>act(async()=>{const p=mountPreview;mountPreview=null;return bridge(()=>window.dmsg.secretMountSave(p.session))},true)}>{w('review')}</button>{/if}
      </div>
    {/if}
    {#each mounts as mount}<article class={card}><strong>{mount.input.project_label}</strong><p class="break-all text-xs">{mount.input.mount_point}</p><p>{statuses[mount.id]?.state ?? 'unknown'}</p>{#if statuses[mount.id]?.error}<p class="text-xs text-warn">{statuses[mount.id].error}</p>{/if}<div class="flex flex-wrap gap-2"><button class={button} disabled={busy||statuses[mount.id]?.state!=='stopped'} onclick={()=>act(()=>call('secrets/mounts/start',{id:mount.id}))}>{w('start')}</button><button class={button} disabled={busy||statuses[mount.id]?.state==='stopped'} onclick={()=>act(()=>call('secrets/mounts/revoke_reads',{id:mount.id}))}>{w('revoke')}</button><button class={button} disabled={busy||statuses[mount.id]?.state==='stopped'} onclick={()=>act(()=>call('secrets/mounts/stop',{id:mount.id}))}>{w('stop')}</button><button class={button} disabled={busy} onclick={()=>act(()=>call('secrets/mounts/remove',{id:mount.id}))}>{t('remove')}</button></div>{#each statuses[mount.id]?.core?.grants??[] as grant}<p class="text-sm">{grant.entry} · {Math.ceil(grant.remaining_ms/1000)}s · {grant.released_bytes} bytes <button class={button} onclick={()=>open(grant.request_id)}>{w('review')}</button></p>{/each}</article>{/each}
  {:else if tab==='ssh'}
    <p class="text-sm text-muted">{w('local')}</p><code class="block break-all text-xs">SSH_AUTH_SOCK={ssh.socket??w('unavailable')}</code>{#if ssh.error}<Notice tone="warn">{ssh.error}</Notice>{/if}
    {#each keys as key}<article class={card}><strong>{key.title}</strong><p class="break-all font-mono text-xs">{key.fingerprint??key.reason}</p><pre class="whitespace-pre-wrap break-all text-xs">{key.public_key??''}</pre><div class="flex gap-2">{#each ['ssh_auth','git_sign'] as purpose}<button class={button} disabled={busy||!key.supported||!ssh.available} onclick={()=>act(()=>call('secrets/ssh/binding_request',{reference:key.reference,purposes:[purpose]}),true)}>{w('enable')} · {purpose}</button>{/each}</div></article>{/each}
    {#each bindings.filter((b)=>b.enabled) as binding}<article class={card}><p class="break-all text-xs">{binding.fingerprint} · {binding.purposes.join(', ')}</p><button class={button} disabled={busy} onclick={()=>act(()=>call('secrets/ssh/disable',{id:binding.id}))}>{w('disable')}</button></article>{/each}
  {:else if tab==='health'}
    {#each findings as finding}<article class={card}><strong>{finding.rule}</strong><p class="break-all text-xs">{finding.subject} · {finding.revision}</p><p class="text-xs">{finding.evidence} · {when(finding.observed_at)} · {finding.action}</p><button class={button} disabled={busy||finding.dismissed} onclick={()=>act(()=>call('secrets/health/dismiss',{id:finding.id}))}>{w('dismiss')}</button></article>{/each}
    <div class={card}><p class="text-sm">{w('scanHint')}</p><button class={button} disabled={busy||scan?.state==='running'} onclick={()=>act(async()=>{const result=await bridge(()=>window.dmsg.secretScanDirectory());if(alive && result){scan={id:result.scan_id,state:result.state};scanPicks=[]}})}>{w('scan')}</button>
      {#if scan}{#if scan.state==='running'}<button class={button} disabled={busy} onclick={()=>act(()=>call('secrets/discovery/cancel',{scan_id:scan.id}))}>{t('cancel')}</button>{/if}<p class="break-all text-xs">{scan.root} · {scan.state} · {scan.report?.files??0} files · {scan.report?.bytes??0} bytes</p>{#if scan.report && !scan.report.complete}<Notice tone="warn">{w('partial')}</Notice>{/if}{#each scan.report?.candidates??[] as candidate}<label class="block break-all text-sm"><input type="checkbox" bind:group={scanPicks} value={candidate.id} disabled={candidate.warnings.length>0} /> {candidate.relative_path} · {candidate.kind} · {candidate.variables.join(', ')} · {candidate.warnings.join(', ')}</label>{/each}<button class={primary} disabled={busy||!scanPicks.length} onclick={()=>act(()=>call('secrets/discovery/import_request',{scan_id:scan.id,candidate_ids:scanPicks}),true)}>{w('import')}</button>{/if}
    </div>
    <div class={card}><h3>{w('rotation')}</h3><p class="text-sm text-muted">{w('reported')}</p><label class="block text-xs">{w('item')}<select class={input} bind:value={rotationItem} onchange={()=>rotationField=''}><option value="">{w('choose')}</option>{#each items as item}<option value={item.id}>{item.title}</option>{/each}</select></label><label class="block text-xs">{w('field')}<select class={input} bind:value={rotationField}><option value="">{w('choose')}</option>{#each items.find((i)=>i.id===rotationItem)?.fields.filter((f:any)=>f.type==='concealed')??[] as field}<option value={field.id}>{field.label||field.id}</option>{/each}</select></label><button class={button} disabled={busy||!rotationField} onclick={()=>act(async()=>{const ref:any=await call('secrets/ref',{item_id:rotationItem,field_id:rotationField});if(alive)return call('secrets/rotation/create',{reference:ref.reference})})}>{w('start')}</button>
      {#each rotations as rotation}<div class="space-y-2 border-t border-line pt-3"><p class="break-all text-xs">{rotation.reference.item_id} / {rotation.reference.field_id}</p>{#each ['created','tested','references_checked','upstream_revoked'] as step}<button class={button} disabled={busy||rotation.completed_at||rotation[step]} onclick={()=>act(()=>call('secrets/rotation/update',{id:rotation.id,step}))}>{rotation[step]?'✓ ':''}{w(step)}</button>{/each}</div>{/each}
    </div>
  {:else if tab==='sharing'}
    <Notice tone="warn">{w('shareWarning')}</Notice>
    <div class={card}><label class="block text-xs">{w('item')}<select class={input} bind:value={itemId} onchange={selectItem}><option value="">{w('choose')}</option>{#each items as item}<option value={item.id}>{item.title}</option>{/each}</select></label>
      {#each chosen?.fields??[] as field}<label class="block text-sm"><input type="checkbox" bind:group={fields} value={field.id} onchange={()=>sharePreview=null} /> {field.label||field.id} · {field.type}</label>{/each}<label class="block text-sm"><input type="checkbox" bind:checked={notes} onchange={()=>sharePreview=null} /> {w('notes')}</label>
      {#each chosen?.files??[] as id}<label class="block break-all text-sm"><input type="checkbox" bind:group={attachments} value={id} disabled={!details[id]?.file?.current} onchange={()=>sharePreview=null} /> {details[id]?.file?.name??id} · {details[id]?.file?.current??w('unavailable')}</label>{/each}
      <label class="block text-xs">{w('hours')}<input class={input} type="number" min="1" max="168" bind:value={hours} onchange={()=>sharePreview=null} /></label><label class="block text-xs">{w('sessions')}<input class={input} type="number" min="1" max="100" bind:value={sessions} onchange={()=>sharePreview=null} /></label><label class="block text-xs">{w('password')}<input class={input} type="password" bind:value={password} autocomplete="new-password" onchange={()=>sharePreview=null} /></label>
      {#if chosen?.fields.some((f:any)=>fields.includes(f.id) && f.type==='otp')}<p class="text-warn">{w('otpSeedWarning')}</p>{/if}
      {#if chosen?.type==='ssh' && chosen.fields.some((f:any)=>fields.includes(f.id) && f.type==='concealed')}<p class="text-warn">{w('privateKeyWarning')}</p>{/if}
      {#if chosen?.type==='wallet' && (notes || chosen.fields.some((f:any)=>fields.includes(f.id) && f.type==='concealed'))}<p class="text-warn">{w('walletSeedWarning')}</p>{/if}
      <button class={primary} disabled={busy||!chosen||(!fields.length&&!notes&&!attachments.length)} onclick={prepareShare}>{w('prepareSnapshot')}</button>
      {#if sharePreview}<p class="text-sm">{sharePreview.plan.plain_bytes} bytes · ~{sharePreview.plan.estimated_ciphertext_bytes} encrypted bytes · {when(sharePreview.plan.expires_at)}</p><button class={primary} disabled={busy} onclick={()=>act(()=>call('secrets/shares/request',{id:sharePreview.id}),true)}>{w('review')}</button>{/if}
    </div>
    {#each shares as share}<article class={card}><strong>{share.plan.title}</strong><p class="text-sm">{share.state} · {when(share.plan.expires_at)}</p>{#if share.total_chunks}<p class="text-xs">{share.uploaded_chunks} / {share.total_chunks}</p>{/if}{#if share.error}<p class="font-mono text-xs">{share.error}</p>{/if}<div class="flex flex-wrap gap-2">{#if ['prepared','needs_approval','awaiting_user'].includes(share.state)}<button class={button} disabled={busy} onclick={()=>act(()=>call('secrets/shares/request',{id:share.id}),true)}>{w('review')}</button>{/if}{#if share.state==='published'}<button class={button} disabled={busy||share.cancel_requested} onclick={()=>act(async()=>{const result:any=await call('secrets/shares/get',{id:share.id,include_link:true});if(alive){if(!result.link?.url)throw Error(w('linkUnavailable'));await navigator.clipboard.writeText(result.link.url)}})}>{w('copy')}</button>{/if}{#if ['unknown','revoke_pending','published'].includes(share.state)}<button class={button} disabled={busy} onclick={()=>act(()=>call('secrets/shares/reconcile',{id:share.id}))}>{w('reconcile')}</button>{/if}<button class={button} disabled={busy||['cancelled','revoked'].includes(share.state)} onclick={()=>act(()=>call('secrets/shares/revoke',{id:share.id}))}>{w('revokeShare')}</button></div></article>{/each}
  {:else if tab==='receive'}
    <Notice>{w('sessionNotice')}</Notice><p class="text-sm text-muted">{w('localFiles')}</p><p class="text-sm text-warn">{w('filesPermissions')}</p><label class="block text-xs">{w('link')}<input class={input} type="password" bind:value={link} autocomplete="off" /></label><label class="block text-xs">{w('password')}<input class={input} type="password" bind:value={receivePassword} autocomplete="off" /></label><button class={primary} disabled={busy||!link} onclick={()=>act(async()=>{const params={link,...(receivePassword?{password:receivePassword}:{})};link='';receivePassword='';received=null;receiveFields=[];receiveAttachments=[];receiveNotes=false;const result=await call('secrets/packages/import_preview',params);if(alive)received=result})}>{w('preview')}</button>
    {#if received}<div class="flex items-center gap-2 text-xs"><span>{w('destination')}: {receiveFolder==='root'?t('library'):folders.find((f)=>f.id===receiveFolder)?.name??receiveFolder}</span><button class={button} disabled={busy} onclick={()=>act(async()=>{const tree:any[]=await call('files/tree');if(alive){folders=tree;folderPicker=true}})}>{w('choose')}</button></div><article class={card}><strong>{received.preview.title}</strong>{#each received.preview.fields as field}<label class="block text-sm"><input type="checkbox" bind:group={receiveFields} value={field.id} /> {field.label||field.id} · {field.type}</label>{/each}{#if received.preview.notes}<label class="block text-sm"><input type="checkbox" bind:checked={receiveNotes} />{w('notes')}</label>{/if}{#each received.preview.attachments as attachment,i}<label class="block text-sm"><input type="checkbox" bind:group={receiveAttachments} value={i} />{attachment.name} · {attachment.size} bytes</label>{/each}<button class={primary} disabled={busy||(!receiveFields.length&&!receiveNotes&&!receiveAttachments.length)} onclick={()=>act(()=>call('secrets/packages/import_request',{selection:{package_id:received.package_id,digest:received.digest,fields:receiveFields,include_notes:receiveNotes,attachments:receiveAttachments,folder:receiveFolder}}),true)}>{w('import')}</button></article>{/if}
  {:else}
    <p class="text-sm text-muted">{w('activityHint')}</p>{#each activity as request}<article class={card}><p>{request.type} · {stateLabel(request.state)}</p><p class="text-xs">{when(request.updated_at??request.created_at)} · {request.error??''}</p><button class={button} onclick={()=>open(request.id)}>{w('review')}</button>{#if request.state==='awaiting_user'}<button class={button} disabled={busy} onclick={()=>act(()=>call('approval/cancel',{id:request.id}))}>{t('cancel')}</button>{/if}</article>{/each}
  {/if}
</section>

<FolderPicker bind:open={folderPicker} {folders} title={w('destination')} description={w('filesPermissions')} action={w('choose')} onPick={(folder)=>receiveFolder=folder} />
