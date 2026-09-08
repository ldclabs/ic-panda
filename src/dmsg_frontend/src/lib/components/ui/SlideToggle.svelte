<script lang="ts">
  import { Switch } from 'bits-ui'

  /**
   * Replaces Skeleton's `SlideToggle` on top of the bits-ui `Switch`, keeping
   * the `active`/`size` props the call sites already pass.
   */
  interface Props {
    class?: string
    name: string
    checked?: boolean
    disabled?: boolean
    active?: string
    size?: 'sm' | 'md' | 'lg'
    label?: string
    /**
     * Fired with the value the switch just moved to.
     *
     * Prefer this over `onclick`: bits-ui composes a forwarded `onclick`
     * ahead of its own handler, so a click listener still sees the previous
     * `checked` value.
     */
    onCheckedChange?: (checked: boolean) => void
  }

  let {
    class: selfClass = '',
    name,
    checked = $bindable(false),
    disabled = false,
    active = 'bg-primary-500',
    size = 'md',
    label = '',
    onCheckedChange = () => {}
  }: Props = $props()

  const track = {
    sm: 'h-6 w-10',
    md: 'h-8 w-14',
    lg: 'h-10 w-[4.5rem]'
  }

  const thumb = {
    sm: 'size-4',
    md: 'size-6',
    lg: 'size-8'
  }
</script>

<Switch.Root
  {name}
  {disabled}
  {onCheckedChange}
  bind:checked
  aria-label={label || name}
  class="inline-flex shrink-0 cursor-pointer items-center rounded-full p-1 transition-colors duration-200 disabled:cursor-not-allowed disabled:opacity-50 {track[
    size
  ]} {checked ? active : 'bg-surface-400 dark:bg-surface-600'} {selfClass}"
>
  <Switch.Thumb
    class="pointer-events-none rounded-full bg-white shadow-sm transition-transform duration-200 {thumb[
      size
    ]} {checked ? 'translate-x-full rtl:-translate-x-full' : 'translate-x-0'}"
  />
</Switch.Root>
