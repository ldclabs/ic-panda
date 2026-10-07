import { CryptoEngine } from '../../src/lib/crypto/engine'
import { currentWorkspace, WorkspaceDB } from '../../src/lib/db'
import { xidText } from '../../src/lib/protocol/identity'

export const fixtureAccount = xidText(new Uint8Array(12).fill(1))
/** A workspace bound to a test account without the user home. Real binding
 * (unlock secret, root bundle) is exercised by PocketIC and the e2e probe. */
export async function boundEngine(
  account = fixtureAccount,
  home = 'aaaaa-aa',
  progress?: ConstructorParameters<typeof CryptoEngine>[0]
) {
  const engine = new CryptoEngine(progress)
  const meta = await engine.initialize()
  const bound = {
    ...meta,
    subjectId: account,
    registered: true,
    account: {
      id: account,
      issuer: `https://dmsg.test/u/${account}`,
      homeUser: home,
      rootDigest: '1'.repeat(64)
    }
  }
  const db = await WorkspaceDB.open((await currentWorkspace())!)
  await db.db.put('meta', { id: 'workspace', value: bound })
  db.db.close()
  ;(engine as any).meta = bound
  return { engine, meta: bound }
}
