<script lang="ts">
  export let value = ''
  export let minHeight = '40'
  export let maxHeight = '200'
  export let containerClass = ''
  export let onKeydown: (event: KeyboardEvent) => void = () => {}
</script>

<div class="relative !p-0 {containerClass}">
  <pre
    aria-hidden="true"
    class="invisible w-full text-pretty break-all px-3 py-2"
    style="min-height: {minHeight}px; max-height: {maxHeight}px"
    >{value + ' '}</pre
  >

  <textarea bind:value on:keydown={onKeydown} {...$$restProps}></textarea>
</div>

<style>
  textarea {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    resize: none;
  }
</style>
