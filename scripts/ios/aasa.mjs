// Emit the association document only after the operator supplies real identifiers.
// Redirecting or wildcard association documents are intentionally not generated.
import releaseApps from './associated-apps.json' with { type: 'json' }

const [team, bundle] = process.argv.slice(2)
if (!/^[A-Z0-9]{10}$/.test(team || '') || !/^net\.dmsg\.[a-zA-Z0-9.-]+$/.test(bundle || '')) {
  throw new Error('Usage: node scripts/ios/aasa.mjs TEAM_ID net.dmsg.ios')
}
const app = `${team}.${bundle}`
const appIDs = releaseApps.includes(app) ? releaseApps : [app]
process.stdout.write(JSON.stringify({
  applinks: { details: [{ appIDs, components: [{ '/': '/_/ios/auth/callback' }] }] },
  webcredentials: { apps: appIDs }
}, null, 2) + '\n')
