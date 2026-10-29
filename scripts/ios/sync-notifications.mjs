#!/usr/bin/env node
/** Keep fixed native/APNs notification copy aligned with the desktop catalogs. */
import { notificationCatalogs } from '../../src/dmsg_desktop/src/shared/notification-text.ts'
import { readFileSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { spawnSync } from 'node:child_process'

const root = fileURLToPath(new URL('../../', import.meta.url))
const check = process.argv.includes('--check')
const catalogs = Object.fromEntries(Object.entries(notificationCatalogs).map(([language, values]) => [language === 'zh' ? 'zh-Hans' : language, values]))
const result = spawnSync('python3', ['-c', `
import json,plistlib,sys
from pathlib import Path
root=Path(sys.argv[1]); check=sys.argv[2]=='check'; catalogs=json.load(sys.stdin)
for language,values in catalogs.items():
    path=root/'src/dmsg_ios/Resources/Localizations'/f'{language}.lproj/Localizable.strings'
    original=path.read_bytes(); content=plistlib.loads(original)
    expected={'dmsg.'+key:value for key,value in values.items()}
    for category,key in [('SECURITY','noticeSecurityBody'),('ACTION','noticeActionBody'),('MESSAGE','noticeMessageBody'),('SERVICE','noticeServiceBody'),('GENERIC','noticeUpdatesBody')]:
        expected['DMSG_NOTIFICATION_'+category]=values[key]
    if check:
        if any(content.get(key)!=value for key,value in expected.items()): raise SystemExit('Notification copy drift: '+language)
    else:
        content.update(expected); path.write_bytes(plistlib.dumps(content,fmt=plistlib.FMT_XML,sort_keys=True))
`, root, check ? 'check' : 'write'], { input: JSON.stringify(catalogs), encoding: 'utf8' })
if (result.status !== 0) throw new Error(result.stderr || result.stdout || 'Notification localization failed')
const path = new URL('../../src/dmsg_ios/Platform/NotificationModels.swift', import.meta.url)
const source = readFileSync(path, 'utf8')
const start = source.indexOf('    static let fallback: [String: String] = [')
if (start < 0) throw new Error('Missing notification fallback marker')
const end = source.indexOf('\n    ]', start) + '\n    ]'.length
const replacement = '    static let fallback: [String: String] = [\n' + Object.entries(notificationCatalogs.en).map(([key, value]) => `        ${JSON.stringify(key)}: ${JSON.stringify(value)}`).join(',\n') + '\n    ]'
if (check) {
  if (source.slice(start, end) !== replacement) throw new Error('Notification fallback drift')
} else writeFileSync(path, source.slice(0, start) + replacement + source.slice(end))
console.log(`Notification copy ${check ? 'verified' : 'updated'} in ${Object.keys(catalogs).length} languages`)
