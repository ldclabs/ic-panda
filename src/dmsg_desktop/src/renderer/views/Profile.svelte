<script lang="ts">
  import TextField from '../components/TextField.svelte'
  import TextArea from '../components/TextArea.svelte'

  import { onDestroy, onMount } from 'svelte'
  import type { PrincipalDocument, PrincipalLink } from 'agent-protocols/delegation'
  import Dialog from '../components/Dialog.svelte'
  import Collapsible from '../components/Collapsible.svelte'
  import { httpsUrl, profileChanged, profileText } from './profile-form'
  import Avatar from '../components/Avatar.svelte'
  import Icon from '../components/Icon.svelte'
  import Fact from '../components/page/Fact.svelte'
  import Field from '../components/page/Field.svelte'
  import Id from '../components/page/Id.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Section from '../components/page/Section.svelte'
  import Select from '../components/Select.svelte'
  import { buttonClass } from '../components/ui'
  import { t } from '../i18n/index.svelte'
  import { bridge, call, code, errorText, single } from '../rpc'

  /** The same certified PrincipalDocument used by JSON and human readers. */
  let { status }: { status: any } = $props()

  let alive = true
  onDestroy(() => { alive = false })
  let loaded = $state({ document: false, status: false })
  let loadErrors = $state<Record<string, string>>({})
  let editing = $state(false)
  let lookupOpen = $state(false)
  let lookupFor = $state('')
  let lookupBusy = $state(false)
  let lookupError = $state('')
  let lookupRevision = 0
  const imageReads = new Set<string>()
  const ready = $derived(loaded.document && loaded.status && !loadErrors.document && !loadErrors.status)
  let mine = $state<PrincipalDocument | null>(null)
  let avatarUrl = $state<string | null>(null)
  let extra = $state<Record<string, unknown>>({})
  let linkImages = $state<Record<string, string>>({})
  let pending = $state(false)
  let displayName = $state('')
  let kind = $state('person')
  let bio = $state('')
  let links = $state<PrincipalLink[]>([])
  let other = $state('')
  let found = $state<any>(undefined)
  let error = $state('')
  let busy = $state(false)
  let archives = $state<any[]>([])
  let archive = $state('')
  let fromLegacy = $state(false)
  let legacyAvatarReference = $state(false)
  const draft = $derived.by(() => {
    if (!mine) return null
    const keptExtra = { ...extra }
    const artwork = Object.fromEntries(Object.entries(linkImages).filter(([url, image]) => image && links.some(link => link.url === url)))
    if (Object.keys(artwork).length) keptExtra['dmsg.link_images'] = artwork
    else delete keptExtra['dmsg.link_images']
    return { ...mine, type: kind as PrincipalDocument['type'], name: displayName.trim() || undefined, description: bio.trim() || undefined, avatar_url: avatarUrl ?? undefined, links, extra: keptExtra }
  })
  const changed = $derived(!!mine && !!draft && profileChanged(mine, draft))
  const valid = $derived(profileText(displayName.trim(), 80) && profileText(bio.trim(), 2000, true) && links.length <= 100 && links.every(link => !!link.name.trim() && profileText(link.name, 128) && !!link.rel && new TextEncoder().encode(link.rel).length <= 64 && profileText(link.rel, 64) && httpsUrl(link.url) && (!linkImages[link.url] || httpsUrl(linkImages[link.url]))))

  function edit() {
    if (!ready || busy || !mine) return
    if (!pending) show($state.snapshot(mine))
    error = ''
    editing = true
  }

  /** Data URLs of the avatars shown, keyed by the document's complete URL. */
  let images = $state<Record<string, string>>({})

  async function attempt(work: () => Promise<void>) {
    if (busy) return
    busy = true
    error = ''
    try {
      await work()
    } catch (e) {
      if (!alive) return
      error = errorText(e)
    } finally {
      if (alive) busy = false
    }
  }

  /** Public images, including legacy URLs, are loaded through the daemon. */
  async function avatarOf(document: PrincipalDocument | null) {
    if (!document?.id) return
    const url = document.avatar_url
    if (!url || images[url] || imageReads.has(url)) return
    imageReads.add(url)
    try {
      const image: any = await call('profile/avatar_image', { avatar_url: url }).catch(() => null)
      if (alive && image) images = { ...images, [url]: image.url }
    } finally { imageReads.delete(url) }
  }

  function show(document: PrincipalDocument) {
    if (!alive) return
    if (!document) return
    mine = document
    fromLegacy = false
    legacyAvatarReference = false
    kind = document.type ?? 'person'
    displayName = document.name ?? ''
    bio = document.description ?? ''
    avatarUrl = document.avatar_url ?? null
    links = (document.links ?? []).map(link => ({ ...link }))
    extra = structuredClone(document.extra ?? {})
    linkImages = { ...(extra['dmsg.link_images'] as Record<string, string> ?? {}) }
    void avatarOf(document)
  }

  const loadDocument = single(async () => {
    if (!alive) return
    try {
      const document = await call('profile/show')
      if (alive) { show(document); loaded.document = true; loadErrors.document = '' }
    } catch (e) { if (alive) loadErrors.document = errorText(e) }
  })
  const loadStatus = single(async () => {
    if (!alive) return
    try {
      const [journal, publication] = await Promise.all([call<boolean>('profile/pending'), call('agent/principal')])
      if (alive) { pending = journal || (!!publication && !publication.published); loaded.status = true; loadErrors.status = '' }
    } catch (e) { if (alive) loadErrors.status = errorText(e) }
  })
  const loadArchives = single(async () => {
    if (!alive) return
    try {
      const list = await call<any[]>('legacy/list')
      if (alive) {
        archives = list.sort((a, b) => b.imported_at - a.imported_at)
        archive = archives[0]?.id ?? ''; loadErrors.archives = ''
      }
    } catch (e) { if (alive) loadErrors.archives = errorText(e) }
  })
  function load() { void loadDocument(); void loadStatus(); void loadArchives() }
  function retryLoad() {
    if (loadErrors.document) void loadDocument()
    if (loadErrors.status) void loadStatus()
    if (loadErrors.archives) void loadArchives()
  }

  const useLegacy = () => attempt(async () => {
    const selected = archive
    const old: any = await call('legacy/report', { id: selected, limit: 0 })
    if (!alive || selected !== archive) return
    displayName = old.profile.name
    bio = old.profile.bio
    links = old.profile.links ?? []
    extra = old.profile.extra ?? {}
    linkImages = { ...(extra['dmsg.link_images'] as Record<string, string> ?? {}) }
    fromLegacy = true
    legacyAvatarReference = false
    if (old.profile.image) { avatarUrl = old.profile.image; legacyAvatarReference = true }
    const image = old.files.find((file: any) => file.kind === 'avatar' && file.verified)
    if (image?.mime) {
      const url = await call('profile/avatar_legacy', { archive: selected, source: image.source })
      if (!alive || selected !== archive) return
      avatarUrl = url
      legacyAvatarReference = false
    }
    await avatarOf({ ...mine!, avatar_url: avatarUrl ?? undefined })
  })

  const avatar = (remove: boolean) => attempt(async () => {
    if (remove) { avatarUrl = null; legacyAvatarReference = false; return }
    const url = await bridge(() => window.dmsg.avatar())
    if (!alive) return
    if (typeof url === 'string') {
      avatarUrl = url
      legacyAvatarReference = false
      await avatarOf({ ...mine!, avatar_url: url })
    }
  })

  const publish = () => attempt(async () => {
    if (!mine || !ready || (!pending && (!changed || !valid))) return
    if (pending) {
      let outcome: any
      try { outcome = await call('profile/retry') } catch (cause) {
        if (code(cause) === 'EXPIRED') pending = false
        throw cause
      }
      if (!alive || !outcome.published) return
    } else {
      let outcome: any
      try { outcome = await call('profile/publish', draft) } catch (cause) {
        if (code(cause) === 'EXECUTION_UNKNOWN') pending = true
        if (code(cause) === 'PENDING') pending = await call<boolean>('profile/pending')
        throw cause
      }
      if (!alive) return
      if (!outcome.published) { pending = true; return }
    }
    pending = false
    fromLegacy = false
    show(await call('profile/show'))
  })

  function changeLinkUrl(index: number, url: string) {
    const old = links[index]!.url
    const image = linkImages[old]
    links[index]!.url = url
    if (image) linkImages[url] = image
    if (!links.some(link => link.url === old)) delete linkImages[old]
  }

  async function resolve() {
    if (lookupBusy || !other.trim()) return
    const turn = ++lookupRevision
    lookupFor = other.trim()
    found = undefined
    const account = other.trim()
    lookupBusy = true; lookupError = ''
    try {
      const result = await call('profile/resolve', { account })
      if (!alive || turn !== lookupRevision || account !== other.trim()) return
      found = result
      void avatarOf(result)
    } catch (e) { if (alive && turn === lookupRevision) lookupError = errorText(e) }
    finally { if (alive && turn === lookupRevision) lookupBusy = false }
  }

  onMount(() => {
    if (status?.state === 'unlocked') void load()
  })
</script>

{#snippet picture(published: any, size: string)}
  {#if published?.avatar_url && images[published.avatar_url]}
    <img class="{size} shrink-0 rounded-full object-cover ring-1 ring-line" src={images[published.avatar_url]} alt={t('avatar')} data-testid={'profile-avatar'} />
  {:else}
    <Avatar id={published?.id ?? status?.account ?? ''} name={published?.name || published?.id || status?.account || ''} size="lg" class="{size} rounded-full" />
  {/if}
{/snippet}

{#if error && !editing}<Notice failure={error} tone="danger">{error}</Notice>{/if}
{#each Object.values(loadErrors).filter(Boolean) as failure}<Notice failure={failure} tone="danger">{failure}<button class={buttonClass('secondary','sm')} onclick={retryLoad}>{t('retry')}</button></Notice>{/each}
<Section title={t('publicProfile')} icon="profile" testid="profile-summary">
  {#if mine}
    <div class="flex items-start gap-4">
      {@render picture(mine, 'size-16')}
      <dl class="min-w-0 flex-1">
        <Fact label={t('displayName')}>{mine.name || '-'}</Fact>
        <Fact label={t('bio')}>{mine.description || '-'}</Fact>
        {#each mine.links ?? [] as link}<Fact label={t('link')}><span class="break-all">{link.name} · {link.url}</span></Fact>{/each}
      </dl>
    </div>
  {:else if !Object.values(loadErrors).some(Boolean)}<p class="text-[13px] text-muted">{loaded.document ? '-' : t('loading')}</p>{/if}
  {#if pending}<Notice tone="warn">{t('principalPublicationPending')}</Notice>{/if}
  <div class="mt-3 flex flex-wrap gap-2">
    <button class={buttonClass('secondary', 'sm')} disabled={busy || !ready || !mine} onclick={edit} data-testid="profile-edit">{pending ? t('retry') : t('edit')}</button>
    <button class={buttonClass('secondary', 'sm')} onclick={() => (lookupOpen = true)} data-testid="profile-lookup-open"><Icon name="search" />{t('resolveProfile')}</button>
  </div>
</Section>

<Dialog bind:open={editing} title={t('publicProfile')} description={t('avatarHint')} icon="profile" wide dismissible={!busy} testid="profile">
    <div class="grid gap-4">
      {#if error}<Notice failure={error} tone="danger" testid="profile-edit-error">{error}</Notice>{/if}
      {#if archives.length}
        <Collapsible testid="profile-import">
          {#snippet title()}<span class="text-[12.5px]">{t('legacyUseProfile')}</span>{/snippet}
        <div class="flex flex-wrap items-center gap-2">
          <Select bind:value={archive} label={t('legacyArchives')} options={archives.map((a) => ({ value: a.id, label: a.principal, mono: true }))} disabled={busy || pending} testid="profile-legacy-archive" />
          <button class={buttonClass('secondary', 'sm')} disabled={busy || pending || !ready || !archive} onclick={useLegacy} data-testid="profile-use-legacy">{t('legacyUseProfile')}</button>
        </div>
        </Collapsible>
      {/if}
      {#if fromLegacy}<Notice>{t('legacyProfileDraft')}</Notice>{/if}
      {#if legacyAvatarReference}<Notice>{t('legacyAvatarReference')}</Notice>{/if}
      {#if pending}<Notice tone="warn">{t('principalPublicationPending')}</Notice>{/if}
      {#if !ready && !Object.values(loadErrors).some(Boolean)}<p class="text-[13px] text-muted">{t('loading')}</p>{/if}
      <fieldset disabled={busy || pending || !ready} class="contents">
      <div class="flex items-center gap-4">
        {@render picture(mine ? { ...mine, avatar_url: avatarUrl, name: displayName } : null, 'size-16')}
        <div class="flex flex-wrap gap-1.5">
          <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => avatar(false)} data-testid="avatar-set"><Icon name="upload" />{t('setAvatar')}</button>
          {#if avatarUrl}
            <button class={buttonClass('ghost', 'sm')} disabled={busy} onclick={() => avatar(true)} data-testid="avatar-remove">{t('removeAvatar')}</button>
          {/if}
        </div>
      </div>
      {#if avatarUrl}<TextField label={t('avatarAddress')} value={avatarUrl} readonly mono data-testid="profile-avatar-url" />{/if}
      <Field label={t('principalKind')}><Select bind:value={kind} label={t('principalKind')} options={[
        {value:'person',label:t('principalPerson')}, {value:'organization',label:t('principalOrganization')},
        {value:'team',label:t('principalTeam')}, {value:'project',label:t('principalProject')}, {value:'other',label:t('principalOther')}
      ]} testid="profile-kind" class="w-full" /></Field>
      <TextField label={t('displayName')} bind:value={displayName} error={!profileText(displayName.trim(), 80) ? t('settingsNameLimit') : ''} data-testid="profile-name" />
      <TextArea label={t('bio')} bind:value={bio} error={!profileText(bio.trim(), 2000, true) ? t('settingsBioLimit') : ''} rows={3} data-testid="profile-bio" />
      <div class="grid gap-2">
        {#each links as link, index (index)}
          <div class="grid gap-3 border-b border-line py-3 @min-[600px]:grid-cols-2" data-testid="profile-link-row">
            <TextField label={t('principalLinkName')} bind:value={link.name} error={!link.name.trim() || !profileText(link.name, 128) ? t('settingsLinkName') : ''} />
            <TextField label={t('principalLinkRelation')} bind:value={link.rel} error={!link.rel || !profileText(link.rel, 64) || new TextEncoder().encode(link.rel).length > 64 ? t('settingsLinkRelation') : ''} />
            <TextField label={t('link')} value={link.url} oninput={(event) => changeLinkUrl(index, event.currentTarget.value)} type="url" dir="ltr" error={!httpsUrl(link.url) ? t('settingsHttps') : ''} />
            <TextField label={t('principalLinkImage')} value={linkImages[link.url] ?? ''} oninput={(event) => linkImages[link.url] = event.currentTarget.value} type="url" dir="ltr" error={linkImages[link.url] && !httpsUrl(linkImages[link.url]) ? t('settingsHttps') : ''} />
            <button class={buttonClass('ghost', 'sm', 'justify-self-start')} onclick={() => links = links.filter((_, i) => i !== index)} aria-label={`${t('remove')} · ${link.name || t('link')}`}>{t('remove')}</button>
          </div>
        {/each}
        <button class={buttonClass('secondary', 'sm')} disabled={links.length >= 100} onclick={() => links = [...links, { name: '', url: '', rel: 'related' }]}>{t('principalAddLink')}</button>
      </div>
      </fieldset>
    </div>
  {#snippet footer()}
    {#if !changed && !pending}<span class="text-[12px] text-faint">{t('securityUnchanged')}</span>{/if}
    <span class="flex-1"></span>
    <button class={buttonClass('secondary')} disabled={busy} onclick={() => { editing = false; error = '' }}>{t('close')}</button>
    <button class={buttonClass('primary')} disabled={busy || !ready || (!pending && (!changed || !valid))} onclick={publish} data-testid="profile-publish">{pending ? t('retry') : t('publish')}</button>
  {/snippet}
</Dialog>

<Dialog bind:open={() => lookupOpen, (value) => { lookupOpen = value; if (!value) { lookupRevision++; lookupBusy = false; found = undefined } }} title={t('resolveProfile')} icon="search" testid="profile-lookup">
  <div class="grid gap-3">
    <div class="flex flex-wrap items-end gap-2">
      <TextField label={t('accountId')} bind:value={other} oninput={() => { lookupRevision++; found = undefined; lookupError = ''; lookupBusy = false }} mono class="min-w-48 flex-1" data-testid="profile-lookup-account" />
      <button class={buttonClass('secondary')} disabled={lookupBusy || !other.trim()} onclick={resolve}>{t('lookUp')}</button>
    </div>
    {#if lookupError}<Notice failure={lookupError} tone="danger">{lookupError}</Notice>{/if}
    {#if found && lookupFor === other.trim()}
      <div class="flex flex-wrap items-start gap-3 rounded-lg bg-surface-2 p-3.5" data-testid="profile-found">
        {@render picture(found, 'size-12')}
        <dl class="min-w-0 flex-1 divide-y divide-line">
          <Fact label={t('displayName')}>{found.name || '-'}</Fact>
          <Fact label={t('bio')}>{found.description || '-'}</Fact>
          {#each (found.links ?? []) as link, i (i)}<Fact label={t('link')}><bdi dir="ltr" class="font-mono text-[12px]">{link.name} · {link.url}</bdi></Fact>{/each}
          <Fact label={t('principalId')}><Id id={found.id} /></Fact>
        </dl>
        {#if status?.state === 'unlocked' && found.id.split('/').pop() !== status.account}
          <button
            class={buttonClass('secondary', 'sm', 'shrink-0')}
            onclick={() => (location.hash = `/channels?direct=${found.id.split('/').pop()}&name=${encodeURIComponent(found.name || '')}`)}
            data-testid="profile-converse"><Icon name="channels" />{t('startConversation')}</button
          >
        {/if}
      </div>
    {:else if found === null}
      <p class="text-[12.5px] text-faint">-</p>
    {/if}
  </div>
</Dialog>
