/** A launcher or test runner may close its output pipes while the app is
 * still alive. Logging must not turn that into a main-process crash dialog.
 * Handle only broken output pipes; other stream failures remain errors. */
export function handleBrokenOutput(stream: NodeJS.WritableStream): void {
  stream.on('error', (error: NodeJS.ErrnoException) => {
    if (error.code !== 'EPIPE') throw error
  })
}
