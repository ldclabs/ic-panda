import { skeleton } from '@skeletonlabs/tw-plugin'
import forms from '@tailwindcss/forms'
import { join } from 'path'
import colors from 'tailwindcss/colors'

const config = {
  // darkMode: 'class',
  content: [
    './src/lib/**/*.{html,svelte,ts}',
    './src/routes/**/*.{html,svelte,ts}',
    join(
      require.resolve('@skeletonlabs/skeleton'),
      '../**/*.{html,svelte,js,ts}'
    )
  ],
  theme: {
    colors: {
      transparent: 'transparent',
      current: 'currentColor',
      panda: '#11c291',
      gold: '#fbbf24',
      pink: colors.pink,
      gray: '#0a0a0a',
      orange: colors.orange,
      amber: colors.amber,
      white: colors.white,
      black: colors.black,
      red: colors.red
    },
    extend: {}
  },
  plugins: [
    forms,
    skeleton({
      themes: { preset: [{ name: 'skeleton', enhancements: true }] }
    })
  ],
  safelist: ['text-black']
}

export default config
