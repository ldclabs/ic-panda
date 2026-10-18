import { mount } from 'svelte'
import App from './App.svelte'
import { showPendingButtons } from './components/pending'
import '@fontsource-variable/inter'
import './app.css'

// The main window on macOS shows the sidebar's material through a transparent
// page; full screen hides its traffic lights, and the title bar takes back their room.
if (window.dmsg.role === 'main' && window.dmsg.platform === 'darwin') {
  document.documentElement.dataset.chrome = 'inset'
  window.dmsg.onFullScreen((fullScreen) => document.documentElement.toggleAttribute('data-fullscreen', fullScreen))
}

showPendingButtons()
mount(App, { target: document.getElementById('app')! })
