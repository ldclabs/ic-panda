<script lang="ts">
  import Reveal from '$lib/components/ui/Reveal.svelte'
  import { DMSG_CAPABILITIES, LINKS } from '$lib/site'
  import LinkOut from './LinkOut.svelte'
</script>

<Reveal>
  <article
    id="dmsg"
    class="border-ink/15 mt-6 mb-16 scroll-mt-24 overflow-hidden rounded-xl border bg-white md:mb-24"
    aria-labelledby="dmsg-title"
  >
    <header class="border-ink/10 border-b p-6 md:p-10">
      <div class="flex flex-wrap items-center justify-between gap-4">
        <p class="eyebrow">dMsg · Private Workspace · Identity Signing</p>
        <span
          class="border-ink/20 text-ink-70 rounded-md border px-3 py-1.5 font-mono text-xs"
        >
          Chrome Extension
        </span>
      </div>
      <h3
        id="dmsg-title"
        class="display mt-6 max-w-3xl text-[clamp(2rem,4.5vw,3.25rem)]"
      >
        Work privately.<br />Sign with intent.
      </h3>
      <p class="text-ink-70 mt-6 max-w-2xl leading-relaxed text-pretty">
        Your private workspace in Chrome. Keep credentials and files in an
        encrypted vault, collaborate in private channels, and approve identity
        and signing requests across the applications you use.
      </p>
      <p class="text-ink-70 mt-4 max-w-2xl text-sm leading-relaxed">
        Useful before you invite anyone. Connected when you choose to share.
      </p>
    </header>

    <div class="bg-ink/10 grid grid-cols-1 gap-px md:grid-cols-2">
      {#each DMSG_CAPABILITIES as capability (capability.index)}
        <div class="bg-white p-6 md:p-8">
          <p class="eyebrow">{capability.index} / dMsg</p>
          <h4 class="mt-5 font-mono text-lg font-semibold tracking-tight">
            {capability.name}
          </h4>
          <p class="text-ink-70 mt-4 text-sm leading-relaxed text-pretty">
            {capability.body}
          </p>
        </div>
      {/each}
    </div>

    <div
      class="border-ink/10 bg-paper/60 grid grid-cols-1 gap-8 border-t p-6 md:grid-cols-2 md:p-10"
    >
      <div>
        <h4 class="font-mono text-sm font-semibold">Encrypted on your device</h4
        >
        <p class="text-ink-70 mt-3 text-sm leading-relaxed text-pretty">
          Your extension handles encryption and decryption. Cloudflare stores
          ciphertext and runs delivery, while ICP anchors identity, key control,
          and payment settlement. Ordinary messages stay off the blockchain.
        </p>
        <p class="text-ink-70 mt-3 text-sm leading-relaxed text-pretty">
          Keep working with your local data, search on your device, and take
          encrypted exports with you. Account recovery and content recovery
          remain distinct, with recovery material in your hands.
        </p>
      </div>
      <div>
        <h4 class="font-mono text-sm font-semibold">Connect with permission</h4>
        <p class="text-ink-70 mt-3 text-sm leading-relaxed text-pretty">
          Sign a TokenList file statement or manage a personal alink controller
          key. Each connection has a defined scope, and formal signatures return
          to the extension for your confirmation.
        </p>
        <p class="text-ink-70 mt-3 text-sm leading-relaxed text-pretty">
          Connecting an app does not give it your vault or private keys.
          Permissions stay tied to the identity, content, and action you
          approve.
        </p>
      </div>
    </div>

    <footer
      class="border-ink/10 flex flex-col gap-6 border-t p-6 md:flex-row md:items-center md:justify-between md:p-10"
    >
      <p class="display-sm max-w-md text-xl md:text-2xl">
        Useful on your own.<br />Connected on your terms.
      </p>
      <div class="flex flex-wrap items-center gap-x-8 gap-y-4">
        <LinkOut href={LINKS.dmsgOverview} label="How dMsg Works" />
        <LinkOut href={LINKS.dmsg} label="Open dMsg" />
      </div>
    </footer>
  </article>
</Reveal>
