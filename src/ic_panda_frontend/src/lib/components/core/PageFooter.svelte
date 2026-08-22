<script lang="ts">
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import { LINKS } from '$lib/site'

  const year = new Date().getFullYear()

  const columns = [
    {
      title: 'Explore',
      items: [
        { label: 'PANDA', href: '/#panda', external: false },
        { label: 'Governance', href: '/#dao', external: false },
        { label: 'Projects', href: '/#projects', external: false }
      ]
    },
    {
      title: 'Projects',
      items: [
        { label: 'Anda', href: LINKS.anda, external: true },
        { label: 'TokenList', href: LINKS.tokenlist, external: true },
        { label: 'dMsg · Archive', href: '/#archive', external: false }
      ]
    },
    {
      title: 'Community',
      items: [
        { label: 'GitHub', href: LINKS.github, external: true },
        { label: 'X', href: LINKS.x, external: true },
        { label: 'OpenChat', href: LINKS.openchat, external: true },
        { label: 'ICP Dashboard', href: LINKS.snsDashboard, external: true }
      ]
    }
  ]
</script>

<div class="border-t border-ink/15">
  <div class="mx-auto w-full max-w-[1180px] px-5 py-14 md:px-10 md:py-16">
    <div class="grid grid-cols-1 gap-12 lg:grid-cols-12 lg:gap-10">
      <div class="lg:col-span-5">
        <div class="flex items-center gap-3">
          <span
            class="size-8 shrink-0 overflow-hidden rounded-full ring-1 ring-ink/15 *:size-8"
          >
            <IconPanda />
          </span>
          <span class="font-mono text-sm font-semibold">ICPanda DAO</span>
        </div>
        <p class="display mt-6 max-w-sm text-2xl">
          From Sovereign Minds to Sovereign Markets.
        </p>
        <p
          class="mt-4 max-w-sm text-pretty text-sm leading-relaxed text-ink-70"
        >
          Open-source infrastructure for persistent intelligence and verifiable
          markets.
        </p>

        <div class="mt-8 max-w-sm border-t border-ink/10 pt-6">
          <p class="eyebrow">Get in touch</p>
          <a
            class="link-mark mt-3 !text-base"
            href={LINKS.alink}
            target="_blank"
            rel="noreferrer"
          >
            <span>al.ink/ICPanda</span>
            <span class="text-ink-30">↗</span>
          </a>
          <p class="mt-3 text-sm leading-relaxed text-ink-70">
            alink is the preferred way to reach ICPanda DAO.
          </p>
        </div>
      </div>

      {#each columns as column (column.title)}
        <nav class="lg:col-span-2 lg:col-start-auto">
          <p class="eyebrow">{column.title}</p>
          <ul class="mt-5 space-y-3">
            {#each column.items as item (item.label)}
              <li>
                <a
                  class="font-mono text-sm text-ink-70 transition-colors duration-200 hover:text-ink"
                  href={item.href}
                  target={item.external ? '_blank' : undefined}
                  rel={item.external ? 'noreferrer' : undefined}
                >
                  {item.label}{#if item.external}<span class="ml-1 text-ink-30"
                      >↗</span
                    >{/if}
                </a>
              </li>
            {/each}
          </ul>
        </nav>
      {/each}
    </div>

    <div
      class="mt-14 flex flex-col gap-4 border-t border-ink/10 pt-6 sm:flex-row sm:items-center sm:justify-between"
    >
      <a
        class="font-mono text-xs text-ink-70 transition-colors hover:text-ink"
        href={LINKS.internetComputer}
        target="_blank"
        rel="noreferrer"
      >
        Built on the Internet Computer.
      </a>
      <p class="font-mono text-xs text-ink-70">
        © 2024-{year} ICPanda DAO
      </p>
    </div>
  </div>
</div>
