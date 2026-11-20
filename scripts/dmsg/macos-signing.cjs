/** Reviewed release identities. No environment variable can add a trusted team. */
const policy = require('./macos-signing.json')
const { execFileSync, spawnSync } = require('node:child_process')

for (const key of ['sourceTeam', 'recipientTeam', 'releaseTeam']) {
  if (!/^[A-Z0-9]{10}$/.test(policy[key])) throw new Error(`Invalid macOS signing ${key}`)
}
if (policy.sourceTeam === policy.recipientTeam || policy.releaseTeam !== policy.recipientTeam) {
  throw new Error('New macOS releases must use the recipient team')
}
for (const key of ['desktopIdentifier', 'daemonIdentifier']) {
  if (!/^[a-zA-Z0-9.-]+$/.test(policy[key])) throw new Error(`Invalid macOS signing ${key}`)
}
/** Releases supply one signer; both are used to inspect historical bridges. */
function requirement(identifier, ...teams) {
  if (typeof identifier !== 'string' || !/^[a-zA-Z0-9.-]+$/.test(identifier) ||
      !teams.length || teams.some(team => typeof team !== 'string' || !/^[A-Z0-9]{10}$/.test(team))) {
    throw new Error('Invalid signing requirement')
  }
  const signers = teams.map(team => `certificate leaf[subject.OU] = "${team}"`).join(' or ')
  return `identifier "${identifier}" and anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists and certificate leaf[field.1.2.840.113635.100.6.1.13] exists and (${signers})`
}

function assertReleaseTeam(team) {
  if (team !== policy.releaseTeam) throw new Error(`This source release must be signed by ${policy.releaseTeam}; received ${team}`)
}

function designatedRequirement(app) {
  const result = spawnSync('/usr/bin/codesign', ['-d', '-r-', app], { encoding: 'utf8' })
  if (result.status !== 0) throw new Error(`Cannot read the signature on ${app}`)
  const expression = /^designated => (.+)$/m.exec(result.stdout + result.stderr)?.[1]
  if (!expression) throw new Error(`Missing designated requirement on ${app}`)
  return expression
}

function verifyDesignatedRequirement(app, expected) {
  const canonical = expression => execFileSync('/usr/bin/csreq', ['-r', '=' + expression, '-t'], { encoding: 'utf8' }).trim()
  const actual = designatedRequirement(app)
  if (canonical(actual) !== canonical(expected)) {
    throw new Error(`Unexpected designated requirement for ${app}: ${actual}`)
  }
  return actual
}

module.exports = { policy, requirement, assertReleaseTeam, designatedRequirement, verifyDesignatedRequirement }

if (require.main === module) {
  const [operation, team] = process.argv.slice(2)
  if (operation !== 'check-team') throw new Error('Usage: macos-signing.cjs check-team TEAM_ID')
  assertReleaseTeam(team)
}
