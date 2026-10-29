use super::*;
use dmsg_client::wallet;

#[derive(uniffi::Record)]
pub struct WalletTransfer {
    pub wallet: String,
    pub from: u32,
    pub asset: String,
    pub recipient: String,
    pub amount: String,
    pub max_fee: String,
    pub memo: Option<String>,
    pub request_id: String,
}

#[uniffi::export]
impl MobileEngine {
    pub async fn wallets(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| Box::pin(async { wallet::list(h).await }))
            .await
    }

    pub async fn wallet_details(&self, wallet: String) -> Result<ObjectSnapshot> {
        let wallet = wallet::parse_wallet(&wallet)?;
        self.held_object(move |h| Box::pin(async move { wallet::show(h, wallet).await }))
            .await
    }

    pub async fn wallet_proposals(
        &self,
        wallet: String,
        before: Option<u64>,
    ) -> Result<ObjectSnapshot> {
        let wallet = wallet::parse_wallet(&wallet)?;
        self.held_object(move |h| {
            Box::pin(async move { wallet::proposals(h, wallet, before, 30).await })
        })
        .await
    }

    pub async fn wallet_proposal(&self, wallet: String, proposal: u64) -> Result<ObjectSnapshot> {
        let wallet = wallet::parse_wallet(&wallet)?;
        self.held_object(move |h| {
            Box::pin(async move { wallet::proposal(h, wallet, proposal).await })
        })
        .await
    }

    pub async fn wallet_proposal_attempt(&self, request_id: String) -> Result<String> {
        self.host
            .run(move |c| {
                Box::pin(async move { c.wallet_proposal_attempt(&request_id).map(str::to_string) })
            })
            .await
            .map_err(Into::into)
    }

    pub async fn pending_wallet_proposals(&self) -> Result<ObjectSnapshot> {
        self.object(|c| Box::pin(async { c.pending_wallet_proposals() }))
            .await
    }

    pub async fn resume_wallet_proposal(&self, request_id: String) -> Result<ObjectSnapshot> {
        self.held_object(move |h| {
            Box::pin(async move { wallet::ops::resume_proposal(h, &request_id).await })
        })
        .await
    }

    /// Ask to join `wallet`, binding this device and taking over the
    /// bindings of the devices in `replaces` (hex), or of every other device
    /// when `exclusive`.
    pub async fn wallet_join(
        &self,
        wallet: String,
        replaces: Vec<String>,
        exclusive: bool,
    ) -> Result<ObjectSnapshot> {
        let wallet = wallet::parse_wallet(&wallet)?;
        let replaces = devices(&replaces)?;
        self.held_object(move |h| {
            Box::pin(async move {
                wallet::ops::join_request(h, "gui", wallet, replaces, exclusive).await
            })
        })
        .await
    }

    /// Ask to remove this account's bindings of `devices` (hex), such as
    /// lost ones.
    pub async fn wallet_unbind(
        &self,
        wallet: String,
        devices: Vec<String>,
    ) -> Result<ObjectSnapshot> {
        let wallet = wallet::parse_wallet(&wallet)?;
        let devices = self::devices(&devices)?;
        self.held_object(move |h| {
            Box::pin(async move { wallet::ops::unbind_request(h, "gui", wallet, devices).await })
        })
        .await
    }

    /// This account's bindings in `wallet`, with the state of their devices
    /// and the ones an unbind can remove.
    pub async fn wallet_bindings(&self, wallet: String) -> Result<ObjectSnapshot> {
        let wallet = wallet::parse_wallet(&wallet)?;
        self.held_object(move |h| Box::pin(async move { wallet::bindings(h, wallet).await }))
            .await
    }

    pub async fn wallet_transfer(&self, transfer: WalletTransfer) -> Result<ObjectSnapshot> {
        let wallet = wallet::parse_wallet(&transfer.wallet)?;
        let id = dmsg_types::Hash::new(
            hex::decode(&transfer.request_id)
                .ok()
                .and_then(|v| v.try_into().ok())
                .ok_or_else(|| dmsg_types::invalid("request ID"))?,
        );
        self.held_object(move |h| {
            Box::pin(async move {
                wallet::ops::send_with_id(
                    h,
                    wallet,
                    transfer.from,
                    &transfer.asset,
                    &transfer.recipient,
                    &transfer.amount,
                    &transfer.max_fee,
                    transfer.memo.as_deref(),
                    None,
                    Some(id),
                )
                .await
            })
        })
        .await
    }

    pub async fn wallet_decision(
        &self,
        wallet: String,
        proposal: u64,
        approve: bool,
    ) -> Result<ObjectSnapshot> {
        let wallet = wallet::parse_wallet(&wallet)?;
        self.held_object(move |h| {
            Box::pin(async move {
                wallet::ops::decide_request(
                    h,
                    "gui",
                    wallet,
                    proposal,
                    if approve {
                        dmsg_types::wallet::Decision::Approve
                    } else {
                        dmsg_types::wallet::Decision::Reject
                    },
                )
                .await
            })
        })
        .await
    }

    pub async fn execute_wallet(
        &self,
        wallet: String,
        proposal: u64,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        let wallet = wallet::parse_wallet(&wallet)?;
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move { wallet::ops::execute(h, "gui", wallet, proposal).await })
            }))
            .await
    }

    pub async fn reconcile_wallet(
        &self,
        wallet: String,
        proposal: u64,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        let wallet = wallet::parse_wallet(&wallet)?;
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move { wallet::ops::reconcile(h, wallet, proposal).await })
            }))
            .await
    }

    pub async fn register_wallet_cosigner(
        &self,
        wallet: String,
        label: String,
        request_id: String,
    ) -> Result<ObjectSnapshot> {
        let wallet = wallet::parse_wallet(&wallet)?;
        let id = dmsg_types::Hash::new(
            hex::decode(request_id)
                .ok()
                .and_then(|v| v.try_into().ok())
                .ok_or_else(|| dmsg_types::invalid("request ID"))?,
        );
        self.held_object(move |h| {
            Box::pin(async move {
                wallet::state(h, wallet).await?;
                let input = h
                    .with(|c| {
                        let key = c
                            .wallet_cosigner_keys()?
                            .into_iter()
                            .find(|k| k.label == label)
                            .ok_or(Error::NotFound)?;
                        let account = c
                            .session_state()
                            .account
                            .ok_or(Error::AuthRequired)?
                            .parse()
                            .map_err(|_| Error::IntegrityFailed)?;
                        Ok(key.input(dmsg_types::wallet::MemberId::Account(account)))
                    })
                    .await?;
                wallet::ops::propose_with_id(
                    h,
                    wallet,
                    dmsg_types::wallet::Intent::AddCosigner(input),
                    None,
                    id,
                )
                .await
            })
        })
        .await
    }

    /// Propose giving cosigner `cosigner` of this account the key `label`
    /// of this device, which first signs a local challenge.
    pub async fn replace_wallet_cosigner(
        &self,
        wallet: String,
        cosigner: u32,
        label: String,
        request_id: String,
    ) -> Result<ObjectSnapshot> {
        let wallet = wallet::parse_wallet(&wallet)?;
        let id = dmsg_client::upload::hex_id(&request_id)?;
        self.held_object(move |h| {
            Box::pin(async move {
                let intent = wallet::replacement(h, wallet, cosigner, &label).await?;
                wallet::ops::propose_with_id(h, wallet, intent, None, id).await
            })
        })
        .await
    }

    pub async fn wallet_cosigners(&self) -> Result<ObjectSnapshot> {
        self.object(|c| Box::pin(async { c.wallet_cosigners_local() }))
            .await
    }

    pub async fn new_wallet_cosigner(&self, label: String) -> Result<ObjectSnapshot> {
        self.object(move |c| Box::pin(async move { c.wallet_cosigner_new(&label, true) }))
            .await
    }

    /// Sign a local challenge with the cosigner key `label`; nothing leaves
    /// the device.
    pub async fn wallet_cosigner_test(&self, label: String) -> Result<ObjectSnapshot> {
        self.held_object(move |h| {
            Box::pin(async move { wallet::cosign::test_key(h, &label).await })
        })
        .await
    }

    /// Ask this device to cosign an existing plan (0 for an ICP proposal):
    /// once approved, its signatures join this device's packet.
    pub async fn wallet_cosign_request(
        &self,
        wallet: String,
        proposal: u64,
        plan: u32,
    ) -> Result<ObjectSnapshot> {
        let wallet = wallet::parse_wallet(&wallet)?;
        self.held_object(move |h| {
            Box::pin(
                async move { wallet::packets::request(h, "gui", wallet, proposal, plan).await },
            )
        })
        .await
    }

    /// The signatures collected for a plan, per address, and the packet to
    /// share with the other cosigners.
    pub async fn wallet_cosign_status(
        &self,
        wallet: String,
        proposal: u64,
        plan: u32,
    ) -> Result<ObjectSnapshot> {
        let wallet = wallet::parse_wallet(&wallet)?;
        self.held_object(move |h| {
            Box::pin(async move { wallet::packets::status(h, wallet, proposal, plan).await })
        })
        .await
    }

    /// Collect the signatures of another device's packet for the same plan.
    pub async fn wallet_cosign_import(
        &self,
        wallet: String,
        proposal: u64,
        plan: u32,
        packet: String,
    ) -> Result<ObjectSnapshot> {
        let wallet = wallet::parse_wallet(&wallet)?;
        self.held_object(move |h| {
            Box::pin(async move {
                wallet::packets::import(h, wallet, proposal, plan, packet.trim()).await
            })
        })
        .await
    }

    /// Execute a plan with the collected signatures once they are enough.
    pub async fn wallet_cosign_execute(
        &self,
        wallet: String,
        proposal: u64,
        plan: u32,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        let wallet = wallet::parse_wallet(&wallet)?;
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move { wallet::packets::execute(h, wallet, proposal, plan).await })
            }))
            .await
    }
}

#[derive(uniffi::Enum)]
pub enum WalletChange {
    AddAddress {
        chain: String,
        label: String,
        cosigners: Vec<u32>,
        threshold: u8,
    },
    AddMember {
        identity: String,
        role: String,
        label: String,
    },
    UpdateMember {
        identity: String,
        role: String,
        label: String,
    },
    RemoveMember {
        identity: String,
    },
    RemoveCosigner {
        cosigner: u32,
    },
    SetCosign {
        address: u32,
        cosigners: Vec<u32>,
        threshold: u8,
    },
    ApprovalThresholds {
        approvers: u8,
        administrators: u8,
    },
    Unfreeze,
}

/// Device IDs in hex.
fn devices(ids: &[String]) -> dmsg_types::Result<Vec<dmsg_types::Hash>> {
    ids.iter()
        .map(|id| dmsg_client::upload::hex_id(id))
        .collect()
}

fn member_id(text: &str) -> dmsg_types::Result<dmsg_types::wallet::MemberId> {
    if let Ok(account) = text.parse::<dmsg_types::AccountId>() {
        return Ok(dmsg_types::wallet::MemberId::Account(account));
    }
    wallet::parse_wallet(text).map(dmsg_types::wallet::MemberId::Principal)
}

fn member_role(text: &str) -> dmsg_types::Result<dmsg_types::wallet::Role> {
    use dmsg_types::wallet::Role;
    match text {
        "Admin" => Ok(Role::Admin),
        "Approver" => Ok(Role::Approver),
        "Proposer" => Ok(Role::Proposer),
        "Viewer" => Ok(Role::Viewer),
        "Recovery" => Ok(Role::Recovery),
        _ => Err(dmsg_types::invalid("role")),
    }
}

fn cosign(
    cosigners: Vec<u32>,
    threshold: u8,
) -> dmsg_types::Result<Option<dmsg_types::wallet::CosignPolicy>> {
    if cosigners.is_empty() {
        return Ok(None);
    }
    ensure_valid(
        threshold > 0
            && usize::from(threshold) <= cosigners.len()
            && cosigners
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == cosigners.len(),
        "cosigners",
    )?;
    Ok(Some(dmsg_types::wallet::CosignPolicy {
        cosigners,
        threshold,
    }))
}

#[uniffi::export]
impl MobileEngine {
    pub async fn wallet_balances(&self, wallet: String) -> Result<ObjectSnapshot> {
        let wallet = wallet::parse_wallet(&wallet)?;
        self.held_object(move |h| Box::pin(async move { wallet::balances(h, wallet).await }))
            .await
    }

    pub async fn wallet_change(
        &self,
        wallet: String,
        change: WalletChange,
        request_id: String,
    ) -> Result<ObjectSnapshot> {
        use dmsg_types::wallet::{AddressKind, Chain, Intent, MemberInput};
        let wallet = wallet::parse_wallet(&wallet)?;
        let id = dmsg_client::upload::hex_id(&request_id)?;
        self.held_object(move |h| {
            Box::pin(async move {
                let intent = match change {
                    WalletChange::AddAddress {
                        chain,
                        label,
                        cosigners,
                        threshold,
                    } => Intent::AddAddress {
                        chain: match chain.as_str() {
                            "Icp" => Chain::Icp,
                            "Bitcoin" => Chain::Bitcoin,
                            "Evm" => Chain::Evm,
                            "Solana" => Chain::Solana,
                            _ => return Err(dmsg_types::invalid("chain")),
                        },
                        kind: AddressKind::ChainKey,
                        label,
                        cosign: cosign(cosigners, threshold)?,
                    },
                    WalletChange::AddMember {
                        identity,
                        role,
                        label,
                    } => Intent::AddMember(MemberInput {
                        id: member_id(&identity)?,
                        role: member_role(&role)?,
                        label,
                        agent_ids: vec![],
                    }),
                    WalletChange::UpdateMember {
                        identity,
                        role,
                        label,
                    } => {
                        let state = wallet::state(h, wallet).await?;
                        let id = member_id(&identity)?;
                        let current = state
                            .members
                            .iter()
                            .find(|m| m.id == id)
                            .ok_or(Error::NotFound)?;
                        Intent::UpdateMember(MemberInput {
                            id,
                            role: member_role(&role)?,
                            label,
                            agent_ids: current.agent_ids.clone(),
                        })
                    }
                    WalletChange::RemoveMember { identity } => {
                        Intent::RemoveMember(member_id(&identity)?)
                    }
                    WalletChange::RemoveCosigner { cosigner } => Intent::RemoveCosigner(cosigner),
                    WalletChange::SetCosign {
                        address,
                        cosigners,
                        threshold,
                    } => Intent::SetCosign {
                        address,
                        cosign: cosign(cosigners, threshold)?,
                    },
                    WalletChange::ApprovalThresholds {
                        approvers,
                        administrators,
                    } => {
                        ensure_valid(approvers > 0 && administrators > 0, "threshold")?;
                        let mut policy = wallet::state(h, wallet).await?.policy;
                        policy.version =
                            policy.version.checked_add(1).ok_or(Error::QuotaExceeded)?;
                        policy.approver_threshold = approvers;
                        policy.admin_threshold = administrators;
                        Intent::SetPolicy(policy)
                    }
                    WalletChange::Unfreeze => Intent::Unfreeze,
                };
                wallet::ops::propose_with_id(h, wallet, intent, None, id).await
            })
        })
        .await
    }

    pub async fn freeze_wallet(&self, wallet: String) -> Result<ObjectSnapshot> {
        let wallet = wallet::parse_wallet(&wallet)?;
        self.held_object(move |h| Box::pin(async move { wallet::ops::freeze(h, wallet).await }))
            .await
    }

    pub async fn cancel_wallet_proposal(
        &self,
        wallet: String,
        proposal: u64,
    ) -> Result<ObjectSnapshot> {
        let wallet = wallet::parse_wallet(&wallet)?;
        self.held_object(move |h| {
            Box::pin(async move { wallet::ops::cancel(h, wallet, proposal).await })
        })
        .await
    }

    pub async fn broadcast_wallet(
        &self,
        wallet: String,
        proposal: u64,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        let wallet = wallet::parse_wallet(&wallet)?;
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move { wallet::ops::broadcast(h, wallet, proposal, None).await })
            }))
            .await
    }
}
