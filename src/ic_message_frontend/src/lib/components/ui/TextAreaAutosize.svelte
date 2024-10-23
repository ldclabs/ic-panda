<script lang="ts">
  export let value = ''
  export let minHeight = '40'
  export let maxHeight = '200'
  export let containerClass = ''
  export let onFilesChange: null | ((files: FileList) => void) = null
  export let onKeydown: (event: KeyboardEvent) => void

  let dragOver = false

  function handleDragEnter(event: DragEvent) {
    event.preventDefault()
    dragOver = true
  }

  function handleDragLeave(event: DragEvent) {
    event.preventDefault()
    dragOver = false
  }

  function handleDrop(event: DragEvent) {
    event.preventDefault()
    dragOver = false
    const files = event.dataTransfer?.files || []
    if (files.length > 0) {
      onFilesChange!(files as FileList)
    }
  }

  function handlePaste(event: ClipboardEvent) {
    const files = event.clipboardData?.files || []
    if (files.length > 0) {
      event.preventDefault()
      onFilesChange!(files as FileList)
    }
  }
</script>

<div class="relative !p-0 {containerClass}" class:drag-over={dragOver}>
  <pre
    aria-hidden="true"
    class="invisible w-full text-pretty break-all px-3 py-2"
    style="min-height: {minHeight}px; max-height: {maxHeight}px"
    >{value + ' '}</pre
  >

  <textarea
    bind:value
    on:keydown={onKeydown}
    on:dragenter={onFilesChange && handleDragEnter}
    on:dragover|preventDefault
    on:dragleave={onFilesChange && handleDragLeave}
    on:drop={onFilesChange && handleDrop}
    on:paste={onFilesChange && handlePaste}
    {...$$restProps}
  ></textarea>
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

  .drag-over::after {
    content: '';
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background-color: rgba(88, 212, 178, 0.2);
    pointer-events: none;
  }
</style>
