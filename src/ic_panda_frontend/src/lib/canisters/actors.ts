import { AuthAgent, dynAgent } from '$lib/utils/auth'
import { Actor, type ActorMethod, type ActorSubclass } from '@dfinity/agent'
import type { IDL } from '@dfinity/candid'
import type { Principal } from '@dfinity/principal'

export function createActor<T = Record<string, ActorMethod>>({
  canisterId,
  idlFactory,
  agent
}: {
  canisterId: string | Principal
  idlFactory: IDL.InterfaceFactory
  agent?: AuthAgent
}): ActorSubclass<T> {
  // Creates an actor with using the candid interface and the HttpAgent
  agent = agent || dynAgent
  return Actor.createActor(idlFactory, {
    // queryTransform or callTransform
    // callTransform: (methodName: string, args: unknown[], callConfig: any) => {
    //   console.log('callTransform', methodName, args, callConfig)
    //   console.log('caller', agent?.id.getPrincipal().toText())
    // },
    agent,
    canisterId
  })
}
