//! Handing this device's cosigner keys to a successor device. The successor
//! proves once that it holds its new key, for no wallet in particular; each
//! old key here then signs the rotation digests of all its wallets in one
//! batch, and this device's wallet identity, still bound, submits them.
//! Rotation keeps each cosigner's ID, so every policy naming it follows.
use super::*;

/// How long the old key's authorization stays valid; the wallet allows ten
/// minutes, and the margin covers clocks running apart.
const ROTATION_MS: u64 = 8 * dmsg_types::MINUTE;

/// A successor device's new cosigner key, with its proof of possession.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Successor {
    /// The new key, of this account's member.
    pub input: CosignerInput,
    /// The new key's signature over its possession digest; its cosigner ID
    /// is unused, since the proof names no wallet.
    pub possession: CosignerSig,
}

impl Successor {
    /// Check that the key is a valid one of `member` that proves possession.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` for another member's key, and
    /// `Error::IntegrityFailed` for an invalid key or proof.
    pub fn check(&self, member: &MemberId) -> Result<()> {
        ensure(self.input.member == *member, Error::Forbidden)?;
        validate_cosigner(&self.input)?;
        let proof = Cosigner {
            id: 0,
            input: self.input.clone(),
            counter: 0,
            added_at_ms: 0,
        };
        verify_cosign(
            &proof,
            &possession_digest(member, &self.input.public_key),
            &self.possession,
        )
        .map(|_| ())
    }
}

/// Prove possession of this device's cosigner key `label` as `member`'s,
/// for a device whose keys hand over to it. A device that has not joined its
/// account yet names the account's member itself.
///
/// # Errors
/// Returns `Error::NotFound` for an unknown label, `Error::Locked` when the
/// device locks meanwhile, and the keystore's error when the person declines.
pub async fn successor<H: Hold>(h: &H, member: MemberId, label: &str) -> Result<Successor> {
    let (input, signer, generation) = h
        .with(|c| {
            let key = c
                .wallet_cosigner_keys()?
                .into_iter()
                .find(|k| k.label == label)
                .ok_or(Error::NotFound)?;
            Ok((
                key.input(member),
                c.prepare_cosign(&key.public_key)?,
                c.session_generation(),
            ))
        })
        .await?;
    let digest = possession_digest(&input.member, &input.public_key);
    let signature = signer.sign(vec![digest]).await?;
    h.with(|c| {
        c.keys()?;
        ensure(c.session_generation() == generation, Error::Locked)
    })
    .await?;
    let signature = signature.into_iter().next().ok_or(Error::IntegrityFailed)?;
    Ok(Successor {
        input,
        possession: CosignerSig {
            cosigner: 0,
            digest_index: 0,
            signature: signature.into(),
            authenticator_data: None,
            client_data_json: None,
        },
    })
}

/// One cosigner of a wallet that a key of this device holds.
struct Held {
    wallet: Principal,
    name: String,
    cosigner: u32,
    old: serde_bytes::ByteBuf,
}

/// Rotate every cosigner that a key of this device holds, in every wallet
/// of this account, to the successor's key. Each old key asks for presence
/// once per batch of the keystore. A wallet whose cosigner already holds the
/// successor's key counts as rotated, so a retry finishes what remains.
///
/// # Errors
/// Returns `Error::Forbidden` when the successor's key is for another
/// member, `Error::IntegrityFailed` for a possession signature that does not
/// verify, and the keystore's error when the person declines. A wallet that
/// cannot be read or refuses the rotation is reported in its entry instead.
pub async fn rotate<H: Hold>(h: &H, successor: &Successor) -> Result<Json> {
    let bound = Unlocked::new(h).await?;
    let h = &bound;
    let (hub, me, local) = h
        .with(|c| {
            Ok((
                c.wallet_hub()?,
                c.wallet_member()?,
                c.wallet_cosigner_keys()?,
            ))
        })
        .await?;
    successor.check(&me)?;
    let new = &successor.input;
    let ic = h.anonymous().await?;
    let records: Vec<WalletRecord> = ic.query(hub, "list_wallets_of", (me.clone(),)).await?;
    let mut outcomes = vec![];
    let mut held = vec![];
    for record in records {
        let view = match state(h, record.wallet).await {
            Ok(view) => view,
            Err(error) => {
                outcomes.push(json!({"wallet": record.wallet.to_text(), "name": record.name, "error": error.to_string()}));
                continue;
            }
        };
        for c in view.cosigners.iter().filter(|c| c.input.member == me) {
            if c.input == *new {
                outcomes.push(json!({"wallet": view.wallet.to_text(), "name": view.name, "cosigner": c.id, "rotated": true}));
            } else if local
                .iter()
                .any(|k| k.public_key.as_slice() == c.input.public_key.as_slice())
            {
                held.push(Held {
                    wallet: view.wallet,
                    name: view.name.clone(),
                    cosigner: c.id,
                    old: c.input.public_key.clone(),
                });
            }
        }
    }
    let expires_at_ms = now_ms() + ROTATION_MS;
    let mut olds: Vec<&serde_bytes::ByteBuf> = held.iter().map(|x| &x.old).collect();
    olds.sort();
    olds.dedup();
    let mut signed = vec![];
    for old in olds {
        let batch: Vec<&Held> = held.iter().filter(|x| x.old == *old).collect();
        let digests = batch
            .iter()
            .map(|x| rotate_digest(x.wallet, x.cosigner, old, new, expires_at_ms))
            .collect();
        let signer = h.with(|c| c.prepare_cosign(old)).await?;
        let signatures = signer.sign(digests).await?;
        h.with(|c| c.keys().map(|_| ())).await?;
        ensure(signatures.len() == batch.len(), Error::IntegrityFailed)?;
        signed.extend(batch.into_iter().zip(signatures));
    }
    let (_, wallet_ic) = identity(h).await?;
    for (x, signature) in signed {
        let args = RotateCosignerArgs {
            cosigner: x.cosigner,
            input: new.clone(),
            expires_at_ms,
            authorization: CosignerSig {
                cosigner: x.cosigner,
                digest_index: 0,
                signature: signature.into(),
                authenticator_data: None,
                client_data_json: None,
            },
            possession: successor.possession.clone(),
        };
        let rotated: Result<Result<Cosigner>> =
            wallet_ic.update(x.wallet, "rotate_cosigner", (args,)).await;
        let mut outcome =
            json!({"wallet": x.wallet.to_text(), "name": x.name, "cosigner": x.cosigner});
        match rotated.and_then(|r| r) {
            Ok(_) => {
                outcome["rotated"] = json!(true);
                if let Err(error) = desktop::notify(h, x.wallet, None).await {
                    outcome["notification_error"] = json!(error.to_string());
                }
            }
            Err(error) => outcome["error"] = json!(error.to_string()),
        }
        outcomes.push(outcome);
    }
    Ok(json!({"rotations": outcomes}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TempDir, unlocked};

    struct Shared(tokio::sync::Mutex<Client>);

    impl Hold for Shared {
        async fn with<T: Send>(
            &self,
            step: impl FnOnce(&mut Client) -> Result<T> + Send,
        ) -> Result<T> {
            step(&mut *self.0.lock().await)
        }
    }

    #[tokio::test]
    async fn possession_and_rotation_signatures_verify_as_the_wallet_checks_them() {
        let dir = TempDir::new();
        let mut client = unlocked(&dir.0, false, None);
        client.wallet_cosigner_create("old", false).unwrap();
        client.wallet_cosigner_create("new", false).unwrap();
        let me = client.wallet_member().unwrap();
        let old = client.wallet_cosigner_keys().unwrap()[0].input(me.clone());
        let h = Shared(tokio::sync::Mutex::new(client));

        let successor = successor(&h, me.clone(), "new").await.unwrap();
        assert_eq!(successor.input.member, me);
        let new = Cosigner {
            id: 0,
            input: successor.input.clone(),
            counter: 0,
            added_at_ms: 0,
        };
        verify_cosign(
            &new,
            &possession_digest(&me, &successor.input.public_key),
            &successor.possession,
        )
        .unwrap();
        // Bound to the member: another member cannot use it.
        assert!(
            verify_cosign(
                &new,
                &possession_digest(
                    &MemberId::Principal(Principal::anonymous()),
                    &successor.input.public_key
                ),
                &successor.possession,
            )
            .is_err()
        );

        let wallet = Principal::from_slice(&[7; 10]);
        let digest = rotate_digest(wallet, 3, &old.public_key, &successor.input, 9);
        let signer = h.with(|c| c.prepare_cosign(&old.public_key)).await.unwrap();
        let signature = signer.sign(vec![digest]).await.unwrap().remove(0);
        let current = Cosigner {
            id: 3,
            input: old,
            counter: 0,
            added_at_ms: 0,
        };
        let authorization = CosignerSig {
            cosigner: 3,
            digest_index: 0,
            signature: signature.into(),
            authenticator_data: None,
            client_data_json: None,
        };
        verify_cosign(&current, &digest, &authorization).unwrap();
        // The new key cannot authorize its own rotation.
        assert!(verify_cosign(&new, &digest, &authorization).is_err());
    }
}
