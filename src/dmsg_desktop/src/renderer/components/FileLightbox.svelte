<script lang="ts">
  import { Dialog, DropdownMenu } from 'bits-ui'
  import { onDestroy } from 'svelte'
  import type { FallbackReason, MarkdownDocument, PreviewMeta } from '../../shared/contract'
  import { localeDir } from '../../shared/locales'
  import { locale, t } from '../i18n/index.svelte'
  import type { Label } from '../i18n/messages/en'
  import { bridge, code, errorText, size } from '../rpc'
  import { copyText } from './clipboard'
  import Icon from './Icon.svelte'
  import { leaving } from './link-confirm.svelte'
  import MarkdownView from './MarkdownView.svelte'
  import Notice from './page/Notice.svelte'
  import {
    clampZoom,
    filterCounts,
    itemPreviewClass,
    matchesPreviewFilter,
    pickNearestFiltered,
    PREVIEW_FILTERS,
    stepZoom,
    ZOOM_MAX,
    ZOOM_MIN,
    type PreviewFilter,
    type PreviewItem
  } from './preview'
  import { toast } from './toast.svelte'
  import Tooltip from './Tooltip.svelte'
  import { buttonClass, cn, menuContentClass, menuItemClass } from './ui'

  /**
   * Fullscreen file preview lightbox shared by Files and Channels:
   * raster images with fit/zoom/pan, safe Markdown AST reading + source toggle,
   * plain text, clean metadata fallback for unsupported/oversized/cloud-only files,
   * keyboard navigation, and inline filter bar with nearest-neighbor focus preservation.
   */
  let {
    open = $bindable(false),
    items,
    activeKey = $bindable<string | null>(null),
    showFilter = true,
    onActiveChange,
    onDownload,
    onSave,
    onOpen
  }: {
    open?: boolean
    items: PreviewItem[]
    activeKey?: string | null
    showFilter?: boolean
    onActiveChange?: (item: PreviewItem) => void
    onDownload?: (item: PreviewItem) => void
    onSave?: (item: PreviewItem) => void
    onOpen?: (item: PreviewItem) => void
  } = $props()

  type LoadedState =
    | { kind: 'loading' }
    | { kind: 'image'; meta: PreviewMeta; url: string; bytes: Uint8Array }
    | { kind: 'markdown'; meta: PreviewMeta; document: MarkdownDocument; text: string }
    | { kind: 'text'; meta: PreviewMeta; text: string }
    | { kind: 'fallback'; meta: PreviewMeta; reason: FallbackReason }
    | { kind: 'error'; message: string }

  const REQUEST_ID = 'lightbox'

  let filter = $state<PreviewFilter>('all')
  let loaded = $state.raw<LoadedState>({ kind: 'loading' })
  let mdMode = $state<'reading' | 'source'>('reading')
  let zoomMode = $state<'fit' | 'manual'>('fit')
  let manualScale = $state(1)
  let naturalWidth = $state(0)
  let naturalHeight = $state(0)
  let stageWidth = $state(0)
  let stageHeight = $state(0)
  let panX = $state(0)
  let panY = $state(0)
  let dragging = $state(false)
  let dragStart = { x: 0, y: 0, panX: 0, panY: 0, moved: false }
  let suppressBackdropClick = false

  let currentObjectUrl: string | null = null
  let loadGeneration = 0
  let loadedForKey: string | null = null

  const counts = $derived(filterCounts(items))
  const filteredItems = $derived(items.filter((item) => matchesPreviewFilter(item, filter)))
  const activeIndex = $derived.by(() => {
    if (!filteredItems.length) return -1
    const idx = activeKey ? filteredItems.findIndex((item) => item.key === activeKey) : -1
    return idx >= 0 ? idx : 0
  })
  const activeItem = $derived(activeIndex >= 0 ? filteredItems[activeIndex] : null)
  const hasPrev = $derived(activeIndex > 0)
  const hasNext = $derived(activeIndex >= 0 && activeIndex < filteredItems.length - 1)
  const isRtl = $derived(localeDir(locale()) === 'rtl')
  const dateFormat = $derived(new Intl.DateTimeFormat(locale(), { dateStyle: 'medium', timeStyle: 'short' }))

  const fitScale = $derived.by(() => {
    if (!naturalWidth || !naturalHeight || !stageWidth || !stageHeight) return 1
    return Math.min(1, stageWidth / naturalWidth, stageHeight / naturalHeight)
  })
  const effectiveScale = $derived(zoomMode === 'fit' ? fitScale : manualScale)
  const displayPercent = $derived(Math.max(1, Math.round(effectiveScale * 100)))
  const canPan = $derived(
    loaded.kind === 'image' &&
      naturalWidth > 0 &&
      naturalHeight > 0 &&
      (naturalWidth * effectiveScale > stageWidth + 1 || naturalHeight * effectiveScale > stageHeight + 1)
  )

  const filterLabel: Record<PreviewFilter, Label> = {
    all: 'previewFilterAll',
    previewable: 'previewFilterPreviewable',
    images: 'previewFilterImages',
    documents: 'previewFilterDocuments'
  }

  const fallbackLabel: Record<FallbackReason, Label> = {
    unsupported_type: 'previewUnsupported',
    too_large: 'previewTooLarge',
    cloud_only: 'previewCloudOnly',
    decode_failed: 'previewDecodeFailed',
    unavailable: 'previewUnavailable'
  }

  function revokeUrl() {
    if (currentObjectUrl) {
      URL.revokeObjectURL(currentObjectUrl)
      currentObjectUrl = null
    }
  }

  function resetView() {
    mdMode = 'reading'
    zoomMode = 'fit'
    manualScale = 1
    naturalWidth = 0
    naturalHeight = 0
    panX = 0
    panY = 0
    dragging = false
  }

  function cleanup() {
    loadGeneration++
    loadedForKey = null
    revokeUrl()
    loaded = { kind: 'loading' }
    void window.dmsg.previewCancel(REQUEST_ID).catch(() => {})
  }

  onDestroy(cleanup)

  // Reset filter when opening a fresh session or closing.
  $effect(() => {
    if (!open) {
      filter = 'all'
      resetView()
      cleanup()
    }
  })

  // Keep activeKey reconciled with filteredItems if items list updates (e.g. channel/attachments arrives or filter removes current item).
  $effect(() => {
    if (!open) return
    if (!items.length) {
      open = false
      return
    }
    if (!filteredItems.length) {
      filter = 'all'
      return
    }
    if (!activeKey || !filteredItems.some((item) => item.key === activeKey)) {
      const { activeKey: next } = pickNearestFiltered(items, activeKey, filter)
      if (next) activeKey = next
    }
  })

  // Notify parent when active item changes (e.g., to sync list selection in Files).
  $effect(() => {
    if (open && activeItem) onActiveChange?.(activeItem)
  })

  // Load preview whenever open + activeItem changes.
  $effect(() => {
    if (!open || !activeItem) return
    const key = activeItem.key
    if (loadedForKey === key) return
    loadedForKey = key
    resetView()
    void loadItem(activeItem)
  })

  async function loadItem(item: PreviewItem) {
    const gen = ++loadGeneration
    revokeUrl()
    loaded = { kind: 'loading' }
    try {
      const result = await bridge(() => window.dmsg.previewFile(item.source, REQUEST_ID))
      if (gen !== loadGeneration || !open) return
      if (result.kind === 'image') {
        const blob = new Blob([result.bytes.slice()], { type: result.meta.mime || item.mime || 'application/octet-stream' })
        const url = URL.createObjectURL(blob)
        currentObjectUrl = url
        loaded = { kind: 'image', meta: result.meta, url, bytes: result.bytes }
      } else {
        const p = result.payload
        if (p.reason) {
          loaded = { kind: 'fallback', meta: p, reason: p.reason }
        } else if (p.kind === 'markdown' && p.document) {
          loaded = { kind: 'markdown', meta: p, document: p.document, text: p.text ?? '' }
        } else if (p.kind === 'text' && p.text !== undefined) {
          loaded = { kind: 'text', meta: p, text: p.text }
        } else {
          loaded = { kind: 'fallback', meta: p, reason: 'unsupported_type' }
        }
      }
    } catch (error) {
      if (gen !== loadGeneration || !open) return
      if (code(error) === 'CANCELLED') return
      loaded = { kind: 'error', message: errorText(error) }
    }
  }

  function applyFilter(next: PreviewFilter) {
    if (next === filter || counts[next] === 0) return
    const { activeKey: nextKey } = pickNearestFiltered(items, activeKey, next)
    filter = next
    if (nextKey) activeKey = nextKey
  }

  function step(delta: -1 | 1) {
    const next = filteredItems[activeIndex + delta]
    if (next) activeKey = next.key
  }

  function zoomBy(direction: 1 | -1) {
    const next = stepZoom(effectiveScale, direction)
    zoomMode = 'manual'
    manualScale = next
    if (next <= fitScale + 0.01) { panX = 0; panY = 0 }
  }

  function toggleFitOrActual() {
    if (zoomMode === 'fit') {
      zoomMode = 'manual'
      manualScale = Math.abs(fitScale - 1) < 0.02 ? 2 : 1
    } else {
      zoomMode = 'fit'
      panX = 0
      panY = 0
    }
  }

  /** Formats the window cannot draw, such as HEIC, or damaged bytes: the fallback card with its actions. */
  function onImageError(event: Event) {
    const current = loaded
    if (current.kind !== 'image' || (event.currentTarget as HTMLImageElement).src !== current.url) return
    revokeUrl()
    loaded = { kind: 'fallback', meta: current.meta, reason: 'decode_failed' }
  }

  function onImageDblClick() {
    if (zoomMode === 'fit') {
      zoomMode = 'manual'
      manualScale = clampZoom(Math.max(2, fitScale * 2))
    } else {
      zoomMode = 'fit'
      panX = 0
      panY = 0
    }
  }

  function onImageWheel(event: WheelEvent) {
    if (loaded.kind !== 'image') return
    event.preventDefault()
    const factor = event.deltaY < 0 ? 1.12 : 1 / 1.12
    const next = clampZoom(effectiveScale * factor)
    zoomMode = 'manual'
    manualScale = next
    if (next <= fitScale + 0.01) { panX = 0; panY = 0 }
  }

  function onPointerDown(event: PointerEvent) {
    if (!canPan || event.button !== 0) return
    event.preventDefault()
    ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
    dragging = true
    dragStart = { x: event.clientX, y: event.clientY, panX, panY, moved: false }
  }

  function onPointerMove(event: PointerEvent) {
    if (!dragging) return
    const dx = event.clientX - dragStart.x
    const dy = event.clientY - dragStart.y
    if (Math.abs(dx) > 3 || Math.abs(dy) > 3) dragStart.moved = true
    panX = dragStart.panX + dx
    panY = dragStart.panY + dy
  }

  function onPointerUp(event: PointerEvent) {
    if (!dragging) return
    dragging = false
    if ((event.currentTarget as HTMLElement).hasPointerCapture?.(event.pointerId)) {
      ;(event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId)
    }
    if (dragStart.moved) {
      suppressBackdropClick = true
      setTimeout(() => { suppressBackdropClick = false }, 0)
    }
  }

  /** The clipboard takes PNG or JPEG; other formats are re-encoded as PNG, at most 4096 px on a side. */
  async function clipboardImage(bytes: Uint8Array, mime: string): Promise<Uint8Array> {
    if (mime === 'image/png' || mime === 'image/jpeg' || mime === 'image/jpg') return bytes
    const bitmap = await createImageBitmap(new Blob([bytes.slice()], { type: mime }))
    const scale = Math.min(1, 4096 / Math.max(bitmap.width, bitmap.height))
    const canvas = new OffscreenCanvas(Math.max(1, Math.round(bitmap.width * scale)), Math.max(1, Math.round(bitmap.height * scale)))
    canvas.getContext('2d')?.drawImage(bitmap, 0, 0, canvas.width, canvas.height)
    bitmap.close()
    return new Uint8Array(await (await canvas.convertToBlob({ type: 'image/png' })).arrayBuffer())
  }

  async function copyContent() {
    const current = loaded
    if (current.kind === 'image') {
      try {
        const bytes = await clipboardImage(current.bytes, current.meta.mime)
        await bridge(() => window.dmsg.copyImage(bytes))
        toast(() => t('copiedText'))
      } catch {
        toast(() => t('copyFailed'))
      }
    } else if (current.kind === 'markdown' || current.kind === 'text') {
      await copyText(current.text)
    }
  }

  function onWindowKey(event: KeyboardEvent) {
    if (!open || event.isComposing || event.defaultPrevented || leaving.url !== null) return
    if (document.querySelector('[role="menu"]')) return
    const held = event.metaKey || event.ctrlKey
    if (event.key === 'ArrowLeft') {
      event.preventDefault()
      step(isRtl ? 1 : -1)
    } else if (event.key === 'ArrowRight') {
      event.preventDefault()
      step(isRtl ? -1 : 1)
    } else if (event.key === ' ' && !held && !event.altKey) {
      const target = event.target instanceof HTMLElement ? event.target : null
      if (!target || target.tagName !== 'BUTTON') {
        event.preventDefault()
        open = false
      }
    } else if (loaded.kind === 'image') {
      if (event.key === '+' || event.key === '=') {
        event.preventDefault()
        zoomBy(1)
      } else if (event.key === '-' || event.key === '_') {
        event.preventDefault()
        zoomBy(-1)
      } else if (event.key === '0') {
        event.preventDefault()
        zoomMode = 'fit'
        panX = 0
        panY = 0
      } else if (event.key === '1') {
        event.preventDefault()
        zoomMode = 'manual'
        manualScale = 1
        panX = 0
        panY = 0
      }
    }
  }

  function onStageClick(event: MouseEvent) {
    if (suppressBackdropClick) return
    if (event.target === event.currentTarget) open = false
  }

  function iconFor(item: PreviewItem) {
    const cls = itemPreviewClass(item)
    if (cls === 'image') return 'image' as const
    if (cls === 'markdown' || cls === 'text') return 'file-text' as const
    return 'file' as const
  }

  const circleBtn =
    'grid size-9 shrink-0 place-items-center rounded-full border border-lightbox-control-border bg-lightbox-control text-lightbox-control-ink shadow-pop outline-none transition-[background-color,transform,opacity] duration-150 hover:not-disabled:bg-lightbox-control-hover active:not-disabled:scale-95 focus-visible:ring-3 focus-visible:ring-focus/45 disabled:cursor-not-allowed disabled:opacity-40 [&_svg]:size-4.5'

  const canDownload = $derived(!!onDownload && !!activeItem && activeItem.status !== 'cloud_only')
  const canOpenExternally = $derived(!!onOpen && !!activeItem && activeItem.status !== 'cloud_only')
  const canCopy = $derived(loaded.kind === 'image' || loaded.kind === 'markdown' || loaded.kind === 'text')
  const showFilterBar = $derived(showFilter && items.length > 1)
</script>

<svelte:window onkeydown={onWindowKey} />

<Dialog.Root bind:open>
  <Dialog.Portal>
    <Dialog.Overlay
      class="fixed inset-0 z-40 bg-scrim backdrop-blur-[2px] data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:animate-in data-[state=open]:fade-in-0"
    />
    <Dialog.Content
      class="@container fixed inset-0 z-40 flex flex-col outline-none data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:animate-in data-[state=open]:fade-in-0"
      data-testid="file-lightbox"
    >
      <Dialog.Title class="sr-only">{activeItem?.name ?? t('preview')}</Dialog.Title>
      <Dialog.Description class="sr-only">
        {filteredItems.length ? `${activeIndex + 1} / ${filteredItems.length}` : ''}
      </Dialog.Description>

      <!-- Top floating chrome -->
      <header class="pointer-events-none relative z-10 flex items-center gap-3 px-4 pt-3.5 pb-2">
        <!-- File name & counter pill -->
        {#if activeItem}
          <div
            class="pointer-events-auto flex min-w-0 max-w-[min(40vw,360px)] items-center gap-2 rounded-full border border-lightbox-border bg-lightbox-bar px-3.5 py-1.5 text-lightbox-ink shadow-pop backdrop-blur-md"
            data-testid="lightbox-meta"
          >
            <Icon name={iconFor(activeItem)} class="size-4 shrink-0 text-lightbox-muted" />
            <bdi class="min-w-0 truncate text-callout font-medium" title={activeItem.name} data-testid="lightbox-filename">{activeItem.name}</bdi>
            {#if activeItem.versionLabel}
              <span class="shrink-0 rounded-full bg-lightbox-bar-hover px-2 py-0.5 text-caption text-lightbox-muted">{activeItem.versionLabel}</span>
            {/if}
            {#if filteredItems.length > 1}
              <span dir="ltr" class="shrink-0 text-caption text-lightbox-muted tabular-nums" data-testid="lightbox-counter">{activeIndex + 1} / {filteredItems.length}</span>
            {/if}
          </div>
        {/if}

        <!-- Filter bar: segmented on wide windows, dropdown pill on narrow windows -->
        {#if showFilterBar}
          <div class="pointer-events-auto mx-auto hidden @min-[920px]:flex">
            <div
              class="inline-flex items-center gap-0.5 rounded-full border border-lightbox-border bg-lightbox-bar p-0.5 shadow-pop backdrop-blur-md"
              role="group"
              aria-label={t('previewFilter')}
              data-testid="lightbox-filter-segmented"
            >
              {#each PREVIEW_FILTERS as option (option)}
                {@const count = counts[option]}
                {@const active = filter === option}
                <button
                  type="button"
                  disabled={count === 0}
                  aria-pressed={active}
                  onclick={() => applyFilter(option)}
                  data-testid="lightbox-filter-{option}"
                  class={cn(
                    'inline-flex h-7 items-center gap-1.5 rounded-full px-2.5 text-caption transition-colors duration-150 outline-none focus-visible:ring-3 focus-visible:ring-focus/45 disabled:cursor-not-allowed disabled:opacity-40',
                    active
                      ? 'bg-lightbox-control font-semibold text-lightbox-control-ink shadow-control'
                      : 'text-lightbox-ink hover:not-disabled:bg-lightbox-bar-hover'
                  )}
                >
                  <span>{t(filterLabel[option])}</span>
                  <span class={cn('tabular-nums', active ? 'opacity-70' : 'text-lightbox-muted')}>{count}</span>
                </button>
              {/each}
            </div>
          </div>

          <div class="pointer-events-auto mx-auto flex @min-[920px]:hidden">
            <DropdownMenu.Root dir={localeDir(locale())}>
              <DropdownMenu.Trigger
                class="inline-flex h-8 items-center gap-1.5 rounded-full border border-lightbox-border bg-lightbox-bar px-3 text-caption font-medium text-lightbox-ink shadow-pop backdrop-blur-md outline-none hover:bg-lightbox-bar-hover focus-visible:ring-3 focus-visible:ring-focus/45"
                aria-label={t('previewFilter')}
                data-testid="lightbox-filter-dropdown"
              >
                <Icon name="filter" class="size-3.5 text-lightbox-muted" />
                <span>{t(filterLabel[filter])}</span>
                <span class="text-lightbox-muted tabular-nums">{counts[filter]}</span>
                <Icon name="chevron-down" class="size-3.5 text-lightbox-muted" />
              </DropdownMenu.Trigger>
              <DropdownMenu.Portal>
                <DropdownMenu.Content align="center" sideOffset={6} class={cn(menuContentClass, 'min-w-48')} data-testid="lightbox-filter-menu">
                  {#each PREVIEW_FILTERS as option (option)}
                    {@const count = counts[option]}
                    <DropdownMenu.Item
                      class={menuItemClass}
                      disabled={count === 0}
                      onSelect={() => applyFilter(option)}
                      data-testid="lightbox-filter-option-{option}"
                    >
                      <span class="flex items-center gap-2">
                        <span>{t(filterLabel[option])}</span>
                        <span class="text-caption text-faint tabular-nums">{count}</span>
                      </span>
                      {#if filter === option}<Icon name="check" class="size-3.5 text-forest" />{/if}
                    </DropdownMenu.Item>
                  {/each}
                </DropdownMenu.Content>
              </DropdownMenu.Portal>
            </DropdownMenu.Root>
          </div>
        {/if}

        <!-- Top-right action controls (matching reference: floating circular controls + close) -->
        <div class="pointer-events-auto ms-auto flex shrink-0 items-center gap-2">
          {#if loaded.kind === 'markdown'}
            <div
              class="inline-flex items-center gap-0.5 rounded-full border border-lightbox-border bg-lightbox-bar p-0.5 shadow-pop backdrop-blur-md"
              role="group"
              aria-label={t('previewMode')}
              data-testid="lightbox-md-toggle"
            >
              <button
                type="button"
                aria-pressed={mdMode === 'reading'}
                onclick={() => (mdMode = 'reading')}
                data-testid="lightbox-md-reading"
                class={cn(
                  'h-7 rounded-full px-2.5 text-caption transition-colors outline-none focus-visible:ring-3 focus-visible:ring-focus/45',
                  mdMode === 'reading' ? 'bg-lightbox-control font-semibold text-lightbox-control-ink shadow-control' : 'text-lightbox-ink hover:bg-lightbox-bar-hover'
                )}
              >
                {t('previewReading')}
              </button>
              <button
                type="button"
                aria-pressed={mdMode === 'source'}
                onclick={() => (mdMode = 'source')}
                data-testid="lightbox-md-source"
                class={cn(
                  'h-7 rounded-full px-2.5 text-caption transition-colors outline-none focus-visible:ring-3 focus-visible:ring-focus/45',
                  mdMode === 'source' ? 'bg-lightbox-control font-semibold text-lightbox-control-ink shadow-control' : 'text-lightbox-ink hover:bg-lightbox-bar-hover'
                )}
              >
                {t('previewSource')}
              </button>
            </div>
          {/if}

          {#if canCopy}
            <Tooltip content={t('copy')}>
              {#snippet children(props)}
                <button {...props} type="button" class={circleBtn} aria-label={t('copy')} onclick={copyContent} data-testid="lightbox-copy">
                  <Icon name="copy" />
                </button>
              {/snippet}
            </Tooltip>
          {/if}

          {#if onSave && activeItem}
            <Tooltip content={t('saveToLibrary')}>
              {#snippet children(props)}
                <button {...props} type="button" class={circleBtn} aria-label={t('saveToLibrary')} onclick={() => activeItem && onSave(activeItem)} data-testid="lightbox-save">
                  <Icon name="folder-add" />
                </button>
              {/snippet}
            </Tooltip>
          {/if}

          {#if canOpenExternally}
            <Tooltip content={t('openFile')}>
              {#snippet children(props)}
                <button {...props} type="button" class={circleBtn} aria-label={t('openFile')} onclick={() => activeItem && onOpen?.(activeItem)} data-testid="lightbox-open-external">
                  <Icon name="external" />
                </button>
              {/snippet}
            </Tooltip>
          {/if}

          {#if canDownload}
            <Tooltip content={t('download')}>
              {#snippet children(props)}
                <button {...props} type="button" class={circleBtn} aria-label={t('download')} onclick={() => activeItem && onDownload?.(activeItem)} data-testid="lightbox-download">
                  <Icon name="download" />
                </button>
              {/snippet}
            </Tooltip>
          {/if}

          <Tooltip content={t('close')}>
            {#snippet children(props)}
              <Dialog.Close {...props} class={circleBtn} aria-label={t('close')} data-testid="lightbox-close">
                <Icon name="close" />
              </Dialog.Close>
            {/snippet}
          </Tooltip>
        </div>
      </header>

      <!-- Main stage -->
      <div
        role="presentation"
        class="relative flex min-h-0 flex-1 items-center justify-center overflow-hidden px-16 pt-2 pb-16"
        onclick={onStageClick}
      >
        {#if filteredItems.length > 1}
          <Tooltip content={t('previousOne')}>
            {#snippet children(props)}
              <button
                {...props}
                type="button"
                disabled={!hasPrev}
                onclick={(event) => { event.stopPropagation(); step(-1) }}
                aria-label={t('previousOne')}
                data-testid="lightbox-prev"
                class={cn(circleBtn, 'absolute start-4 top-1/2 z-10 size-10 -translate-y-1/2 [&_svg]:size-5')}
              >
                <Icon name="back" class="rtl:-scale-x-100" />
              </button>
            {/snippet}
          </Tooltip>

          <Tooltip content={t('nextOne')}>
            {#snippet children(props)}
              <button
                {...props}
                type="button"
                disabled={!hasNext}
                onclick={(event) => { event.stopPropagation(); step(1) }}
                aria-label={t('nextOne')}
                data-testid="lightbox-next"
                class={cn(circleBtn, 'absolute end-4 top-1/2 z-10 size-10 -translate-y-1/2 [&_svg]:size-5')}
              >
                <Icon name="forward" class="rtl:-scale-x-100" />
              </button>
            {/snippet}
          </Tooltip>
        {/if}

        {#if loaded.kind === 'loading'}
          {#if activeItem?.placeholderUrl}
            <img
              src={activeItem.placeholderUrl}
              alt={activeItem.name}
              class="max-h-full max-w-full rounded-lg object-contain opacity-80"
            />
          {:else}
            <div
              class="inline-flex items-center gap-2.5 rounded-full border border-lightbox-border bg-lightbox-bar px-4 py-2 text-callout text-lightbox-ink shadow-pop backdrop-blur-md"
              role="status"
              data-testid="lightbox-loading"
            >
              <Icon name="pending" class="size-4 animate-spin text-lightbox-muted" />
              <span>{t('loading')}</span>
            </div>
          {/if}
        {:else if loaded.kind === 'image'}
          <div
            role="presentation"
            bind:clientWidth={stageWidth}
            bind:clientHeight={stageHeight}
            class={cn(
              'relative flex h-full w-full items-center justify-center overflow-hidden',
              canPan ? (dragging ? 'cursor-grabbing' : 'cursor-grab') : 'cursor-zoom-in'
            )}
            onwheel={onImageWheel}
            onpointerdown={onPointerDown}
            onpointermove={onPointerMove}
            onpointerup={onPointerUp}
            onpointercancel={onPointerUp}
            onclick={onStageClick}
            data-testid="lightbox-image-stage"
          >
            <img
              src={loaded.url}
              alt={loaded.meta.name || activeItem?.name || ''}
              draggable="false"
              onload={(event) => {
                const img = event.currentTarget as HTMLImageElement
                naturalWidth = img.naturalWidth
                naturalHeight = img.naturalHeight
              }}
              onerror={onImageError}
              ondblclick={onImageDblClick}
              style:width={naturalWidth ? `${naturalWidth * effectiveScale}px` : undefined}
              style:height={naturalHeight ? `${naturalHeight * effectiveScale}px` : undefined}
              style:transform={panX || panY ? `translate3d(${panX}px, ${panY}px, 0)` : undefined}
              class="max-w-none select-none rounded-lg shadow-pop"
              data-testid="lightbox-image"
            />
          </div>

          <!-- Bottom-center floating zoom pill (matching reference: − | 81% | +) -->
          <div
            dir="ltr"
            class="absolute bottom-4 left-1/2 z-10 inline-flex -translate-x-1/2 items-center gap-0.5 rounded-full border border-lightbox-control-border bg-lightbox-control px-1.5 py-1 text-lightbox-control-ink shadow-pop"
            data-testid="lightbox-zoom"
          >
            <Tooltip content={t('previewZoomOut')}>
              {#snippet children(props)}
                <button
                  {...props}
                  type="button"
                  disabled={effectiveScale <= ZOOM_MIN + 0.005}
                  onclick={() => zoomBy(-1)}
                  aria-label={t('previewZoomOut')}
                  data-testid="lightbox-zoom-out"
                  class="grid size-7 place-items-center rounded-full outline-none transition-colors hover:not-disabled:bg-lightbox-control-hover focus-visible:ring-3 focus-visible:ring-focus/45 disabled:cursor-not-allowed disabled:opacity-40"
                >
                  <Icon name="minus" class="size-4" />
                </button>
              {/snippet}
            </Tooltip>
            <Tooltip content={t('previewResetZoom')}>
              {#snippet children(props)}
                <button
                  {...props}
                  type="button"
                  onclick={toggleFitOrActual}
                  aria-label={t('previewResetZoom')}
                  data-testid="lightbox-zoom-reset"
                  class="h-7 min-w-14 rounded-full px-2 text-center text-callout font-medium tabular-nums outline-none transition-colors hover:bg-lightbox-control-hover focus-visible:ring-3 focus-visible:ring-focus/45"
                >
                  {displayPercent}%
                </button>
              {/snippet}
            </Tooltip>
            <Tooltip content={t('previewZoomIn')}>
              {#snippet children(props)}
                <button
                  {...props}
                  type="button"
                  disabled={effectiveScale >= ZOOM_MAX - 0.005}
                  onclick={() => zoomBy(1)}
                  aria-label={t('previewZoomIn')}
                  data-testid="lightbox-zoom-in"
                  class="grid size-7 place-items-center rounded-full outline-none transition-colors hover:not-disabled:bg-lightbox-control-hover focus-visible:ring-3 focus-visible:ring-focus/45 disabled:cursor-not-allowed disabled:opacity-40"
                >
                  <Icon name="add" class="size-4" />
                </button>
              {/snippet}
            </Tooltip>
          </div>
        {:else if loaded.kind === 'markdown'}
          <div
            class="h-full w-full max-w-[820px] overflow-y-auto rounded-2xl border border-line bg-surface px-7 py-6 text-ink shadow-pop"
            data-testid="lightbox-markdown"
          >
            {#if mdMode === 'reading'}
              <MarkdownView document={loaded.document} />
            {:else}
              <pre class="font-mono text-code leading-relaxed whitespace-pre-wrap text-ink select-text [overflow-wrap:anywhere]" dir="ltr" data-testid="lightbox-markdown-source">{loaded.text}</pre>
            {/if}
          </div>
        {:else if loaded.kind === 'text'}
          <div
            class="h-full w-full max-w-[820px] overflow-y-auto rounded-2xl border border-line bg-surface px-7 py-6 text-ink shadow-pop"
            data-testid="lightbox-text"
          >
            <pre class="font-mono text-code leading-relaxed whitespace-pre-wrap text-ink select-text [overflow-wrap:anywhere]" dir="ltr">{loaded.text}</pre>
          </div>
        {:else if loaded.kind === 'fallback' && activeItem}
          <div
            class="w-[min(440px,100%)] rounded-2xl border border-line bg-surface p-6 text-ink shadow-pop"
            data-testid="lightbox-fallback"
          >
            <div class="flex items-start gap-3.5">
              <span class="grid size-11 shrink-0 place-items-center rounded-xl bg-surface-2 text-forest [&_svg]:size-6">
                <Icon name={iconFor(activeItem)} />
              </span>
              <div class="min-w-0 flex-1">
                <h2 class="text-title-3 font-semibold [overflow-wrap:anywhere]">{loaded.meta.name || activeItem.name}</h2>
                <p class="mt-1 text-callout text-muted">{t(fallbackLabel[loaded.reason])}</p>
              </div>
            </div>
            <dl class="mt-4 grid gap-2 border-t border-line pt-4 text-callout">
              <div class="flex items-baseline justify-between gap-4">
                <dt class="text-muted">{t('fileType')}</dt>
                <dd class="min-w-0 truncate font-mono text-code text-ink-2 select-text" dir="ltr">{loaded.meta.mime || activeItem.mime || '—'}</dd>
              </div>
              <div class="flex items-baseline justify-between gap-4">
                <dt class="text-muted">{t('sortSize')}</dt>
                <dd class="tabular-nums text-ink-2">{size(loaded.meta.size ?? activeItem.size)}</dd>
              </div>
              {#if activeItem.updatedAt}
                <div class="flex items-baseline justify-between gap-4">
                  <dt class="text-muted">{t('modified')}</dt>
                  <dd class="text-ink-2">{dateFormat.format(activeItem.updatedAt)}</dd>
                </div>
              {/if}
              {#if loaded.meta.sha256 || activeItem.sha256}
                <div class="flex items-baseline justify-between gap-4">
                  <dt class="text-muted">SHA-256</dt>
                  <dd class="min-w-0 truncate font-mono text-code text-ink-2 select-text" dir="ltr">{(loaded.meta.sha256 || activeItem.sha256)?.slice(0, 16)}…</dd>
                </div>
              {/if}
            </dl>
            {#if canDownload || canOpenExternally || onSave}
              <div class="mt-5 flex flex-wrap justify-end gap-2">
                {#if onSave}
                  <button type="button" class={buttonClass('secondary', 'md')} onclick={() => onSave(activeItem)}>
                    <Icon name="folder-add" />{t('saveToLibrary')}
                  </button>
                {/if}
                {#if canOpenExternally}
                  <button type="button" class={buttonClass('secondary', 'md')} onclick={() => onOpen?.(activeItem)}>
                    <Icon name="external" />{t('openFile')}
                  </button>
                {/if}
                {#if canDownload}
                  <button type="button" class={buttonClass('primary', 'md')} onclick={() => onDownload?.(activeItem)}>
                    <Icon name="download" />{t('download')}
                  </button>
                {/if}
              </div>
            {/if}
          </div>
        {:else if loaded.kind === 'error'}
          <div class="w-[min(440px,100%)] rounded-2xl border border-line bg-surface p-6 text-ink shadow-pop" data-testid="lightbox-error">
            <Notice tone="danger" failure={loaded.message}>{loaded.message}</Notice>
            {#if activeItem}
              <div class="mt-4 flex justify-end gap-2">
                <button type="button" class={buttonClass('primary', 'md')} onclick={() => activeItem && loadItem(activeItem)}>
                  {t('retry')}
                </button>
              </div>
            {/if}
          </div>
        {/if}
      </div>
    </Dialog.Content>
  </Dialog.Portal>
</Dialog.Root>
