use super::*;

#[test]
fn pinyin_full_initials_hybrid_and_heteronym_recall() {
    let bank = "招商银行信用卡";

    // Full pinyin, initials, hybrid, half-typed prefix
    for q in [
        "zhaoshang",
        "yinhang",
        "zhaoshangyinhang",
        "zsyh",
        "xyk",
        "zsyhxyk",
        "zhaoshangyh",
        "zsyinhang",
        "zhaos",
        "zhsh",
        "zs银行",
        "招商yh",
    ] {
        let query = Query::parse(q);
        let hit = query
            .match_text(bank)
            .unwrap_or_else(|| panic!("expected `{q}` to match `{bank}`"));
        assert!(hit.score > 0, "query `{q}` score should be positive");
        let spans = highlight_spans(bank, &hit.ranges);
        assert!(
            spans.iter().any(|s| s.matched),
            "query `{q}` must produce matched highlight span"
        );
    }

    // Highlight accuracy: `zsyh` highlights exactly "招商银行"
    let hit = Query::parse("zsyh").match_text(bank).unwrap();
    let spans = highlight_spans(bank, &hit.ranges);
    assert_eq!(
        spans,
        vec![
            HighlightSpan {
                text: "招商银行".into(),
                matched: true,
            },
            HighlightSpan {
                text: "信用卡".into(),
                matched: false,
            },
        ]
    );

    // Heteronyms: 行 (hang/xing), 重 (chong/zhong), 长 (chang/zhang), 曾 (zeng/ceng), 单 (dan/shan)
    assert!(Query::parse("yinhang").match_text("中国银行").is_some());
    assert!(Query::parse("xingzou").match_text("行走路线").is_some());
    assert!(Query::parse("chongqing").match_text("重庆火锅").is_some());
    assert!(Query::parse("zhongyao").match_text("重要文件").is_some());
    assert!(Query::parse("changcheng").match_text("长城宽带").is_some());
    assert!(Query::parse("zhangda").match_text("长大").is_some());
    assert!(Query::parse("zengjing").match_text("曾经").is_some());
    assert!(Query::parse("cengjing").match_text("曾经").is_some());
    assert!(Query::parse("shanshan").match_text("单珊").is_some());

    // Prevent mid-syllable false positives: `osh` must NOT match `招商` (zh[aosh]ang),
    // `ngan` must NOT match `方案` (fa[ngan]).
    assert!(Query::parse("osh").match_text("招商").is_none());
    assert!(Query::parse("ngan").match_text("方案").is_none());
}

#[test]
fn umlaut_fuzzy_pinyin_and_traditional_simplified_folding() {
    // ü <-> v / u / ü
    for q in ["lvxing", "luxing", "lüxing", "lv", "nv", "nu", "nvsheng"] {
        let target = if q.starts_with('l') {
            "日本旅行计划"
        } else {
            "女生宿舍"
        };
        assert!(
            Query::parse(q).match_text(target).is_some(),
            "`{q}` should match `{target}`"
        );
    }

    // Fuzzy pinyin pairs: zh/z, ch/c, sh/s, l/n, in/ing, en/eng, an/ang
    assert!(Query::parse("zaosang").match_text("招商银行").is_some());
    assert!(Query::parse("yinhan").match_text("招商银行").is_some());
    assert!(Query::parse("yinghan").match_text("招商银行").is_some());
    assert!(Query::parse("niuhai").match_text("刘海").is_some());
    assert!(Query::parse("laoshi").match_text("脑师").is_some());
    assert!(Query::parse("suiwu").match_text("税务发票").is_some());
    assert!(Query::parse("cenben").match_text("成本核算").is_some());

    // Exact pinyin outranks fuzzy pinyin
    let exact = Query::parse("zhaoshang").match_text("招商银行").unwrap();
    let fuzzy = Query::parse("zaosang").match_text("招商银行").unwrap();
    assert!(exact.score > fuzzy.score);

    // Traditional <-> Simplified bidirectional matching
    assert!(Query::parse("银行").match_text("招商銀行").is_some());
    assert!(Query::parse("銀行").match_text("招商银行").is_some());
    assert!(Query::parse("账户").match_text("台灣帳戶").is_some());
    assert!(Query::parse("zsyh").match_text("招商銀行").is_some());
    assert!(Query::parse("taiwan").match_text("臺灣").is_some());
}

#[test]
fn multi_token_cross_field_normalization_and_typo_tolerance() {
    // Multi-token cross-field AND
    let fields = [
        WeightedField {
            name: "title",
            weight: 100,
            text: "GitHub - 个人账号",
        },
        WeightedField {
            name: "username",
            weight: 85,
            text: "alice@example.com",
        },
        WeightedField {
            name: "notes",
            weight: 50,
            text: "主力代码托管和财务脚本",
        },
    ];
    let hit = Query::parse("github zh alice")
        .match_fields(&fields)
        .unwrap();
    assert!(hit.score > 0);
    assert!(!hit.field_ranges[0].is_empty());
    assert!(!hit.field_ranges[1].is_empty());

    // Non-primary match reports matched_field
    let note_hit = Query::parse("caiwu").match_fields(&fields).unwrap();
    assert_eq!(note_hit.matched_field, Some("notes"));

    // Separator-insensitive phone number / URL / fullwidth / diacritics / Arabic
    assert!(
        Query::parse("13800138000")
            .match_text("138-0013-8000")
            .is_some()
    );
    assert!(Query::parse("alipay").match_text("Ａｌｉｐａｙ").is_some());
    assert!(Query::parse("cafe").match_text("Café").is_some());
    assert!(Query::parse("احمد").match_text("أحمد").is_some());
    assert!(Query::parse("gh").match_text("GitHub").is_some());

    // 1-char typo tolerance (ASCII and Han pinyin)
    assert!(Query::parse("gihub").match_text("GitHub").is_some());
    assert!(Query::parse("alipy").match_text("Alipay").is_some());
    assert!(Query::parse("zhaoshnag").match_text("招商银行").is_some());

    // Typo tolerance steps over non-ASCII letters instead of stalling on them.
    assert!(Query::parse("wrld").match_text("Привет world").is_some());
    assert!(
        Query::parse("helo")
            .match_text("こんにちは hello")
            .is_some()
    );
    assert!(Query::parse("vodka").match_text("Водка").is_none());
}

#[test]
fn relevance_ranking_orders_best_match_first() {
    let q = Query::parse("git");
    let title_prefix = q.match_text("GitHub").unwrap().score;
    let mid_word = q.match_text("digital bank").unwrap().score;
    assert!(
        title_prefix > mid_word,
        "prefix {title_prefix} must outrank mid-word {mid_word}"
    );
}

#[test]
fn snippet_windowing_and_autofill_search_keys() {
    let long_msg = "这是一条非常非常长的测试聊天记录，前面有很多背景说明和上下文内容，真正关键的招商银行账单核对信息出现在句子的中后段位置，后面还有一些补充细节。";
    let hit = Query::parse("zsyh").match_text(long_msg).unwrap();
    let snippet = snippet_spans(long_msg, &hit.ranges, 40);
    assert!(snippet.iter().any(|s| s.matched && s.text == "招商银行"));
    let joined: String = snippet.iter().map(|s| s.text.as_str()).collect();
    assert!(joined.contains('…'));
    assert!(joined.chars().count() < long_msg.chars().count());
    assert_eq!(snippet_spans("ABCDEFGHIJ", &[], 3)[0].text, "ABC…");
    assert!(
        Query::parse("表格")
            .match_text(mime_aliases("text/csv"))
            .is_some()
    );

    let keys = search_keys(&["招商銀行", "alice@bank.com"]);
    assert!(keys.iter().any(|k| k == "zhaoshangyinhang"));
    assert!(keys.iter().any(|k| k == "zsyh"));
    assert!(keys.iter().any(|k| k.contains("招商银行")));
}

#[tokio::test]
async fn vault_cache_v01_isolation_pinyin_and_sec01_sqlite_encryption() {
    use crate::testing::{TempDir, unlocked};
    use crate::vault::{Changes, FieldChange, Filter, reading};

    let dir = TempDir::new();
    let mut c = unlocked(&dir.0, false, None);
    let created = c
        .vault_create(
            "login",
            &Changes {
                title: Some("招商银行个人网银".into()),
                tags: Some(vec!["金融".into(), "bank".into()]),
                notes: Some("这是私密备注内容含报销指引".into()),
                set: Some(vec![
                    FieldChange {
                        id: Some("username".into()),
                        value: Some("zhangsan@example.com".into()),
                        ..Default::default()
                    },
                    FieldChange {
                        id: Some("password".into()),
                        label: Some("Hardware passphrase".into()),
                        value: Some("超绝密口令SecretPass999".into()),
                        ..Default::default()
                    },
                    FieldChange {
                        id: Some("otp".into()),
                        kind: Some("otp".into()),
                        value: Some("otpauth://totp/Test?secret=JBSWY3DPEHPK3PXP".into()),
                        ..Default::default()
                    },
                ]),
                ..Default::default()
            },
        )
        .unwrap();

    // Pinyin initials hit title with accurate highlight spans
    let page = reading::browse(
        &c.hold(),
        &reading::BrowseQuery {
            filter: Filter {
                trash: false,
                query: Some("zsyh".into()),
            },
            kind: None,
            favorites: false,
            tag: None,
            sort: "relevance".into(),
            offset: 0,
            limit: 50,
        },
    )
    .await
    .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id, created.id);
    assert!(
        page.items[0]
            .title_spans
            .iter()
            .any(|s| s.matched && s.text == "招商银行")
    );

    // Fuzzy pinyin + Traditional Chinese hit
    let fuzzy = reading::browse(
        &c.hold(),
        &reading::BrowseQuery {
            filter: Filter {
                trash: false,
                query: Some("zaosang 銀行".into()),
            },
            kind: None,
            favorites: false,
            tag: None,
            sort: "relevance".into(),
            offset: 0,
            limit: 50,
        },
    )
    .await
    .unwrap();
    assert_eq!(fuzzy.items.len(), 1);

    // Notes match succeeds WITHOUT leaking note plaintext into Summary (V01 / DESIGN.md)
    let note_hit = reading::browse(
        &c.hold(),
        &reading::BrowseQuery {
            filter: Filter {
                trash: false,
                query: Some("baoxiao".into()),
            },
            kind: None,
            favorites: false,
            tag: None,
            sort: "relevance".into(),
            offset: 0,
            limit: 50,
        },
    )
    .await
    .unwrap();
    assert_eq!(note_hit.items.len(), 1);
    assert_eq!(note_hit.items[0].match_field.as_deref(), Some("notes"));
    assert!(note_hit.items[0].match_value.is_none());
    assert_eq!(
        global_search(
            &c.hold(),
            &GlobalSearchRequest {
                query: "hardware".into(),
                scopes: vec!["vault".into()],
                limit: None,
            }
        )
        .await
        .unwrap()
        .vault
        .len(),
        1
    );
    let json = serde_json::to_string(&note_hit).unwrap();
    assert!(!json.contains("报销指引"));
    assert!(!json.contains("超绝密口令"));
    assert!(!json.contains("SecretPass999"));
    assert!(!json.contains("JBSWY3DPEHPK3PXP"));

    // V01: concealed password and OTP secret NEVER match (neither plaintext nor pinyin)
    for forbidden in ["SecretPass999", "超绝密口令", "cjmk", "JBSWY3DPEHPK3PXP"] {
        let empty = reading::browse(
            &c.hold(),
            &reading::BrowseQuery {
                filter: Filter {
                    trash: false,
                    query: Some(forbidden.into()),
                },
                kind: None,
                favorites: false,
                tag: None,
                sort: "relevance".into(),
                offset: 0,
                limit: 50,
            },
        )
        .await
        .unwrap();
        assert!(empty.items.is_empty(), "`{forbidden}` must never match");
    }

    // Incremental cache reflects edit immediately
    c.vault_edit(
        &created.id,
        None,
        &Changes {
            title: Some("工商银行企业网银".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let edited = reading::browse(
        &c.hold(),
        &reading::BrowseQuery {
            filter: Filter {
                trash: false,
                query: Some("gsyh".into()),
            },
            kind: None,
            favorites: false,
            tag: None,
            sort: "relevance".into(),
            offset: 0,
            limit: 50,
        },
    )
    .await
    .unwrap();
    assert_eq!(edited.items.len(), 1);
    assert_eq!(edited.items[0].title, "工商银行企业网银");

    // Autofill catalog includes pinyin search_keys and never includes password in search_keys
    let catalog = c.autofill_catalog("", false).unwrap();
    assert_eq!(catalog.entries.len(), 1);
    assert!(
        catalog.entries[0]
            .search_keys
            .iter()
            .any(|k| k.contains("gsyh"))
    );
    assert!(
        !catalog.entries[0]
            .search_keys
            .iter()
            .any(|k| k.contains("secretpass"))
    );

    // Global search returns vault hits and respects lock (SEC01)
    let global = global_search(
        &c.hold(),
        &GlobalSearchRequest {
            query: "gsyh".into(),
            scopes: vec![],
            limit: Some(10),
        },
    )
    .await
    .unwrap();
    assert_eq!(global.vault.len(), 1);

    // SEC01: verify raw SQLite database file contains NO plaintext title or pinyin
    let db_path = dir
        .0
        .join("accounts")
        .join(c.account().unwrap().id.to_string())
        .join("workspace.sqlite");
    let raw_bytes = std::fs::read(&db_path).unwrap();
    let raw_lossy = String::from_utf8_lossy(&raw_bytes);
    for secret_word in [
        "工商银行",
        "招商银行",
        "gsyh",
        "zsyh",
        "报销指引",
        "SecretPass999",
    ] {
        assert!(
            !raw_lossy.contains(secret_word),
            "SQLite on disk must never contain plaintext `{secret_word}`"
        );
    }

    c.lock();
    assert!(
        global_search(
            &c.hold(),
            &GlobalSearchRequest {
                query: "gsyh".into(),
                scopes: vec![],
                limit: Some(10),
            },
        )
        .await
        .is_err()
    );
}

#[test]
fn files_search_listing_supports_pinyin_paths_folders_tags_and_trash() {
    use crate::library::SearchFilter;
    use crate::testing::{TempDir, unlocked};

    let dir = TempDir::new();
    let mut c = unlocked(&dir.0, false, None);
    let tax = c.files_mkdir("root", "税务资料").unwrap();
    let src = dir.0.join("2026差旅发票.txt");
    std::fs::write(&src, b"invoice bytes").unwrap();
    let imported = c
        .files_import(&src, &tax.id, false, &mut |_, _| {})
        .unwrap();
    c.files_update(
        &imported.file.id,
        Some(vec!["财务".into(), "Q1".into()]),
        Some("一季度机票与酒店报销".into()),
        Some(true),
    )
    .unwrap();

    // Cross-field pinyin search: folder path (`shuiwu`) + filename (`fp`)
    let listing = c
        .files_search_listing("shuiwu fp", &SearchFilter::default(), false)
        .unwrap();
    assert_eq!(listing.files.len(), 1);
    assert_eq!(listing.files[0].file.id, imported.file.id);
    assert!(
        listing.files[0]
            .name_spans
            .iter()
            .any(|s| s.matched && s.text == "发票")
    );

    // Folder matches by pinyin initials (`swzl`)
    let folder_hit = c
        .files_search_listing("swzl", &SearchFilter::default(), false)
        .unwrap();
    assert_eq!(folder_hit.folders.len(), 1);
    assert_eq!(folder_hit.folders[0].folder.id, tax.id);

    // Trash search works after moving to trash
    c.files_trash(std::slice::from_ref(&imported.file.id))
        .unwrap();
    let live = c
        .files_search_listing("fapiao", &SearchFilter::default(), false)
        .unwrap();
    assert!(live.files.is_empty());
    let trashed = c
        .files_search_listing("fapiao", &SearchFilter::default(), true)
        .unwrap();
    assert_eq!(trashed.files.len(), 1);
}

#[tokio::test]
async fn channel_encrypted_buckets_bidirectional_window_and_pinyin_search() {
    use crate::channel::{INBOX, Message};
    use crate::testing::{TempDir, unlocked};

    let dir = TempDir::new();
    let mut c = unlocked(&dir.0, false, None);
    let me = c.account().unwrap().id.to_string();
    let channel = "0".repeat(64);
    crate::testing::joined_channel(&mut c, &channel);
    let keys = c.keys().unwrap();

    for seq in 1..=10u64 {
        let text = match seq {
            1 => "普通".to_string(),
            5 => "请今天下班前核对招商银行的差旅报销单".to_string(),
            _ => format!("普通日常讨论消息 #{seq}"),
        };
        let msg = Message {
            seq,
            account: me.clone(),
            device: "0".repeat(64),
            message_id: format!("{seq:064x}"),
            epoch: 1,
            created_at: seq * 1000,
            stored_at: seq * 1000,
            text,
            digest: format!("{seq:064x}"),
            ..Message::default()
        };
        c.workspace
            .put_private(keys, &format!("{INBOX}{channel}:{seq:020}"), &msg)
            .unwrap();
    }

    // Pinyin initials + fuzzy pinyin + multi-token across message body
    let page = c
        .channel_search_rich(&channel, "zsyh baoxiao", None, 20, false, None)
        .unwrap();
    assert_eq!(page.hits.len(), 1);
    assert_eq!(page.hits[0].message.seq, 5);
    assert!(
        page.hits[0]
            .snippet_spans
            .iter()
            .any(|s| s.matched && s.text == "招商银行")
    );

    // Verify LDK-sealed bucket was created and NO plaintext leaked to SQLite
    let bucket_keys = c
        .workspace
        .store
        .sealed_keys(&format!("search-msg:{channel}:"))
        .unwrap();
    assert_eq!(bucket_keys.len(), 1);
    let db_path = dir.0.join("accounts").join(&me).join("workspace.sqlite");
    let raw = String::from_utf8_lossy(&std::fs::read(&db_path).unwrap()).into_owned();
    assert!(!raw.contains("招商银行"));
    assert!(!raw.contains("差旅报销单"));

    // Bidirectional context window around seq=5 returns both earlier (3,4) and newer (6,7)
    let win = c.channel_messages_around(&channel, 5, 2, 2).unwrap();
    let seqs: Vec<u64> = win.items.iter().map(|m| m.seq).collect();
    assert_eq!(seqs, vec![3, 4, 5, 6, 7]);
    assert!(win.has_before);
    assert!(win.has_after);
    assert_eq!(win.next_before, Some(3));
    assert_eq!(win.next_after, Some(7));

    // Stepping forward after seq=7 loads remaining newer messages (8, 9, 10)
    let newer = c.channel_messages_after_page(&channel, 7, 10).unwrap();
    let newer_seqs: Vec<u64> = newer.items.iter().map(|m| m.seq).collect();
    assert_eq!(newer_seqs, vec![8, 9, 10]);
    assert!(!newer.has_more);

    // Hiding seq=5 immediately removes it from search results
    let name = format!("{INBOX}{channel}:{:020}", 5);
    let rolled_back: dmsg_types::Result<()> = c.workspace.store.atomic(|_| {
        let mut message = c.workspace.private::<Message>(keys, &name)?.unwrap();
        message.hidden = true;
        c.workspace.put_private(keys, &name, &message)?;
        c.invalidate_indexed_message(&channel, message.seq)?;
        Err(dmsg_types::Error::VersionConflict)
    });
    assert!(rolled_back.is_err());
    assert_eq!(
        c.channel_search_rich(&channel, "zsyh", None, 20, false, None)
            .unwrap()
            .hits
            .len(),
        1
    );
    c.channel_hide(&channel, 5, true).unwrap();
    let hidden_search = c
        .channel_search_rich(&channel, "zsyh", None, 20, false, None)
        .unwrap();
    assert!(hidden_search.hits.is_empty());

    // Inbox search matches local encrypted draft text by pinyin and returns highlighted snippet
    let draft_id = "a".repeat(64);
    c.save_letter_draft(&draft_id, &me, "下周二上海项目复盘会议纪要", None)
        .unwrap();
    let inbox_hits = c.inbox_search("shanghai fupan", Some("draft")).unwrap();
    assert_eq!(inbox_hits.len(), 1);
    assert_eq!(inbox_hits[0].id, draft_id);
    assert!(
        inbox_hits[0]
            .snippet_spans
            .iter()
            .any(|s| s.matched && s.text.contains("上海"))
    );

    let global = global_search(
        &c.hold(),
        &GlobalSearchRequest {
            query: "普通".into(),
            scopes: vec!["messages".into()],
            limit: Some(5),
        },
    )
    .await
    .unwrap();
    assert_eq!(global.messages.len(), 5);
    assert_eq!(
        global.messages[0].message.seq, 1,
        "the oldest exact hit outranks newer prefixes"
    );
}
