/** Reviewed release identities. No environment variable can add a trusted team. */
const policy = require('./macos-signing.json')

for (const key of ['sourceTeam', 'recipientTeam', 'releaseTeam']) {
  if (!/^[A-Z0-9]{10}$/.test(policy[key])) throw new Error(`Invalid macOS signing ${key}`)
}
if (policy.sourceTeam === policy.recipientTeam || ![policy.sourceTeam, policy.recipientTeam].includes(policy.releaseTeam)) {
  throw new Error('Invalid macOS signing transition')
}
for (const key of ['desktopIdentifier', 'daemonIdentifier']) {
  if (!/^[a-zA-Z0-9.-]+$/.test(policy[key])) throw new Error(`Invalid macOS signing ${key}`)
}
if (!/^\d+\.\d+\.\d+$/.test(policy.bridgeVersion)) throw new Error('Invalid macOS bridge version')

function teamsFor(signer) {
  if (signer === policy.sourceTeam) return [policy.sourceTeam, policy.recipientTeam]
  if (signer === policy.recipientTeam) return [policy.recipientTeam]
  throw new Error(`Unrecognized dMsg release signing team: ${signer}`)
}

/** Apple-issued Developer ID Application, exact code identifier and pinned teams. */
function requirement(identifier, teams) {
  if (!/^[a-zA-Z0-9.-]+$/.test(identifier) || !teams.length || teams.some(team => !/^[A-Z0-9]{10}$/.test(team))) {
    throw new Error('Invalid signing requirement')
  }
  return `identifier "${identifier}" and anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists and certificate leaf[field.1.2.840.113635.100.6.1.13] exists and (${teams.map(team => `certificate leaf[subject.OU] = "${team}"`).join(' or ')})`
}

function assertReleaseTeam(team) {
  if (team !== policy.releaseTeam) throw new Error(`This source release must be signed by ${policy.releaseTeam}; received ${team}`)
}

module.exports = { policy, teamsFor, requirement, assertReleaseTeam }

if (require.main === module) {
  const [operation, team] = process.argv.slice(2)
  if (operation !== 'check-team') throw new Error('Usage: macos-signing.cjs check-team TEAM_ID')
  assertReleaseTeam(team)
}
