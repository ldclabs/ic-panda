<script lang="ts">
  let { children } = $props()
</script>

{@render children()}
<footer id="page-footer" class="px-4 pb-24 pt-12 text-surface-400">
  <div class="flex h-16 flex-col items-center">
    <p class="flex flex-row items-center gap-1">
      <span class="text-sm">© 2024-2025</span>
      <a class="" href="https://panda.fans" target="_blank"
        ><img
          class="w-28"
          src="/_assets/icpanda-dao-white.svg"
          alt="ICPanda DAO"
        /></a
      >
    </p>
    <p class="mt-2 text-center text-sm capitalize antialiased">
      A technical panda fully running on the <a
        class="underline underline-offset-4"
        href="https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai"
        target="_blank"
      >
        Internet Computer
      </a> blockchain, building chain-native infras and practical Web3 apps.
    </p>
  </div>
</footer>
