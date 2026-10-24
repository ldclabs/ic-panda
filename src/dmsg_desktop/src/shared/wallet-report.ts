/** CSV is assembled in the main process; spreadsheet formula text stays text. */
export function walletHistoryCsv(report: unknown): string {
  const value = report as { format?: string; rows?: unknown[] }
  if (value?.format !== 'dmsg-wallet-history/1' || !Array.isArray(value.rows) || value.rows.length > 100_000)
    throw new Error('Invalid wallet report')
  const cell = (value: unknown) => {
    if (typeof value !== 'string' || value.length > 10_000) throw new Error('Invalid wallet report cell')
    const safe = /^[\s]*[=+\-@\t\r]/.test(value) ? `'${value}` : value
    return `"${safe.replaceAll('"', '""')}"`
  }
  const rows = value.rows.map((row) => {
    if (!Array.isArray(row) || row.length !== 14) throw new Error('Invalid wallet report row')
    return row.map(cell).join(',')
  })
  return (
    '\uFEFF' +
    [
      'wallet,proposal,result,operation,network,asset,amount_display,amount_atomic,recipient,created_at,transaction_ids,ledger_blocks,local_note,coverage',
      ...rows
    ].join('\r\n') +
    '\r\n'
  )
}
