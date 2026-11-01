//! Owner display reads shared by desktop and mobile. These never authorize actions.
use crate::{
    cache::Kind,
    display,
    hold::{Hold, Unlocked},
    now_ms,
};
use dmsg_types::{Error, Result, ensure};
use serde_json::{Value, json};

/// Resolve a closed display domain, never an arbitrary RPC or storage prefix.
pub fn kind(name: &str) -> Result<Kind> {
    crate::cache::KINDS
        .iter()
        .copied()
        .find(|k| k.name() == name && *k != Kind::Asset)
        .ok_or_else(|| dmsg_types::invalid("display kind"))
}

/// Local snapshot and freshness. No network operation is performed.
pub fn read(c: &crate::Client, kind: Kind, id: &str) -> Result<Value> {
    let saved = c.cache_read(kind, id)?;
    let stale = saved
        .as_ref()
        .is_none_or(|s| display::stale(s.observed_at_ms, kind.ttl(), now_ms()));
    Ok(match saved {
        Some(s) => json!({"state":if s.data.is_null() {"absent"} else {"present"},
            "data":s.data,"observed_at_ms":s.observed_at_ms,"stale":stale,"refreshing":false,"error":null}),
        None => {
            json!({"state":"missing","data":null,"observed_at_ms":null,"stale":true,"refreshing":false,"error":null})
        }
    })
}

/// Fetch one observation outside Client ownership, then commit in the same scope.
/// Hosts coalesce requests and own visibility, cancellation and retry timing.
pub async fn refresh<H: Hold>(h: &H, kind: Kind, id: &str) -> Result<()> {
    use crate::{
        commerce, handle, inbox,
        library::{cloud, storage},
        principal, security, wallet,
    };
    let unlocked = Unlocked::new(h).await?;
    let session = unlocked.with(|c| c.display_session()).await?;
    let h = &display::Scoped {
        inner: &unlocked,
        session,
    };
    let scope = h.with(|c| c.cache_scope(kind)).await?;
    let wallet_id = || candid::Principal::from_text(id).map_err(|_| dmsg_types::invalid("wallet"));
    let data = match kind {
        Kind::Profile => {
            let document = principal::show(h).await?;
            h.with(|c| {
                c.keep_profile_display(display::Profile::from_document(
                    (document.updated_at > 0).then_some(&document),
                    now_ms(),
                ))?;
                Ok(())
            })
            .await?;
            serde_json::to_value(document)
        }
        Kind::ProfileEditor => serde_json::to_value(principal::editable(h).await?),
        Kind::Wallets => Ok(wallet::list(h).await?),
        Kind::Plan => Ok(commerce::plan_status(h, false).await?),
        Kind::Catalog => Ok(commerce::catalog_display(h).await?),
        Kind::Assets => Ok(commerce::plan_assets(h).await?),
        Kind::Panda => Ok(commerce::plan_panda(h).await?),
        Kind::Handles => Ok(Value::Array(handle::mine(h).await?)),
        Kind::Account => Ok(security::account_security(h).await?),
        Kind::Summary => {
            let state = h
                .warm(crate::client::Warm::Home)
                .await?
                .ok_or(Error::PolicyStale)?;
            h.with(|c| Ok(serde_json::to_value(c.account_view_of(&state)?)))
                .await?
        }
        Kind::Recovery => Ok(crate::recovery::status(h).await?),
        Kind::Wallet => Ok(wallet::show(h, wallet_id()?).await?),
        Kind::Balances => Ok(wallet::balances(h, wallet_id()?).await?),
        Kind::History => Ok(wallet::desktop::history(h, wallet_id()?, false).await?),
        Kind::Inbox => Ok(inbox::policy(h).await?.unwrap_or(Value::Null)),
        #[allow(deprecated)]
        Kind::Storage => Ok(storage::refresh(h).await?),
        Kind::Usage => serde_json::to_value(cloud::cloud_usage(h).await?),
        Kind::Asset => return Err(Error::Forbidden),
    }
    .map_err(|_| Error::IntegrityFailed)?;
    h.with(|c| {
        ensure(
            !(kind == Kind::History && data["persisted"] == false),
            Error::Unavailable("storage: display snapshot was not persisted".into()),
        )?;
        if !matches!(kind, Kind::History | Kind::Inbox | Kind::Storage) {
            if matches!(kind, Kind::Profile | Kind::ProfileEditor)
                && c.cache_read(kind, id)?.is_some_and(|old| {
                    old.data["updated_at"].as_u64() > data["updated_at"].as_u64()
                })
            {
                return Err(Error::VersionConflict);
            }
            if kind == Kind::Profile
                && c.profile_display()?
                    .and_then(|p| p.updated_at)
                    .is_some_and(|version| Some(version) > data["updated_at"].as_u64())
            {
                return Err(Error::VersionConflict);
            }
            c.cache_save_checked(&scope, kind, id, &data, now_ms())?;
        }
        Ok(())
    })
    .await
}

/// Best-effort avatar update; an unavailable image does not discard a valid document.
pub async fn refresh_avatar<H: Hold>(h: &H) {
    let Ok(session) = h.with(|c| c.display_session()).await else {
        return;
    };
    let scoped = display::Scoped { inner: h, session };
    let h = &scoped;
    let Ok(Some(profile)) = h.with(|c| c.profile_display()).await else {
        return;
    };
    let Some(url) = profile.avatar_url.clone() else {
        return;
    };
    let Ok(due) = h
        .with(|c| {
            Ok(c.profile_avatar(&url)?
                .is_none_or(|a| display::stale(a.fetched_at_ms, display::AVATAR_TTL, now_ms())))
        })
        .await
    else {
        return;
    };
    if !due {
        return;
    }
    if let Ok(Some((mime, bytes))) = crate::principal::avatar(h, &url).await {
        let _ = h
            .with(|c| {
                if c.profile_display()?.is_some_and(|p| {
                    p.updated_at == profile.updated_at && p.avatar_url.as_ref() == Some(&url)
                }) {
                    c.keep_profile_avatar(display::Avatar {
                        url,
                        mime: mime.into(),
                        bytes,
                        fetched_at_ms: now_ms(),
                    })?;
                }
                Ok(())
            })
            .await;
    }
}
