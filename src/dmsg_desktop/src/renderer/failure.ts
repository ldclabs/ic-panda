/**
 * What a failure says in a person's words. A failed call reaches a window as
 * the daemon's code and the message it wrote for a log:
 *
 * `Unavailable: ic: The replica returned a rejection error: reject code
 *  CanisterError, reject message Error from Canister uwist-…: Canister has no
 *  query method 'get_membership_terms'.. Check that the method being called is
 *  exported by the target canister. See documentation: https://…, error code
 *  Some("IC0536")`
 *
 * The code alone tells nobody why a page stayed empty, and the message as it
 * stands is mostly the transport talking about itself. `reasonOf` keeps what
 * the far end said, which is the part a person or a report needs.
 */

/** How much of a reason a notice shows before it stops. */
const LIMIT = 200

/** The message up to its last whole word within the limit. */
function shorten(text: string): string {
  const space = text.lastIndexOf(' ', LIMIT)
  return (space > LIMIT / 2 ? text.slice(0, space) : text.slice(0, LIMIT)).trimEnd() + '…'
}

/**
 * The reason inside a daemon message, on one line: a canister's rejection in
 * its own words with its IC error code, without the wrapper the transport
 * puts around it or the documentation link it offers. Empty when the message
 * carries nothing beyond its code.
 */
export function reasonOf(message: string): string {
  let text = message.replace(/\s+/g, ' ').trim()
  // An IC failure ends with its error code, kept for a report.
  const coded = /\s*,?\s*error code (?:Some\("([^"]+)"\)|None)\.?$/.exec(text)
  if (coded) text = text.slice(0, coded.index)
  // What the canister said, rather than how the call carried it there.
  const said = text.indexOf('reject message ')
  if (said >= 0) text = text.slice(said + 'reject message '.length)
  text = text
    .replace(/^(?:Unavailable|InvalidInput): /, '')
    .replace(/^Error from Canister \S+: /, '')
    .replace(/ See documentation: \S+/, '')
    // A rejection the replica ended itself with a period reads without two.
    .replace(/(?<!\.)\.{2}(?!\.)/g, '.')
    .trim()
  // A message that only names the code says no more than the code.
  if (!text || /^[A-Z][A-Z_]*$/.test(text)) return ''
  if (text.length > LIMIT) text = shorten(text)
  return coded?.[1] ? text + ' (' + coded[1] + ')' : text
}
