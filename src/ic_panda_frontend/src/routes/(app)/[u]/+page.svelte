<script lang="ts">
  import { page } from '$app/stores'
  export let data // from load function in +page.ts
  console.log('$page.url', $page.url)
  console.log('data', data)
</script>

<div
  class="mb-6 mt-6 flex flex-col flex-nowrap content-center items-center px-4"
>
  <div
    class="flex w-full max-w-4xl flex-col flex-nowrap content-center items-center"
  >
    hello {data.u}
  </div>
</div>
