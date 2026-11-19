// Real Electron windows and trusted picker IPC with synthetic owner RPC responses.
import assert from 'node:assert/strict'
import { join } from 'node:path'

export async function vaultBackupFlow({ app, main, methods, requests, request, account, emit, formMatrix, shots, answer, picks, latest }) {
  const token = '42'.repeat(32), row = '21'.repeat(32), approvalId = '37'.repeat(32)
  const title = '测试钱包 · Wallet de démonstration avec un nom très long'
  const summary = { items: 1, files: 0, omitted: 0 }
  const target = join(picks, 'portable.dmsgvault')
  Object.assign(methods, {
    'vault/backup_begin': () => token,
    'vault/backup_cancel': () => null,
    'vault/backup_status': () => ({state:'active',bytes:0,items:0}),
    'vault/backup_inventory': () => ({id:token,account,at:Date.now(),inventory:{
      items:[{id:row,title,kind:'wallet',synced:false,files:[]}],files:{},managed:1,trash:2,conflicts:1
    }}),
    'vault/backup_request': p => {
      assert.deepEqual(p.items,[row])
      assert.equal(p.password,p.confirmation)
      const r={...request,id:approvalId,source:'gui',kind:'account',type:'vault_backup',state:'awaiting_user',risk:'high',expires_at:Date.now()+600000,detail:{plan:token,...summary,wallet:true}}
      requests.set(r.id,r)
      emit('approval/changed')
      return r
    },
    'vault/backup_export': p => { assert.equal(requests.get(p.approval).state,'done'); return summary },
    'vault/backup_save': p => { assert.equal(p.path,target); return summary },
    'vault/backup_inspect': p => {
      assert.equal(p.path,target)
      return {id:token,digest:'fixture',source:'040g2081040g2081040g',account,at:Date.now(),omitted:1,files:0,items:[
        {index:0,title,kind:'wallet',files:[null],status:'new'},
        {index:1,title:'Mail',kind:'login',files:[],status:'same'},
        {index:2,title:'Previous password',kind:'login',files:[],status:'changed'},
        {index:3,title:'Removed entry',kind:'login',files:[],status:'deleted'}]}
    },
    'vault/backup_commit': p=>{assert.deepEqual(p.items,[0]);return {...summary,omitted:1,skipped:3}}
  })
  const modal=main.getByTestId('vault-backup')
  async function start(mode) {
    await main.getByTestId('vault-tools-menu').click()
    await main.getByTestId('vault-backup-open').click()
    await main.getByTestId('backup-'+mode+'-start').click()
  }
  await main.evaluate(()=>{ location.hash='/vault' })
  await start('export')
  await main.getByTestId('backup-password').fill('12345')
  await main.getByTestId('backup-confirmation').fill('12345')
  assert.ok(await main.getByTestId('backup-request').isDisabled())
  await main.getByTestId('backup-password').fill('临时2026')
  await main.getByTestId('backup-confirmation').fill('临时2026')
  await formMatrix(app,main,shots,'vault-backup-export','[data-testid="vault-backup"]')
  const opened=app.waitForEvent('window')
  await main.getByTestId('backup-request').focus()
  await main.keyboard.press('Enter')
  const approval=await opened
  const decision=approval.getByTestId('approve')
  await decision.waitFor()
  await approval.waitForFunction(()=>!document.querySelector('[data-testid="approve"]')?.disabled)
  const win=await app.browserWindow(approval)
  await win.evaluate(w=>w.setContentSize(400,560))
  await approval.screenshot({path:join(shots,'vault-backup-approval.png')})
  const bounds=await decision.boundingBox()
  await approval.mouse.move(bounds.x+bounds.width/2,bounds.y+bounds.height/2)
  await approval.mouse.down()
  await approval.waitForTimeout(1250)
  await approval.mouse.up()
  await main.getByTestId('backup-save').waitFor()
  await answer('__save',null)
  await main.getByTestId('backup-save').click()
  await answer('__save',target)
  await main.getByTestId('backup-save').click()
  await modal.getByText('portable.dmsgvault').waitFor()
  await main.keyboard.press('Escape')
  await start('import')
  await answer('__open',target)
  await main.getByTestId('backup-pick').click()
  await main.getByTestId('backup-import-password').fill('临时2026')
  await main.getByTestId('backup-inspect').click()
  await main.getByTestId('backup-commit').waitFor()
  await formMatrix(app,main,shots,'vault-backup-review','[data-testid="vault-backup"]')
  assert.ok(await modal.getByRole('checkbox').nth(1).isDisabled())
  await main.getByTestId('backup-commit').click()
  await modal.getByText('Vault entries follow normal sync.',{exact:false}).waitFor()
  assert.equal(latest('vault/backup_commit').params.folder,'root')
  await main.keyboard.press('Escape')
}
