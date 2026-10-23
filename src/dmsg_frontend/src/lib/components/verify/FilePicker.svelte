<script lang="ts">
  import IconArticleLine from '$lib/components/icons/IconArticleLine.svelte'
  import { t } from '$lib/i18n'

  let {
    label,
    hint,
    accept,
    file = $bindable<File | null>(null),
    disabled = false,
    optional = false,
    testId,
    onchange
  }: {
    label: string
    hint: string
    accept?: string
    file?: File | null
    disabled?: boolean
    optional?: boolean
    testId: string
    onchange: () => void
  } = $props()
  const id = $props.id()
  let input: HTMLInputElement
</script>

<div class="file-field" class:selected={!!file} class:disabled>
  <label for={id}>
    <span class="file-heading" id={`${id}-label`}>
      <span class="file-icon" aria-hidden="true"><IconArticleLine /></span>
      {label}
    </span>
    <span class="file-name" dir="auto">{file?.name ?? hint}</span>
    <span class="choose">{file ? $t('Change file') : $t('Choose file')}</span>
    <input
      bind:this={input}
      {id}
      type="file"
      {accept}
      {disabled}
      aria-labelledby={`${id}-label`}
      data-testid={testId}
      onchange={(event) => {
        file = event.currentTarget.files?.[0] ?? null
        onchange()
      }}
    />
  </label>
  {#if optional && file}
    <button
      type="button"
      class="remove"
      {disabled}
      onclick={() => {
        input.value = ''
        file = null
        onchange()
      }}>{$t('Remove file')}</button
    >
  {/if}
</div>

<style>
  .file-field {
    border: 1px solid #607566;
    border-radius: 12px;
    background: #fff;
    overflow: hidden;
  }
  .file-field.selected {
    border-color: var(--action);
    background: #f0f4ee;
  }
  .file-field:focus-within {
    outline: 3px solid var(--action);
    outline-offset: 4px;
  }
  label {
    position: relative;
    display: grid;
    gap: 12px;
    padding: 20px;
    cursor: pointer;
  }
  .file-field:not(.disabled):hover {
    background: var(--pale);
  }
  .file-heading {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 14px;
    font-weight: 600;
    line-height: 1.5;
  }
  .file-icon {
    display: flex;
    color: var(--action);
    flex-shrink: 0;
  }
  .file-name {
    min-width: 0;
    min-height: 42px;
    color: var(--muted);
    font-size: 13px;
    line-height: 1.6;
    overflow-wrap: anywhere;
  }
  .selected .file-name {
    color: var(--ink);
  }
  .choose {
    color: var(--action);
    font-size: 13px;
    font-weight: 600;
    text-decoration: underline;
    text-underline-offset: 3px;
  }
  input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    opacity: 0;
    cursor: pointer;
  }
  .file-field .remove {
    position: relative;
    margin: 0 12px 8px;
    padding: 8px;
    min-height: 44px;
    color: var(--muted);
    font-size: 13px;
    text-decoration: underline;
    text-underline-offset: 3px;
  }
  .disabled,
  .disabled input {
    cursor: wait;
  }
</style>
