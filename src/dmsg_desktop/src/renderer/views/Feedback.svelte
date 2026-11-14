<script lang="ts">
  import { onMount, onDestroy, untrack } from 'svelte'
  import Dialog from '../components/Dialog.svelte'
  import TextArea from '../components/TextArea.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Section from '../components/page/Section.svelte'
  import Icon from '../components/Icon.svelte'
  import { buttonClass } from '../components/ui'
  import { feedback as form, feedbackRecipient, type FeedbackScreenshot } from '../feedback.svelte'
  import { t } from '../i18n/index.svelte'
  import { bridge, call, code, errorText, CallError } from '../rpc'

  let { status }: { status: any } = $props()
  let importing = $state(false)
  let importError = $state('')
  let discarding = $state(false)
  let images = $state<Record<string, string>>({})
  let alive = true
  const owner = untrack(() => status.account)
  const valid = () => alive && status.state === 'unlocked' && status.account === owner
  const frozen = $derived(form.busy || importing || !form.loaded || (!!form.record && form.record.state !== 'draft'))
  const bytes = $derived(new TextEncoder().encode(form.text).length)
  const failure = $derived(importError || form.failure || form.record?.error || '')
  const failureText = $derived(failure === 'VERSION_CONFLICT' && form.record?.state === 'pending' ? t('letterPendingContact')
    : failure === 'REKEY_REQUIRED' ? t('letterRecipientKeyStale')
    : failure === 'FORBIDDEN' ? t('letterContactNotAllowed')
    : failure ? errorText(new CallError(failure)) : '')

  async function load() {
    const info = await bridge(() => window.dmsg.updates()).catch(() => null)
    if (valid()) await form.load(`dMsg desktop ${info?.version ?? ''} · ${window.dmsg.platform}`)
  }
  onMount(() => { if (status.state === 'unlocked') void load() })
  onDestroy(() => { alive = false; images = {} })

  async function add(pasted?: File[]) {
    if (frozen) return
    importing = true; importError = ''
    try {
      const result: any = await bridge(() => pasted ? window.dmsg.filesPaste('root', pasted) : window.dmsg.filesAdd('root', undefined, false, true))
      if (!valid() || !result) return
      const next: FeedbackScreenshot[] = result.files.map((r: any) => r.file).filter(Boolean).map((f: any) => ({ file: f.id, version: f.current, name: f.name, size: f.size, mime: f.mime }))
      if (result.files.some((r: any) => !r.file) || next.some((s: any) => !['image/png', 'image/jpeg', 'image/webp', 'image/gif'].includes(s.mime) || s.size > 10 * 1024 * 1024)
        || new Set([...form.screenshots, ...next].map(s => s.file)).size > 3) { importError = 'INVALID_INPUT'; return }
      form.update(form.text, [...form.screenshots, ...next.filter(s => !form.screenshots.some(old => old.file === s.file))])
    } catch (e) { if (valid()) importError = code(e) }
    finally { if (valid()) importing = false }
  }
  function paste(event: ClipboardEvent) {
    const files = [...(event.clipboardData?.files ?? [])]
    if (files.length && !frozen) { event.preventDefault(); void add(files) }
  }
  $effect(() => {
    for (const screenshot of form.screenshots) {
      if (images[screenshot.file]) continue
      void call('files/preview', { id: screenshot.file, version: screenshot.version }).then((p: any) => {
        if (valid() && form.screenshots.some(s => s.file === screenshot.file) && p.kind === 'image' && p.content_base64url) {
          images[screenshot.file] = `data:${p.mime};base64,${p.content_base64url.replace(/-/g, '+').replace(/_/g, '/')}`
        }
      }).catch(() => {})
    }
  })
  function key(event: KeyboardEvent) {
    if (event.key === 'Enter' && (event.metaKey || event.ctrlKey) && !frozen && bytes <= 4500 && (form.text.trim() || form.screenshots.length)) {
      event.preventDefault(); void form.send()
    }
  }
</script>

<Section title={t('feedbackTitle')} description={t('feedbackHint')}>
  <p class="text-[12px] text-muted">{t('to')}: <bdi class="select-text font-mono" dir="ltr">{feedbackRecipient}</bdi></p>
  {#if status.state !== 'unlocked'}
    <Notice>{t('feedbackLocked')}</Notice>
  {:else}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="mt-4 grid gap-4" onpaste={paste} onkeydown={key} data-testid="feedback-form">
      {#if !form.loaded}
        <button class={buttonClass('secondary')} disabled={form.busy} onclick={load}>{t('retry')}</button>
      {:else}
        <div class="grid gap-1">
          <TextArea label={t('message')} value={form.text} oninput={(e) => form.update(e.currentTarget.value)} readonly={!!form.record && form.record.state !== 'draft'} disabled={form.busy} dir="auto"
            rows={6} aria-describedby="feedback-count" error={bytes > 4500 ? t('feedbackLimit') : undefined} data-testid="feedback-text" />
          <p id="feedback-count" class="text-[12px] text-muted"><bdi dir="ltr">{bytes} / 4500 B</bdi></p>
        </div>
        <div class="grid gap-2">
          <p class="text-[12.5px] font-medium">{t('feedbackScreenshots')}</p>
          <p class="text-[12px] text-muted">{t('feedbackImageHint')}</p>
          <div class="flex flex-wrap gap-2">
            {#each form.screenshots as screenshot (screenshot.file)}
              <div class="grid w-36 gap-1.5 rounded-lg border border-line p-2" data-testid="feedback-screenshot">
                {#if images[screenshot.file]}<img src={images[screenshot.file]} alt={screenshot.name} class="h-24 w-full rounded object-contain" />{:else}<Icon name="file" class="h-24 w-full text-faint" />{/if}
                <span class="break-all text-[11px]">{screenshot.name}</span>
                {#if !frozen}<button class={buttonClass('ghost', 'xs')} onclick={() => form.update(form.text, form.screenshots.filter(s => s.file !== screenshot.file))}>{t('remove')}</button>{/if}
              </div>
            {/each}
          </div>
          <button class={buttonClass('secondary', 'sm', 'justify-self-start')} disabled={frozen || form.screenshots.length >= 3} onclick={() => add()} data-testid="feedback-attach"><Icon name="attach" />{t('feedbackAttach')}</button>
        </div>
        <p class="break-words text-[12px] text-muted" data-testid="feedback-context">{form.context}</p>
        <p class="text-[12px] text-muted" role="status">{form.saving || form.dirty ? t('feedbackSaving') : form.record?.state === 'sent' ? t('feedbackSent') : form.record?.state === 'pending' ? t('feedbackPending') : form.record ? t('feedbackSaved') : ''}</p>
        <div class="flex flex-wrap justify-end gap-2">
          {#if form.record?.state === 'sent'}
            <button class={buttonClass('primary')} onclick={() => form.clear()} data-testid="feedback-new">{t('feedbackNew')}</button>
          {:else}
            {#if form.record?.state === 'pending' && form.record.editable}<button class={buttonClass('secondary')} disabled={form.busy} onclick={() => form.edit()}>{t('edit')}</button>{/if}
            {#if !form.record || form.record.state === 'draft'}<button class={buttonClass('ghost')} disabled={form.busy || importing} onclick={() => discarding = true}>{t('discard')}</button>{/if}
            <button class={buttonClass('primary')} disabled={form.busy || importing || bytes > 4500 || (!form.text.trim() && !form.screenshots.length)} onclick={() => form.send()} data-testid="feedback-send">{form.record?.state === 'pending' ? t('retry') : t('send')}</button>
          {/if}
        </div>
      {/if}
      {#if failure}<Notice failure={failure} tone="danger" testid="feedback-error">{failureText}{#if form.dirty}<button class={buttonClass('secondary', 'sm')} onclick={() => form.flush()}>{t('retry')}</button>{/if}</Notice>{/if}
    </div>
  {/if}
</Section>
<Dialog bind:open={discarding} title={t('discard')} description={t('feedbackDiscardHint')}>
  <p class="text-[13px]">{t('feedbackDiscardTitle')}</p>
  {#snippet footer()}<button class={buttonClass('ghost')} onclick={() => discarding = false}>{t('cancel')}</button><button class={buttonClass('danger')} onclick={async () => { await form.clear(); discarding = false }}>{t('discard')}</button>{/snippet}
</Dialog>
