<script lang="ts">
  import { onDestroy, onMount, untrack } from 'svelte'
  import Dialog from '../../components/Dialog.svelte'
  import SecretInput from '../../components/SecretInput.svelte'
  import Checkbox from '../../components/Checkbox.svelte'
  import Select from '../../components/Select.svelte'
  import Progress from '../../components/Progress.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import { buttonClass } from '../../components/ui'
  import { t, locale } from '../../i18n/index.svelte'
  import { bridge, call, code, errorText, onChange, size } from '../../rpc'
  import { typeLabel, typeOf } from './vault'

  interface Inventory {
    at: number
    inventory: {
      items: { id: string; title: string; kind: string; files: string[]; synced: boolean; save_state?: string }[]
      files: Record<string, { available: boolean; meta: { size: number } } | null>
      trash: number
      managed: number
      conflicts: number
    }
  }

  interface Preview {
    digest: string
    source: string
    account: string
    at: number
    omitted: number
    items: { index: number; title: string; kind: string; files: (number | null)[]; status: 'new' | 'same' | 'changed' | 'deleted' }[]
  }

  interface Summary { items: number; files: number; omitted: number; skipped?: number; name?: string }

  interface Replies {
    begin: string | null
    inventory: Inventory
    password: string
    request: { id: string }
    export: Summary
    pick: string | null
    inspect: Preview
    commit: Summary
    save: Summary | null
    status: { receipt?: Summary; bytes?: number; items?: number }
    cancel: null
  }

  let { open = $bindable(false), account }: { open?: boolean; account: string } = $props()
  let step = $state<'home'|'export'|'import'|'review'|'waiting'|'ready'|'done'>('home')
  let id = '', epoch = 0
  let password = $state(''), confirmation = $state(''), file = $state(''), approval = $state('')
  let busy = $state(false), error = $state(''), attachments = $state(true), folder = $state('root')
  let processed = $state(0), processedItems = $state(0)
  let folders = $state<{value:string;label:string}[]>([])
  let list = $state<Inventory | null>(null), review = $state<Preview | null>(null), result = $state<Summary | null>(null)
  let selected = $state<string[]>([]), picks = $state<number[]>([])
  const chosen = $derived(list?.inventory.items.filter(row => selected.includes(row.id)) ?? [])
  const fileIds = $derived([...new Set<string>(chosen.flatMap(row => row.files))])
  const unavailable = $derived(fileIds.some(f => !list?.inventory.files[f]?.available))
  const bytes = $derived(fileIds.reduce((n,f)=>n+(list?.inventory.files[f]?.meta.size ?? 0),0))
  const pickedFiles = $derived(new Set(review?.items.filter(r=>picks.includes(r.index)).flatMap(r=>r.files.filter(f=>f!==null)) ?? []).size)
  const normalized = $derived(password.normalize('NFC'))
  const valid = $derived([...new Intl.Segmenter(undefined,{granularity:'grapheme'}).segment(normalized)].length >= 6 && normalized === confirmation.normalize('NFC') && new TextEncoder().encode(normalized).length <= 256)
  const receiptKey = $derived('dmsg-vault-backup-receipt-'+account)
  function rpc<K extends keyof Replies>(action: K, params: Record<string, unknown> = {}) {
    return bridge<Replies[K]>(()=>window.dmsg.vaultBackup(action,{ id,...params }))
  }
  function reset() {
    ++epoch
    if (id) void rpc('cancel').catch(()=>{})
    id=''; password=''; confirmation=''; approval=''; file=''; list=review=result=null
    selected=[]; picks=[]; folders=[]; folder='root'; attachments=true; step='home'; busy=false; error=''
  }
  $effect(()=>{ if (!open) untrack(reset) })
  onDestroy(reset)
  async function work(job:(current:()=>boolean)=>Promise<void>) {
    if (busy) return
    const version=epoch
    const current=()=>version===epoch && open
    busy=true; error=''; processed=processedItems=0
    try { await job(current) } catch(e) { if(current()) error=errorText(e) }
    finally { if(current()) busy=false }
  }
  async function start(mode:'export'|'import') {
    await work(async current=>{
      const token=await rpc('begin',{import:mode==='import'})
      if (!current()) { if (token) void rpc('cancel',{id:token}).catch(()=>{}); return }
      if (!token) return
      id=token
      if(mode==='export') {
        const value=await rpc('inventory')
        if(!current()) return
        list=value; selected=value.inventory.items.map(r=>r.id)
      }
      step=mode
    })
  }
  function choose() {
    return work(async current=>{
      const path=await rpc('pick')
      if(current()) { file=path ?? ''; review=null; picks=[] }
    })
  }
  function ask() {
    return work(async current=>{
      const r=await rpc('request',{items:[...selected],attachments,password,confirmation})
      if(!current()) return
      approval=r.id; step='waiting'; password=confirmation=''
      await bridge(()=>window.dmsg.openApproval(r.id))
    })
  }
  async function approved() {
    if(step!=='waiting'||busy||!open) return
    await work(async current=>{
      const r=await call('approval/show',{id:approval})
      if(!current()) return
      if(r.state==='awaiting_user'||r.state==='executing') return
      if(r.state!=='done') {
        const next=await rpc('inventory')
        if(current()) {list=next; step='export'; error=errorText(r.error ?? r.state)}
        return
      }
      try {
        const saved=await rpc('export',{approval})
        if(current()) { result=saved; step='ready' }
      } catch(e) {
        if(current()) {step='home'; error=errorText(e)}
      }
    })
  }
  onMount(()=>{
    const changed=onChange(['approval/changed'],()=>void approved())
    const timer=setInterval(()=>{
      if(step==='waiting'&&!busy) void approved()
      else if(open && busy && id) {
        const ticket=id
        void rpc('status').then(v=>{if(id===ticket && busy){processed=v.bytes??0;processedItems=v.items??0}}).catch(()=>{})
      }
    },1000)
    return ()=>{ changed(); clearInterval(timer) }
  })
  function inspect() {
    return work(async current=>{
      let preview
      try { preview=await rpc('inspect',{password}) }
      catch(e) { if(current() && code(e)==='INTEGRITY_FAILED') {password=''; error=t('vaultBackupUnlockError'); return} throw e }
      const tree = await call<{ id: string; name: string; path?: string[] }[]>('files/tree')
      if(!current()) return
      folders=tree.map(f=>({value:f.id,label:f.path?.join(' / ') || f.name}))
      review=preview; picks=preview.items.filter(r=>r.status==='new').map(r=>r.index); password=''; step='review'
    })
  }
  function commit() {
    return work(async current=>{
      if (!review) return
      localStorage.setItem(receiptKey,id)
      const saved=await rpc('commit',{digest:review.digest,items:[...picks],folder})
      if(current()) {result=saved;step='done';localStorage.removeItem(receiptKey)}
    })
  }
  function reconcile() {
    return work(async current=>{
      const token=localStorage.getItem(receiptKey)
      if(!token) return
      const status=await rpc('status',{id:token})
      if(current()) {
        if (status.receipt) {result=status.receipt; step='done'}
        else {error=t('vaultBackupUncommitted')}
        localStorage.removeItem(receiptKey)
      }
    })
  }
  function save() {
    return work(async current=>{
      const saved=await rpc('save')
      if(current() && saved) {result=saved;step='done'}
    })
  }
</script>

<Dialog bind:open title={t('vaultBackupTitle')} wide testid="vault-backup">
  <div class="grid gap-3 text-[13px]">
    {#if account}<p class="break-all text-muted">{t('account')} · <bdi>{account}</bdi></p>{/if}
    {#if step==='home'}
      <div class="flex flex-wrap gap-2">
        <button class={buttonClass('primary')} disabled={busy} data-testid="backup-export-start" onclick={()=>start('export')}>{t('vaultBackupExport')}</button>
        <button class={buttonClass('secondary')} disabled={busy} data-testid="backup-import-start" onclick={()=>start('import')}>{t('vaultBackupImport')}</button>
        {#if localStorage.getItem(receiptKey)}<button class={buttonClass('secondary')} disabled={busy} onclick={reconcile}>{t('confirmCheckStatus')}</button>{/if}
      </div>
    {:else if step==='export' && list}
      <p>{t('vaultBackupScope')}</p>
      <p class="text-muted">{new Date(list.at).toLocaleString(locale())} · {t('trash')} {list.inventory.trash} · {t('conflictBanner',{n:list.inventory.conflicts})} · {t('managedByDmsg')} {list.inventory.managed}</p>
      <div class="flex flex-wrap gap-2"><button class={buttonClass('secondary','xs')} disabled={busy} onclick={()=>selected=list?.inventory.items.map(r=>r.id) ?? []}>{t('selectAll')}</button><button class={buttonClass('ghost','xs')} disabled={busy} onclick={()=>selected=[]}>{t('clearSelection')}</button></div>
      <div class="grid max-h-48 gap-1 overflow-y-auto">
        {#each list.inventory.items as row (row.id)}
          <Checkbox checked={selected.includes(row.id)} disabled={busy} onCheckedChange={yes=>selected=yes?[...selected,row.id]:selected.filter(v=>v!==row.id)}><span class="break-all">{row.title}</span> · {t(typeLabel[typeOf({type:row.kind})])}{row.save_state === 'local_only' ? ' · '+t('savedOnDevice') : (row.save_state === 'pending' || (!row.save_state && !row.synced)) ? ' · '+t('pendingSync',{n:1}) : ''}</Checkbox>
        {/each}
      </div>
      {#if fileIds.length}<Checkbox bind:checked={attachments} disabled={busy}>{t('vaultBackupFiles')} ({fileIds.length}, {size(bytes)})</Checkbox>{/if}
      {#if unavailable && attachments}<Notice tone="warn">{t('vaultBackupUnavailable')}</Notice>{/if}
      {#if chosen.some(r=>r.kind==='wallet')}<Notice>{t('vaultBackupWallet')}</Notice>{/if}
      <SecretInput label={t('vaultBackupPassword')} hint={t('vaultBackupHint')} bind:value={password} disabled={busy} testid="backup-password"/>
      <button class={buttonClass('ghost','xs','justify-self-start')} disabled={busy} onclick={()=>work(async current=>{const value=await rpc('password');if(current())password=value})}>{t('generate')}</button>
      <SecretInput label={t('vaultBackupRepeat')} bind:value={confirmation} disabled={busy} testid="backup-confirmation"
        hint={confirmation && confirmation.normalize('NFC') !== normalized ? t('vaultBackupMismatch') : undefined}/>
    {:else if step==='waiting'}
      <p>{t('experience.awaiting')}</p><button class={buttonClass('secondary')} disabled={busy} onclick={()=>bridge(()=>window.dmsg.openApproval(approval))}>{t('review')}</button>
    {:else if step==='import'}
      <button class={buttonClass('secondary')} disabled={busy} data-testid="backup-pick" onclick={choose}>{t('chooseFile')}</button>
      {#if file}<p class="break-all">{file}</p><SecretInput label={t('vaultBackupPassword')} testid="backup-import-password" bind:value={password} disabled={busy}/><button class={buttonClass('primary')} disabled={busy||!password} data-testid="backup-inspect" onclick={inspect}>{t('review')}</button>{/if}
    {:else if step==='review' && review}
      <p>{t('experience.source')} · <bdi>{review.source}</bdi> · {new Date(review.at).toLocaleString(locale())}</p>
      <p>{t('vaultBackupNew')} {review.items.filter(r=>r.status==='new').length} · {t('vaultBackupSame')} {review.items.filter(r=>r.status==='same').length} · {t('review')} {review.items.filter(r=>r.status==='changed'||r.status==='deleted').length}</p>
      <p>{t('vaultBackupOmitted')} · {review.omitted}</p>
      <div class="flex gap-2"><button class={buttonClass('secondary','xs')} disabled={busy} onclick={()=>picks=review?.items.filter(r=>r.status==='new').map(r=>r.index) ?? []}>{t('vaultBackupNew')}</button><button class={buttonClass('ghost','xs')} disabled={busy} onclick={()=>picks=[]}>{t('clearSelection')}</button></div>
      <div class="grid max-h-64 gap-1 overflow-y-auto">
        {#each review.items as row (row.index)}
          {@const omitted = row.files.filter(file => file === null).length}
          <Checkbox disabled={busy||row.status==='same'} checked={picks.includes(row.index)} onCheckedChange={on=>picks=on?[...picks,row.index]:picks.filter(i=>i!==row.index)}>
            <span class="break-all">{row.title}</span> · {t(typeLabel[typeOf({type:row.kind})])} · {t('attachments')} {row.files.length - omitted}
            {#if omitted} · {t('vaultBackupOmitted')} {omitted}{/if}
            · {t(row.status==='same'?'vaultBackupSame':row.status==='changed'?'vaultBackupChanged':row.status==='deleted'?'vaultBackupDeleted':'vaultBackupNew')}
          </Checkbox>
        {/each}
      </div>
      {#if pickedFiles}<Select bind:value={folder} label={t('experience.destination')} options={[{value:'root',label:t('files')},...folders.filter(f=>f.value!=='root')]} disabled={busy}/>{/if}
    {:else if result}
      <p>{step==='done'?t('vaultBackupSaved'):t('vaultBackupExport')} · {t('vaultBackupItems')} {result.items} · {t('attachments')} {result.files} · {t('vaultBackupOmitted')} {result.omitted}</p>
      {#if result.name}<p class="break-all">{result.name}</p>{/if}
      {#if result.skipped !== undefined}<p>{t('vaultBackupSync')}</p>{/if}
      {#if step==='ready'}<button class={buttonClass('primary')} disabled={busy} data-testid="backup-save" onclick={save}>{t('save')}</button>{/if}
    {/if}
    {#if busy}<Progress label={t('loading')+(processedItems?' · '+processedItems:'')+(processed?' · '+size(processed):'')}/>{/if}
    {#if error}<Notice tone="danger">{error}</Notice>{/if}
  </div>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} onclick={()=>open=false}>{step==='done'?t('done'):t('cancel')}</button>
    {#if step==='export' && list}
      <button class={buttonClass('primary','md','justify-self-start')} disabled={busy||!valid||!chosen.length||(attachments&&unavailable)} data-testid="backup-request" onclick={ask}>{t('vaultBackupExport')} ({chosen.length})</button>
    {:else if step==='review' && review}
      <button class={buttonClass('primary')} disabled={busy} data-testid="backup-commit" onclick={()=>picks.length?commit():(open=false)}>{picks.length?t('vaultBackupImport')+' ('+picks.length+' · '+t('attachments')+' '+pickedFiles+')':t('done')}</button>
    {/if}
  {/snippet}
</Dialog>
