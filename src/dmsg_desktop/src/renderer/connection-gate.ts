/** Whether this window is waiting for its daemon: views keep what they show and fetch nothing new. */
export const daemonBlocked = () => document.body.hasAttribute('data-daemon-unverified')
