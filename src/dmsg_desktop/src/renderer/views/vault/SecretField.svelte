<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import { toast } from '../../components/toast.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { bridge, call, errorText } from '../../rpc'
  import FieldActions from './FieldActions.svelte'
  import FieldViewer from './FieldViewer.svelte'

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
    copyable = true,
    name = field,
    context,
    reference,
    referenceDisabled,
    referenceHint
  }: {
    id: string; field: string; revision?: string | null; unlocked: boolean; copyable?: boolean
    name?: string; context?: string; reference?: () => Promise<boolean>; referenceDisabled?: boolean; referenceHint?: string
  } = $props()

  const ARMED_MS = 3000
  const SHOWN_MS = 60_000

  let generation = 0
  let armed = $state(false)
  let armedUntil = $state(0)
  let secret = $state<string | null>(null)
  let shownUntil = $state(0)
  let busy = $state(false)
  let viewing = $state(false)
  let trigger = $state<HTMLElement | null>(null)
  let failure = $state('')
  let armTimer: ReturnType<typeof setTimeout> | undefined
  let hideTimer: ReturnType<typeof setTimeout> | undefined
  // Sync ACKs replace metadata objects without changing the value's identity.
  const source = $derived(`${id}:${field}:${revision ?? ''}:${unlocked}`)

  function arm() {
    clearTimeout(armTimer)
    failure = ''
    armedUntil = Date.now() + ARMED_MS
    armed = true
    armTimer = setTimeout(() => (armed = false), ARMED_MS)
  }

  function hide() {
    clearTimeout(hideTimer)
    secret = null
  }

  async function reveal() {
    if (!unlocked || busy) return
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
      shownUntil = Date.now() + SHOWN_MS
      hideTimer = setTimeout(hide, SHOWN_MS)
    } catch (error) {
      if (at === generation && unlocked) {
        if (viewing) failure = errorText(error)
        else toast(() => errorText(error))
      }
    } finally {
      if (at === generation) busy = false
    }
  }

  async function copy() {
    if (!unlocked || !copyable || busy) return false
    const at = generation
    busy = true
    try {
      const { clear_after_ms } = await bridge<{ clear_after_ms: number }>(() => window.dmsg.copySecret(id, field, revision))
      // The clipboard holds it whichever view is open now; only a lock has cleared it.
      if (!unlocked) return false
      toast((seconds) => t('copied', { seconds }), { ms: clear_after_ms, countdown: true, testid: 'secret-copied' })
      return true
    } catch (error) {
      if (at === generation && unlocked) toast(() => errorText(error))
      return false
    } finally {
      if (at === generation) busy = false
    }
  }

  $effect(() => {
    // Forget plaintext and invalidate late reveals on lock or a changed field.
    void source
    generation += 1
    armed = false
    viewing = false
    busy = false
    failure = ''
    hide()
    clearTimeout(armTimer)
    return () => {
      generation += 1
      clearTimeout(armTimer)
      clearTimeout(hideTimer)
    }
  })

  // Closing the viewer also cancels a reveal that has not returned yet.
  $effect(() => {
    if (!viewing) {
      generation += 1
      armed = false
      busy = false
      failure = ''
      hide()
      clearTimeout(armTimer)
    }
  })
</script>

{#snippet bar(ms: number, until: number)}
  <span
    class="pointer-events-none absolute inset-x-1.5 bottom-0.5 h-0.5 origin-left rounded-full bg-current opacity-60 rtl:origin-right motion-reduce:hidden"
    style:animation="drain {ms}ms linear {Math.min(0, until - Date.now() - ms)}ms forwards"
    aria-hidden="true"
  ></span>
{/snippet}

{#snippet revealButton()}
  {#if unlocked}
    <button
      class={buttonClass(armed ? 'primary' : 'secondary', 'sm', 'relative h-auto min-h-8 max-w-44 overflow-hidden whitespace-normal')}
      disabled={busy}
      onclick={reveal}
      data-testid="reveal-secret"
      data-armed={armed}
    >
      <Icon name={secret !== null ? 'eye-off' : 'eye'} />
      {secret !== null ? t('hideSecret') : armed ? t('clickAgainShow') : t('show')}
      {#if armed}{@render bar(ARMED_MS, armedUntil)}{:else if secret !== null}{@render bar(SHOWN_MS, shownUntil)}{/if}
    </button>
  {/if}
{/snippet}

<FieldActions {name} {busy} copy={unlocked && copyable ? copy : undefined} copyTestid="copy-secret"
  {reference} {referenceDisabled} {referenceHint} beforeActions={revealButton}
  view={unlocked ? (element) => { trigger = element; viewing = true } : undefined}>
  <span class={cn('min-w-0 flex-1 py-1 font-mono text-[12.5px] leading-[1.55]',
    secret === null ? 'overflow-hidden tracking-[0.18em] whitespace-nowrap text-faint' : 'max-h-64 overflow-auto break-all whitespace-pre-wrap text-ink select-text')}
    dir="ltr" data-testid="vault-secret" data-field={field} data-shown={secret !== null}>{secret ?? '••••••••••••'}</span>
</FieldActions>

{#if viewing && unlocked}
  <FieldViewer bind:open={viewing} {name} {context} value={secret} mono {trigger}>
    {#snippet footer()}
      {#if failure}<Notice tone="danger" class="w-full">{failure}</Notice>{/if}
      {@render revealButton()}
      {#if copyable}
        <button class={buttonClass('secondary', 'md')} disabled={busy} onclick={copy} data-testid="vault-large-copy"><Icon name="copy" />{t('copy')}</button>
      {/if}
    {/snippet}
  </FieldViewer>
{/if}
