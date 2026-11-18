import assert from 'node:assert/strict'

/** Exercise a refusal and a lost acknowledgement without changing a real credential. */
export async function confirmationFlow({ app, main, methods, hold, waitFor, formMatrix, shots }) {
  const ids = methods['agent/ids']
  const remove = methods['agent/remove']
  const withdraw = methods['agent/credential_revoke']
  const revoke = methods['agent/revoke']
  const writes = []
  let reads = 0, credential = true, hosting = true, refuse = true
  const hosted = ids()
  const agent = { ...hosted.agents[0], label: 'Confirmation fixture' }
  methods['agent/ids'] = () => {
    reads++
    return { ...hosted, agents: hosting ? [{ ...agent, credential }] : [] }
  }
  methods['agent/credential_revoke'] = p => {
    writes.push(p.agent)
    if (refuse) throw { code: 'FORBIDDEN' }
    credential = false
    return {}
  }
  methods['agent/remove'] = p => {
    writes.push(p.agent)
    hosting = false
    throw { code: 'EXECUTION_UNKNOWN' }
  }
  try {
    await main.evaluate(() => { location.hash = '/agents/ids' })
    const entry = main.getByTestId('agent-id').filter({ hasText: agent.label })
    await entry.getByTestId('agent-withdraw').click()
    const dialog = main.getByTestId('agent-withdraw-dialog')
    const confirm = dialog.getByTestId('agent-withdraw-dialog-confirm')
    const release = hold('agent/credential_revoke')
    await confirm.press('Enter')
    await waitFor(() => writes.length === 1)
    assert.ok(await confirm.isDisabled())
    await main.keyboard.press('Escape')
    assert.ok(await dialog.isVisible())
    await release()
    await dialog.getByTestId('confirmation-error').getByText(/permission/i).waitFor()
    await formMatrix(app, main, shots, 'confirmation-refused-dialog', '[data-testid="agent-withdraw-dialog"]')
    refuse = false
    await dialog.getByRole('button', { name: 'Retry', exact: true }).press('Enter')
    await dialog.waitFor({ state: 'detached' })
    assert.deepEqual(writes, [agent.agent_id, agent.agent_id])

    await entry.getByTestId('agent-actions').click()
    await main.getByTestId('agent-remove').click()
    const unknown = main.getByTestId('agent-remove-dialog')
    await unknown.getByTestId('agent-remove-dialog-confirm').press('Enter')
    const review = unknown.getByTestId('confirmation-review')
    await review.waitFor()
    assert.equal(await unknown.getByTestId('agent-remove-dialog-confirm').count(), 0)
    await formMatrix(app, main, shots, 'confirmation-unknown-dialog', '[data-testid="agent-remove-dialog"]')
    const before = reads
    await review.press('Enter')
    await waitFor(() => reads > before)
    await entry.waitFor({ state: 'detached' })
    assert.equal(writes.length, 3, 'checking the result only rereads the agent list')

    const reasons = []
    methods['agent/revoke'] = params => {
      reasons.push(params.reason)
      throw { code: reasons.length === 1 ? 'FORBIDDEN' : 'EXECUTION_UNKNOWN' }
    }
    await main.evaluate(() => { location.hash = '/agents' })
    await main.getByTestId('credential-revoke').first().click()
    const reason = main.getByTestId('revoke-reason')
    await reason.fill('Retire this delegation')
    await main.getByTestId('revoke-confirm').press('Enter')
    await main.getByTestId('revoke-dialog').getByRole('button', { name: 'Retry' }).waitFor()
    assert.equal(await reason.inputValue(), reasons[0])
    await main.getByTestId('revoke-confirm').press('Enter')
    const reviewRevocation = main.getByTestId('confirmation-review')
    await reviewRevocation.waitFor()
    await main.keyboard.press('Escape')
    assert.equal(await main.evaluate(() => location.hash), '#/agents')
    assert.ok(await reviewRevocation.isVisible(), 'Escape cannot open approval history')
    await reviewRevocation.press('Enter')
    await main.waitForFunction(() => location.hash === '#/approvals')
    assert.deepEqual(reasons, ['Retire this delegation', 'Retire this delegation'])
    console.log('Sensitive confirmations passed: explicit retry, retained reason, status and approval history')
  } finally {
    methods['agent/ids'] = ids
    methods['agent/remove'] = remove
    if (withdraw) methods['agent/credential_revoke'] = withdraw
    else delete methods['agent/credential_revoke']
    if (revoke) methods['agent/revoke'] = revoke
    else delete methods['agent/revoke']
  }
}
