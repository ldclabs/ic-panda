<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { toast } from '../../components/toast.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { bridge, call, errorText } from '../../rpc'

  /**
   * A concealed field: shown only on a second click within three seconds of
   * the first, and hidden again after a minute, every line of it; a bar under
   * the button shows the time left. A copy takes one click and goes from the
   * daemon to the clipboard through the main process, so this window never
   * holds it. `revision` reads an earlier version.
   */
  let {
    id,
    field,
    revision = null,
    unlocked,
    copyable = true
  }: { id: string; field: string; revision?: string | null; unlocked: boolean; copyable?: boolean } = $props()

  const ARMED_MS = 3000
  const SHOWN_MS = 60_000

  let generation = 0
  let armed = $state(false)
  let secret = $state<string | null>(null)
  let busy = $state(false)
  let armTimer: ReturnType<typeof setTimeout> | undefined
  let hideTimer: ReturnType<typeof setTimeout> | undefined

  function arm() {
    clearTimeout(armTimer)
    armed = true
    armTimer = setTimeout(() => (armed = false), ARMED_MS)
  }

  function hide() {
    clearTimeout(hideTimer)
    secret = null
  }

  async function reveal() {
    if (!unlocked) return
    if (secret !== null) return hide()
    if (!armed) return arm()
    clearTimeout(armTimer)
    armed = false
    busy = true
    const at = generation
    try {
      const shown: any = await call('vault/reveal', { id, field, revision })
      if (at !== generation || !unlocked) return
      secret = shown.value
      hideTimer = setTimeout(hide, SHOWN_MS)
    } catch (error) {
      toast(() => errorText(error))
    } finally {
      busy = false
    }
  }

  async function copy() {
    busy = true
    try {
      const { clear_after_ms } = await bridge<{ clear_after_ms: number }>(() => window.dmsg.copySecret(id, field, revision))
      toast((seconds) => t('copied', { seconds }), { ms: clear_after_ms, countdown: true, testid: 'secret-copied' })
    } catch (error) {
      toast(() => errorText(error))
    } finally {
      busy = false
    }
  }

  $effect(() => {
    // Forget plaintext and invalidate late reveals on lock or a changed field.
    void [id, field, revision, unlocked]
    generation += 1
    armed = false
    hide()
    clearTimeout(armTimer)
    return () => {
      generation += 1
      clearTimeout(armTimer)
      clearTimeout(hideTimer)
    }
  })
</script>

{#snippet bar(ms: number)}
  <span
    class="pointer-events-none absolute inset-x-1.5 bottom-0.5 h-0.5 origin-left rounded-full bg-current opacity-60 rtl:origin-right motion-reduce:hidden"
    style:animation="drain {ms}ms linear forwards"
    aria-hidden="true"
  ></span>
{/snippet}

<div class="flex min-w-0 flex-1 items-start gap-1.5">
  <span
    class={cn(
      'min-w-0 flex-1 py-1 font-mono text-[12.5px] leading-[1.55]',
      secret === null ? 'overflow-hidden tracking-[0.18em] whitespace-nowrap text-faint' : 'max-h-64 overflow-auto break-all whitespace-pre-wrap text-ink select-text'
    )}
    data-testid="vault-secret"
    data-field={field}
    data-shown={secret !== null}
  >
    {secret ?? '••••••••••••'}
  </span>
  {#if unlocked}
    <button
      class={buttonClass(armed ? 'primary' : 'secondary', 'sm', 'relative overflow-hidden')}
      disabled={busy}
      onclick={reveal}
      data-testid="reveal-secret"
      data-armed={armed}
    >
      <Icon name={secret !== null ? 'eye-off' : 'eye'} />
      {secret !== null ? t('hideSecret') : armed ? t('clickAgainShow') : t('show')}
      {#if armed}{@render bar(ARMED_MS)}{:else if secret !== null}{@render bar(SHOWN_MS)}{/if}
    </button>
    {#if copyable}
      <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={copy} data-testid="copy-secret">
        <Icon name="copy" />{t('copy')}
      </button>
    {/if}
  {/if}
</div>
