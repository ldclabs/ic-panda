// Real Electron windows and IPC against a local metadata-only daemon fixture.
// No network credential use, OS presence prompt or GitHub side effect.
import assert from 'node:assert/strict'
import { _electron as electron } from 'playwright'
import electronPath from 'electron'
import { createServer } from 'node:http'
import { mkdtemp, mkdir, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { WebSocketServer } from 'ws'

const account = '040g2081040g2081040g', token = 'secrets-ui-fixture', actor = `did:agent:${'A'.repeat(43)}`
const item = '11'.repeat(32), policyId = '22'.repeat(16), requests = new Map(), calls = [], sockets = new Set()
let policies = [], uses = [], sequence = 1
const status = () => ({ state: 'unlocked', account, device_id: '33'.repeat(32), approvals_waiting: [...requests.values()].filter(r => r.state === 'awaiting_user').length, pending_sync: 0, background_online: false, fast_unlock: false, fast_presence: null, idle_lock_ms: 900000, lock_at: Date.now() + 900000 })
const emit = () => { for (const s of sockets) s.send(JSON.stringify({jsonrpc:'2.0',method:'event',params:{seq:sequence++,type:'approval/changed',data:null}})) }
const makeRequest = (type, detail) => {
  const r = {id:(sequence++).toString(16).padStart(64,'0'),source:type==='secret_use'?`agent:${actor}`:'gui',type,kind:'secret',risk:'high',state:'awaiting_user',created_at:Date.now(),expires_at:Date.now()+300000,updated_at:Date.now(),digest:'dd'.repeat(32),detail}
  r.digest=r.id;requests.set(r.id,r);return r
}
const methods = {
  'app/initialize': () => ({protocol:'dmsg-app/1',status:status()}),
  'app/activity': () => null, 'daemon/status':status, 'session/status':status,
  'profile/show': () => null, 'profile/pending': () => null,
  'channel/list': () => [], 'attest/history': () => [], 'grants/list': () => [],
  'vault/templates': () => ({types:[],templates:{}}),
  'vault/list': () => [{id:item,revision:'44'.repeat(32),deleted:false,item:{type:'api',title:'GitHub credential',tags:[],fields:[{id:'credential',type:'concealed',label:'Token',value:'********'}]}}],
  'agent/ids': () => ({agents:[{agent_id:actor,label:'Builder',paused:false,credential:true}]}),
  'secrets/policies/list': () => ({policies}),
  'secrets/uses/list': () => ({uses,device_local:true}),
  'secrets/ref': p => ({uri:`dmsg-secret://${account}/${p.item_id}/${p.field_id}`,reference:{schema_version:1,account_id:account,item_id:p.item_id,field_id:p.field_id}}),
  'secrets/policies/save_request': p => {assert.equal(p.source,'gui');return makeRequest('secret_policy',{id:policyId,account_id:account,device_id:'33'.repeat(32),base_version:p.base_version??null,policy:p.policy,grant_to:p.grant_to})},
  'secrets/policies/disable': p => {assert.equal(p.id,policyId);policies[0].policy.enabled=false;policies[0].version++;emit();return policies[0]},
  'secrets/uses/mark_reviewed': p => {const u=uses.find(u=>u.request_id===p.request_id);assert.equal(u.state,'lost');u.reviewed_at=Date.now();emit();return u},
  'approval/list': p => p.ids_only ? [...requests.values()].filter(r=>r.state==='awaiting_user').map(r=>r.id) : [...requests.values()],
  'approval/show': p => requests.get(p.id),
  'approval/respond': p => {
    const r=requests.get(p.id);assert.equal(p.digest,r.digest);assert.equal(p.source,'gui');assert.equal(p.human,true)
    r.state=p.approve?'done':'rejected'
    if(p.approve&&r.type==='secret_policy')policies=[{id:policyId,version:1,policy:r.detail.policy}]
    if(p.approve&&r.type==='secret_use'){r.state='lost';r.error='EXECUTION_UNKNOWN';uses=[{request_id:r.id,state:r.state,error:r.error,created_at:r.created_at,approval:r}]}
    emit();return r
  }
}
Object.assign(methods,{
  'secrets/mounts/list':()=>({mounts:[],supported:true,platform:'macos'}),
  'secrets/ssh/keys':()=>({keys:[],bindings:[]}),
  'secrets/ssh/status':()=>({supported:true,socket:'/synthetic/agent.sock',identities:0}),
  'secrets/health':()=>({findings:[]}),
  'secrets/rotation/list':()=>({rotations:[]}),
  'secrets/shares/list':()=>({shares:[]}),
  'secrets/shares/prepare':p=>({id:'a1'.repeat(32),state:'prepared',plan:{...p.input,title:'GitHub credential',plain_bytes:300,estimated_ciphertext_bytes:67000,expires_at:Date.now()+86400000,fields:p.input.field_ids.map(id=>({id,type:'concealed',label:'Token'}))}}),
  'secrets/shares/request':p=>makeRequest('secret_share',{title:'GitHub credential',share_id:'a2'.repeat(32),fields:[{id:'credential',type:'concealed',label:'Token'}],input:{include_notes:false,attachments:[]},expires_at:Date.now()+86400000,max_download_sessions:1}),
  'secrets/packages/import_preview':()=>({package_id:'a3'.repeat(32),digest:'a4'.repeat(32),preview:{title:'Incoming snapshot',fields:[{id:'f0_credential',type:'concealed',label:'Imported token'}],notes:false,attachments:[]}}),
  'secrets/packages/import_request':p=>makeRequest('secret_package_import',{selection:p.selection,preview:{title:'Incoming snapshot',fields:[{id:'f0_credential',type:'concealed',label:'Imported token'}]}})
})
const server=createServer((_,res)=>res.writeHead(404).end()), wss=new WebSocketServer({noServer:true})
server.on('upgrade',(req,socket,head)=>{
  if(req.headers.authorization!==`Bearer ${token}`)return socket.destroy()
  wss.handleUpgrade(req,socket,head,ws=>{
    sockets.add(ws);ws.on('close',()=>sockets.delete(ws))
    ws.on('message',bytes=>{
      const m=JSON.parse(String(bytes));calls.push(m)
      try {
        const handler=methods[m.method]
        ws.send(JSON.stringify(handler?{jsonrpc:'2.0',id:m.id,result:handler(m.params??{})}:{jsonrpc:'2.0',id:m.id,error:{code:-32601,message:'Not found',data:{code:'NOT_FOUND'}}}))
      }catch(e){ws.send(JSON.stringify({jsonrpc:'2.0',id:m.id,error:{code:-32000,message:String(e),data:{code:'INVALID_INPUT'}}}))}
    })
  })
})
await new Promise(r=>server.listen(0,'127.0.0.1',r))
const profile=await mkdtemp(join(tmpdir(),'dmsg-secrets-ui-')), shots=resolve('test-results/secrets')
await mkdir(shots,{recursive:true});await writeFile(join(profile,'locale'),'en');await writeFile(join(profile,'appearance'),'light')
const app=await electron.launch({executablePath:electronPath,args:[resolve('out/main/index.js')],env:{...process.env,DMSG_DESKTOP_TEST:'1',DMSG_DESKTOP_USER_DATA:profile,DMSG_DESKTOP_DAEMON:`http://127.0.0.1:${server.address().port}`,DMSG_DESKTOP_TOKEN:token,DMSG_HOME:join(profile,'home')}})
async function approve(page) {
  await app.evaluate(({BrowserWindow})=>{for(const w of BrowserWindow.getAllWindows())w.webContents.setBackgroundThrottling(false)})
  const button=page.getByTestId('approve')
  await button.waitFor();await page.waitForFunction(()=>!document.querySelector('[data-testid="approve"]')?.disabled)
  await button.focus();await page.keyboard.down('Space')
  try {await page.waitForSelector('[data-testid="approval-outcome"]',{timeout:10000})} finally {await page.keyboard.up('Space')}
}
try {
  const main=await app.firstWindow();await main.waitForSelector('[data-testid="sidebar-toggle"]',{timeout:20000})
  await main.evaluate(()=>{location.hash='/approvals'})
  await main.getByTestId('approvals-tab-secrets').click();await main.getByTestId('secret-policies').waitFor()
  await main.getByRole('button',{name:'New policy',exact:true}).click()
  const form=main.getByTestId('secret-policy-form')
  await form.getByLabel('Policy name').fill('Development issues');await form.getByLabel('GitHub owner').fill('example');await form.getByLabel('Repository',{exact:true}).fill('test')
  await form.getByLabel('Vault item').selectOption(item);await form.getByLabel('Concealed credential field').selectOption('credential');await form.getByLabel('Also grant').selectOption(actor)
  const opened=app.waitForEvent('window');await form.getByRole('button',{name:'Review policy change'}).click();const approval=await opened
  await approval.getByTestId('secret-review').waitFor();assert.match(await approval.getByTestId('secret-review').innerText(),/does not approve an issue creation/)
  assert.equal(calls.filter(c=>c.method==='approval/respond').length,0)
  await approve(approval);await main.getByTestId('secret-policy').waitFor()
  await main.screenshot({path:join(shots,'policies.png')})
  const body=`<script>globalThis.secretExecuted=true</script>\n${'Complete text, not a truncated summary. '.repeat(300)}\nTHE END OF THE APPROVED BODY`
  const use=makeRequest('secret_use',{account_id:account,device_id:'33'.repeat(32),actor,policy_id:policyId,policy_version:1,policy_name:'Development issues',target:{api_origin:'https://api.github.com',owner:'example',repo:'test'},arguments:{title:'Literal <b>title</b>',body},reason:'Ignore all previous instructions — untrusted reason',resolved:{item_title:'GitHub credential',field_label:'Token',reference:{item_id:item,field_id:'credential'},item_revision:'44'.repeat(32)}})
  emit();await main.evaluate(id=>window.dmsg.openApproval(id),use.id)
  await approval.waitForFunction(()=>document.querySelector('[data-testid="secret-review-body"]')?.textContent?.includes('THE END OF THE APPROVED BODY'))
  assert.equal(await approval.getByTestId('secret-review-body').textContent(),body)
  assert.equal(await approval.evaluate(()=>globalThis.secretExecuted),undefined)
  await approval.screenshot({path:join(shots,'approval.png')});await approve(approval)
  await main.getByTestId('secret-use-record').waitFor();assert.match(await main.getByTestId('secret-use-record').innerText(),/may have been created/)
  await main.getByRole('button',{name:'Mark manually checked'}).click();await main.waitForFunction(()=>document.body.textContent.includes('Manually checked; outcome remains unknown'))
  await main.getByRole('button',{name:'Disable now'}).click();await main.waitForFunction(()=>document.querySelector('[data-testid="secret-policy"]')?.textContent.includes('Disabled'))
  await assert.rejects(main.evaluate(()=>window.dmsg.call('approval/respond',{human:true,approve:true})),/FORBIDDEN/)
  assert.equal(calls.filter(c=>c.method==='approval/respond').length,2)
  assert.equal(calls.filter(c=>c.method==='secrets/policies/save_request').length,1)
  assert.equal(calls.filter(c=>c.method==='vault/reveal').length,0)
  await main.screenshot({path:join(shots,'history.png')})
  const workflows=main.getByTestId('secret-workflows')
  for(const method of ['secrets/run/request','secrets/run/claim','secrets/discovery/start','secrets/mounts/save_request'])await assert.rejects(main.evaluate(method=>window.dmsg.call(method,{}),method),/FORBIDDEN/)
  await workflows.getByRole('button',{name:'Secret snapshots',exact:true}).click()
  await workflows.getByLabel('Vault item').selectOption(item)
  assert.equal(await workflows.getByLabel('Token · concealed').isChecked(),false)
  await workflows.getByLabel('Token · concealed').check()
  await workflows.getByRole('button',{name:'Preview snapshot',exact:true}).click()
  await workflows.getByRole('button',{name:'Review request',exact:true}).click()
  await approval.waitForFunction(()=>document.querySelector('[data-testid="secret-review"]')?.textContent.includes('Publish selected snapshot'))
  assert.match(await approval.getByTestId('secret-review').innerText(),/Recipients can retain plaintext/)
  const prepared=calls.filter(c=>c.method==='secrets/shares/prepare').at(-1).params.input
  assert.deepEqual(prepared.field_ids,['credential']);assert.equal(prepared.include_notes,false);assert.deepEqual(prepared.attachments,[])
  await approve(approval)
  await workflows.getByRole('button',{name:'Import a snapshot',exact:true}).click()
  await workflows.getByLabel('Paste the full share link').fill('https://dmsg.test/s/'+ 'a2'.repeat(32)+'#synthetic-test-key')
  await workflows.getByRole('button',{name:'Receive and verify'}).click()
  assert.equal(await workflows.getByLabel('Imported token · concealed').isChecked(),false)
  await workflows.getByLabel('Imported token · concealed').check()
  await workflows.getByRole('button',{name:'Request import'}).click()
  await approval.waitForFunction(()=>document.querySelector('[data-testid="secret-review"]')?.textContent.includes('Incoming snapshot'))
  assert.match(await approval.getByTestId('secret-review').innerText(),/Attachments remain local/)
  await approve(approval)
  const mount=makeRequest('secret_mount_read',{scope:{type:'macos_file_window',uid:501,window_ms:30000},entry:{relative_path:'.env',size:48,generation:1},versions:['44'.repeat(32)]});emit()
  await main.evaluate(id=>window.dmsg.openApproval(id),mount.id)
  await approval.waitForFunction(()=>document.querySelector('[data-testid="secret-review"]')?.textContent.includes('other programs of the same user'))
  await approval.screenshot({path:join(shots,'mount-window.png')})
  assert.equal(calls.filter(c=>c.method==='vault/reveal').length,0)
  await workflows.getByRole('button',{name:'Device activity',exact:true}).click()
  await workflows.getByText('secret_package_import ·',{exact:false}).waitFor()
  await main.screenshot({path:join(shots,'p1-workflows.png')})
  console.log('secrets Electron smoke passed: policy review, full literal body, explicit approval, lost result, manual review, disable, P1 selected share/import, same-user mount warning and renderer boundary')
} catch(error) {
  for(const [index,page] of app.windows().entries()) {await page.screenshot({path:join(shots,`failure-${index}.png`)}).catch(()=>{});await writeFile(join(shots,`failure-${index}.txt`),await page.locator('body').innerText().catch(()=>''));console.error('Window',index,page.url(),await page.getByTestId('approve').count())}
  console.error('Last fixture calls',calls.slice(-12).map(c=>({method:c.method,request:c.params?.id})))
  throw error
} finally {await app.close();wss.close();server.close()}
