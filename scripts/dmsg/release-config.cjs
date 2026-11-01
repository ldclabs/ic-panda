/** One version rule for CI and both desktop packagers. Tags equal the version, without a prefix. */
function releaseConfig(version) {
  if (typeof version !== 'string' || !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(-beta\.(0|[1-9]\d*))?$/.test(version)) {
    throw new Error(`Expected X.Y.Z or X.Y.Z-beta.N, got ${version}`)
  }
  const prerelease = version.includes('-')
  return { version, channel: prerelease ? 'beta' : 'latest', prerelease, make_latest: !prerelease }
}

function publishConfig(version) {
  const { channel } = releaseConfig(version)
  return { provider: 'generic', url: `https://github.com/ldclabs/ic-panda/releases/download/${version}/`, channel, useMultipleRangeRequest: false }
}

module.exports = { releaseConfig, publishConfig }

if (require.main === module) {
  const { readFileSync, appendFileSync } = require('node:fs')
  const { resolve } = require('node:path')
  const root = resolve(__dirname, '../..')
  const cli = readFileSync(resolve(root, 'src/dmsg_cli/Cargo.toml'), 'utf8').match(/^version = "([^"]+)"/m)?.[1]
  const desktop = JSON.parse(readFileSync(resolve(root, 'src/dmsg_desktop/package.json'), 'utf8')).version
  if (cli !== desktop) throw new Error(`dmsg_cli is ${cli}, the desktop is ${desktop}`)
  const config = releaseConfig(cli)
  if (process.env.GITHUB_REF_TYPE === 'tag' && process.env.GITHUB_REF_NAME !== cli) {
    throw new Error(`Tag ${process.env.GITHUB_REF_NAME} does not name dMsg ${cli}`)
  }
  const output = Object.entries(config).map(([key, value]) => `${key}=${value}`).join('\n') + '\n'
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, output)
  process.stdout.write(output)
}
