<script lang="ts">
  import { untrack } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import { toast } from '../../components/toast.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { bridge, call, code, errorText } from '../../rpc'
  import { spaced } from './vault'
  import FieldActions from './FieldActions.svelte'
  import FieldViewer from './FieldViewer.svelte'

  /**
   * A one-time password: the current code, from the daemon, and how long
   * it stays current. The secret behind it never reaches this window; a
   * copy goes to the clipboard through the main process and leaves it once
   * the code would have changed.
   */
  let { id, field, unlocked, name = field, context }: { id: string; field: string; unlocked: boolean; name?: string; context?: string } = $props()

  let current = $state<{ code: string; period_ms: number; remaining_ms: number } | null>(null)
  let error = $state('')
  let left = $state(0)
  let viewing = $state(false)
  let trigger = $state<HTMLElement | null>(null)
  let copying = $state(false)
  let retry = $state(0)
  let generation = 0
  // A code past its time is not shown, but its row and copy action stay until the next one arrives.
  const value = $derived(current ? (left > 0 ? spaced(current.code) : '••• •••') : null)
  const source = $derived(`${id}:${field}:${unlocked}`)

  async function copy() {
    if (!unlocked || copying || !value) return false
    const at = generation
    copying = true
    try {
      const { clear_after_ms } = await bridge<{ clear_after_ms: number }>(() => window.dmsg.copySecret(id, field, null))
      // The clipboard holds it whichever view is open now; only a lock has cleared it.
      if (!unlocked) return false
      toast((seconds) => t('copied', { seconds }), { ms: clear_after_ms, countdown: true, testid: 'otp-copied' })
      return true
    } catch (e) {
      if (at === generation && unlocked) toast(() => errorText(e))
      return false
    } finally { if (at === generation) copying = false }
  }

  $effect(() => { void source; viewing = false })

  $effect(() => {
    void [source, retry]
    const [itemId, fieldId, canRead] = untrack(() => [id, field, unlocked] as const)
    generation += 1
    copying = false
    current = null
    error = ''
    if (!canRead) return
    let active = true
    let due = 0
    let timer: ReturnType<typeof setTimeout> | undefined
    async function load() {
      try {
        const requestedAt = Date.now()
        const answer = await call('vault/otp', { id: itemId, field: fieldId })
        // A reply after navigation must not restart the old item's timer.
        if (!active) return
        current = answer
        error = ''
        due = requestedAt + answer.remaining_ms
        left = Math.max(0, due - Date.now())
        timer = setTimeout(load, Math.max(50, left + 50))
      } catch (e) {
        if (!active) return
        current = null
        error = code(e)
      }
    }
    void load()
    const tick = setInterval(() => (left = Math.max(0, due - Date.now())), 250)
    return () => {
      active = false
      generation += 1
      clearTimeout(timer)
      clearInterval(tick)
    }
  })
</script>

<div class="flex min-w-0 flex-1 items-center gap-2" data-testid="vault-otp" data-field={field}>
  <FieldActions {name} copy={value && unlocked ? copy : undefined} copyLabel={t('copyCode')} copyTestid="copy-otp"
    view={unlocked ? (element) => { trigger = element; viewing = true } : undefined}>
    {#if current && value}
      <div class="flex min-w-0 flex-1 flex-wrap items-center gap-2 py-1" dir="ltr">
        <span class="font-mono text-[17px] font-semibold tracking-[0.08em] text-ink tabular-nums select-text" data-testid="vault-otp-code">{value}</span>
        <span class="relative h-1 w-14 overflow-hidden rounded-full bg-wash" aria-label={t('otpLeft', { seconds: Math.ceil(left / 1000) })}>
          <span class="absolute inset-y-0 start-0 rounded-full {left < 5000 ? 'bg-warn' : 'bg-forest'}" style:width="{(left / current.period_ms) * 100}%"></span>
        </span>
        <span class="text-[11.5px] text-faint tabular-nums">{Math.ceil(left / 1000)}s</span>
      </div>
    {:else if error}
      <Notice tone="danger" class="min-w-0 flex-1">{t('error')}: {error}
        {#snippet action()}<button class={buttonClass('ghost', 'xs')} onclick={() => retry++}>{t('retry')}</button>{/snippet}
      </Notice>
    {:else}
      <span class="min-w-0 flex-1 py-1 font-mono text-faint">••• •••</span>
    {/if}
  </FieldActions>
</div>

{#if viewing && unlocked}
  <FieldViewer bind:open={viewing} {name} {context} {value} mono {trigger}>
    {#snippet footer()}
      {#if error}<Notice tone="danger" class="w-full">{t('error')}: {error}</Notice><button class={buttonClass('secondary', 'md')} onclick={() => retry++}>{t('retry')}</button>{/if}
      <button class={buttonClass('secondary', 'md')} disabled={copying || !value} onclick={copy} data-testid="vault-large-copy"><Icon name="copy" />{t('copyCode')}</button>
      {#if value}<span class="text-[12.5px] text-muted">{t('otpLeft', { seconds: Math.ceil(left / 1000) })}</span>{/if}
    {/snippet}
  </FieldViewer>
{/if}
