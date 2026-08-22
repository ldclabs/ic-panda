<script lang="ts">
  import IconArrowDownLine from '$lib/components/icons/IconArrowDownLine.svelte'
  import HeroSchematic from './HeroSchematic.svelte'
</script>

<section id="hero" class="relative">
  <div
    class="mx-auto w-full max-w-[1180px] px-5 pb-16 pt-14 md:px-10 md:pb-24 md:pt-20"
  >
    <p class="eyebrow">
      Open Source <span class="text-panda">·</span> On-chain Governed
      <span class="text-panda">·</span> Built on ICP
    </p>

    <h1 class="display mt-7 text-[clamp(2.25rem,7.2vw,6rem)]">
      From Sovereign Minds<br />to Sovereign Markets.<span
        class="caret ml-3 inline-block h-[0.58em] w-[0.26em] bg-panda align-baseline"
        aria-hidden="true"
      ></span>
    </h1>

    <div
      class="mt-12 grid grid-cols-1 gap-10 border-t border-ink/15 pt-10 lg:grid-cols-12 lg:gap-12"
    >
      <div class="lg:col-span-7">
        <p
          class="max-w-2xl text-pretty text-lg leading-relaxed text-ink-70 md:text-xl"
        >
          ICPanda DAO is an on-chain builder collective creating open
          infrastructure for
          <span class="font-medium text-ink">persistent AI cognition</span>
          and
          <span class="font-medium text-ink"
            >verifiable crypto capital formation</span
          >.
        </p>

        <div class="mt-10 flex flex-col gap-3 sm:flex-row">
          <a class="btn-ink" href="#panda">Explore PANDA</a>
          <a class="btn-outline" href="#projects">
            What We Build
            <span class="*:size-4"><IconArrowDownLine /></span>
          </a>
        </div>

        <p class="mt-10 max-w-md font-mono text-xs leading-relaxed text-ink-70">
          Governed by the community through PANDA and the Internet Computer SNS.
        </p>
      </div>

      <div class="lg:col-span-5">
        <HeroSchematic />
      </div>
    </div>
  </div>
</section>
