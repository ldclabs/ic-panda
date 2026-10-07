import { readFileSync } from 'node:fs'
import { describe, expect, it, vi } from 'vitest'
import { render } from 'svelte/server'
import { decodeCanonical, type AppAction, type AppRegistration } from 'dmsg-sdk'
import ActionDetails from '../src/lib/components/ActionDetails.svelte'
import { unhex } from '../src/lib/protocol/codec'
// The session module binds browser globals; only its date formatter is used here.
vi.mock('../src/lib/session.svelte', () => ({
  dateLabel: (ms: number) => new Date(ms).toISOString()
}))
const vectors = JSON.parse(
  readFileSync(
    new URL('../../../src/dmsg_types/tests/integration_vectors.json', import.meta.url),
    'utf8'
  )
)
const fixture = (name: string): any =>
  decodeCanonical(unhex(vectors.find((v: any) => v.name === name).cbor_hex))
const app = (): AppRegistration => fixture('action_app')
const action = (i: number): AppAction => fixture(`app_action_${i}`)[2]

describe('schema-rendered action review', () => {
  it('shows every field with labels from the registered schema', () => {
    const { body } = render(ActionDetails, { props: { action: action(1), app: app() } })
    for (const text of [
      '记录审阅决定',
      '轮次',
      '要求修改',
      '07/total_supply',
      'Explain the complete supply allocation',
      '必须解决',
      'The allocation differs from the ledger.'
    ])
      expect(body).toContain(text)
    expect(body).not.toContain('ChangesRequested')
  })

  it('renders optional artifacts, booleans and hashes', () => {
    const transition = render(ActionDetails, { props: { action: action(2), app: app() } }).body
    expect(transition).toContain('https://example.test/analysis.pdf')
    expect(transition).toContain('0a'.repeat(32))
    const absent = action(2)
    absent.command.args[4]!.value = 'Null'
    expect(render(ActionDetails, { props: { action: absent, app: app() } }).body).toContain(
      '未提供'
    )
    expect(render(ActionDetails, { props: { action: action(3), app: app() } }).body).toContain(
      '拒绝'
    )
  })

  it('refuses a schema other than the one the action signs', () => {
    const other = app()
    other.action_schema!.commands.pop()
    expect(() => render(ActionDetails, { props: { action: action(1), app: other } }).body).toThrow(
      /INTEGRITY_FAILED/
    )
  })
})
