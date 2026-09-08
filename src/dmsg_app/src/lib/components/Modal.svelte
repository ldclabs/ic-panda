<script lang="ts">
  import { onMount, type Snippet } from 'svelte'
  import Icon from './Icon.svelte'
  let { title, onclose, children }: { title: string; onclose: () => void; children: Snippet } =
    $props()
  let dialog: HTMLDialogElement
  onMount(() => {
    const before = document.activeElement as HTMLElement
    dialog.showModal()
    return () => before?.focus()
  })
</script>

<dialog
  bind:this={dialog!}
  oncancel={(event) => {
    event.preventDefault()
    onclose()
  }}
  aria-label={title}
  class="modal"
>
  <div class="modal-head">
    <h2>{title}</h2>
    <button type="button" class="icon-button" aria-label="关闭对话框" onclick={onclose}
      ><Icon name="close" /></button
    >
  </div>
  <div class="modal-body">{@render children()}</div>
</dialog>
