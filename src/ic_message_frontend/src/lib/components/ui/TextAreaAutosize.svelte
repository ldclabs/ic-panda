<script lang="ts">
  interface Props {
    value?: string
    minHeight?: string
    maxHeight?: string
    containerClass?: string
    onFilesChange?: null | ((files: FileList) => void)
    onKeydown?: (event: KeyboardEvent) => void
    [key: string]: any
  }

  let {
    value = $bindable(''),
    minHeight = '40',
    maxHeight = '200',
    containerClass = '',
    onFilesChange = null,
    onKeydown,
    ...rest
  }: Props = $props()

  let dragOver = $state(false)

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
    class="invisible w-full text-pretty break-words px-3 py-2"
    style="min-height: {minHeight}px; max-height: {maxHeight}px"
    >{value + ' '}</pre
  >

  <textarea
    bind:value
    onkeydown={onKeydown}
    ondragenter={onFilesChange && handleDragEnter}
    ondragleave={onFilesChange && handleDragLeave}
    ondrop={onFilesChange && handleDrop}
    onpaste={onFilesChange && handlePaste}
    {...rest}
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
