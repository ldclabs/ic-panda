import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  auditNativeControls,
  inspectSvelte,
  reviewCounts
} from '../scripts/audit-native-controls.ts'

test('new UI uses shared controls; existing native-control debt must shrink, not spread', () => {
  // Each entry is an open task in docs/native-controls-audit.md, not permission
  // to reuse that pattern. Remove entries as the corresponding UI is fixed.
  const existing = {
    'views/Files.svelte | input:password': 1,
    'views/Wallet.svelte | dialog:confirm': 1,
    'views/agents/Overview.svelte | input:datetime-local': 1,
    'views/files/Storage.svelte | input:date': 1,
    'views/secrets/Sharing.svelte | progress': 1,
    'views/vault/Generator.svelte | input:range': 2,
    'views/vault/Import.svelte | input:checkbox': 1,
    'views/vault/ItemForm.svelte | input:dynamic': 1,
    'views/wallet/History.svelte | input:date': 2
  }
  const { controls } = auditNativeControls()
  assert.deepEqual(
    reviewCounts(controls),
    existing,
    'Use DESIGN.md shared controls. After fixing existing debt, remove its entry here; do not expand the baseline to accept new controls.\n' +
      controls
        .filter((c) => c.review)
        .map((c) => `${c.file}:${c.line} ${c.kind}`)
        .join('\n')
  )
})

test('control inventory distinguishes actual markup from comments and themed text fields', () => {
  const controls = inspectSvelte(
    `
    <script>const example = '<select><option>example</option></select>'</script>
    <!-- <input type="checkbox" /> -->
    <input /><textarea></textarea><input type="number" />
    <select><option>actual</option></select><input type="checkbox" />
    <input type={kind} /><button onclick={() => window.confirm('sure')}>Go</button>
  `,
    'fixture.svelte'
  )
  assert.deepEqual(
    controls.filter((c) => c.review).map((c) => c.kind),
    ['select', 'input:checkbox', 'input:dynamic', 'dialog:confirm']
  )
  assert.equal(controls.filter((c) => !c.review).length, 3)
})
