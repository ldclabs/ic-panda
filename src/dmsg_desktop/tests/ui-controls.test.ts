import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  auditNativeControls,
  inspectSvelte,
  reviewCounts,
  sharedImplementation
} from '../scripts/audit-native-controls.ts'

test('views have no remaining browser-default controls or dialogs', () => {
  const { controls } = auditNativeControls()
  assert.deepEqual(
    reviewCounts(controls.filter((c) => !sharedImplementation(c))),
    {},
    'Use DESIGN.md shared controls; do not add view-level exceptions.\n' +
      controls
        .filter((c) => c.review)
        .map((c) => `${c.file}:${c.line} ${c.kind}`)
        .join('\n')
  )
  assert.equal(controls.filter(sharedImplementation).length, 1)
  const settings = [
    'views/Security.svelte',
    'views/Profile.svelte',
    'views/Onboarding.svelte',
    'views/Legacy.svelte'
  ]
  assert.equal(
    controls.filter((c) => settings.includes(c.file)).length,
    0,
    'settings fields compose shared controls'
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
