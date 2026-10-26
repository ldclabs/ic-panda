<script lang="ts">
  import { Calendar, Popover } from 'bits-ui'
  import { parseDate, type DateValue } from '@internationalized/date'
  import { locale, t } from '../i18n/index.svelte'
  import { localeDir } from '../../shared/locales'
  import Icon from './Icon.svelte'
  import { buttonClass, iconButtonClass } from './ui'
  import { validDateText, type DateMode } from './form-values'

  const id = $props.id()
  let {
    value = $bindable(''),
    label,
    mode = 'date',
    max,
    disabled = false,
    hideLabel = false,
    testid,
    onChange
  }: {
    value?: string
    label: string
    mode?: DateMode
    max?: string
    disabled?: boolean
    hideLabel?: boolean
    testid?: string
    onChange?: (value: string) => void
  } = $props()
  let open = $state(false),
    year = $state(new Date().getFullYear())
  const format = $derived(
    mode === 'month'
      ? 'YYYY-MM'
      : mode === 'datetime'
        ? 'YYYY-MM-DD HH:mm'
        : 'YYYY-MM-DD'
  )
  const error = $derived(
    value && !validDateText(value, mode)
      ? t('invalidDate')
      : value && max && value > max
        ? t('dateMaximum', { date: max })
        : ''
  )
  const dateValue = $derived.by(() => {
    if (!validDateText(value, mode)) return undefined
    return parseDate(mode === 'month' ? `${value}-01` : value.slice(0, 10))
  })
  const maximum = $derived(
    max && validDateText(max.slice(0, 10), 'date')
      ? parseDate(max.slice(0, 10))
      : undefined
  )
  const zone = Intl.DateTimeFormat().resolvedOptions().timeZone
  const months = $derived(
    Array.from({ length: 12 }, (_, i) =>
      new Intl.DateTimeFormat(locale(), {
        month: 'short',
        calendar: 'gregory'
      }).format(new Date(2024, i, 1))
    )
  )
  function change(next: string) {
    value = next
    onChange?.(next)
  }
  function pick(date: DateValue | undefined) {
    if (!date) return
    const day = date.toString()
    change(
      mode === 'datetime'
        ? `${day}T${validDateText(value, mode) ? value.slice(11) : '00:00'}`
        : day
    )
    open = false
  }
</script>

<div class="grid min-w-0 gap-1.5">
  <label for={id} class={hideLabel ? 'sr-only' : 'text-[12px] text-muted'}
    >{label}</label
  >
  <Popover.Root
    bind:open
    onOpenChange={(next) => {
      if (next) year = dateValue?.year ?? new Date().getFullYear()
    }}
  >
    <div class="relative">
      <input
        {id}
        type="text"
        value={mode === 'datetime' ? value.replace('T', ' ') : value}
        oninput={(event) =>
          change(
            mode === 'datetime'
              ? event.currentTarget.value.replace(' ', 'T')
              : event.currentTarget.value
          )}
        {disabled}
        placeholder={format}
        autocomplete="off"
        spellcheck="false"
        dir="ltr"
        aria-invalid={!!error}
        aria-describedby={`${id}-hint${error ? ` ${id}-error` : ''}`}
        class={`${localeDir(locale()) === 'rtl' ? 'pl-16 pr-2.5' : 'pl-2.5 pr-16'} text-[12.5px] tabular-nums aria-invalid:border-danger`}
        data-testid={testid}
      />
      <div class="absolute end-0 top-0 flex h-8 items-center">
        {#if value}<button
            type="button"
            class={iconButtonClass('ghost', 'md', 'size-7 [&_svg]:size-3.5')}
            aria-label={`${t('clearValue')} · ${label}`}
            title={t('clearValue')}
            {disabled}
            onclick={() => change('')}
            data-testid={testid && `${testid}-clear`}
            ><Icon name="close" /></button
          >{/if}
        <Popover.Trigger
          {disabled}
          class={iconButtonClass('ghost', 'md')}
          aria-label={`${t('chooseDate')} · ${label}`}
          title={t('chooseDate')}
          data-testid={testid && `${testid}-calendar`}
          ><Icon name="time" /></Popover.Trigger
        >
      </div>
    </div>
    <Popover.Portal
      ><Popover.Content
        align="start"
        sideOffset={4}
        collisionPadding={8}
        dir={localeDir(locale())}
        class="z-50 max-h-[min(360px,var(--bits-floating-available-height))] w-[min(296px,calc(100vw-24px))] overflow-y-auto rounded-xl border border-line bg-surface p-3 text-ink shadow-pop outline-none"
        data-testid={testid && `${testid}-popover`}
      >
        {#if mode === 'month'}
          <div class="mb-2 flex items-center justify-between gap-2">
            <button
              type="button"
              class={iconButtonClass()}
              aria-label={t('previousYear')}
              title={t('previousYear')}
              disabled={year <= 1}
              onclick={() => year--}><Icon name="back" /></button
            >
            <span class="font-medium tabular-nums" aria-live="polite"
              >{year}</span
            >
            <button
              type="button"
              class={iconButtonClass()}
              aria-label={t('nextYear')}
              title={t('nextYear')}
              disabled={year >= 9999}
              onclick={() => year++}><Icon name="forward" /></button
            >
          </div>
          <div class="grid grid-cols-3 gap-1">
            {#each months as month, index}{@const next = `${String(year).padStart(4, '0')}-${String(index + 1).padStart(2, '0')}`}
              <button
                type="button"
                class={buttonClass(value === next ? 'primary' : 'ghost', 'sm')}
                aria-pressed={value === next}
                data-month={next}
                onclick={() => {
                  change(next)
                  open = false
                }}>{month}</button
              >
            {/each}
          </div>
        {:else}
          <Calendar.Root
            type="single"
            value={dateValue}
            onValueChange={pick}
            locale={locale()}
            calendarLabel={label}
            maxValue={maximum}
            fixedWeeks
            preventDeselect
          >
            {#snippet children({ months: visibleMonths, weekdays })}
              <Calendar.Header
                class="mb-2 flex items-center justify-between gap-2"
              >
                <Calendar.PrevButton title={t('previousMonth')} aria-label={t('previousMonth')} class={iconButtonClass()}
                  ><Icon name="back" /></Calendar.PrevButton
                >
                <Calendar.Heading class="text-[13px] font-medium" />
                <Calendar.NextButton title={t('nextMonth')} aria-label={t('nextMonth')} class={iconButtonClass()}
                  ><Icon name="forward" /></Calendar.NextButton
                >
              </Calendar.Header>
              {#each visibleMonths as month}<Calendar.Grid
                  class="w-full border-collapse"
                >
                  <Calendar.GridHead
                    ><Calendar.GridRow
                      >{#each weekdays as day}<Calendar.HeadCell
                          class="h-7 text-[11px] font-normal text-muted"
                          >{day}</Calendar.HeadCell
                        >{/each}</Calendar.GridRow
                    ></Calendar.GridHead
                  >
                  <Calendar.GridBody
                    >{#each month.weeks as week}<Calendar.GridRow
                        >{#each week as day}<Calendar.Cell
                            date={day}
                            month={month.value}
                            class="p-0.5"
                          >
                            <Calendar.Day
                              class="grid size-8 cursor-pointer place-items-center rounded-lg text-[12.5px] outline-none hover:bg-wash focus-visible:ring-2 focus-visible:ring-focus data-selected:bg-forest data-selected:text-on-forest data-today:font-bold data-outside-month:text-faint data-disabled:pointer-events-none data-disabled:opacity-35"
                              data-date={day.toString()}>{day.day}</Calendar.Day
                            >
                          </Calendar.Cell>{/each}</Calendar.GridRow
                      >{/each}</Calendar.GridBody
                  >
                </Calendar.Grid>{/each}
            {/snippet}
          </Calendar.Root>
        {/if}
      </Popover.Content></Popover.Portal
    >
  </Popover.Root>
  <p id={`${id}-hint`} class="text-[11.5px] text-faint">
    {t('dateFormatHint', { format })}{#if mode === 'datetime'}<span
        class="block">{t('localTimeZone', { zone })}</span
      >{/if}
  </p>
  {#if error}<p id={`${id}-error`} class="text-[12px] text-danger" role="alert">
      {error}
    </p>{/if}
</div>
