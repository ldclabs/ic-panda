import { Principal } from '@icp-sdk/core/principal'
import type { AppRegistration } from '@dmsg/sdk'
import { decodeCanonical, validateApp } from '@dmsg/sdk'
import { certifiedValue } from './certified'
import { controlResult } from './account'
import { config } from '../config'
import { digest } from '../protocol/codec'
import { ensure } from '../errors'

export async function registeredApplication(
  api: {
    commerce: import('../canisters/generated/commerce')._SERVICE | null
    agent: import('@icp-sdk/core/agent').HttpAgent
  },
  appId: string,
  origin: string,
  capability: 'Authenticate' | 'SignDocument' | 'SignAction' | 'Checkout',
  deployment = {
    commerce: config.canisters.commerce,
    user: config.canisters.user,
    cose: config.canisters.cose,
    environment: config.environment
  }
) {
  ensure(api.commerce && deployment.commerce, 'UNAVAILABLE')
  const certified = await certifiedValue(
    controlResult(await api.commerce.integration_configuration_certificate(appId, [])),
    api.agent,
    deployment.commerce,
    digest('dmsg/registration/app/v1', appId)
  )
  const app = decodeCanonical(certified.value) as unknown as AppRegistration
  validateApp(app)
  ensure(
    !app.paused &&
      app.app_id === appId &&
      app.environment.toLowerCase() === deployment.environment &&
      app.origins.includes(origin) &&
      app.capabilities.includes(capability) &&
      app.user_homes.some((p) => Principal.fromUint8Array(p).toText() === deployment.user) &&
      app.cose_homes.some((p) => Principal.fromUint8Array(p).toText() === deployment.cose),
    'FORBIDDEN'
  )
  return app
}
