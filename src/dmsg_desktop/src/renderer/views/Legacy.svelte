<script lang="ts">
  import { RadioGroup } from 'bits-ui'
  import { onMount } from 'svelte'
  import Checkbox from '../components/Checkbox.svelte'
  import Collapsible from '../components/Collapsible.svelte'
  import Icon from '../components/Icon.svelte'
  import Fact from '../components/page/Fact.svelte'
  import Field from '../components/page/Field.svelte'
  import Id from '../components/page/Id.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Pill from '../components/page/Pill.svelte'
  import Section from '../components/page/Section.svelte'
  import Radio from '../components/Radio.svelte'
  import Select from '../components/Select.svelte'
  import { buttonClass, cn } from '../components/ui'
  import { t } from '../i18n/index.svelte'
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
  let sourceFile = $state<HTMLInputElement>()
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

{#snippet step(n: number, title: string)}
  <h3 class="mt-2 flex items-center gap-2 text-[13px] font-semibold">
    <span class="grid size-5 place-items-center rounded-full bg-pale font-mono text-[11px] text-forest">{n}</span>{title}
  </h3>
{/snippet}

{#snippet links(list: string[])}
  <div class="flex flex-wrap gap-1.5">
    {#each list as link (link)}
      <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => window.dmsg.openLegacy(link)}><Icon name="external" />{new URL(link).host}</button>
    {/each}
  </div>
{/snippet}

{#if error}<Notice tone="danger">{error}</Notice>{/if}
{#if notice}<Notice tone="ok">{notice}</Notice>{/if}

<Section title={t('legacyTitle')} description={t('legacyHint')} icon="legacy" testid="legacy">
  {#snippet actions()}
    <Select bind:value={origin} label={t('legacyTitle')} options={['https://dmsg.net', 'https://panda.fans'].map((o) => ({ value: o, label: o, mono: true }))} />
    <button class={buttonClass('primary')} disabled={busy} onclick={pair} data-testid="legacy-pair"><Icon name="external" />{t('legacyPair')}</button>
  {/snippet}
  {#if pairs.length}
    <h3 class="mb-1.5 text-[12px] font-medium text-faint">{t('legacyOffers')}</h3>
    <RadioGroup.Root bind:value={selected} onValueChange={() => (confirmed = false)} aria-label={t('legacyOffers')}>
      <ul class="divide-y divide-line rounded-xl border border-line">
        {#each pairs as p (p.nonce)}
          <li class={cn('flex flex-wrap items-center gap-2 px-3.5 py-2.5', selected === p.nonce && 'bg-pale/60')}>
            <Radio value={p.nonce} testid="legacy-pick-{p.nonce}" class="min-w-0 flex-1">
              <span class="flex flex-wrap items-center gap-1.5 text-[12.5px]">
                <bdi dir="ltr" class="font-mono">{p.origin}</bdi>
                <span class="font-mono text-faint">{p.fingerprint.slice(0, 16)}</span>
                <span class="text-faint">· {t('expires')} {when(p.expires_at)}</span>
                {#if p.started}<Pill tone="warn">{t('legacyStarted')}</Pill>{/if}
              </span>
            </Radio>
            <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => window.dmsg.openLegacy(p.link)}>{t('open')}</button>
            <button class={buttonClass('ghost', 'xs')} disabled={busy} onclick={() => cancel(p.nonce)}>{t('cancel')}</button>
          </li>
        {/each}
      </ul>
    </RadioGroup.Root>
  {/if}
  {#if pairing}
    <div class="mt-3 grid gap-3 rounded-xl border border-forest/30 bg-pale/40 p-4">
      <dl class="divide-y divide-line" data-testid="legacy-offer">
        <Fact label={t('legacyLink')}><bdi dir="ltr" class="font-mono text-[11.5px] break-all">{pairing.link}</bdi></Fact>
        <Fact label={t('device')}><span class="font-mono text-[12px] break-all">{pairing.target}</span></Fact>
        <Fact label={t('fingerprint')}><span class="font-mono text-[12px] font-semibold break-all text-forest">{pairing.fingerprint}</span></Fact>
      </dl>
      <Field label={t('legacyPrincipal')}><input bind:value={principal} autocomplete="off" class="font-mono text-[12.5px]" /></Field>
      <Checkbox bind:checked={confirmed}>{t('legacyConfirm')}</Checkbox>
      <button class={buttonClass('primary', 'md', 'justify-self-start')} disabled={busy || !confirmed || !principal.trim()} onclick={importFile}>
        <Icon name="verified" />{t('legacyImport')}
      </button>
    </div>
  {/if}
</Section>

<Section title={t('legacyArchives')} icon="archive" flush>
  <ul class="divide-y divide-line border-t border-line" data-testid="legacy-archives">
    {#each archives as a (a.id)}
      <li class="flex flex-wrap items-center gap-2 px-5 py-3">
        <Pill>{a.mode}</Pill>
        <Id id={a.principal} full class="max-w-[50%]" />
        <span class="min-w-0 flex-1 text-[12px] text-faint">{a.objects} {t('legacyObjects')} · {a.gaps} {t('legacyGaps')}</span>
        <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => open(a.id)}>{t('legacyRead')}</button>
      </li>
    {:else}
      <li class="px-5 py-3 text-[13px] text-faint">{t('legacyNone')}</li>
    {/each}
  </ul>
</Section>

{#if report}
  <Section title={report.stage === 'partial' ? t('legacyPartial') : t('legacyVerified')} description={t('legacyClaims')} icon={report.stage === 'partial' ? 'warning' : 'verified'} testid="legacy-report">
    <div class="grid gap-4">
      <dl class="divide-y divide-line">
        <Fact label={t('legacyPrincipal')}><Id id={report.principal} full /></Fact>
        <Fact label={t('displayName')}>{report.profile.name || '-'}</Fact>
        <Fact label={t('bio')}>{report.profile.bio || '-'}</Fact>
      </dl>
      {#if report.gaps.length}
        <Notice tone="warn">
          <p class="font-medium">{t('legacyGapList')}</p>
          <ul class="mt-1 grid gap-0.5 font-mono text-[11.5px]">
            {#each report.gaps.slice(0, 30) as gap, i (i)}<li>{gap.code} · {gap.source}: {gap.detail}</li>{/each}
          </ul>
        </Notice>
      {/if}
      {#if report.files.length}
        <div>
          <h3 class="mb-1.5 text-[12px] font-medium text-faint">{t('legacyFiles')}</h3>
          <ul class="divide-y divide-line rounded-xl border border-line">
            {#each report.files as file (file.source)}
              <li class="flex items-center gap-2 px-3.5 py-2">
                <Icon name="file" class="shrink-0 text-faint" />
                <span class="min-w-0 flex-1 truncate text-[13px]">{file.name} <span class="font-mono text-[11px] text-faint">{file.source}</span></span>
                <button class={buttonClass('secondary', 'xs')} disabled={busy || !file.verified} onclick={() => saveFile(file)}>
                  {file.verified ? t('legacySave') : t('legacyUnverified')}
                </button>
              </li>
            {/each}
          </ul>
        </div>
      {/if}
      {#if report.resources.length || report.entitlements.length}
        <div class="grid gap-1.5">
          <h3 class="text-[12px] font-medium text-faint">{t('legacyResources')}</h3>
          <p class="text-[12px] text-muted">{t('legacyResourcesHint')}</p>
          {#each report.resources as r (r.source)}
            <p class="font-mono text-[11.5px] text-muted">{r.name} · {t('legacyResource', { paid: r.paid, gas: r.gas })}</p>
          {/each}
          {#each report.entitlements as e (e.source)}
            <Collapsible>
              {#snippet title()}<span class="font-mono text-[12px]">{e.source}</span>{/snippet}
              <pre class="overflow-x-auto rounded-lg bg-surface-2 p-2.5 font-mono text-[11px]">{pretty(e.value)}</pre>
            </Collapsible>
          {/each}
        </div>
      {/if}
      <div class="flex flex-wrap items-center gap-2 rounded-xl border border-line px-3.5 py-2.5">
        <Icon name={report.frozen.matched ? 'verified' : 'warning'} class={report.frozen.matched ? 'text-ok' : 'text-warn'} />
        <span class="min-w-0 flex-1 text-[12.5px]">
          <b class="font-medium">{t('legacyFrozen')}</b> · {report.frozen.matched ? t('legacyFrozenMatched') : `${report.frozen.total_changes} ${t('legacyFrozenChanges')}`}
        </span>
        <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={frozen}>{t('legacyFrozenImport')}</button>
      </div>
      <div>
        <h3 class="mb-1.5 text-[12px] font-medium text-faint">{t('legacyMessages')} · {report.total_messages}</h3>
        <ul class="divide-y divide-line rounded-xl border border-line" data-testid="legacy-messages">
          {#each report.messages as m (m.source)}
            <li class="px-3.5 py-2.5">
              <p class="font-mono text-[11px] text-faint">{m.author} · {m.time}{m.system ? ` · ${t('legacySystem')}` : ''}</p>
              <p class={cn('text-[13px] whitespace-pre-wrap [overflow-wrap:anywhere]', m.deleted && 'text-faint italic')}>
                {m.deleted ? t('legacyDeleted') : m.text}
              </p>
              {#if m.file}<p class="mt-0.5 inline-flex items-center gap-1 text-[12px] text-faint"><Icon name="attach" class="size-3.5" />{m.file.name}</p>{/if}
            </li>
          {/each}
        </ul>
        {#if report.total_messages > report.messages.length}
          <button class={buttonClass('ghost', 'sm', 'mt-2')} disabled={busy} onclick={more}>{t('more')}</button>
        {/if}
      </div>
    </div>
  </Section>
{/if}

<Section title={t('legacyShared')} description={t('legacySharedHint')} icon="members" testid="legacy-shared">
  <Collapsible>
    {#snippet title()}<span class="text-[12.5px] font-medium">{t('legacySteps')}</span>{/snippet}
    <div class="grid gap-3 pt-2 pb-1">
      {@render step(1, t('legacySource'))}
      <input bind:this={sourceFile} class="hidden" type="file" accept=".json,application/json" onchange={readFile} />
      <div class="flex flex-wrap items-center gap-2">
        <button class={buttonClass('secondary', 'sm')} onclick={() => sourceFile?.click()}><Icon name="upload" />{t('chooseFile')}</button>
        <span class="text-[12px] text-faint">{t('legacyPasteOr')}</span>
      </div>
      <textarea bind:value={sourceText} rows="3" class="font-mono text-[11.5px]" placeholder="legacy-shared-source.json"></textarea>
      <div class="flex flex-wrap items-end gap-2">
        <button class={buttonClass('secondary')} disabled={busy || !sourceText.trim()} onclick={checkSource}>{t('legacyCheckSource')}</button>
        <span class="flex-1"></span>
        <Field label={t('legacyVersion')}><input bind:value={version} class="w-20 tabular-nums" /></Field>
        <button class={buttonClass('primary')} disabled={busy || !sourceText.trim() || !/^\d+$/.test(version)} onclick={prepare}>{t('legacyPropose')}</button>
      </div>
      {#if source}
        <dl class="divide-y divide-line rounded-xl border border-line px-3.5">
          <Fact label={t('legacyManagers')}><span class="font-mono text-[11.5px] break-all">{source.managers.join(', ')}</span></Fact>
          <Fact label={t('legacyMembers')}><span class="font-mono text-[11.5px] break-all">{source.members.join(', ')}</span></Fact>
          <Fact label={t('legacyRange')}>{source.messageStart} – {source.messageEnd}</Fact>
        </dl>
      {/if}

      {@render step(2, t('legacyDraft'))}
      <textarea bind:value={draftText} rows="3" class="font-mono text-[11.5px]" placeholder="dmsg-shared-migration-draft/1"></textarea>
      <div class="flex flex-wrap items-end gap-2">
        <button class={buttonClass('secondary')} disabled={busy || !draftText.trim()} onclick={checkDraft}>{t('legacyCheckDraft')}</button>
        <Field label={t('legacyManager')} class="min-w-48 flex-1"><input bind:value={manager} class="font-mono text-[12px]" /></Field>
        <button class={buttonClass('secondary')} disabled={busy || !draftText.trim() || !manager.trim()} onclick={managerRequest}>{t('legacyRequest')}</button>
      </div>
      {#if request}
        <textarea readonly rows="3" class="font-mono text-[11px]" value={pretty(request.request)}></textarea>
        {@render links(request.links)}
      {/if}
      <Field label={t('legacyApproval')}><textarea bind:value={approvalText} rows="3" class="font-mono text-[11.5px]"></textarea></Field>
      <div class="flex flex-wrap gap-2">
        <button class={buttonClass('primary')} disabled={busy || !draftText.trim() || !approvalText.trim()} onclick={() => vote('legacy/shared_propose')}>{t('legacySubmitProposal')}</button>
        <button class={buttonClass('secondary')} disabled={busy || !draftText.trim() || !approvalText.trim()} onclick={() => vote('legacy/shared_consent')}>{t('legacySubmitConsent')}</button>
      </div>

      {@render step(3, t('legacyDirectory'))}
      <textarea bind:value={target} rows="2" class="font-mono text-[11.5px]" placeholder={t('legacyDirectoryHint')}></textarea>
      <button class={buttonClass('secondary', 'md', 'justify-self-start')} disabled={busy || !target.trim()} onclick={read}><Icon name="sync-now" />{t('refresh')}</button>
      {#if view}
        <div class="grid gap-3 rounded-xl border border-line p-3.5">
          <div class="flex flex-wrap items-center gap-2 text-[12.5px]">
            <Pill tone={view.stage === 'active' ? 'ok' : 'neutral'}>{view.stage}</Pill>
            <span>v{view.proposal.version}</span>
            <span class="text-faint">· {Object.keys(view.votes).length}/{view.source.managers.length} {t('legacyVotes')}</span>
            <span class="flex-1"></span>
            {#if view.proposal.owner === me}
              <button class={buttonClass('secondary', 'xs')} disabled={busy || view.stage !== 'pending'} onclick={() => act('legacy/shared_commit')}>{t('legacyCommit')}</button>
              <button class={buttonClass('primary', 'xs')} disabled={busy || !['committed', 'active'].includes(view.stage)} onclick={() => act('legacy/shared_activate')}>{t('legacyActivate')}</button>
            {/if}
          </div>
          <p class="font-mono text-[11px] break-all text-faint">{view.key}</p>
          <Collapsible>
            {#snippet title()}<span class="text-[12px]">{t('legacyExportView')}</span>{/snippet}
            <textarea readonly rows="4" class="font-mono text-[11px]" value={pretty(view)}></textarea>
          </Collapsible>
        </div>

        {@render step(4, t('legacyClaim'))}
        <div class="flex flex-wrap items-end gap-2">
          <Field label={t('legacyMember')} class="min-w-48 flex-1"><input bind:value={member} class="font-mono text-[12px]" /></Field>
          <button class={buttonClass('secondary')} disabled={busy || !member.trim()} onclick={askClaim}>{t('legacyRequest')}</button>
        </div>
        {#if claimRequest}
          <textarea readonly rows="3" class="font-mono text-[11px]" value={pretty(claimRequest.request)}></textarea>
          <div class="flex flex-wrap items-center gap-2">
            {@render links(claimRequest.links)}
            <span class="flex-1"></span>
            <button class={buttonClass('primary', 'sm')} disabled={busy || !approvalText.trim()} onclick={claim}>{t('legacySubmitClaim')}</button>
          </div>
        {/if}
        {#if claims.length}
          <ul class="divide-y divide-line rounded-xl border border-line">
            {#each claims as c (c.claim.member)}
              <li class="flex flex-wrap items-center gap-2 px-3.5 py-2">
                <span class="min-w-0 flex-1 font-mono text-[11.5px] break-all">{c.claim.member} → {c.claim.account}</span>
                {#if c.joined}<Pill tone="ok">{t('legacyJoined')}</Pill>{/if}
                {#if view.proposal.owner === me && !c.joined}
                  <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => invite(c.claim.account)}>{t('invite')}</button>
                {/if}
                {#if c.claim.account === me && !c.joined}
                  <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => act('legacy/shared_joined')}>{t('legacyMarkJoined')}</button>
                {/if}
              </li>
            {/each}
          </ul>
        {/if}
        {#if invitation}<textarea readonly rows="3" class="font-mono text-[11px]" value={invitation}></textarea>{/if}
        {#if view.stage === 'active'}
          {@render step(5, t('legacyGrant'))}
          <div class="flex flex-wrap items-center gap-2">
            <Select
              bind:value={grantArchive}
              placeholder={t('legacyArchives')}
              options={archives.map((a) => ({ value: a.id, label: `${a.principal} · ${a.objects}`, mono: true }))}
            />
            <Select
              bind:value={grantMember}
              placeholder={t('legacyMember')}
              options={claims.filter((c) => c.joined).map((c) => ({ value: c.claim.member, label: c.claim.member, mono: true }))}
            />
            <button
              class={buttonClass('primary')}
              disabled={busy || !grantArchive || !grantMember}
              onclick={() => act('legacy/shared_grant', { archive: grantArchive, member: grantMember })}
            >
              {t('legacyGrantSend')}
            </button>
          </div>
          <ul class="divide-y divide-line rounded-xl border border-line">
            {#each grants.filter((g) => g.grant.scope.account === me) as g (g.grant.scope.grant_id)}
              <li class="flex items-center gap-2 px-3.5 py-2">
                <span class="min-w-0 flex-1 font-mono text-[11.5px]">{g.grant.scope.member} · {g.grant.scope.archive_digest.slice(0, 16)}</span>
                <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => receive(g.grant.scope.grant_id)}>{t('legacyReceive')}</button>
              </li>
            {/each}
          </ul>
        {/if}
      {/if}
    </div>
  </Collapsible>
</Section>
