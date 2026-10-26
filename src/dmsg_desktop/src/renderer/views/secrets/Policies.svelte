<script lang="ts">
  import Panel from './Panel.svelte'
  import SourcePicker from './SourcePicker.svelte'
  import SearchSelect from '../../components/SearchSelect.svelte'
  import Field from '../../components/page/Field.svelte'
  import Id from '../../components/page/Id.svelte'
  import { panel } from './workflow-state.svelte'
  import { sourceItems, card, type SourceItem } from './model'
  import { openVault } from './navigation.svelte'
  import { call, when } from '../../rpc'
  import { buttonClass } from '../../components/ui'
  import { stateLabel, t } from '../../i18n/index.svelte'
  import { secretText as s } from './text'
  import { ux } from './experience-text'
  let {
    open,
    navigate,
    target = ''
  }: {
    open: (id: string) => void
    navigate: (tab: string, id?: string) => void
    target?: string
  } = $props()
  let policies = $state<any[]>([]),
    items = $state<SourceItem[]>([]),
    agents = $state<any[]>([]),
    grants = $state<any[]>([]),
    uses = $state<any[]>([])
  let editing = $state(false),
    selected = $state<any>(null),
    name = $state(''),
    owner = $state(''),
    repo = $state(''),
    itemId = $state(''),
    fieldId = $state(''),
    grantTo = $state(''),
    reference = $state('')
  let pages = $state<(string | undefined)[]>([undefined]),
    page = $state(0),
    next = $state<string | undefined>(),
    search = $state('')
  const model = panel(async () =>
    Promise.all([
      model.request(
        'policies',
        'secrets/policies/list',
        {},
        (p: any) => (policies = p.policies)
      ),
      model.request(
        'vault',
        'vault/list',
        {},
        (v: any[]) => (items = sourceItems(v))
      ),
      model.request('agents', 'agent/ids', {}, (a: any) => (agents = a.agents)),
      model.request('grants', 'grants/list', {}, (g: any[]) => (grants = g)),
      model.request(
        'history',
        'secrets/uses/list',
        { before: pages[page] },
        (u: any) => {
          uses = u.uses
          next =
            u.uses.length >= 50
              ? (u.next ?? u.uses.at(-1)?.request_id)
              : undefined
        }
      )
    ])
  )
  const shown = $derived(
    policies.filter(
      (p) =>
        !search ||
        `${p.policy.name} ${p.policy.target.owner}/${p.policy.target.repo} ${p.id}`
          .toLowerCase()
          .includes(search.toLowerCase())
    )
  )
  function edit(p: any = null) {
    selected = p
    editing = true
    name = p?.policy.name ?? ''
    owner = p?.policy.target.owner ?? ''
    repo = p?.policy.target.repo ?? ''
    itemId = p?.policy.credential.item_id ?? ''
    fieldId = p?.policy.credential.field_id ?? ''
    grantTo = ''
    reference = ''
  }
  async function save(event: SubmitEvent) {
    event.preventDefault()
    const source = { item_id: itemId, field_id: fieldId }
    const change = {
      ...(selected ? { id: selected.id, base_version: selected.version } : {}),
      ...(grantTo ? { grant_to: grantTo } : {}),
      policy: {
        schema_version: 1,
        name,
        enabled: true,
        adapter: 'github.issues/1',
        target: { api_origin: 'https://api.github.com', owner, repo },
        operations: ['issue.create']
      }
    }
    await model.act(
      async () => {
        const r: any = await call('secrets/ref', source)
        if (!model.state.alive) return
        return call('secrets/policies/save_request', {
          ...change,
          policy: { ...change.policy, credential: r.reference }
        })
      },
      (r: any) => {
        editing = false
        open(r.id)
      }
    )
  }
  const permitted = (id: string) =>
    grants.filter(
      (g) =>
        g.capabilities?.includes(`secret.use:${id}`) &&
        !g.revoked_at &&
        (!g.expires_at || g.expires_at > Date.now())
    )
</script>

<div data-testid="secret-policies">
  <Panel
    {model}
    title={ux('githubPolicies')}
    description={`${ux('policyIntro')} ${s('local')}`}
  >
    {#snippet actions()}<button
        class={buttonClass('primary')}
        disabled={model.state.busy}
        onclick={() => edit()}>{s('new')}</button
      >{/snippet}
    {#if !policies.length && !editing && model.state.loaded && !model.state.errors.policies}<div
        class={card}
      >
        <h3 class="font-medium">{ux('setup')}</h3>
        <p class="max-w-2xl text-sm leading-relaxed text-muted">
          {ux('policySteps')}
        </p>
        <button class={buttonClass('secondary')} onclick={() => openVault()}
          >{ux('openVault')}</button
        ><button
          class={buttonClass('ghost')}
          onclick={() => (location.hash = '/agents')}>{ux('openAgents')}</button
        >
      </div>{/if}
    {#if editing}<form
        onsubmit={save}
        class={card}
        data-testid="secret-policy-form"
      >
        <p class="text-sm text-muted">{s('scope')}</p>
        <Field label={s('name')}
          ><input bind:value={name} required maxlength="100" /></Field
        >
        <div class="grid gap-3 sm:grid-cols-2">
          <Field label={s('owner')}
            ><input
              bind:value={owner}
              required
              maxlength="39"
              pattern="[A-Za-z0-9][A-Za-z0-9-]*"
              placeholder="octocat"
            /></Field
          ><Field label={s('repo')}
            ><input
              bind:value={repo}
              required
              maxlength="100"
              pattern="[A-Za-z0-9_.-]+"
              placeholder="project"
            /></Field
          >
        </div>
        <SourcePicker
          {items}
          bind:itemId
          bind:fieldId
          concealed
          onChange={() => (reference = '')}
          testid="policy-source"
        />
        <Field label={s('agent')} hint={s('grantLifetime')}
          ><SearchSelect
            bind:value={grantTo}
            options={[
              { value: 'none', label: s('none') },
              ...agents.map((a) => ({
                value: a.agent_id,
                label: a.label,
                hint: a.agent_id,
                disabled: !!a.paused
              }))
            ]}
            label={s('agent')}
            placeholder={s('none')}
            onChange={() => {
              if (grantTo === 'none') grantTo = ''
            }}
            testid="policy-agent"
          /></Field
        >
        {#if reference}<Id
            id={reference}
            full
            wrap
            copyLabel={t('copy')}
          />{/if}
        <div class="flex flex-wrap gap-2">
          <button
            class={buttonClass('primary')}
            disabled={model.state.busy ||
              !fieldId ||
              !!items.find((i) => i.id === itemId)?.blocked}
            >{s('submit')}</button
          ><button
            type="button"
            class={buttonClass('secondary')}
            disabled={model.state.busy || !fieldId}
            onclick={() =>
              model.act(
                () =>
                  call('secrets/ref', { item_id: itemId, field_id: fieldId }),
                (r: any) => {
                  if (
                    r.reference.item_id === itemId &&
                    r.reference.field_id === fieldId
                  )
                    reference = r.uri
                },
                false
              )}>{s('reference')}</button
          ><button
            type="button"
            class={buttonClass('ghost')}
            onclick={() => (editing = false)}>{s('cancel')}</button
          >
        </div>
      </form>{/if}
    {#if policies.length}<Field label={ux('search')}
        ><input bind:value={search} type="search" /></Field
      >{/if}
    {#each shown as p (p.id)}<article
        class={card}
        class:ring-2={target === p.id}
        class:ring-forest={target === p.id}
        data-testid="secret-policy"
      >
        <div class="flex flex-wrap justify-between gap-2">
          <h3 class="font-semibold">{p.policy.name}</h3>
          <span class="text-xs text-muted"
            >{p.policy.enabled ? s('enabled') : s('disabled')} · {s('version')}
            {p.version}</span
          >
        </div>
        <p class="text-sm">
          GitHub · {p.policy.target.owner}/{p.policy.target.repo}
        </p>
        <button
          class={buttonClass('ghost')}
          onclick={() => openVault(p.policy.credential.item_id)}
          >{items.find((i) => i.id === p.policy.credential.item_id)?.title ??
            ux('openSource')} · {p.policy.credential.field_id}</button
        >
        <div>
          <p class="text-xs text-muted">{ux('requesters')}</p>
          {#each permitted(p.id) as grant}<p class="text-sm break-all">
              {agents.find((a) => a.agent_id === grant.grantee)?.label ??
                grant.grantee}
            </p>{:else}<p class="text-sm text-muted">
              {model.state.errors.grants ? ux('unknown') : s('none')}
            </p>{/each}
        </div>
        <details>
          <summary class="cursor-pointer text-sm text-forest"
            >{ux('setup')}</summary
          >
          <p class="my-2 text-sm text-muted">{ux('requestHelp')}</p>
          <Id id={p.id} full wrap copyLabel={t('copy')} />
        </details>
        <div class="flex flex-wrap gap-2">
          <button
            class={buttonClass('secondary')}
            disabled={model.state.busy}
            onclick={() => edit(p)}>{s('edit')}</button
          ><button
            class={buttonClass('secondary')}
            disabled={model.state.busy || !p.policy.enabled}
            onclick={() =>
              model.act(() => call('secrets/policies/disable', { id: p.id }))}
            >{s('disable')}</button
          ><button
            class={buttonClass('danger')}
            disabled={model.state.busy}
            onclick={() =>
              model.act(
                () =>
                  call('secrets/policies/delete_request', {
                    id: p.id,
                    base_version: p.version
                  }),
                (r: any) => open(r.id)
              )}>{s('delete')}</button
          ><button
            class={buttonClass('ghost')}
            onclick={() => navigate('activity', p.id)}>{s('history')}</button
          >
        </div>
      </article>{/each}
    <p class="text-xs text-muted">{s('revokeHint')}</p>
    <div class="border-t border-line pt-4">
      <h3 class="font-semibold">{s('history')}</h3>
      <p class="my-2 text-xs text-muted">{ux('policyHistory')}</p>
    </div>
    {#each uses as use (use.request_id)}<article
        class={card}
        data-testid="secret-use-record"
      >
        <div class="flex flex-wrap justify-between gap-2">
          <strong>{use.approval?.detail?.policy_name}</strong><span
            class="text-sm">{stateLabel(use.state)}</span
          >
        </div>
        <p class="text-xs text-muted">
          {agents.find((a) => a.agent_id === use.approval?.detail?.actor)
            ?.label ?? use.approval?.detail?.actor} · {when(use.created_at)}
        </p>
        <p class="whitespace-pre-wrap break-words text-sm">
          {use.approval?.detail?.arguments?.title}
        </p>
        {#if use.result}<p class="break-all text-sm">
            #{use.result.number} · {use.result.html_url}
          </p>{/if}{#if use.state === 'lost'}<p class="text-sm text-warn">
            {s('unknown')}
          </p>{/if}
        <div class="flex flex-wrap gap-2">
          <button
            class={buttonClass('secondary')}
            onclick={() => open(use.request_id)}>{s('review')}</button
          >{#if use.state === 'awaiting_user'}<button
              class={buttonClass('secondary')}
              disabled={model.state.busy}
              onclick={() =>
                model.act(() =>
                  call('secrets/uses/cancel', { request_id: use.request_id })
                )}>{s('cancel')}</button
            >{/if}{#if use.state === 'lost' && !use.reviewed_at}<button
              class={buttonClass('secondary')}
              disabled={model.state.busy}
              onclick={() =>
                model.act(() =>
                  call('secrets/uses/mark_reviewed', {
                    request_id: use.request_id
                  })
                )}>{s('mark')}</button
            >{/if}
        </div>
        {#if use.reviewed_at}<p class="text-xs text-muted">
            {s('reviewed')} · {when(use.reviewed_at)}
          </p>{/if}
      </article>{:else}{#if model.state.loaded && !model.state.errors.history}<p
          class="text-sm text-muted"
        >
          {s('noHistory')}
        </p>{/if}{/each}
    <div class="flex gap-2">
      <button
        class={buttonClass('secondary')}
        disabled={page === 0 || model.state.loading}
        onclick={() => {
          page--
          void model.refresh()
        }}>{ux('previous')}</button
      ><span class="self-center text-sm tabular-nums">{page + 1}</span><button
        class={buttonClass('secondary')}
        disabled={!next || model.state.loading}
        onclick={() => {
          pages = [...pages.slice(0, page + 1), next]
          page++
          void model.refresh()
        }}>{ux('next')}</button
      >
    </div>
  </Panel>
</div>
