import assert from 'node:assert/strict'

/** Synthetic daemon data, real Electron rendering and visibility scheduling. */
export async function cacheFlow({ app, main, shots, methods, calls, emit, hold, waitFor, formMatrix, displayStates }) {
  const goto = async route => main.evaluate(route => { location.hash = route }, route)
  const bodies = () => calls.filter(c => c.method === 'channel/sync' || (c.method === 'inbox/list' && c.params?.refresh !== false))
  await goto('/vault')
  await main.getByText('Smoke note', { exact: true }).first().waitFor()
  const before = bodies().length
  emit('channel/hint', { id: '*' }); emit('inbox/hint')
  await main.waitForTimeout(200)
  assert.equal(bodies().length, before, 'hidden resources do not fetch message bodies')

  const releaseChannel = hold('channel/sync')
  await goto('/channels')
  await main.getByTestId('channel-list').getByText('Smoke channel', { exact: true }).click()
  await main.getByText('Hello channel', { exact: true }).first().waitFor()
  await waitFor(() => calls.some(c => c.method === 'channel/sync'))
  await main.getByTestId('channel-text').fill('Unsent offline draft')
  await formMatrix(app, main, shots, 'cache-channel', '[data-testid="channel-messages"]')
  await goto('/vault')
  await releaseChannel()
  const hidden = bodies().length
  emit('channel/hint', { id: '*' }); emit('inbox/hint')
  await main.waitForTimeout(200)
  assert.equal(bodies().length, hidden)

  const releaseInbox = hold('inbox/list', p => p?.refresh !== false)
  await goto('/inbox')
  await main.locator('[data-testid="letter"]').getByText('A paid letter', { exact: true }).waitFor()
  await waitFor(() => bodies().length > hidden)
  const refreshing = bodies().length
  emit('inbox/changed')
  await main.waitForTimeout(200)
  assert.equal(bodies().length, refreshing, 'local changes do not launch another network read')
  await formMatrix(app, main, shots, 'cache-inbox', '[data-testid="letter"]')
  await releaseInbox()

  displayStates.set('wallet/list', { stale: true, observed_at_ms: 1, error: 'NETWORK_UNAVAILABLE' })
  await goto('/wallet')
  await main.getByTestId('wallet-view').waitFor()
  await main.getByTestId('display-stale').first().waitFor()
  assert.ok(await main.getByTestId('wallet-list').innerText())
  await formMatrix(app, main, shots, 'cache-wallet', '[data-testid="wallet-view"]')

  displayStates.set('profile/show', { stale: true, observed_at_ms: 1, error: 'NETWORK_UNAVAILABLE' })
  await goto('/settings/profile')
  await main.getByTestId('profile-edit').click()
  await main.getByTestId('profile-name').fill('Draft kept during refresh')
  emit('cache/changed', { kind: 'profile-document' })
  await main.waitForTimeout(200)
  assert.equal(await main.getByTestId('profile-name').inputValue(), 'Draft kept during refresh')
  await formMatrix(app, main, shots, 'cache-profile', '[data-testid="profile-name"]')
  await main.keyboard.press('Escape')

  let stored = 4096
  methods['cache/status'] = () => ({ domains: [{ kind: 'attachments', sealed_bytes: stored }] })
  methods['cache/clear'] = () => { stored = 0; return null }
  await goto('/settings/files')
  await main.getByTestId('cache-clear').waitFor()
  await formMatrix(app, main, shots, 'cache-clear', '[data-testid="cache-clear"]')
  await main.getByTestId('cache-clear').focus()
  await main.keyboard.press('Enter')
  await waitFor(() => calls.some(c => c.method === 'cache/clear'))
  assert.equal(stored, 0)
  await waitFor(() => main.getByTestId('cache-clear').isDisabled())
  assert.equal(calls.some(c => c.method === 'files/backup'), false, 'reading and clearing never schedules a file backup')
}
