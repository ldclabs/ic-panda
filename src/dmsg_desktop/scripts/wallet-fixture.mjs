/** Populated wallet fixture for the real Electron renderer, not a substitute for chain tests. */
export function walletFixture(account, created) {
  const a = 'rrkah-fqaaa-aaaaa-aaaaq-cai',
    b = 'ryjl3-tyaaa-aaaaa-aaaba-cai'
  const policy = {
    version: 1,
    approver_threshold: 2,
    admin_threshold: 2,
    tiers: [],
    daily_limits: [],
    spendable_assets: [
      { asset: 'ICP', key: 'icp', decimals: 8, symbol: 'ICP' },
      { asset: 'ETH', key: 'evm:1', chain_id: 1, decimals: 18, symbol: 'ETH' }
    ],
    call_targets: [],
    evm_chains: [1],
    address_book_cooldown_ms: 86400000,
    proposal_ttl_ms: 604800000,
    unfreeze_delay_ms: 86400000,
    daily_signatures: 50,
    refuel: null,
    hardware_cosigners: false,
    public: false
  }
  const state = { enabled: false, role: 'Admin', frozen: false, failBalances: false, changed: null, transfer: null, approval: 900,
    readinessError: false, blockers: [], replacement: false, executed: null, activation: 'unpaid', creation: null, cosigners: [], localKeys: [], btcCosign: null, collectionError: false }
  const view = (id) => ({
    wallet: id,
    name: id === a ? 'Engineering' : 'Operations',
    audit_head: 'aa'.repeat(32),
    me: { id: account, role: state.role, status: 'Active' },
    members: [
      { id: account, role: state.role, status: 'Active', label: 'You', agent_ids: [], principals: ['xkbqi-2qaaa-aaaah-qbpqq-cai'] }
    ],
    policy,
    addresses: [
      { id: 0, chain: 'Icp', control: 'chain_key', text: id, label: 'Operating account', verified: true, cosign: null },
      { id: 1, chain: 'Bitcoin', control: 'chain_key', text: 'bc1pfixture', label: 'Bitcoin reserve', verified: true, cosign: state.btcCosign },
      {
        id: 2,
        chain: 'Evm',
        control: 'chain_key',
        text: '0x0000000000000000000000000000000000000001',
        label: 'Ethereum',
        verified: true,
        cosign: state.replacement ? { threshold: 2, cosigners: [1, 2] } : null
      }
    ],
    cosigners: state.cosigners,
    frozen: state.frozen,
    stats: { cycles: '1987788848105', signatures_today: 2, proposals: 55, active_proposals: 3 },
    code: 'None',
    code_acceptance: 'None'
  })
  const proposal = (id) => ({
    id,
    status: id === 55 ? 'Open' : id === 54 ? 'Ready' : id === 53 ? 'Delayed' : 'Settled',
    intent: {
      kind: 'transfer',
      from: 0,
      asset: policy.spendable_assets[0],
      outputs: [{ to: b, amount: '100000000' }],
      max_fee: '10000',
      memo: null
    },
    proposer: account,
    requirement: { approvals: 2, admin: false, delay_ms: 0 },
    votes: [],
    created_at: Date.now() - 60000,
    expires_at: Date.now() + 86400000,
    ready_at: id === 53 ? Date.now() + 5000 : null,
    plans: 0,
    signed: [],
    broadcasts: [],
    digest: 'dd'.repeat(32),
    outcome: null
  })
  const proposals = Array.from({ length: 55 }, (_, i) => proposal(55 - i))
  proposals[0].intent = { kind: 'set_policy', policy: { ...policy, approver_threshold: 1, public: true } }
  proposals[0].requirement.admin = true
  const methods = {
    'wallet/create': (p) => {
      state.activation = 'unpaid'
      state.creation = created((state.approval++).toString(16).padStart(64, '0'), 'wallet', {
        action: 'create', quote_id: p.quote, name: p.name, members: p.members, policy,
        price_e8s: '200000000', ledger_fee_e8s: '10000'
      })
      return state.creation
    },
    'wallet/activation': () => ({ stage: state.activation, wallet: state.activation === 'ready' ? a : null }),
    'wallet/show': (p) => view(p.wallet),
    'wallet/health': () => ({ cycles: '1995000000000', idle_days: 100, read_at: Date.now() }),
    'wallet/assets': (p) => ({ wallet: p.wallet, assets: policy.spendable_assets }),
    'wallet/book': () => ({ entries: [], next: null }),
    'wallet/proposals': (p) => {
      const rows = proposals.filter((v) => p.before == null || v.id < p.before).slice(0, p.limit ?? 50)
      return { proposals: rows, next: rows.length === (p.limit ?? 50) ? rows.at(-1).id : null }
    },
    'wallet/proposal': (p) => {
      const v = proposals.find((v) => v.id === p.proposal)
      return { ...v, plan_details: v.plan_details ?? Array.from({ length: v.plans }, (_, i) => ({ seq: i + 1, chain: 'Evm', from: '0x01', fee: '10000', expires_at: Date.now() + 600000 })) }
    },
    'wallet/prepare': (p) => {
      const v = proposals.find((v) => v.id === p.proposal)
      if (!v.plan_details?.some((v) => v.expires_at > Date.now())) {
        v.plans++
        v.plan_details = [{ seq: v.plans, chain: 'Bitcoin', from: 'bc1pfixture', fee: '10000', expires_at: Date.now() + 600000 }]
      }
      return v
    },
    'wallet/balances': (p) => {
      if (state.failBalances) throw { code: 'UNAVAILABLE', message: 'simulated chain outage' }
      return {
        wallet: p.wallet,
        read_at: Date.now(),
        addresses: [
          {
            address: 0,
            balances: [{ asset: 'ICP', key: 'icp', metadata: policy.spendable_assets[0], balance: { Ok: '12345678900' }, source: b }]
          }
        ]
      }
    },
    'wallet/change': (p) => {
      state.changed = p
      const v = proposal(proposals.length + 1)
      v.wallet = p.wallet
      v.status = 'Open'
      v.requirement = { approvals: policy.admin_threshold, admin: true, delay_ms: 86400000 }
      v.intent = p.change.kind === 'cosign' ? { kind: 'set_cosign', address: p.change.address, cosign: p.change.cosign }
        : p.change.kind === 'cosigner' ? { kind: 'add_cosigner', ...state.localKeys.find((k) => k.label === p.change.label), member: account, curve: 'P256', envelope: 'Raw' }
        : p.change.kind === 'unfreeze' ? { kind: 'unfreeze' } : { kind: 'set_policy', policy }
      proposals.unshift(v)
      return v
    },
    'wallet/preview': (p) => ({
      wallet: p.wallet,
      policy_version: 1,
      intent: {
        kind: 'transfer',
        from: p.draft.from,
        asset: policy.spendable_assets.find((a) => a.key === p.draft.asset),
        outputs: p.draft.outputs,
        max_fee: p.draft.max_fee ?? '12000',
        memo: p.draft.memo
      },
      estimated_fee: '10000',
      recommended_fee: '12000',
      max_fee: p.draft.max_fee ?? '12000',
      fee_asset: 'ICP',
      fee_decimals: 8,
      additional_reserve: '0',
      native_balance: '12345678900',
      requirement: { approvals: 2, delay_ms: 0 },
      blockers: [],
      read_at: Date.now()
    }),
    'wallet/transfer': (p) => {
      state.transfer = p
      const v = proposal(proposals.length + 1)
      v.wallet = p.wallet
      v.status = 'Open'
      proposals.unshift(v)
      return v
    },
    'wallet/freeze': () => {
      state.frozen = true
      return { frozen: true }
    },
    'wallet/approve': (p) =>
      created((state.approval++).toString(16).padStart(64, '0'), 'wallet', {
        action: 'decide',
        decision: 'approve',
        wallet: p.wallet,
        name: view(p.wallet).name,
        before: view(p.wallet),
        proposal: proposals.find((v) => v.id === p.proposal)
      }),
    'wallet/audit_page': (p) => ({
      wallet: p.wallet,
      verified: true,
      events: 1,
      head: 'aa'.repeat(32),
      read_at: Date.now(),
      from: 0,
      to: 1,
      next: null,
      recent: [{ seq: 0, at: Date.now(), actor: account, kind: 'Created', proposal: null }]
    }),
    'wallet/versions': () => ({
      record: { version: 1, locked: false },
      versions: [{ version: 1, module_hash: 'ee'.repeat(32), notes: 'Initial', retired: false }]
    }),
    'wallet/channel': () => ({ channel: '' }),
    'wallet/rpc': () => ({
      bitcoin_api: 'https://example.test/btc',
      solana_rpc: 'https://example.test/sol',
      evm_rpc: [[1, 'https://example.test/evm']]
    }),
    'wallet/cosign_status': () => {
      if (state.collectionError) throw { code: 'UNAVAILABLE', message: 'cosign collection unavailable' }
      return { collected: state.replacement ? 2 : 0, required: 2, ready: state.replacement, packet: '', can_cosign: true }
    },
    'wallet/cosign': (p) => created((state.approval++).toString(16).padStart(64, '0'), 'wallet', {
      action: 'cosign', wallet: p.wallet, name: view(p.wallet).name, before: view(p.wallet),
      proposal: proposals.find((v) => v.id === p.proposal), plan: {seq:p.plan, chain:'Bitcoin', from:'bc1pfixture', fee:'10000', outputs:[], expires_at:Date.now()+600000}, collect_only: true
    }),
    'wallet/cosign_execute': (p) => {
      state.executed = p
      const proposal = proposals.find((v) => v.id === p.proposal)
      proposal.signed.push({ plan: p.plan, txid: 'replacement' })
      return proposal
    }
  }
  return {
    state,
    methods,
    a,
    b,
    view,
    proposals,
    list: () =>
      state.enabled
        ? [
            { wallet: a, name: 'Engineering' },
            { wallet: b, name: 'Operations' }
          ]
        : []
  }
}
