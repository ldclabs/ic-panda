<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { toast } from '../../components/toast.svelte'
  import Tooltip from '../../components/Tooltip.svelte'
  import { iconButtonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { bridge, call, code, errorText } from '../../rpc'
  import { spaced } from './vault'

  /**
   * A one-time password: the current code, from the daemon, and how long
   * it stays current. The secret behind it never reaches this window; a
   * copy goes to the clipboard through the main process and leaves it once
   * the code would have changed.
   */
  let { id, field, unlocked }: { id: string; field: string; unlocked: boolean } = $props()

  let current = $state<{ code: string; period_ms: number; remaining_ms: number } | null>(null)
  let error = $state('')
  let left = $state(0)

  async function copy() {
    try {
      const { clear_after_ms } = await bridge<{ clear_after_ms: number }>(() => window.dmsg.copySecret(id, field, null))
      toast((seconds) => t('copied', { seconds }), { ms: clear_after_ms, countdown: true, testid: 'otp-copied' })
    } catch (e) {
      toast(() => errorText(e))
    }
  }

  $effect(() => {
    const itemId = id
    const fieldId = field
    current = null
    error = ''
    if (!unlocked) return
    let active = true
    let due = 0
    let timer: ReturnType<typeof setTimeout> | undefined
    async function load() {
      try {
        const answer = await call('vault/otp', { id: itemId, field: fieldId })
        // A reply after navigation must not restart the old item's timer.
        if (!active) return
        current = answer
        error = ''
        due = Date.now() + answer.remaining_ms
        left = answer.remaining_ms
        timer = setTimeout(load, answer.remaining_ms + 50)
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
      clearTimeout(timer)
      clearInterval(tick)
    }
  })

</script>

<div class="flex min-w-0 flex-1 items-center gap-2" data-testid="vault-otp" data-field={field}>
  {#if current}
    <span class="font-mono text-[17px] font-semibold tracking-[0.08em] text-ink tabular-nums select-text" data-testid="vault-otp-code">{spaced(current.code)}</span>
    <span class="relative h-1 w-14 overflow-hidden rounded-full bg-wash" aria-label={t('otpLeft', { seconds: Math.ceil(left / 1000) })}>
      <span class="absolute inset-y-0 start-0 rounded-full {left < 5000 ? 'bg-warn' : 'bg-forest'}" style:width="{(left / current.period_ms) * 100}%"></span>
    </span>
    <span class="text-[11.5px] text-faint tabular-nums">{Math.ceil(left / 1000)}s</span>
    <span class="min-w-0 flex-1"></span>
    <Tooltip content={t('copyCode')}>
      {#snippet children(props)}
        <button {...props} class={iconButtonClass('ghost', 'md')} aria-label={t('copyCode')} onclick={copy} data-testid="copy-otp">
          <Icon name="copy" />
        </button>
      {/snippet}
    </Tooltip>
  {:else if error}
    <span class="text-[12.5px] text-danger">{t('error')}: {error}</span>
  {:else}
    <span class="font-mono text-faint">••• •••</span>
  {/if}
</div>
