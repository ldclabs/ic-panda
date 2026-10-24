//! Public Profile display pins survive new Runtime instances and different services.
use agent_protocols::{
    identity::{AgentSigner, unix_ms},
    profile::profile_update_event,
};
use dmsg_agent::{Host, Options, Runtime};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
};

fn service(reply: Arc<Mutex<Value>>) -> String {
    service_at(reply, None)
}

fn service_at(reply: Arc<Mutex<Value>>, profiles: Option<String>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let service_origin = origin.clone();
    std::thread::spawn(move || {
        for socket in listener.incoming() {
            let Ok(mut socket) = socket else { break };
            let mut bytes = [0; 4096];
            let n = socket.read(&mut bytes).unwrap();
            let discovery = String::from_utf8_lossy(&bytes[..n]).contains(".well-known");
            let body = if discovery {
                profiles.as_ref().map_or_else(||"{}".into(),|profiles|json!({"protocol":"agent-profile/1.0","service":service_origin,"endpoints":{"profiles":profiles}}).to_string())
            } else {
                json!({"result":[reply.lock().unwrap().clone()]}).to_string()
            };
            let status = if discovery && profiles.is_none() {
                404
            } else {
                200
            };
            write!(socket,"HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
        }
    });
    origin
}

#[tokio::test]
async fn latest_display_profile_does_not_roll_back_across_services_or_restart() {
    let signer = AgentSigner::from_seed([51; 32]);
    let id = signer.agent_id();
    let update = |nonce, name| {
        json!(
            signer
                .sign_event(profile_update_event(
                    id.clone(),
                    unix_ms(),
                    nonce,
                    serde_json::from_value(json!({"id":id,"name":name})).unwrap()
                ))
                .unwrap()
        )
    };
    let (old, new, conflict) = (update(1, "Old"), update(2, "New"), update(2, "Conflict"));
    let a = Arc::new(Mutex::new(old.clone()));
    let b = Arc::new(Mutex::new(new.clone()));
    let origins = [service(a.clone()), service(b.clone())];
    let options = Options {
        local: true,
        hosts: origins
            .iter()
            .map(|origin| Host {
                host: origin.clone(),
                profile_service: Some(origin.clone()),
                label: None,
            })
            .collect(),
        ..Options::default()
    };
    let dir = std::env::temp_dir().join(format!(
        "dmsg-profile-pin-{}-{}",
        std::process::id(),
        unix_ms()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("agents.sqlite");
    let runtime = Runtime::new(&options).unwrap();
    let result = runtime.profiles(&path, id.as_str()).await.unwrap();
    assert_eq!(result[0]["error"]["code"], "stale_profile");
    assert_eq!(result[1]["profile"]["name"], "New");
    drop(runtime);
    *a.lock().unwrap() = conflict;
    *b.lock().unwrap() = old;
    let restored = Runtime::new(&options).unwrap();
    let result = restored.profiles(&path, id.as_str()).await.unwrap();
    assert!(
        result
            .iter()
            .all(|entry| entry["error"]["code"] == "stale_profile")
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn discovery_cannot_select_an_unconfigured_profile_origin() {
    let signer = AgentSigner::from_seed([52; 32]);
    let id = signer.agent_id();
    let envelope = json!(
        signer
            .sign_event(profile_update_event(
                id.clone(),
                unix_ms(),
                1,
                serde_json::from_value(json!({"id":id,"name":"Other origin"})).unwrap()
            ))
            .unwrap()
    );
    let reply = Arc::new(Mutex::new(envelope));
    let other = service(reply.clone());
    let origin = service_at(reply, Some(format!("{other}/v1/profiles")));
    let options = Options {
        local: true,
        hosts: vec![Host {
            host: origin.clone(),
            profile_service: Some(origin),
            label: None,
        }],
        ..Options::default()
    };
    let dir = std::env::temp_dir().join(format!(
        "dmsg-profile-origin-{}-{}",
        std::process::id(),
        unix_ms()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let result = Runtime::new(&options)
        .unwrap()
        .profiles(&dir.join("agents.sqlite"), id.as_str())
        .await
        .unwrap();
    assert_eq!(result[0]["error"]["code"], "permission_denied");
    std::fs::remove_dir_all(dir).unwrap();
}
