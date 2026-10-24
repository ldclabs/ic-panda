<script lang="ts">
  import { onMount } from 'svelte'
  import FolderPicker from '../files/FolderPicker.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import { buttonClass } from '../../components/ui'
  import { call, errorText } from '../../rpc'
  import { workflowText as w } from './workflow-text'
  let { received, open }: { received:any; open:(id:string)=>void } = $props()
  let fields=$state<string[]>([]), attachments=$state<number[]>([]), notes=$state(false), folder=$state('root'), folders=$state<any[]>([]), picker=$state(false), busy=$state(false), error=$state('')
  let alive=true
  onMount(()=>()=>{alive=false})
  async function choose(){busy=true;try{const tree:any[]=await call('files/tree');if(alive){folders=tree;picker=true}}catch(e){if(alive)error=errorText(e)}finally{if(alive)busy=false}}
  async function submit(){busy=true;error='';try{const result:any=await call('secrets/packages/import_request',{selection:{package_id:received.package_id,digest:received.digest,fields,include_notes:notes,attachments,folder}});if(alive){if(result.id)open(result.id);else if(result.state==='done')error=w('alreadyImported')}}catch(e){if(alive)error=errorText(e)}finally{if(alive)busy=false}}
</script>
<div class="space-y-3" data-testid="channel-snapshot-import">
  <Notice tone="warn">{w('localFiles')} {w('filesPermissions')}</Notice>
  {#if error}<Notice tone="danger">{error}</Notice>{/if}
  <strong>{received.preview.title}</strong>
  <p class="break-all text-xs">{received.channel_source?.channel_id} · #{received.channel_source?.seq}</p>
  {#each received.preview.fields as field}<label class="block text-sm"><input type="checkbox" bind:group={fields} value={field.id} /> {field.label||field.id} · {field.type}</label>{/each}
  {#if received.preview.notes}<label class="block text-sm"><input type="checkbox" bind:checked={notes} /> {w('notes')}</label>{/if}
  {#each received.preview.attachments as attachment,i}<label class="block text-sm"><input type="checkbox" bind:group={attachments} value={i} /> {attachment.name} · {attachment.size} bytes</label>{/each}
  <p class="text-xs">{w('destination')}: {folders.find(f=>f.id===folder)?.name??folder}</p><button class={buttonClass('secondary','sm')} disabled={busy} onclick={choose}>{w('choose')}</button>
  <button class={buttonClass('primary','sm')} disabled={busy||(!fields.length&&!notes&&!attachments.length)} onclick={submit}>{w('import')}</button>
</div>
<FolderPicker bind:open={picker} {folders} title={w('destination')} description={w('filesPermissions')} action={w('choose')} onPick={id=>folder=id} />
