import XCTest
@testable import dMsg

final class SearchModelsTests: XCTestCase {
    private var decoder: JSONDecoder {
        let d = JSONDecoder()
        d.keyDecodingStrategy = .convertFromSnakeCase
        return d
    }

    func testVaultSummaryDecodesCamelCaseHighlightsAndMatchHint() throws {
        let json = """
        {
          "id": "v1",
          "revision": "r1",
          "kind": "login",
          "title": "招商银行",
          "subtitle": "finance@example.com",
          "favorite": true,
          "tags": ["财务", "银行"],
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
        """
        let summary = try decoder.decode(VaultSummary.self, from: Data(json.utf8))
        XCTAssertEqual(summary.titleSpans?.count, 2)
        XCTAssertEqual(summary.titleSpans?.first?.text, "招商")
        XCTAssertEqual(summary.titleSpans?.first?.matched, true)
        XCTAssertEqual(summary.matchField, "notes")
        XCTAssertEqual(vaultMatchHint(matchField: summary.matchField, matchValue: summary.matchValue, subtitle: summary.subtitle),
                       String(localized: "Matched in notes"))
    }

    func testGlobalSearchResponseDecodesFlatRustWireFormat() throws {
        let json = """
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
              "score": 900
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
              "score": 880
            }
          ],
          "messages": [
            {
              "channel": "ch1",
              "message": {"seq": 42, "message_id": "m42", "account": "acc1", "text": "明天发布", "stored_at": 500},
              "sender_name": "张三",
              "snippet_spans": [{"text": "明天", "matched": false}, {"text": "发布", "matched": true}],
              "score": 850
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
              "score": 860
            },
            {
              "id": "d1",
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
              "score": 840
            }
          ]
        }
        """
        let response = try decoder.decode(GlobalSearchResponse.self, from: Data(json.utf8))
        XCTAssertFalse(response.isEmpty)
        XCTAssertEqual(response.vault.count, 1)
        XCTAssertEqual(response.channels.first?.matchValue, "张三")
        XCTAssertEqual(response.messages.first?.channel, "ch1")
        XCTAssertEqual(response.messages.first?.sequence, 42)
        XCTAssertEqual(response.letters.first?.displayName, "李四")
        XCTAssertEqual(response.folders.count, 1)
        XCTAssertEqual(response.regularFiles.count, 1)
        XCTAssertEqual(response.regularFiles.first?.matchValue, "财务")
    }

    func testAutoFillCredentialExportMatchesPinyinSearchKeysWithMultiTokenAnd() {
        let entry = CredentialExport(
            id: "1",
            title: "招商银行企业版",
            username: "finance@example.com",
            origin: "https://cmbchina.com",
            itemID: "v1",
            searchKeys: ["zhaoshangyinhangqiyeban", "zsyhqyb"]
        )
        XCTAssertTrue(entry.contains("zsyh"))
        XCTAssertTrue(entry.contains("zsyh finance"))
        XCTAssertTrue(entry.contains("zhaoshang cmb"))
        XCTAssertFalse(entry.contains("zsyh nonexistent"))
    }
}
