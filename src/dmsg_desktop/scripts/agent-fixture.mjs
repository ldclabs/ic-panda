/** Agent tools fixture for desktop inspection: unread state is never consumed. */
import assert from 'node:assert/strict'

export function agentFixture(agent) {
  const now = Date.now()
  const peer = 'did:agent:' + 'P'.repeat(43)
  const capsule = {
    id: 'k1', actor: agent, type: 'knowledge.publish', created_at: now - 3_600_000,
    payload: {
      title: 'Language belongs in the cache key', kind: 'negative_result', language: 'en',
      statement: 'A cache keyed only by user ID can return a response in the wrong language. Including the requested language separates the entries.',
      context: { scope: 'An in-memory greeting cache with English and Chinese responses.', conditions: ['The same user requests both languages.'], limitations: ['This fixture does not establish the behavior of other cache implementations.'] },
      basis: 'Two sequential requests returned the first cached language. Adding the language to the key separated the results.',
      evidence: [{ description: 'Reproduction notes supplied by the author', url: 'https://example.org/research/cache-notes' }],
      license: 'https://creativecommons.org/licenses/by/4.0/'
    }
  }
  const records = [capsule,
    { ...capsule, id: 'k2', payload: { ...capsule.payload, title: 'A follow-up observation', kind: 'observation' } },
    { id: 'a1', actor: peer, type: 'knowledge.assess', created_at: now, payload: { target: 'k1', verdict: 'supports', summary: 'The two-language case reproduces the reported result.', basis: 'Repeated the two requests in a fresh cache.', context: capsule.payload.context, license: capsule.payload.license } },
    { id: 'r1', actor: agent, type: 'knowledge.retract', created_at: now, payload: { target: 'k1', reason: 'Replaced with a narrower statement.' } }
  ]
  const messages = [
    { direction: 'received', message_id: 'm1', thread_id: 't1', from: peer, to: agent, mailbox_id: 'mb1', subject: 'Review the cache experiment', created_at: now, expires_at: now + 86_400_000, received_at: now, parts: [{ media_type: 'text/plain', size: 15, text: 'Mail for Claude\n\nThe reproduction notes are ready. Please review the scope before using these results.' }], read: false },
    { direction: 'sent', message_id: 'm2', thread_id: 't1', from: agent, to: peer, mailbox_id: 'mb2', subject: 'Experiment notes received', created_at: now, expires_at: now + 86_400_000, parts: [{ media_type: 'text/plain', size: 20, text: 'The Agent wrote this reply.' }], delivery: { status: 'queued' } }
  ]
  let mailSynced = false
  let recovered = false
  const mailbox = { mailbox_id: 'mb1', address: 'mail:claude@relay.example', routes: ['https://relay.example'], status: 'open', unread_count: 1, expires_at: now + 86_400_000, key_available: true, pending_routes: [] }
  const lost = { ...mailbox, mailbox_id: 'recovered', address: 'mail:recovered@relay.example', key_available: false }

  return (p) => {
    const scope = { service: p.input.service, checkpoint: 4, as_of: now }
    switch (p.tool) {
      case 'agent_protocols_mail_mailboxes_list':
        if (p.agent !== agent) return { result: { mailboxes: [] } }
        if (p.input.host && !p.input.cursor) return { result: { mailboxes: [mailbox], next_cursor: 'recovery-page-2' } }
        if (p.input.host) { assert.equal(p.input.cursor, 'recovery-page-2'); recovered = true; return { result: { mailboxes: [lost] } } }
        return { result: { mailboxes: recovered ? [mailbox, lost] : [mailbox] } }
      case 'agent_protocols_mail_mailbox_publish':
        assert.equal(p.input.mailbox_id, lost.mailbox_id)
        assert.equal(p.input.rotate_key, true)
        assert.deepEqual(p.input.routes, [])
        Object.assign(lost, { status: 'closed', routes: [], key_available: true })
        return { result: { mailbox: lost, published: ['https://relay.example'], failed: [] } }
      case 'agent_protocols_mail_list':
        assert.notEqual(p.input.mark_read, true)
        return { result: { unread_count: p.agent === agent && mailSynced ? 1 : 0, messages: p.agent === agent ? messages.filter((m) => (mailSynced || m.direction === 'sent') && (!p.input.direction || m.direction === p.input.direction) && (!p.input.unread_only || m.read === false)) : [] } }
      case 'agent_protocols_mail_sync':
        mailSynced = true
        return { result: { received: [], delivered: [], expired: [], queued: 1, errors: [], synced_at: now } }
      case 'agent_protocols_knowledge_query': {
        const all = records.filter((r) => r.actor === p.input.actor && r.type === p.input.type)
        const start = p.input.cursor ? Number(p.input.cursor) : 0
        return { result: { ...scope, items: all.slice(start, start + 1), ...(start + 1 < all.length ? { next_cursor: String(start + 1) } : {}) } }
      }
      case 'agent_protocols_knowledge_status':
        return { result: { ...scope, item: records.find((r) => r.id === p.input.id), complete: p.input.id !== 'k2', ...(p.input.id === 'k2' ? {} : { status: p.input.id === 'k1' ? 'retracted' : 'active' }), assessments: p.input.id === 'k1' ? ['a1'] : [], successors: [], retractions: p.input.id === 'k1' ? ['r1'] : [] } }
      case 'agent_protocols_knowledge_get':
        return { result: { ...scope, items: records.filter((r) => p.input.ids.includes(r.id)), missing: [] } }
      case 'agent_protocols_knowledge_search':
        return { result: { ...scope, hits: [{ item: capsule }], ranking: {}, coverage: {} } }
      default:
        return { error: { code: 'not_found', message: p.tool } }
    }
  }
}
