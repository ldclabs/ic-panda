<script lang="ts">
  import { onDestroy } from 'svelte'
  import { t } from '$lib/i18n'
  import { packageFieldName, type SecretPackage } from '$lib/share/package'

  let { item }: { item: SecretPackage['item'] } = $props()
  const uid = $props.id()
  const fields = $derived(item.fields.filter((f) => f.value !== ''))
  let visible = $state(new Set<number>())
  let copied = $state<{ index: number; status: string } | null>(null)
  let lastAnnouncement = ''
  let copyRequest = 0
  let copyTimer: ReturnType<typeof setTimeout>

  function toggle(index: number) {
    const next = new Set(visible)
    if (next.has(index)) next.delete(index)
    else next.add(index)
    visible = next
  }

  async function copy(index: number, value: string) {
    const id = ++copyRequest
    clearTimeout(copyTimer)
    let ok = false
    try {
      await navigator.clipboard.writeText(value)
      ok = true
    } catch {
      ok = false
    }
    if (id !== copyRequest) return
    let message = $t(ok ? 'Copied' : 'Could not copy')
    if (message === lastAnnouncement) message += '\u00A0'
    lastAnnouncement = message
    copied = { index, status: '' }
    requestAnimationFrame(() => {
      if (id === copyRequest) copied = { index, status: message }
    })
    copyTimer = setTimeout(() => {
      copied = null
    }, 2500)
  }

  onDestroy(() => {
    ++copyRequest
    clearTimeout(copyTimer)
  })
</script>

{#snippet row(index: number, label: string, section: string, value: string)}
  <section class="snapshot-field" aria-labelledby={`${uid}-name-${index}`}>
    <div class="field-top">
      <strong id={`${uid}-name-${index}`} dir="auto">{#if section.trim()}<small>{section}</small>{/if}{label}</strong>
      <div class="field-actions">
        <button type="button" class="button secondary" aria-expanded={visible.has(index)}
          aria-describedby={`${uid}-name-${index}`} onclick={() => toggle(index)}>{$t(visible.has(index) ? 'Hide' : 'Reveal')}</button>
        <button type="button" class="button secondary" aria-describedby={`${uid}-name-${index}`}
          onclick={() => copy(index, value)}>{$t('Copy')}</button>
      </div>
    </div>
    {#if visible.has(index)}<pre dir="auto">{value}</pre>{/if}
    <small class="copy-status" role="status">{copied?.index === index ? copied.status : ''}</small>
  </section>
{/snippet}

{#each fields as field, index (field.id)}
  {@render row(index, packageFieldName(item.type, field, $t), field.section, field.value)}
{/each}
{#if item.notes}{@render row(-1, $t('Notes'), '', item.notes)}{/if}

<style>
  .snapshot-field { padding-block: 14px; border-bottom: 1px solid var(--line); overflow-wrap: anywhere; }
  .field-top, .field-actions { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; }
  .field-top { justify-content: space-between; }
  strong { min-width: 0; }
  small { display: block; color: var(--muted); font-weight: 400; }
  .copy-status:empty { display: none; }
  .field-actions .button { min-height: 40px; padding: 6px 14px; }
  pre { margin: 10px 0 0; padding: 12px; border-radius: 8px; background: var(--canvas);
    white-space: pre-wrap; overflow-wrap: anywhere; font: 14px/1.6 ui-monospace, monospace; }
</style>
