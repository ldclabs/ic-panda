/** A long hexadecimal or base32 value in groups of four, as a person compares it; the page spaces them, a copy keeps the value. */
export const idChunks = (id: string) => (/^[0-9a-z]{16,}$/i.test(id) ? (id.match(/.{1,4}/g) ?? [id]) : [id])
