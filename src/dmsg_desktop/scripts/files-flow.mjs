import assert from 'node:assert/strict'
import { join } from 'node:path'

/** Content management remains usable after removing the generic cloud-space UI. */
export async function filesFlow({ app, main, shots, methods, hold, waitFor, calls, emit, setSession }) {
  const go = route => main.evaluate(route => location.hash = route, route)
  const original = methods['files/share_links']
  const originalRevoke = methods['files/share_revoke']
  let failRead = false, failRevoke = false
  let entries = [{ share_id: '73'.repeat(32), file: '99'.repeat(32), name: 'Shared report from a deleted source.pdf', size: 2048, url: 'https://dmsg.test/s/example#test-key', state: 'active', downloads: 2, expires_at: Date.now() + 86400000 }]
  methods['files/share_links'] = p => {
    if (p.refresh && failRead) throw { code: 'INVALID_INPUT', message: 'Fixture link refresh failure' }
    return structuredClone(entries)
  }
  methods['files/share_revoke'] = p => {
    if (failRevoke) throw { code: 'INVALID_INPUT', message: 'Fixture link revocation failure' }
    entries = entries.map(link => link.share_id === p.share_id ? { ...link, state: 'revoked', revoked_at: Date.now() } : link)
    return structuredClone(entries.find(link => link.share_id === p.share_id))
  }
  await go('/files')
  await main.getByTestId('files-list').waitFor()
  assert.equal(await main.getByTestId('files-scope-storage').count(), 0)
  // A retired route cannot restore the removed screen, including source-file links.
  await go('/files/storage?source=retired')
  await main.getByTestId('files-list').waitFor()
  assert.equal(await main.getByTestId('files-storage').count(), 0)
  await go('/files/links')
  await main.getByTestId('links-copy').waitFor()
  await waitFor(() => main.getByTestId('links-refresh').isEnabled())
  assert.ok(calls.some(call => call.method === 'files/share_links' && call.params.refresh))
  // Library changes, such as a background backup slice, re-read saved links without reaching the relay.
  const linkReads = refresh => calls.filter(call => call.method === 'files/share_links' && !!call.params?.refresh === refresh).length
  const [savedReads, relayReads] = [linkReads(false), linkReads(true)]
  emit('files/changed', null)
  await waitFor(() => linkReads(false) > savedReads)
  await waitFor(() => main.getByTestId('links-refresh').isEnabled())
  assert.equal(linkReads(true), relayReads)
  await main.evaluate(() => {
    window.__filesClipboardWrite = navigator.clipboard.writeText.bind(navigator.clipboard)
    navigator.clipboard.writeText = async value => { window.__filesCopied = value }
  })
  await main.getByTestId('links-copy').focus()
  await main.keyboard.press('Enter')
  await waitFor(async () => await main.evaluate(() => window.__filesCopied) === entries[0].url)
  await main.evaluate(() => { navigator.clipboard.writeText = window.__filesClipboardWrite })
  await main.getByTestId('links-search').fill('no matching link')
  await main.getByTestId('links-empty').waitFor()
  await main.getByTestId('links-search').fill('Shared report')
  await main.getByTestId('links-copy').waitFor()
  failRead = true
  await main.getByTestId('links-refresh').click()
  await main.getByTestId('files-links-page').getByRole('alert').waitFor()
  assert.equal(await main.getByTestId('links-search').inputValue(), 'Shared report')
  assert.equal(await main.getByTestId('links-copy').count(), 1)
  failRead = false
  await main.getByTestId('links-refresh').click()
  await waitFor(() => main.getByTestId('links-refresh').isEnabled())
  await main.getByTestId('links-revoke').focus()
  await main.keyboard.press('Enter')
  await main.getByTestId('links-confirm').waitFor()
  await main.keyboard.press('Escape')
  await main.getByTestId('links-confirm').waitFor({ state: 'detached' })
  failRevoke = true
  await main.getByTestId('links-revoke').click()
  await main.getByTestId('links-confirm-revoke').click()
  await main.getByTestId('links-confirm').getByRole('alert').waitFor()
  assert.match(await main.getByTestId('links-confirm').innerText(), /Shared report/)
  failRevoke = false
  await main.getByTestId('links-confirm-revoke').click()
  await main.getByTestId('links-confirm').waitFor({ state: 'detached' })
  await main.getByTestId('link-state').filter({ hasText: 'Revoked' }).waitFor()
  assert.equal(await main.getByTestId('links-copy').count(), 0)

  // A reply started on this page cannot expose a link after the session locks.
  const release = hold('files/share_links', p => p.refresh === true)
  await main.getByTestId('links-refresh').click()
  setSession('locked'); emit('session/changed', { state: 'locked' })
  await waitFor(() => main.getByTestId('links-search').isDisabled())
  await release()
  assert.equal(await main.getByText(entries[0].name, { exact: true }).count(), 0)
  entries = [{ ...entries[0], state: 'active', name: '项目交付资料 · Shared report with a long descriptive filename · Rapport de livraison avec un nom très détaillé · وثائق المشروع المشتركة.pdf' }]
  setSession('unlocked'); emit('session/changed', { state: 'unlocked' })
  await main.getByTestId('links-copy').waitFor()
  await waitFor(() => main.getByTestId('links-refresh').isEnabled())
  const releaseAfterLeaving = hold('files/share_links', p => p.refresh === true)
  await main.getByTestId('links-refresh').click()
  await go('/files')
  await main.getByTestId('files-list').waitFor()
  await releaseAfterLeaving()
  assert.equal(await main.getByTestId('files-links-page').count(), 0)
  await go('/files/links')
  await main.getByTestId('links-copy').waitFor()
  const window = await app.browserWindow(main)
  for (const [locale, appearance, width, height] of [
    ['zh', 'light', 1180, 760], ['zh', 'dark', 1180, 760],
    ['zh', 'light', 760, 540], ['zh', 'dark', 760, 540],
    ['fr', 'light', 760, 540], ['fr', 'dark', 760, 540],
    ['ar', 'light', 760, 540], ['ar', 'dark', 760, 540]
  ]) {
    await window.evaluate((window, [w, h]) => window.setContentSize(w, h), [width, height])
    await main.evaluate(async ({ locale, appearance }) => { await window.dmsg.setLocale(locale); await window.dmsg.setAppearance(appearance) }, { locale, appearance })
    await main.waitForSelector(`html[lang|="${locale}"]`)
    await main.waitForFunction(dark => matchMedia('(prefers-color-scheme: dark)').matches === dark, appearance === 'dark')
    assert.ok(await main.evaluate(() => document.documentElement.scrollWidth <= innerWidth))
    assert.ok(await main.getByTestId('files-links-page').evaluate(el => el.scrollWidth <= el.clientWidth + 1))
    await main.screenshot({ path: join(shots, `files-links-${locale}-${appearance}-${width}.png`), animations: 'disabled' })
    if (width === 760) {
      await main.getByTestId('files-places-open').click()
      await main.getByTestId('files-places-dialog').waitFor()
      assert.equal(await main.getByTestId('files-scope-storage').count(), 0)
      await main.screenshot({ path: join(shots, `files-places-${locale}-${appearance}.png`), animations: 'disabled' })
      await main.keyboard.press('Escape')
      await main.getByTestId('files-places-dialog').waitFor({ state: 'detached' })
    }
  }
  await window.evaluate(window => window.setContentSize(1180, 760))
  await main.evaluate(async () => { await window.dmsg.setLocale('en'); await window.dmsg.setAppearance('light') })
  await go('/plan')
  await main.getByTestId('usage-storage').waitFor()
  await waitFor(async () => (await main.getByTestId('usage-storage').innerText()).includes('1.3'))
  assert.ok(calls.some(call => call.method === 'files/cloud_usage'))
  await main.screenshot({ path: join(shots, 'files-membership-usage.png'), animations: 'disabled' })
  await go('/files/cloud')
  await main.getByTestId('files-cloud').waitFor()
  await go('/files')
  await main.getByTestId('files-list').waitFor()
  assert.equal(calls.some(call => call.method.startsWith('files/storage')), false)
  methods['files/share_links'] = original
  if (originalRevoke) methods['files/share_revoke'] = originalRevoke
  else delete methods['files/share_revoke']
}
