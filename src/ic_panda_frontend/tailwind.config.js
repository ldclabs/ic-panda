import { skeleton } from '@skeletonlabs/tw-plugin'
import { join } from 'path'
import forms from '@tailwindcss/forms'

const config = {
  // darkMode: 'class',
  content: [
    './src/**/*.{html,js,svelte,ts}',
    join(
      require.resolve('@skeletonlabs/skeleton'),
      '../**/*.{html,js,svelte,ts}'
    )
  ],
  theme: {
    colors: {
      panda: '#11c291',
      gold: '#fbbf24',
      gray: '#0a0a0a',
      white: '#ffffff',
      black: '#000000'
    },
    extend: {}
  },
  plugins: [
    forms,
    skeleton({
      themes: { preset: ['skeleton'] }
    })
  ]
}

export default config
