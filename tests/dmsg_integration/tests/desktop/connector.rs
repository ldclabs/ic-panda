//! MCP 0.12.3 through the real daemon, Profile service and Mail Worker.
use super::*;

#[test]
#[ignore = "needs the dmsg binary, PocketIC and local dmsg-cloud Profile/Mail Workers"]
fn desktop_connector_profile_and_mail() {
    let mut env = Env::launch(
        "connector-workflows",
        Options {
            agents: true,
            services: true,
            ..Options::default()
        },
    );
    let http = env.http.clone();
    let a = &env.a;
    let profile_service = env.profiles.as_ref().unwrap().origin.clone();
    let services = env.services.as_ref().unwrap();
    let mail = services.mail.clone();
    assert_eq!(a.login(&http, 81).unwrap()["state"], "no_account");
    a.run(&["account", "create"]);
    for command in [
        vec!["device", "formal-approve", "on"],
        vec!["agent", "principal", "enable"],
    ] {
        let request = a.run(&command);
        assert_eq!(a.decide(&http, &request, true).unwrap()["state"], "done");
    }
    let request = a.run(&["agent", "controller", "register", "--name", "MCP test"]);
    assert_eq!(a.decide(&http, &request, true).unwrap()["state"], "done");
    let alice = a.run(&["agent", "id", "create", "alice"])["agent_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let bob = a.run(&["agent", "id", "create", "bob"])["agent_id"]
        .as_str()
        .unwrap()
        .to_owned();
    // The real Profile service requires an active delegation hint for this agent.
    let request = a.run(&[
        "agent",
        "grant",
        "alice",
        "--scope",
        "message.draft",
        "--audience",
        "https://dmsg.net",
        "--days",
        "7",
    ]);
    assert_eq!(a.decide(&http, &request, true).unwrap()["state"], "done");
    let credentials = a.run(&["agent", "credentials"]);
    let credential = credentials
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["subject"] == alice)
        .unwrap();
    let hint = json!({"id":credential["id"],"principal":{"id":credential["principal_id"]},"scopes":credential["scopes"]});
    let token = a.run(&["agent", "id", "credential", "alice"])["credential"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut mcp = McpClient::start(a, &token);
    let catalog = mcp.request("tools/list", json!({}));
    let tools = catalog["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 75);
    let standard: Vec<_> = tools
        .iter()
        .filter(|t| t["name"].as_str().unwrap().starts_with("agent_protocols_"))
        .collect();
    assert_eq!(standard.len(), 41);
    assert!(
        standard
            .iter()
            .all(|t| t["inputSchema"].is_object() && t["outputSchema"]["properties"].is_object())
    );
    let update = standard
        .iter()
        .find(|t| t["name"] == "agent_protocols_profile_update")
        .unwrap();
    assert_eq!(
        update["inputSchema"]["properties"]["profile"]["required"],
        json!(["name"])
    );
    let identity = mcp
        .tool("agent_protocols_identity_current", json!({}))
        .unwrap();
    assert_eq!(identity["profiles"], json!([profile_service]));
    assert_eq!(identity["sync_mode"], "explicit");
    assert_eq!(identity["tools"].as_array().unwrap().len(), 41);
    let discovered = mcp
        .tool(
            "agent_protocols_services_list",
            json!({"host":mail,"protocol":"agent-mail/1.0"}),
        )
        .unwrap();
    assert_eq!(discovered["services"][0]["service"], mail);
    // A public bundled schema needs no room grant; a room lookup cannot bypass it.
    assert!(
        !mcp.tool(
            "agent_protocols_schema_get",
            json!({"protocol":"agent-profile/1.0","type":"profile.update"})
        )
        .unwrap()["schemas"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(mcp.tool("agent_protocols_schema_get",json!({"protocol":"agent-discourse/1.0","room_id":"room1","host":env.discourse.as_ref().unwrap().origin})).unwrap_err()["code"],"FORBIDDEN");
    let profile = json!({"name":"Alice before","description":"MCP profile workflow","capabilities":["review"],"links":[{"name":"Home","url":"https://example.com/alice","rel":"homepage"}],"delegations":[hint]});
    for (tool, input) in [
        (
            "agent_protocols_profiles_get",
            json!({"profile_service":profile_service}),
        ),
        (
            "agent_protocols_profile_history",
            json!({"profile_service":profile_service}),
        ),
        (
            "agent_protocols_profile_update",
            json!({"profile_service":profile_service,"profile":profile}),
        ),
        (
            "agent_protocols_mail_mailbox_publish",
            json!({"routes":[mail]}),
        ),
        (
            "agent_protocols_mail_resolve",
            json!({"agent_id":bob,"profile_service":profile_service}),
        ),
        ("agent_protocols_room_watch", json!({"room_id":"room1"})),
        ("agent_protocols_room_archive", json!({"room_id":"room1"})),
    ] {
        assert_eq!(
            mcp.tool(tool, input).unwrap_err()["code"],
            "FORBIDDEN",
            "{tool}"
        );
    }
    let profile_grant = a.run(&[
        "grants",
        "add",
        "alice",
        &format!("agent.profile.read:{profile_service}"),
        &format!("agent.profile.write:{profile_service}"),
    ]);
    assert_eq!(
        mcp.tool(
            "agent_protocols_profiles_get",
            json!({"profile_service":profile_service})
        )
        .unwrap()["missing"],
        json!([alice])
    );
    mcp.tool(
        "agent_protocols_profile_update",
        json!({"profile_service":profile_service,"profile":profile}),
    )
    .unwrap();
    let current = mcp
        .tool(
            "agent_protocols_profiles_get",
            json!({"profile_service":profile_service}),
        )
        .unwrap()["profiles"][0]
        .clone();
    assert_eq!(current["name"], "Alice before");
    let mut replacement = current.clone();
    replacement.as_object_mut().unwrap().remove("updated_at");
    replacement.as_object_mut().unwrap().remove("event_id");
    replacement["name"] = json!("Alice after");
    mcp.tool(
        "agent_protocols_profile_update",
        json!({"profile_service":profile_service,"profile":replacement}),
    )
    .unwrap();
    let updated = mcp
        .tool(
            "agent_protocols_profiles_get",
            json!({"profile_service":profile_service}),
        )
        .unwrap()["profiles"][0]
        .clone();
    assert_eq!(updated["name"], "Alice after");
    assert_eq!(updated["links"], current["links"]);
    assert_eq!(updated["capabilities"], current["capabilities"]);
    assert_eq!(updated["delegations"], current["delegations"]);
    let history = mcp
        .tool(
            "agent_protocols_profile_history",
            json!({"profile_service":profile_service}),
        )
        .unwrap();
    assert_eq!(history["envelopes"].as_array().unwrap().len(), 2);
    // A Profile grant does not grant access to another configured service origin.
    assert_eq!(
        mcp.tool(
            "agent_protocols_profiles_get",
            json!({"profile_service":mail})
        )
        .unwrap_err()["code"],
        "permission_denied"
    );
    a.run(&["grants", "revoke", profile_grant["id"].as_str().unwrap()]);
    assert_eq!(
        mcp.tool(
            "agent_protocols_profiles_get",
            json!({"profile_service":profile_service})
        )
        .unwrap_err()["code"],
        "FORBIDDEN"
    );
    // Publishing a mailbox requires only the relay origin; keys stay inside dMsg.
    let mail_grant = a.run(&[
        "grants",
        "add",
        "alice",
        &format!("mail.read:{mail}"),
        &format!("mail.write:{mail}"),
    ]);
    let alice_box = mcp
        .tool(
            "agent_protocols_mail_mailbox_publish",
            json!({"routes":[mail]}),
        )
        .unwrap()["mailbox"]
        .clone();
    let bob_box = ap(a, "bob", "mail_mailbox_publish", json!({"routes":[mail]}))["mailbox"].clone();
    let resolved = mcp
        .tool(
            "agent_protocols_mail_resolve",
            json!({"agent_id":bob,"address":bob_box["address"]}),
        )
        .unwrap();
    assert_eq!(resolved["mailboxes"][0]["usable"], true);
    // mail_resolve's Profile path needs a separately granted origin, too.
    assert_eq!(
        mcp.tool(
            "agent_protocols_mail_resolve",
            json!({"agent_id":alice,"profile_service":profile_service})
        )
        .unwrap_err()["code"],
        "permission_denied"
    );
    a.run(&[
        "grants",
        "add",
        "alice",
        &format!("mail.read:{profile_service}"),
    ]);
    assert_eq!(
        mcp.tool(
            "agent_protocols_mail_resolve",
            json!({"agent_id":alice,"profile_service":profile_service})
        )
        .unwrap()["mailboxes"],
        json!([])
    );
    let sent=mcp.tool("agent_protocols_mail_send",json!({"to":resolved["mailboxes"][0]["address"],"subject":"MCP integration","text":"Please review the profile.","reply_mailbox_id":alice_box["mailbox_id"]})).unwrap();
    assert_eq!(sent["status"], "delivered");
    assert!(
        ap(a, "bob", "inbox_next", json!({"kinds":["mail.received"]}))["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let sync = ap(a, "bob", "mail_sync", json!({}));
    assert_eq!(sync["errors"], json!([]));
    assert!(sync["synced_at"].as_u64().unwrap() > 0);
    assert_eq!(
        sync["received"][0]["parts"][0]["text"],
        "Please review the profile."
    );
    let incoming = ap(a, "bob", "inbox_next", json!({"kinds":["mail.received"]}));
    assert_eq!(incoming["items"][0]["actor"], alice);
    let read = ap(
        a,
        "bob",
        "mail_list",
        json!({"unread_only":true,"mark_read":true}),
    );
    assert_eq!(read["unread_count"], 0);
    assert_eq!(read["messages"][0]["reply_address"], alice_box["address"]);
    let reply = ap(
        a,
        "bob",
        "mail_send",
        json!({"in_reply_to":sent["message"]["message_id"],"text":"Reviewed."}),
    );
    assert_eq!(reply["status"], "delivered");
    let answer = mcp.tool("agent_protocols_mail_sync", json!({})).unwrap();
    assert_eq!(answer["received"][0]["from"], bob);
    assert_eq!(
        answer["received"][0]["in_reply_to"],
        sent["message"]["message_id"]
    );
    assert_eq!(
        answer["received"][0]["thread_id"],
        sent["message"]["thread_id"]
    );
    let read = mcp
        .tool(
            "agent_protocols_mail_list",
            json!({"unread_only":true,"mark_read":true}),
        )
        .unwrap();
    assert_eq!(read["messages"][0]["parts"][0]["text"], "Reviewed.");
    a.run(&["grants", "revoke", mail_grant["id"].as_str().unwrap()]);
    assert_eq!(
        mcp.tool(
            "agent_protocols_mail_send",
            json!({"to":bob_box["address"],"text":"must not send"})
        )
        .unwrap_err()["code"],
        "FORBIDDEN"
    );
    // The remaining mail.read grant on the Profile origin is for resolution,
    // not a grant to read stored letters from the ungranted relay.
    assert_eq!(
        mcp.tool("agent_protocols_mail_list", json!({}))
            .unwrap_err()["code"],
        "FORBIDDEN"
    );
    drop(mcp);
    env.restart(0);
    let a = &env.a;
    assert_eq!(a.login(&http, 81).unwrap()["state"], "unlocked");
    assert_eq!(
        ap(a, "alice", "mail_mailboxes_list", json!({}))["mailboxes"][0]["mailbox_id"],
        alice_box["mailbox_id"]
    );
    assert_eq!(
        ap(
            a,
            "alice",
            "profiles_get",
            json!({"profile_service":profile_service})
        )["profiles"][0]["name"],
        "Alice after"
    );
}
