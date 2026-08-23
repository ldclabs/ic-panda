<script lang="ts">
  interface TableSource {
    head: Array<string | number>
    body: Array<Array<string | number>>
  }

  let selfClass = ''
  export { selfClass as class }
  export let source: TableSource
  export let hideHead = false
</script>

<div class="w-full overflow-x-auto {selfClass}">
  <table class="w-full border-collapse text-left">
    {#if !hideHead}
      <thead>
        <tr>
          {#each source.head as cell}
            <th
              class="border-b border-black/10 px-3 py-3 text-xs font-semibold whitespace-nowrap"
            >
              {cell}
            </th>
          {/each}
        </tr>
      </thead>
    {/if}
    <tbody>
      {#each source.body as row}
        <tr class="border-b border-black/5 last:border-0">
          {#each row as cell}
            <td class="text-gray/60 px-3 py-3 text-sm whitespace-nowrap"
              >{cell}</td
            >
          {/each}
        </tr>
      {/each}
    </tbody>
  </table>
</div>
