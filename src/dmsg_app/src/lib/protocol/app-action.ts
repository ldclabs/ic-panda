import { toCandid } from '@dmsg/sdk'
import { IDL } from '@icp-sdk/core/candid'
import { idlFactory } from '../canisters/generated/user/index.js'
import type { AppAction as CandidAction } from '../canisters/generated/user'
import { decodeCanonical, type AppAction, validateAppAction } from '@dmsg/sdk'
import { candidValue } from './account'
import { canonical } from './codec'
const service = idlFactory({ IDL }) as IDL.ServiceClass
const requestType = service._fields.find(([name]) => name === 'sign_app_action')![1]
  .argTypes[0] as IDL.RecordClass
const actionType = requestType._fields.find(([name]) => name === 'action')![1]
export { toCandid } from '@dmsg/sdk'
export function actionToCandid(action: AppAction): CandidAction {
  validateAppAction(action)
  return toCandid(actionType, action)
}
export function actionFromCandid(action: CandidAction): AppAction {
  const value = decodeCanonical(
    canonical(candidValue(actionType, action))
  ) as unknown as AppAction
  validateAppAction(value)
  return value
}
