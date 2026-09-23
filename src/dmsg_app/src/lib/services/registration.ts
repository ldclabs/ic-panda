import { Principal } from '@icp-sdk/core/principal'
import type { AppRegistration } from '@dmsg/sdk'
import { decodeCanonical, validateApp } from '@dmsg/sdk'
import { certifiedValue } from './certified'
import { controlResult } from './account'
import type { services } from './ic'
import { config } from '../config'
import { digest } from '../protocol/codec'
import { ensure } from '../errors'

export async function registeredApplication(
  api: Awaited<ReturnType<typeof services>>,
  appId: string,
  origin: string,
  capability: 'Authenticate' | 'SignDocument'
) {
  ensure(api.commerce && config.canisters.commerce, 'UNAVAILABLE')
  const certified = await certifiedValue(
    controlResult(await api.commerce.integration_configuration_certificate(appId, [])),
    api.agent,
    config.canisters.commerce,
    digest('dmsg/registration/app/v1', appId)
  )
  const app = decodeCanonical(certified.value) as unknown as AppRegistration
  validateApp(app)
  ensure(
    !app.paused &&
      app.app_id === appId &&
      app.environment.toLowerCase() === config.environment &&
      app.origins.includes(origin) &&
      app.capabilities.includes(capability) &&
      app.user_homes.some(
        (p) => Principal.fromUint8Array(p).toText() === config.canisters.user
      ) &&
      app.cose_homes.some(
        (p) => Principal.fromUint8Array(p).toText() === config.canisters.cose
      ),
    'FORBIDDEN'
  )
  return app
}
