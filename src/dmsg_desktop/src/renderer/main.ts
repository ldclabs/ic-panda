import { mount } from 'svelte'
import App from './App.svelte'
import { showPendingButtons } from './components/pending'
import { languageReady } from './i18n/index.svelte'
// The optical-size axis tightens large titles and opens small text.
import '@fontsource-variable/inter/opsz.css'
import './app.css'

// The main window on macOS shows the sidebar's material through a transparent
// page; full screen hides its traffic lights, and the title bar takes back their room.
if (window.dmsg.role === 'main' && window.dmsg.platform === 'darwin') {
  document.documentElement.dataset.chrome = 'inset'
  window.dmsg.onFullScreen((fullScreen) => document.documentElement.toggleAttribute('data-fullscreen', fullScreen))
}

showPendingButtons()
await languageReady
mount(App, { target: document.getElementById('app')! })
