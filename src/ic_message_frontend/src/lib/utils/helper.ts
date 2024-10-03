export async function sleep(ms: number): Promise<void> {
  return new Promise((res) => setTimeout(res, ms))
}
