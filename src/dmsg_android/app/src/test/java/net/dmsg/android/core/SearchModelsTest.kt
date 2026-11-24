package net.dmsg.android.core

import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class SearchModelsTest {
    @Test fun vaultListingParsesCamelCaseHighlightsAndMatchHint() {
        val raw = """
            {
              "total": 1,
              "all": 1,
              "favorites": 1,
              "kinds": {"login": 1},
              "hasMore": false,
              "items": [
                {
                  "id": "v1",
                  "revision": "r1",
                  "kind": "login",
                  "title": "招商银行",
                  "subtitle": "alice@example.com",
                  "favorite": true,
                  "tags": ["银行", "金融"],
                  "synced": true,
                  "conflict": false,
                  "reserved": false,
                  "deleted": false,
                  "createdAt": 1000,
                  "updatedAt": 2000,
                  "titleSpans": [{"text": "招商", "matched": true}, {"text": "银行", "matched": false}],
                  "subtitleSpans": [],
                  "matchField": "notes",
                  "matchValue": null
                }
              ],
              "tags": ["银行", "金融"]
            }
        """.trimIndent()
        val page = parseVaultListing(Json.parseToJsonElement(raw))
        assertEquals(1uL, page.total)
        val item = page.items.single()
        assertEquals("login", item.kind)
        assertEquals(listOf(HighlightSpan("招商", true), HighlightSpan("银行", false)), item.titleSpans)
        assertEquals("notes", item.matchField)
        assertNull(item.matchValue)
    }

    @Test fun globalSearchParsesExactRustWireFormat() {
        val raw = """
            {
              "query": "bj",
              "vault": [
                {
                  "id": "v1",
                  "revision": "r1",
                  "kind": "note",
                  "title": "北京出差",
                  "subtitle": "",
                  "favorite": false,
                  "tags": [],
                  "synced": true,
                  "conflict": false,
                  "reserved": false,
                  "deleted": false,
                  "createdAt": 10,
                  "updatedAt": 20,
                  "titleSpans": [{"text": "北京", "matched": true}, {"text": "出差", "matched": false}],
                  "subtitleSpans": [],
                  "score": 920
                }
              ],
              "channels": [
                {
                  "id": "ch1",
                  "name": "研发组",
                  "name_spans": [{"text": "研发", "matched": true}, {"text": "组", "matched": false}],
                  "unread": 2,
                  "direct": false,
                  "match_field": "member",
                  "match_value": "张三",
                  "score": 900
                }
              ],
              "messages": [
                {
                  "channel": "ch1",
                  "message": {"seq": 42, "message_id": "m42", "account": "acc1", "text": "明天发布版本", "stored_at": 500},
                  "sender_name": "张三",
                  "snippet_spans": [{"text": "明天", "matched": false}, {"text": "发布", "matched": true}, {"text": "版本", "matched": false}],
                  "score": 800
                }
              ],
              "letters": [
                {
                  "id": "l1",
                  "box_kind": "inbox",
                  "account": "acc2",
                  "display_name": "李四",
                  "name_spans": [{"text": "李四", "matched": true}],
                  "snippet_spans": [{"text": "发票已寄出", "matched": false}],
                  "created_at": 600,
                  "read": false,
                  "archived": false,
                  "state": "visible",
                  "score": 700
                }
              ],
              "files": [
                {
                  "id": "f1",
                  "kind": "folder",
                  "folder": "root",
                  "name": "财务资料",
                  "name_spans": [{"text": "财务", "matched": true}, {"text": "资料", "matched": false}],
                  "path": [],
                  "path_spans": [],
                  "mime": "",
                  "size": 0,
                  "status": "folder",
                  "score": 850
                },
                {
                  "id": "file1",
                  "kind": "file",
                  "folder": "f1",
                  "name": "账单.pdf",
                  "name_spans": [{"text": "账单", "matched": true}, {"text": ".pdf", "matched": false}],
                  "path": ["财务资料"],
                  "path_spans": [],
                  "mime": "application/pdf",
                  "size": 1024,
                  "status": "local",
                  "match_field": "tag",
                  "match_value": "财务",
                  "score": 820
                }
              ]
            }
        """.trimIndent()
        val result = parseGlobalSearchResult("bj", Json.parseToJsonElement(raw))
        assertFalse(result.isEmpty)
        assertEquals(1, result.vault.size)
        assertEquals("北京", result.vault.single().titleSpans.first().text)
        assertEquals("张三", result.channels.single().matchedMember)
        assertEquals("ch1", result.messages.single().channelId)
        assertEquals(42uL, result.messages.single().sequence)
        assertEquals("李四", result.letters.single().displayName)
        assertEquals(1, result.folders.size)
        assertEquals("财务", result.folders.single().nameSpans.first().text)
        assertEquals(1, result.files.size)
        assertEquals("tag", result.files.single().matchField)
        assertEquals("财务", result.files.single().matchValue)
    }

    @Test fun richChannelGroupsAndFilesSearchListingParseFlatRustFormat() {
        val groupsRaw = """
            [
              {
                "channel_id": "ch9",
                "channel_name": "项目群",
                "hits": [
                  {
                    "channel": "ch9",
                    "message": {"seq": 10, "message_id": "m10", "account": "a1", "text": "合同.pdf", "stored_at": 999, "file": {"name": "合同.pdf"}},
                    "sender_name": "王五",
                    "snippet_spans": [{"text": "合同", "matched": true}, {"text": ".pdf", "matched": false}],
                    "score": 750
                  }
                ],
                "next_before": 9,
                "top_score": 750
              }
            ]
        """.trimIndent()
        val groups = parseRichChannelGroups(Json.parseToJsonElement(groupsRaw))
        assertEquals(1, groups.size)
        assertEquals(9uL, groups.single().nextBefore)
        assertEquals("合同.pdf", groups.single().hits.single().attachmentName)

        val filesRaw = """
            {
              "folders": [
                {
                  "id": "fld1",
                  "name": "计划文档",
                  "parent": "root",
                  "path": [],
                  "name_spans": [{"text": "计划", "matched": true}, {"text": "文档", "matched": false}],
                  "path_spans": [],
                  "score": 880
                }
              ],
              "files": [
                {
                  "id": "d1",
                  "name": "计划书.docx",
                  "size": 500,
                  "status": "local",
                  "sha256": "def",
                  "mime": "application/octet-stream",
                  "current": "v1",
                  "folder": "root",
                  "local": true,
                  "starred": false,
                  "tags": [],
                  "note": "",
                  "updated_at": 2,
                  "path": [],
                  "name_spans": [{"text": "计划书", "matched": true}, {"text": ".docx", "matched": false}],
                  "path_spans": [],
                  "match_field": null,
                  "match_value": null,
                  "score": 900
                }
              ]
            }
        """.trimIndent()
        val (folders, files) = parseFilesSearchListing(Json.parseToJsonElement(filesRaw))
        assertEquals(1, folders.size)
        assertTrue(folders.single().nameSpans.first().matched)
        assertEquals(1, files.size)
        assertTrue(files.single().nameSpans.first().matched)
    }
}
