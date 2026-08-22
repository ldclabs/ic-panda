/**
 * Single source of truth for the panda.fans brand surface.
 *
 * panda.fans answers "who is building & governing?".
 * anda.ai and tokenlist.ing carry their own technical narratives — this site
 * links to them instead of restating them.
 *
 * Narrative belongs to ICPanda DAO. Facts belong on-chain.
 */

const SNS_ROOT = 'd7wvo-iiaaa-aaaaq-aacsq-cai'
const SNS_DASHBOARD = `https://dashboard.internetcomputer.org/sns/${SNS_ROOT}`

export const PANDA_LEDGER_CANISTER_ID = 'druyg-tyaaa-aaaaq-aactq-cai'
export const PANDA_BNB_CONTRACT = '0xe74583edAFF618D88463554b84Bc675196b36990'

export const LINKS = {
  // organization
  github: 'https://github.com/ldclabs',
  githubRepo: 'https://github.com/ldclabs/ic-panda',
  whitepaper:
    'https://github.com/ldclabs/ic-panda/blob/main/whitepaper/2026.en.md',
  x: 'https://x.com/ICPandaDAO',
  openchat: 'https://oc.app/community/dqcvf-haaaa-aaaar-a5uqq-cai',
  // Preferred route for reaching the DAO.
  alink: 'https://al.ink/ICPanda',

  // on-chain facts
  snsDashboard: SNS_DASHBOARD,
  snsProposals: `${SNS_DASHBOARD}/proposals`,
  snsNeurons: `${SNS_DASHBOARD}/neurons`,
  snsTransactions: `${SNS_DASHBOARD}/transactions`,
  ledgerCanister: `https://dashboard.internetcomputer.org/canister/${PANDA_LEDGER_CANISTER_ID}`,
  internetComputer: 'https://internetcomputer.org',

  // projects
  anda: 'https://anda.ai',
  andaGithub: 'https://github.com/ldclabs/anda',
  kipGithub: 'https://github.com/ldclabs/KIP',
  andaDbGithub: 'https://github.com/ldclabs/anda-db',
  mibGithub: 'https://github.com/ldclabs/MIB',
  tokenlist: 'https://tokenlist.ing',

  // archive
  dmsg: 'https://dmsg.net',
  dmsgGithub: 'https://github.com/ldclabs/ic-panda/tree/main/src/ic_message',

  // apps still served from this canister
  bridge: 'https://1bridge.app/?token=PANDA&from=ICP&to=BNB',
  icpswap: `https://app.icpswap.com/swap/pro?input=ryjl3-tyaaa-aaaaa-aaaba-cai&output=${PANDA_LEDGER_CANISTER_ID}`,
  bscscan: `https://bscscan.com/token/${PANDA_BNB_CONTRACT}`
} as const

export const NAV = [
  { label: 'PANDA', href: '/#panda' },
  { label: 'Projects', href: '/#projects' },
  { label: 'Governance', href: '/#dao' },
  { label: 'Community', href: '/#community' }
] as const

/**
 * Section 02 — the fixed genesis figure. Current supply is not hard-coded
 * here: it is read live from the ledger, because it keeps moving.
 */
export const GENESIS_SUPPLY = '1,000,000,000'

/** Contextual facts shown beneath the supply pair. */
export const TOKEN_FACTS = [
  { value: 'Internet Computer', label: 'Network' },
  { value: 'SNS', label: 'On-chain Governance' },
  { value: 'April 3, 2024', label: 'SNS Launch' }
] as const

/**
 * Genesis allocation as recorded in the SNS init parameters.
 * This is a historical snapshot, not today's treasury balance.
 */
export const GENESIS_ALLOCATION = [
  { percent: 4, name: 'Development Team', tokens: '40,000,000' },
  { percent: 4, name: 'Seed Funders', tokens: '40,000,000' },
  { percent: 12, name: 'SNS Swap', tokens: '120,000,000' },
  { percent: 80, name: 'DAO Treasury', tokens: '800,000,000' }
] as const

export const TREASURY_ALLOCATION = [
  { percent: 50, name: 'Community-wide distribution / Lucky Pool' },
  { percent: 10, name: 'Community incentives' },
  { percent: 10, name: 'CEX incentives' },
  { percent: 10, name: 'DEX liquidity' }
] as const

export const PANDA_ROLES = [
  {
    index: '01',
    name: 'Govern',
    body: 'Lock PANDA in SNS neurons to participate in proposals and help shape the direction of ICPanda DAO.'
  },
  {
    index: '02',
    name: 'Coordinate',
    body: 'Align builders, contributors, and the community around shared infrastructure and ecosystem initiatives.'
  },
  {
    index: '03',
    name: 'Fund',
    body: 'Govern the DAO treasury and direct resources toward development, ecosystem growth, and community initiatives.'
  }
] as const

export const ANDA_MODULES = [
  {
    name: 'KIP 2.0',
    kind: 'Cognitive State Protocol',
    body: 'A structured language for memory, belief, evidence, experience, skill, provenance, and cognitive governance.',
    href: LINKS.kipGithub
  },
  {
    name: 'Anda',
    kind: 'Agent Runtime',
    body: 'A composable Rust runtime for autonomous agents with cryptographic identity, tools, skills, and confidential execution.',
    href: LINKS.andaGithub
  },
  {
    name: 'Anda DB',
    kind: 'Cognitive Infrastructure',
    body: 'A persistent cognitive state engine combining structured semantics, graph relations, vector retrieval, and full-text search.',
    href: LINKS.andaDbGithub
  },
  {
    name: 'MIB',
    kind: 'Memory Intelligence Benchmark',
    body: 'Measure whether memory actually improves future cognition and behavior — not simply whether the past can be retrieved.',
    href: LINKS.mibGithub
  }
] as const

export const TOKENLIST_STAGES = [
  {
    index: '01',
    code: 'Mint',
    verb: 'Create',
    body: 'Launch and configure on-chain assets across supported networks.'
  },
  {
    index: '02',
    code: 'CCA',
    verb: 'Issue',
    body: 'Use Continuous Clearing Auctions for transparent price discovery and token distribution.'
  },
  {
    index: '03',
    code: 'DAO',
    verb: 'Govern',
    body: 'Move from issuance to long-term community coordination and on-chain governance.'
  }
] as const

export const DAO_ENTRIES = [
  {
    name: 'Proposals',
    body: 'See what the DAO is deciding.',
    cta: 'View Proposals',
    href: LINKS.snsProposals
  },
  {
    name: 'Neurons',
    body: 'Explore participation and voting power.',
    cta: 'View Neurons',
    href: LINKS.snsNeurons
  },
  {
    name: 'Treasury & Token',
    body: 'Verify assets and transactions on-chain.',
    cta: 'View ICP Dashboard',
    href: LINKS.snsDashboard
  },
  {
    name: 'Source',
    body: 'Follow development in public.',
    cta: 'Explore GitHub',
    href: LINKS.github
  }
] as const

export const PRINCIPLES = [
  {
    index: '01',
    name: 'Open Source',
    body: 'Infrastructure should be inspectable, forkable, and useful beyond the team that created it.'
  },
  {
    index: '02',
    name: 'Verifiable',
    body: 'When systems can prove what happened, trust can move from institutions into protocols.'
  },
  {
    index: '03',
    name: 'Sovereign',
    body: 'Users, agents, and communities should retain meaningful control over their identity, cognition, assets, and coordination.'
  },
  {
    index: '04',
    name: 'Experimental',
    body: 'New infrastructure is discovered by building. We ship, test, learn, and sometimes retire what no longer deserves active development.'
  }
] as const
