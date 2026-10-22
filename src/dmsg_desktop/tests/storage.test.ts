import { test } from 'node:test'
import assert from 'node:assert/strict'
import { candidates, filtered, releaseBytes, targetOf } from '../src/renderer/views/files/storage.ts'
const copies = [
  { id:'a',upload_id:'11',owner_kind:'account',resource:'owner',kind:'share',phase:'stored',name:'notes',source_file:'source',bytes:20,charged_bytes:20,can_revoke:true,share:{share_id:'link'},created_at:2 },
  { id:'b',upload_id:'22',owner_kind:'channel',resource:'channel',kind:'file',phase:'released',bytes:10,charged_bytes:0,can_delete:false,created_at:1 },
  { id:'c',upload_id:'33',owner_kind:'account',resource:'owner',kind:'backup',phase:'staging',bytes:40,reserved_bytes:40,can_cancel:true,created_at:3 }
]
test('live links remain revocable while their underlying objects are protected',()=>{
 assert.deepEqual(candidates(copies,['a','b','c']).map(e=>e.id),['a','c'])
 assert.equal(releaseBytes(candidates(copies,['a','c'])),60)
 assert.deepEqual(targetOf(copies[0],'delete'),{owner_kind:'account',resource:'owner',upload_id:'11',generation:0,action:'delete',share_id:'link'})
 const retired={...copies[0],can_revoke:false,can_delete:true,share:{share_id:'old',state:'revoked'}}
 assert.equal(targetOf(retired,'delete').share_id,null)
})
test('storage filters source association, kind, state and channel without changing the inventory',()=>{
 const filter={text:'',kind:'',phase:'',resource:'',before:'',sort:'size'}
 assert.deepEqual(filtered(copies,filter).map(e=>e.id),['c','a','b'])
 assert.deepEqual(filtered(copies,{...filter,text:'source'}).map(e=>e.id),['a'])
 assert.deepEqual(filtered(copies,{...filter,kind:'file',phase:'released',resource:'channel'}).map(e=>e.id),['b'])
 assert.equal(copies[0].id,'a')
})
