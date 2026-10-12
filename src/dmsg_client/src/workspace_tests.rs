use super::*;
use dmsg_types::AccountId;
use std::str::FromStr;

fn account() -> AccountRef {
    AccountRef {
        id: AccountId::from_str("040g2081040g2081040g").unwrap(),
        home: Principal::from_text("aaaaa-aa").unwrap(),
        issuer: "https://dmsg.test/u/040g2081040g2081040g".into(),
    }
}

#[test]
fn provisional_workspace_binds_to_the_unlock_secret() {
    let mut ws = Workspace::load(Store::memory().unwrap(), &Environment::Local, 1).unwrap();
    assert!(ws.provisional());
    assert_eq!(ws.name(), Err(Error::RecoveryIncomplete));
    let keys = ws.unlock_provisional().unwrap();
    assert_eq!(signing_public(&keys.signing), *ws.meta.signing_pub);
    ws.put_private(&keys, "note", &"before binding").unwrap();

    let secret = [9u8; 32];
    ws.bind(&keys, account(), &secret).unwrap();
    assert_eq!(
        ws.name().unwrap(),
        format!(
            "dmsg:local:040g2081040g2081040g:{}",
            hex::encode(*ws.meta.device_id)
        )
    );
    assert_eq!(ws.unlock_provisional().err(), Some(Error::Forbidden));
    assert!(ws
        .store
        .meta::<ByteArray<32>>(PROVISIONAL)
        .unwrap()
        .is_none());
    assert!(matches!(ws.unlock(&[8; 32]), Err(Error::IntegrityFailed)));

    let again = ws.unlock(&secret).unwrap();
    assert_eq!(*again.hpke, *keys.hpke);
    assert_eq!(
        ws.private::<String>(&again, "note").unwrap().as_deref(),
        Some("before binding")
    );
    // The LUK follows the public formula, so the wrapped LDK opens with it.
    let luk = crate::crypto::unlock_key(&secret, &ws.name().unwrap());
    let ldk = open(
        &luk,
        &ws.store.sealed(LDK).unwrap().unwrap(),
        &("dmsg/local-key/2", ws.name().unwrap()),
    )
    .unwrap();
    assert_eq!(ldk.as_slice(), again.ldk.as_slice());
}

#[test]
fn reopening_keeps_the_device() {
    let store = Store::memory().unwrap();
    let ws = Workspace::load(store, &Environment::Local, 1).unwrap();
    let meta = ws.meta.clone();
    let reopened = Workspace::load(ws.store, &Environment::Local, 2).unwrap();
    assert_eq!(reopened.meta, meta);
    assert!(Workspace::load(reopened.store, &Environment::Staging, 3).is_err());
}

#[test]
fn administrators_manage_the_root() {
    let ws = Workspace::load(Store::memory().unwrap(), &Environment::Local, 1).unwrap();
    let admin = ws.device_input(ControllerRole::Administrator);
    assert_eq!(
        admin.capabilities,
        vec![
            Capability::ContentSign,
            Capability::VaultUnlock,
            Capability::RootManage
        ]
    );
    let member = ws.device_input(ControllerRole::Member);
    assert_eq!(
        member.capabilities,
        vec![Capability::ContentSign, Capability::VaultUnlock]
    );
}
