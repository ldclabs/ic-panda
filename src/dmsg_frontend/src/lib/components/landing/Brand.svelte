<script lang="ts">
  import { t } from '$lib/i18n'

  // The Gate (static/_assets/dmsg/mark.svg): a post and a dome, with the one
  // request that waits for you between them.
  const POST =
    'M4 0h4.5A1.5 1.5 0 0 1 10 1.5v45A1.5 1.5 0 0 1 8.5 48H4a4 4 0 0 1-4-4V4a4 4 0 0 1 4-4Z'
  const DOME =
    'M15.5 0H18a24 24 0 0 1 0 48h-2.5a1.5 1.5 0 0 1-1.5-1.5v-45A1.5 1.5 0 0 1 15.5 0Z'
  const GEM = 'M12 17.5 18.5 24 12 30.5 5.5 24Z'
  const uid = $props.id()
  const cut = `${uid}-cut`
</script>

<a class="brand" href="/" aria-label={$t('dMsg home')} data-sveltekit-reload>
  <svg
    class="brand-mark"
    viewBox="0 0 42 48"
    aria-hidden="true"
    focusable="false"
  >
    <mask
      id={cut}
      maskUnits="userSpaceOnUse"
      x="0"
      y="0"
      width="42"
      height="48"
    >
      <rect width="42" height="48" fill="#fff" />
      <path d={GEM} stroke="#000" stroke-width="5" stroke-linejoin="round" />
    </mask>
    <g mask="url(#{cut})"><path d={POST} /><path d={DOME} /></g>
    <path class="brand-gem" d={GEM} />
  </svg>
  <span>dMsg</span>
</a>
