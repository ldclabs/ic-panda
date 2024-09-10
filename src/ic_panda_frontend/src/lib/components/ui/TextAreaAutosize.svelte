<script lang="ts">
  export let value = ''
  export let minHeight = '40'
  export let maxHeight = '200'
  export let onKeydown: (event: KeyboardEvent) => void = () => {}
</script>

<div class="relative !p-0">
  <pre
    aria-hidden="true"
    class="invisible"
    style="min-height: {minHeight}px; max-height: {maxHeight}px">{value}</pre
  >

  <textarea bind:value on:keydown={onKeydown} {...$$restProps}></textarea>
</div>

<style>
  textarea {
    position: absolute;
    width: 100%;
    height: 100%;
    top: 0;
    resize: none;
  }
</style>
