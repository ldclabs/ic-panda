// Keep the native FSKit entitlement/profile and signature intact when embedding.
// An absent module leaves secret mounts unavailable, without installing a substitute.
const fs = require('node:fs')
const path = require('node:path')
const { execFileSync } = require('node:child_process')
function signing(bundle) {
  execFileSync('/usr/bin/codesign', ['--verify', '--strict', bundle], { stdio: 'pipe' })
  const output = execFileSync('/usr/bin/codesign', ['-d', '--verbose=4', bundle], { encoding: 'utf8', stdio: ['ignore','pipe','pipe'] })
  return output
}
function info(bundle) {
  const result = require('node:child_process').spawnSync('/usr/bin/codesign', ['-d','--verbose=4',bundle], { encoding:'utf8' })
  if(result.status!==0) throw Error('FSKit code signature missing')
  const text=result.stdout+result.stderr
  const team=/^TeamIdentifier=([A-Z0-9]{10})$/m.exec(text)?.[1]
  const id=/^Identifier=(.+)$/m.exec(text)?.[1]
  if(!team)throw Error('FSKit requires a team signature')
  return {team,id,text}
}
async function afterPack(context) {
  if(context.electronPlatformName!=='darwin' || !process.env.DMSG_FSKIT_EXTENSION)return
  const source=path.resolve(process.env.DMSG_FSKIT_EXTENSION)
  signing(source)
  if(info(source).id!=='net.dmsg.desktop.SecretsFS')throw Error('Unexpected FSKit module identifier')
  if(!fs.existsSync(path.join(source,'Contents','embedded.provisionprofile')))throw Error('Missing FSKit Module profile')
  const entitlement=execFileSync('/usr/bin/codesign',['-d','--entitlements',':-',source],{encoding:'utf8',stdio:['ignore','pipe','pipe']})
  if(!/<key>com\.apple\.developer\.fskit\.fsmodule<\/key>\s*<true\s*\/>/.test(entitlement))throw Error('Missing FSKit Module entitlement')
  const app=path.join(context.appOutDir,context.packager.appInfo.productFilename+'.app')
  const daemon=path.join(app,'Contents','Resources','runtime','dmsg')
  if(info(source).team!==info(daemon).team)throw Error('FSKit and daemon must use the same signing team')
  // The sandboxed module reaches the daemon only through the team's App Group container.
  const daemonEntitlements=execFileSync('/usr/bin/codesign',['-d','--entitlements',':-',daemon],{encoding:'utf8',stdio:['ignore','pipe','pipe']})
  if(!daemonEntitlements.includes(`<string>${info(daemon).team}.net.dmsg.secrets</string>`))throw Error('The daemon needs the team App Group entitlement for FSKit')
  const target=path.join(app,'Contents','Extensions','SecretsFS.appex')
  fs.mkdirSync(path.dirname(target),{recursive:true});fs.cpSync(source,target,{recursive:true,errorOnExist:true,force:false})
}
async function afterSign(context) {
  if(context.electronPlatformName!=='darwin' || !process.env.DMSG_FSKIT_EXTENSION)return
  const app=path.join(context.appOutDir,context.packager.appInfo.productFilename+'.app')
  const extension=path.join(app,'Contents','Extensions','SecretsFS.appex')
  signing(extension)
  if(info(app).team!==info(extension).team)throw Error('FSKit host and module signing teams differ')
  // Developer builds can be embedded locally; publish only Developer ID distribution signatures.
  if(process.env.CSC_LINK && !info(extension).text.includes('Authority=Developer ID Application:'))throw Error('Release FSKit module needs a Developer ID distribution signature and profile')
}
module.exports={afterPack,afterSign}
