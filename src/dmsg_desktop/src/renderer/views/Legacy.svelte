<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '../i18n'
  import { call, code, when } from '../rpc'

  /**
   * Legacy migration: a one-time offer opens the old site in the system
   * browser, the old site saves an encrypted file sealed to this device,
   * and the import keeps the checked archive as account content. Shared old
   * channels get their official successor once every frozen manager agreed.
   */
  let { status }: { status: any } = $props()
  let origin = $state('https://dmsg.net')
  let offer = $state<any>(null)
  let pairs = $state<any[]>([])
  let archives = $state<any[]>([])
  let selected = $state('')
  let principal = $state('')
  let confirmed = $state(false)
  let report = $state<any>(null)
  let limit = $state(100)
  let notice = $state('')
  let error = $state('')
  let busy = $state(false)

  // Shared channel inheritance.
  let sourceText = $state('')
  let source = $state<any>(null)
  let version = $state('1')
  let draftText = $state('')
  let manager = $state('')
  let request = $state<any>(null)
  let approvalText = $state('')
  let target = $state('')
  let view = $state<any>(null)
  let member = $state('')
  let claimRequest = $state<any>(null)
  let grantArchive = $state('')
  let grantMember = $state('')
  let invitation = $state('')

  const pairing = $derived(pairs.find((p) => p.nonce === selected))
  const claims = $derived<any[]>(Object.values(view?.claims ?? {}))
  const grants = $derived<any[]>(Object.values(view?.grants ?? {}))
  const me = $derived(status?.account)

  async function attempt(work: () => Promise<void>, done = '') {
    busy = true
    error = ''
    notice = ''
    try {
      await work()
      notice = done
    } catch (e) {
      error = code(e) === 'LOCKED' ? t('locked') : `${t('error')}: ${code(e)}`
    } finally {
      busy = false
    }
  }

  async function load() {
    pairs = await call('legacy/pairs')
    archives = await call('legacy/list')
  }

  const pair = () =>
    attempt(async () => {
      offer = await call('legacy/pair', { origin })
      await window.dmsg.openLegacy(offer.link)
      await load()
      selected = offer.nonce
      confirmed = false
    })

  const importFile = () =>
    attempt(async () => {
      const result = await window.dmsg.legacyImport(pairing.nonce, pairing.fingerprint, principal.trim())
      if (!result) return
      report = result
      limit = 100
      await load()
    }, t('legacyKept'))

  const open = (id: string) =>
    attempt(async () => {
      limit = 100
      report = await call('legacy/report', { id, limit })
    })

  const more = () =>
    attempt(async () => {
      limit += 100
      report = await call('legacy/report', { id: report.id, limit })
    })

  const frozen = () =>
    attempt(async () => {
      const result = await window.dmsg.legacyFrozen(report.id)
      if (result) report = await call('legacy/report', { id: report.id, limit })
    })

  const saveFile = (file: any) =>
    attempt(async () => {
      await window.dmsg.legacyFile(report.id, file.source, file.name)
    })

  const cancel = (nonce: string) =>
    attempt(async () => {
      await call('legacy/cancel', { nonce })
      if (selected === nonce) selected = ''
      await load()
    })

  const json = (text: string) => JSON.parse(text)
  const pretty = (value: unknown) => JSON.stringify(value, null, 2)

  async function readFile(event: Event) {
    const input = event.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    input.value = ''
    if (file && file.size <= 4 * 1024 * 1024) sourceText = await file.text()
  }

  const checkSource = () =>
    attempt(async () => {
      source = await call('legacy/shared_source', { source: json(sourceText) })
    })

  const prepare = () =>
    attempt(async () => {
      draftText = pretty(await call('legacy/shared_prepare', { source: json(sourceText), version: Number(version) }))
    }, t('legacyDraftReady'))

  const checkDraft = () =>
    attempt(async () => {
      source = await call('legacy/shared_check', { draft: json(draftText) })
    }, t('legacyDraftChecked'))

  const managerRequest = () =>
    attempt(async () => {
      request = await call('legacy/shared_request', { draft: json(draftText), manager: manager.trim() })
    })

  const vote = (method: string) =>
    attempt(async () => {
      view = await call(method, { draft: json(draftText), manager: manager.trim(), approval: json(approvalText) })
      target = view.key
      approvalText = ''
    })

  const viewParams = () => (target.trim().startsWith('{') ? { view: json(target) } : { key: target.trim() })

  const read = () =>
    attempt(async () => {
      view = await call('legacy/shared_view', viewParams())
    })

  const act = (method: string, params: Record<string, unknown> = {}) =>
    attempt(async () => {
      view = await call(method, { key: view.key, ...params })
    })

  const askClaim = () =>
    attempt(async () => {
      claimRequest = await call('legacy/shared_claim_request', { ...viewParams(), member: member.trim() })
    })

  const claim = () =>
    attempt(async () => {
      view = await call('legacy/shared_claim', {
        ...viewParams(),
        claim: claimRequest.request.claim,
        approval: json(approvalText)
      })
      approvalText = ''
    })

  const invite = (account: string) =>
    attempt(async () => {
      invitation = JSON.stringify(await call('channel/invite', { id: view.proposal.channel_id, account }))
    })

  const receive = (grant: string) =>
    attempt(async () => {
      report = await call('legacy/shared_receive', { key: view.key, grant })
      await load()
    }, t('legacyKept'))

  onMount(() => {
    void attempt(load)
    return window.dmsg.onEvent((event) => {
      if (event.type === 'legacy/changed') void load().catch(() => {})
    })
  })
</script>

<div class="card" data-testid="legacy">
  <h2>{t('legacyTitle')}</h2>
  <p class="muted">{t('legacyHint')}</p>
  {#if error}<p class="error">{error}</p>{/if}
  {#if notice}<p class="muted">{notice}</p>{/if}
  <div class="row">
    <select bind:value={origin}>
      <option>https://dmsg.net</option>
      <option>https://panda.fans</option>
    </select>
    <button class="primary" disabled={busy} onclick={pair} data-testid="legacy-pair">{t('legacyPair')}</button>
  </div>
  {#if pairs.length}
    <h3>{t('legacyOffers')}</h3>
    <ul class="list">
      {#each pairs as p (p.nonce)}
        <li class="row spread">
          <label class="row">
            <input type="radio" bind:group={selected} value={p.nonce} onchange={() => (confirmed = false)} />
            <span>{p.origin} · <span class="mono">{p.fingerprint.slice(0, 16)}</span> · {t('expires')} {when(p.expires_at)}{p.started ? ` · ${t('legacyStarted')}` : ''}</span>
          </label>
          <span class="row">
            <button disabled={busy} onclick={() => window.dmsg.openLegacy(p.link)}>{t('open')}</button>
            <button class="danger" disabled={busy} onclick={() => cancel(p.nonce)}>{t('cancel')}</button>
          </span>
        </li>
      {/each}
    </ul>
  {/if}
  {#if pairing}
    <dl data-testid="legacy-offer">
      <dt>{t('legacyLink')}</dt>
      <dd class="mono">{pairing.link}</dd>
      <dt>{t('device')}</dt>
      <dd class="mono">{pairing.target}</dd>
      <dt>{t('fingerprint')}</dt>
      <dd class="mono">{pairing.fingerprint}</dd>
    </dl>
    <input placeholder={t('legacyPrincipal')} bind:value={principal} autocomplete="off" />
    <label class="row" style="margin-top: 8px">
      <input type="checkbox" bind:checked={confirmed} />
      {t('legacyConfirm')}
    </label>
    <button class="primary" style="margin-top: 8px" disabled={busy || !confirmed || !principal.trim()} onclick={importFile}>{t('legacyImport')}</button>
  {/if}

  <h3>{t('legacyArchives')}</h3>
  <ul class="list" data-testid="legacy-archives">
    {#each archives as a (a.id)}
      <li class="row spread">
        <span>{a.mode} · <span class="mono">{a.principal}</span> · {a.objects} {t('legacyObjects')} · {a.gaps} {t('legacyGaps')}</span>
        <button disabled={busy} onclick={() => open(a.id)}>{t('legacyRead')}</button>
      </li>
    {:else}
      <li class="muted">{t('legacyNone')}</li>
    {/each}
  </ul>
</div>

{#if report}
  <div class="card" data-testid="legacy-report">
    <h2>{report.stage === 'partial' ? t('legacyPartial') : t('legacyVerified')}</h2>
    <p class="muted">{t('legacyClaims')}</p>
    <dl>
      <dt>{t('legacyPrincipal')}</dt>
      <dd class="mono">{report.principal}</dd>
      <dt>{t('displayName')}</dt>
      <dd>{report.profile.name || '-'}</dd>
      <dt>{t('bio')}</dt>
      <dd>{report.profile.bio || '-'}</dd>
    </dl>
    {#if report.gaps.length}
      <h3>{t('legacyGapList')}</h3>
      <ul class="list">
        {#each report.gaps.slice(0, 30) as gap, i (i)}
          <li class="mono muted">{gap.code} · {gap.source}: {gap.detail}</li>
        {/each}
      </ul>
    {/if}
    {#if report.files.length}
      <h3>{t('legacyFiles')}</h3>
      <ul class="list">
        {#each report.files as file (file.source)}
          <li class="row spread">
            <span>{file.name} <span class="mono muted">{file.source}</span></span>
            <button disabled={busy || !file.verified} onclick={() => saveFile(file)}>{file.verified ? t('legacySave') : t('legacyUnverified')}</button>
          </li>
        {/each}
      </ul>
    {/if}
    {#if report.resources.length || report.entitlements.length}
      <h3>{t('legacyResources')}</h3>
      <p class="muted">{t('legacyResourcesHint')}</p>
      {#each report.resources as r (r.source)}
        <div class="mono muted">{r.name} · paid {r.paid} · gas {r.gas}</div>
      {/each}
      {#each report.entitlements as e (e.source)}
        <details><summary class="mono">{e.source}</summary><pre>{pretty(e.value)}</pre></details>
      {/each}
    {/if}
    <h3>{t('legacyFrozen')}</h3>
    <p class="muted">
      {report.frozen.matched ? t('legacyFrozenMatched') : `${report.frozen.total_changes} ${t('legacyFrozenChanges')}`}
    </p>
    <button disabled={busy} onclick={frozen}>{t('legacyFrozenImport')}</button>
    <h3>{t('legacyMessages')} ({report.total_messages})</h3>
    <ul class="list" data-testid="legacy-messages">
      {#each report.messages as m (m.source)}
        <li>
          <div class="muted mono">{m.author} · {m.time}{m.system ? ` · ${t('legacySystem')}` : ''}</div>
          <div style="white-space: pre-wrap">{m.deleted ? t('legacyDeleted') : m.text}{m.file ? ` 📎 ${m.file.name}` : ''}</div>
        </li>
      {/each}
    </ul>
    {#if report.total_messages > report.messages.length}
      <button disabled={busy} onclick={more}>{t('next')}</button>
    {/if}
  </div>
{/if}

<details class="card" data-testid="legacy-shared">
  <summary><strong>{t('legacyShared')}</strong></summary>
  <p class="muted">{t('legacySharedHint')}</p>
  <h3>{t('legacySource')}</h3>
  <input type="file" accept=".json,application/json" onchange={readFile} />
  <textarea bind:value={sourceText} placeholder="legacy-shared-source.json" style="margin-top: 8px"></textarea>
  <div class="row" style="margin-top: 8px">
    <button disabled={busy || !sourceText.trim()} onclick={checkSource}>{t('legacyCheckSource')}</button>
    <input style="max-width: 80px" bind:value={version} aria-label={t('legacyVersion')} />
    <button class="primary" disabled={busy || !sourceText.trim() || !/^\d+$/.test(version)} onclick={prepare}>{t('legacyPropose')}</button>
  </div>
  {#if source}
    <dl>
      <dt>{t('legacyManagers')}</dt>
      <dd class="mono">{source.managers.join(', ')}</dd>
      <dt>{t('legacyMembers')}</dt>
      <dd class="mono">{source.members.join(', ')}</dd>
      <dt>{t('legacyRange')}</dt>
      <dd>{source.messageStart} – {source.messageEnd}</dd>
    </dl>
  {/if}
  <h3>{t('legacyDraft')}</h3>
  <textarea bind:value={draftText} placeholder="dmsg-shared-migration-draft/1"></textarea>
  <div class="row" style="margin-top: 8px">
    <button disabled={busy || !draftText.trim()} onclick={checkDraft}>{t('legacyCheckDraft')}</button>
    <input placeholder={t('legacyManager')} bind:value={manager} />
    <button disabled={busy || !draftText.trim() || !manager.trim()} onclick={managerRequest}>{t('legacyRequest')}</button>
  </div>
  {#if request}
    <textarea readonly value={pretty(request.request)}></textarea>
    <div class="row">
      {#each request.links as link (link)}
        <button disabled={busy} onclick={() => window.dmsg.openLegacy(link)}>{new URL(link).host}</button>
      {/each}
    </div>
  {/if}
  <textarea bind:value={approvalText} placeholder={t('legacyApproval')} style="margin-top: 8px"></textarea>
  <div class="row" style="margin-top: 8px">
    <button class="primary" disabled={busy || !draftText.trim() || !approvalText.trim()} onclick={() => vote('legacy/shared_propose')}>{t('legacySubmitProposal')}</button>
    <button disabled={busy || !draftText.trim() || !approvalText.trim()} onclick={() => vote('legacy/shared_consent')}>{t('legacySubmitConsent')}</button>
  </div>

  <h3>{t('legacyDirectory')}</h3>
  <textarea bind:value={target} placeholder={t('legacyDirectoryHint')}></textarea>
  <button style="margin-top: 8px" disabled={busy || !target.trim()} onclick={read}>{t('refresh')}</button>
  {#if view}
    <p>{t('status')}: {view.stage} · v{view.proposal.version} · {Object.keys(view.votes).length}/{view.source.managers.length} {t('legacyVotes')}</p>
    <p class="mono muted">{view.key}</p>
    <details><summary>{t('legacyExportView')}</summary><textarea readonly value={pretty(view)}></textarea></details>
    {#if view.proposal.owner === me}
      <div class="row">
        <button disabled={busy || view.stage !== 'pending'} onclick={() => act('legacy/shared_commit')}>{t('legacyCommit')}</button>
        <button class="primary" disabled={busy || !['committed', 'active'].includes(view.stage)} onclick={() => act('legacy/shared_activate')}>{t('legacyActivate')}</button>
      </div>
    {/if}
    <h3>{t('legacyClaim')}</h3>
    <div class="row">
      <input placeholder={t('legacyMember')} bind:value={member} />
      <button disabled={busy || !member.trim()} onclick={askClaim}>{t('legacyRequest')}</button>
    </div>
    {#if claimRequest}
      <textarea readonly value={pretty(claimRequest.request)}></textarea>
      <div class="row">
        {#each claimRequest.links as link (link)}
          <button disabled={busy} onclick={() => window.dmsg.openLegacy(link)}>{new URL(link).host}</button>
        {/each}
        <button class="primary" disabled={busy || !approvalText.trim()} onclick={claim}>{t('legacySubmitClaim')}</button>
      </div>
    {/if}
    <ul class="list">
      {#each claims as c (c.claim.member)}
        <li class="row spread">
          <span class="mono">{c.claim.member} → {c.claim.account}{c.joined ? ` · ${t('legacyJoined')}` : ''}</span>
          <span class="row">
            {#if view.proposal.owner === me && !c.joined}
              <button disabled={busy} onclick={() => invite(c.claim.account)}>{t('invite')}</button>
            {/if}
            {#if c.claim.account === me && !c.joined}
              <button disabled={busy} onclick={() => act('legacy/shared_joined')}>{t('legacyMarkJoined')}</button>
            {/if}
          </span>
        </li>
      {/each}
    </ul>
    {#if invitation}<textarea readonly value={invitation}></textarea>{/if}
    {#if view.stage === 'active'}
      <h3>{t('legacyGrant')}</h3>
      <div class="row">
        <select bind:value={grantArchive}>
          <option value="">{t('legacyArchives')}</option>
          {#each archives as a (a.id)}<option value={a.id}>{a.principal} · {a.objects}</option>{/each}
        </select>
        <select bind:value={grantMember}>
          <option value="">{t('legacyMember')}</option>
          {#each claims.filter((c) => c.joined) as c (c.claim.member)}
            <option value={c.claim.member}>{c.claim.member}</option>
          {/each}
        </select>
        <button class="primary" disabled={busy || !grantArchive || !grantMember} onclick={() => act('legacy/shared_grant', { archive: grantArchive, member: grantMember })}>{t('legacyGrantSend')}</button>
      </div>
      <ul class="list">
        {#each grants.filter((g) => g.grant.scope.account === me) as g (g.grant.scope.grant_id)}
          <li class="row spread">
            <span class="mono">{g.grant.scope.member} · {g.grant.scope.archive_digest.slice(0, 16)}</span>
            <button disabled={busy} onclick={() => receive(g.grant.scope.grant_id)}>{t('legacyReceive')}</button>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</details>
