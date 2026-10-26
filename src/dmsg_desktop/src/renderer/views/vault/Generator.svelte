<script lang="ts">
  import Slider from '../../components/Slider.svelte'
  import { Popover } from 'bits-ui'
  import { localeDir } from '../../../shared/locales'
  import Checkbox from '../../components/Checkbox.svelte'
  import Icon from '../../components/Icon.svelte'
  import Segmented from '../../components/Segmented.svelte'
  import { iconButtonClass, buttonClass, cn } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { defaultOptions, entropy, generate, type GeneratorMode, type GeneratorOptions, type Random } from './vault'

  /**
   * The password generator, beside a hidden field: a random password with
   * the kinds of characters chosen, a passphrase of EFF words, or a PIN.
   * The choices are this window's own and are kept for next time; the
   * secret goes into the field only on Use.
   */
  let { onUse, pin = false, testid }: { onUse: (secret: string) => void; pin?: boolean; testid?: string } = $props()

  const KEY = 'dmsg.vault.generator'

  function stored(): GeneratorOptions {
    try {
      const kept = JSON.parse(localStorage.getItem(KEY) ?? 'null')
      if (kept && typeof kept === 'object') return { ...defaultOptions, ...kept }
    } catch {
      /* none kept */
    }
    return { ...defaultOptions }
  }

  let open = $state(false)
  let options = $state<GeneratorOptions>(stored())
  let list = $state<readonly string[]>([])
  let secret = $state('')

  const random: Random = (values) => crypto.getRandomValues(values)
  const bits = $derived(Math.round(entropy(options, list.length || 7776)))

  async function refresh() {
    if (options.mode === 'words' && !list.length) list = (await import('./words')).words
    secret = generate(options, random, list)
    try {
      localStorage.setItem(KEY, JSON.stringify(options))
    } catch {
      /* kept for this window only */
    }
  }

  // A new secret follows every change of the choices.
  $effect(() => {
    if (!open) return
    void JSON.stringify(options)
    void refresh()
  })

  $effect(() => {
    if (open && pin && options.mode !== 'pin') options = { ...options, mode: 'pin', length: Math.min(options.length, 8) }
  })

  function use() {
    onUse(secret)
    open = false
  }
</script>

<Popover.Root bind:open>
  <Popover.Trigger class={buttonClass('ghost', 'xs', 'text-forest')} data-testid={testid}>
    <Icon name="generate" />{t('generate')}
  </Popover.Trigger>
  <Popover.Portal>
    <Popover.Content
      dir={localeDir(locale())}
      align="end"
      sideOffset={6}
      collisionPadding={8}
      class="z-50 grid max-h-[min(480px,var(--bits-floating-available-height))] w-[min(340px,calc(100vw-16px))] overflow-y-auto gap-3 rounded-xl border border-line bg-surface p-3 text-ink shadow-pop outline-none"
      data-testid="generator"
    >
      <Segmented
        bind:value={() => options.mode, (next) => (options = { ...options, mode: next as GeneratorMode })}
        options={[
          { value: 'random', label: t('genRandom') },
          { value: 'words', label: t('genWords') },
          { value: 'pin', label: t('genPin') }
        ]}
        label={t('generate')}
        class="w-full [&>*]:flex-1"
        testid="generator-mode"
      />
      <div class="min-h-12 rounded-lg border border-line bg-surface-2 px-2.5 py-2 font-mono text-[13px] break-all select-text" data-testid="generator-secret">{secret}</div>
      {#if options.mode === 'words'}
        <Slider label={t('genWordCount')} min={3} max={12} bind:value={options.words} testid="generator-words" />
        <label class="flex items-center justify-between gap-2 text-[12px] text-faint">
          {t('genSeparator')}
          <input bind:value={options.separator} maxlength="3" class="h-7 w-16 text-center font-mono text-[13px] text-ink" />
        </label>
      {:else}
        <Slider label={t('genLength')} min={options.mode === 'pin' ? 4 : 8} max={options.mode === 'pin' ? 12 : 64} bind:value={options.length} testid="generator-length" />
      {/if}
      {#if options.mode === 'random'}
        <div class="grid grid-cols-2 gap-1.5 text-[12.5px]">
          <Checkbox bind:checked={options.upper}>A–Z</Checkbox>
          <Checkbox bind:checked={options.lower}>a–z</Checkbox>
          <Checkbox bind:checked={options.digits}>0–9</Checkbox>
          <Checkbox bind:checked={options.symbols}>!@#$%</Checkbox>
          <Checkbox bind:checked={options.unambiguous} class="col-span-2">{t('genUnambiguous')}</Checkbox>
        </div>
      {/if}
      <div class="flex items-center gap-2">
        <span class={cn('min-w-0 flex-1 text-[11.5px]', bits < 60 ? 'text-warn' : 'text-faint')}>{t('genBits', { bits })}</span>
        <button class={iconButtonClass('ghost', 'md')} aria-label={t('genAgain')} onclick={() => void refresh()}>
          <Icon name="sync-now" />
        </button>
        <button class={buttonClass('primary', 'sm')} disabled={!secret} onclick={use} data-testid="generator-use">{t('genUse')}</button>
      </div>
    </Popover.Content>
  </Popover.Portal>
</Popover.Root>
