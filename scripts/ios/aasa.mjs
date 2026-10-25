// Emit the association document only after the operator supplies real identifiers.
// Redirecting or wildcard association documents are intentionally not generated.
const [team, bundle] = process.argv.slice(2)
if (!/^[A-Z0-9]{10}$/.test(team || '') || !/^net\.dmsg\.[a-zA-Z0-9.-]+$/.test(bundle || '')) {
  throw new Error('Usage: node scripts/ios/aasa.mjs TEAM_ID net.dmsg.ios')
}
process.stdout.write(JSON.stringify({
  applinks: { details: [{ appIDs: [`${team}.${bundle}`], components: [{ '/': '/_/ios/auth/callback' }] }] },
  webcredentials: { apps: [`${team}.${bundle}`] }
}, null, 2) + '\n')
