<script lang="ts">
  let { children } = $props()
  const year = new Date().getFullYear()
  let isDark = $derived(document.documentElement.classList.contains('dark'))
</script>

{@render children()}
<footer id="page-footer" class="px-4 pb-24 pt-12 text-surface-400">
  <div class="flex h-16 flex-col items-center">
    <p class="flex flex-row items-center gap-1">
      <span class="text-sm">© 2024-{year}</span>
      <a class="" href="https://panda.fans" target="_blank"
        ><img
          class="w-28"
          src={isDark
            ? '/_assets/icpanda-dao-white.svg'
            : '/_assets/icpanda-dao.svg'}
          alt="ICPanda DAO"
        /></a
      >
    </p>
    <p class="mt-2 text-center text-sm antialiased">
      Breathing life into sovereign AI.<br />We are building the open-source
      stack for agents to remember, transact, and evolve as first-class citizens
      in Web3.<br />
      <a
        class="underline underline-offset-4"
        href="https://anda.ai"
        target="_blank">Anda.AI</a
      >
      <span class="mx-1">|</span>
      <a class="underline underline-offset-4" href="https://dmsg.net"
        >dMsg.net</a
      >
    </p>
  </div>
</footer>
