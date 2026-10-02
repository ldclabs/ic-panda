import { expect, it, vi } from 'vitest'
import { ChannelClient } from '../src/lib/services/channel'
import { id } from '../src/lib/protocol/codec'

function fixture(deadline = Date.now() - 1) {
  const channel = id(),
    oldMessage = id(),
    request = id(),
    key = `send:${oldMessage}`
  const jobs = new Map<string, any>([
    [
      key,
      {
        action: 'dmsg/channel/message/v1',
        context: { deadline, requestId: request },
        payload: { message_id: oldMessage, epoch: 1 }
      }
    ],
    [`message:${oldMessage}`, { text: 'saved offline draft', state: 'queued' }]
  ])
  const crypto = {
    call: vi.fn(async (method: string, _channel: string, key: string, value?: any) => {
      if (method === 'channelAttachmentSource') return null
      if (method === 'channelJob') {
        if (value !== undefined) jobs.set(key, value)
        return jobs.get(key)
      }
      throw new Error(method)
    })
  }
  const client = new ChannelClient({ crypto } as any, {} as any, '')
  const status = vi.spyOn(client.session, 'find').mockResolvedValue({ found: false })
  vi.spyOn(client, 'pullControl').mockResolvedValue({ status: 'active', epoch: 2 } as any)
  vi.spyOn(client as any, 'contentRootReady').mockReturnValue(undefined)
  const send = vi.spyOn(client, 'send').mockResolvedValue({ messageId: id(), receipt: {} })
  return { client, key, channel, oldMessage, status, send, jobs }
}
it('preserves an unexpired or committed original before considering a replacement', async () => {
  const f = fixture(Date.now() + 30000)
  await expect(f.client.reencrypt(f.channel, f.key)).rejects.toThrow('原批准')
  expect(f.send).not.toHaveBeenCalled()
  f.status.mockResolvedValue({ found: true, result: {} })
  const resume = vi.spyOn(f.client, 'resume').mockResolvedValue({ seq: 3 })
  await f.client.reencrypt(f.channel, f.key)
  expect(resume).toHaveBeenCalledWith(f.channel, f.key)
  expect(f.send).not.toHaveBeenCalled()
})
it('reuses one replacement ID after an uncertain new send', async () => {
  const f = fixture()
  f.send.mockRejectedValueOnce(new Error('offline'))
  await expect(f.client.reencrypt(f.channel, f.key)).rejects.toThrow('offline')
  await f.client.reencrypt(f.channel, f.key)
  expect(f.send.mock.calls[0]).toEqual(f.send.mock.calls[1])
  expect(f.send.mock.calls[1][2]).not.toBe(f.oldMessage)
  expect(f.jobs.get(`message:${f.oldMessage}`).state).toBe('replaced')
})
