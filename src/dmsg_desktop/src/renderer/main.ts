import { mount } from 'svelte'
import App from './App.svelte'
import '@fontsource-variable/inter'
import './app.css'

// The main window on macOS shows the sidebar's material through a transparent page.
if (window.dmsg.role === 'main' && window.dmsg.platform === 'darwin') document.documentElement.dataset.chrome = 'inset'

mount(App, { target: document.getElementById('app')! })
