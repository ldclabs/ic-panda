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
      white: 'rgba(255,255,255,1)',
      black: 'rgba(0,0,0,0.95)'
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
