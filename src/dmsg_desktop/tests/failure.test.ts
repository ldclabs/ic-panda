import assert from 'node:assert/strict'
import { test } from 'node:test'
import { reasonOf } from '../src/renderer/failure.ts'

/** The daemon's message for a query the deployed canister does not have. */
const methodNotFound =
  'Unavailable: ic: The replica returned a rejection error: reject code CanisterError, reject message Error from Canister uwist-sqaaa-aaaaj-qsjyq-cai: Canister has no query method \'get_membership_terms\'..\n' +
  'Check that the method being called is exported by the target canister. See documentation: https://docs.internetcomputer.org/references/execution-errors#method-not-found, error code Some("IC0536")'

test('a rejection reads as what the canister said, with its IC code, on one line', () => {
  assert.equal(
    reasonOf(methodNotFound),
    "Canister has no query method 'get_membership_terms'. Check that the method being called is exported by the target canister. (IC0536)"
  )
  assert.equal(reasonOf(methodNotFound).includes('replica returned'), false)
  assert.equal(reasonOf(methodNotFound).includes('https://'), false)
  assert.equal(reasonOf(methodNotFound).includes('Unavailable'), false)
  // An update the canister traps keeps the code that says why.
  assert.equal(
    reasonOf('Unavailable: ic: The replica returned a rejection error: reject code CanisterError, reject message Error from Canister 2vxsx-fae: Canister 2vxsx-fae trapped: heap out of bounds, error code Some("IC0503")'),
    'Canister 2vxsx-fae trapped: heap out of bounds (IC0503)'
  )
})

test('a message that names only its code leaves nothing to show', () => {
  assert.equal(reasonOf('UNAVAILABLE'), '')
  assert.equal(reasonOf('Unavailable: UNAVAILABLE'), '')
  assert.equal(reasonOf('   '), '')
})

test('a message about the connection itself stands as it is', () => {
  assert.equal(reasonOf('Not connected to the dmsg daemon'), 'Not connected to the dmsg daemon')
  assert.equal(reasonOf('plan/status timed out'), 'plan/status timed out')
  assert.equal(reasonOf('storage: Is a directory (os error 21)'), 'storage: Is a directory (os error 21)')
})

test('a reason too long for a notice stops at a whole word', () => {
  const long = `Unavailable: ic: ${'the relay answered with a rejection that says '.repeat(8)}end`
  const reason = reasonOf(long)
  assert.ok(reason.length <= 203, `${reason.length}`)
  assert.ok(reason.endsWith('…'))
  assert.ok(!reason.includes('  '))
  const words = reason.slice(0, -1).trimEnd()
  assert.equal(long.includes(`${words} `), true)
})
