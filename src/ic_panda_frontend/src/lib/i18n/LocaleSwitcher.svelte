<script lang="ts">
  import { locale, locales, localeNames, setLocale, t } from './index'
  let busy = false
  let error = ''
  async function change(event: Event) {
    const select = event.currentTarget as HTMLSelectElement
    busy = true
    error = ''
    try {
      await setLocale(select.value)
    } catch {
      error = 'Could not load this language. Please try again.'
    } finally {
      busy = false
      select.value = $locale
    }
  }
</script>

<div class="locale-picker">
  <label>
    <span aria-hidden="true">◎</span>
    <select
      aria-label={$t('Language')}
      value={$locale}
      disabled={busy}
      on:change={change}
      dir="ltr"
    >
      {#each locales as code}
        <option value={code} lang={code}>{localeNames[code]}</option>
      {/each}
    </select>
  </label>
  {#if error}<span role="alert">{$t(error)}</span>{/if}
</div>

<style>
  .locale-picker {
    position: relative;
    flex-shrink: 0;
    font-family: system-ui, sans-serif;
    font-size: 13px;
  }
  label {
    display: flex;
    align-items: center;
    gap: 3px;
  }
  select {
    color: inherit;
    background-color: transparent;
    border: 1px solid currentColor;
    border-radius: 6px;
    min-height: 36px;
    max-width: 115px;
    padding: 4px 20px 4px 6px;
    font: inherit;
    cursor: pointer;
  }
  option {
    color: #10251f;
    background: white;
  }
  select:focus-visible {
    outline: 2px solid currentColor;
    outline-offset: 3px;
  }
  [role='alert'] {
    position: absolute;
    inset-inline-end: 0;
    top: 100%;
    z-index: 100;
    min-width: 200px;
    padding: 8px;
    color: #991b1b;
    background: white;
    border: 1px solid;
    border-radius: 6px;
  }
</style>
