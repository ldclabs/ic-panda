import { skeleton } from '@skeletonlabs/tw-plugin'
import forms from '@tailwindcss/forms'
import { join } from 'path'
import colors from 'tailwindcss/colors'

/** @type {import('tailwindcss').Config} */
const config = {
  content: [
    './src/**/*.{html,js,svelte,ts}',
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
      gray: '#0a0a0a',
      white: colors.white,
      black: colors.black,
      pink: colors.pink,
      orange: colors.orange,
      amber: colors.amber,
      indigo: colors.indigo,
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
