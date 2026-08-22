<script lang="ts">
  import IconArrowRightUp from '$lib/components/icons/IconArrowRightUp.svelte'

  interface Props {
    href: string
    label: string
    /** `mark` is the inline underline style; the others are buttons. */
    variant?: 'mark' | 'ink' | 'outline'
    class?: string
  }

  let { href, label, variant = 'mark', class: klass = '' }: Props = $props()

  const base =
    variant === 'ink'
      ? 'btn-ink'
      : variant === 'outline'
        ? 'btn-outline'
        : 'link-mark'
</script>

<a class="{base} {klass}" {href} target="_blank" rel="noreferrer">
  <span>{label}</span>
  <span class="*:size-4" aria-hidden="true"><IconArrowRightUp /></span>
</a>
