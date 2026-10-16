//! The payer: ICRC ledger transfers paid by the login's Principal or an
//! identity PEM file (plan R25). The dMsg account never holds funds; the
//! payer is only the identity that pays. Team wallets are [`crate::wallet`].
//!
//! Every payment is journaled under the LDK before it is sent, with its
//! exact arguments: a lost reply sends the same bytes again, and the
//! ledger's deduplication answers with the original block. Only a typed
//! ledger rejection proves nothing was paid; then a new attempt may get a
//! fresh timestamp. A transport failure, a trap or any other error keeps the
//! journal unknown and the same arguments.

use crate::client::Client;
use crate::ic::Ic;
use candid::{Nat, Principal};
use dmsg_types::{ensure, Error, Result};
use icrc_ledger_types::icrc1::account::Account;
use icrc_ledger_types::icrc1::transfer::{Memo, TransferArg, TransferError};
use icrc_ledger_types::icrc2::allowance::{Allowance, AllowanceArgs};
use icrc_ledger_types::icrc2::approve::{ApproveArgs, ApproveError};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

const JOURNAL: &str = "payer:";

/// The arguments of one journaled ledger call.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Call {
    /// `icrc1_transfer`.
    Transfer(ByteBuf),
    /// `icrc2_approve`.
    Approve(ByteBuf),
}

/// Where a payment stands.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Written, not sent.
    Prepared,
    /// Sent; the reply was not seen or proved nothing.
    Unknown,
    /// The ledger took it at `block`.
    Confirmed,
    /// The ledger refused these arguments.
    Rejected,
}

/// One payment's journal.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Payment {
    /// The ledger.
    pub ledger: Principal,
    /// The paying Principal.
    pub payer: Principal,
    /// Candid arguments.
    pub call: Call,
    /// Progress.
    pub stage: Stage,
    /// The ledger block.
    pub block: Option<u64>,
    /// The last rejection.
    pub error: Option<String>,
}

/// An ICRC ledger as the payer reaches it.
pub struct Ledger<'a> {
    /// The payer's agent.
    pub ic: &'a Ic,
    /// The ledger canister.
    pub canister: Principal,
}

impl Ledger<'_> {
    /// The balance of `owner`'s default account.
    ///
    /// # Errors
    /// Query failures return `Error::Unavailable`.
    pub async fn balance(&self, owner: Principal) -> Result<u128> {
        let n: Nat = self
            .ic
            .query(
                self.canister,
                "icrc1_balance_of",
                (Account {
                    owner,
                    subaccount: None,
                },),
            )
            .await?;
        nat(&n)
    }

    /// The ledger's current fee.
    ///
    /// # Errors
    /// Query failures return `Error::Unavailable`.
    pub async fn fee(&self) -> Result<u128> {
        let n: Nat = self.ic.query(self.canister, "icrc1_fee", ()).await?;
        nat(&n)
    }

    /// The allowance `owner` gave `spender`.
    ///
    /// # Errors
    /// Query failures return `Error::Unavailable`.
    pub async fn allowance(&self, owner: Principal, spender: Principal) -> Result<Allowance> {
        self.ic
            .query(
                self.canister,
                "icrc2_allowance",
                (AllowanceArgs {
                    account: Account {
                        owner,
                        subaccount: None,
                    },
                    spender: Account {
                        owner: spender,
                        subaccount: None,
                    },
                },),
            )
            .await
    }
}

fn nat(n: &Nat) -> Result<u128> {
    u128::try_from(n.0.clone()).map_err(|_| Error::IntegrityFailed)
}

fn block(n: &Nat) -> Result<u64> {
    u64::try_from(n.0.clone()).map_err(|_| Error::IntegrityFailed)
}

/// A transfer's arguments for the journal.
pub fn transfer_call(arg: &TransferArg) -> Result<Call> {
    Ok(Call::Transfer(ByteBuf::from(
        candid::encode_one(arg).map_err(|e| Error::InvalidInput(format!("candid: {e}")))?,
    )))
}

/// An approval's arguments for the journal.
pub fn approve_call(arg: &ApproveArgs) -> Result<Call> {
    Ok(Call::Approve(ByteBuf::from(
        candid::encode_one(arg).map_err(|e| Error::InvalidInput(format!("candid: {e}")))?,
    )))
}

/// A 32-byte memo.
pub fn memo(bytes: &[u8]) -> Memo {
    Memo(ByteBuf::from(bytes.to_vec()))
}

const PAYER_PEM: &str = "payer-pem";

/// An identity from a PEM file: Ed25519, secp256k1 or P-256.
fn pem_identity(path: &std::path::Path) -> Result<std::sync::Arc<dyn ic_agent::Identity>> {
    use ic_agent::identity::{BasicIdentity, Prime256v1Identity, Secp256k1Identity};
    let pem = zeroize::Zeroizing::new(
        std::fs::read(path).map_err(|e| Error::InvalidInput(format!("payer: {e}")))?,
    );
    if let Ok(identity) = BasicIdentity::from_pem(pem.as_slice()) {
        return Ok(std::sync::Arc::new(identity));
    }
    if let Ok(identity) = Secp256k1Identity::from_pem(pem.as_slice()) {
        return Ok(std::sync::Arc::new(identity));
    }
    Prime256v1Identity::from_pem(pem.as_slice())
        .map(|identity| std::sync::Arc::new(identity) as std::sync::Arc<dyn ic_agent::Identity>)
        .map_err(|_| Error::InvalidInput("payer: not an identity PEM".into()))
}

impl Client {
    /// The payer: the identity of the configured PEM file, or the login.
    ///
    /// # Errors
    /// Returns `Error::AuthRequired` without a login or payer.
    pub(crate) async fn payer(&mut self) -> Result<(Principal, Ic)> {
        let Some(path) = self.workspace.store.meta::<String>(PAYER_PEM)? else {
            let login = self.session()?;
            return Ok((login.principal, login.ic.clone()));
        };
        self.keys()?;
        if let Some(payer) = &self.payer {
            return Ok(payer.clone());
        }
        let ic = Ic::connect(&self.config, pem_identity(std::path::Path::new(&path))?).await?;
        let payer = (ic.principal()?, ic);
        self.payer = Some(payer.clone());
        Ok(payer)
    }

    /// Pay with the identity in the PEM file at `pem`, an absolute path the
    /// daemon reads when it pays; with `None`, with the login. Only the
    /// path is stored.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` for a file that holds no identity.
    pub async fn payer_use(&mut self, pem: Option<&std::path::Path>) -> Result<serde_json::Value> {
        self.keys()?;
        self.payer = None;
        match pem {
            Some(path) => {
                ensure(path.is_absolute(), dmsg_types::invalid("path"))?;
                pem_identity(path)?;
                self.workspace
                    .store
                    .set_meta(PAYER_PEM, &path.display().to_string())?;
            }
            None => self.workspace.store.delete_meta(PAYER_PEM)?,
        }
        let (principal, _) = self.payer().await?;
        Ok(serde_json::json!({
            "principal": principal.to_text(),
            "source": if pem.is_some() { "pem" } else { "login" },
        }))
    }

    /// A payment's journal, if any.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn payment_journal(&self, key: &str) -> Result<Option<Payment>> {
        self.workspace
            .private(self.keys()?, &format!("{JOURNAL}{key}"))
    }

    fn save_payment(&self, key: &str, payment: &Payment) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, &format!("{JOURNAL}{key}"), payment)
    }

    /// Pay once under `key`: the journaled arguments, or those `create`
    /// builds for a first attempt or after a definite rejection, with a
    /// fresh ledger timestamp. `fee` is
    /// the network fee the person approved; it must be the ledger's.
    ///
    /// # Errors
    /// Returns `Error::FeeBlocked` when the ledger's fee changed,
    /// `Error::ExecutionUnknown` when the outcome is unknown and the
    /// ledger's rejection otherwise.
    pub(crate) async fn pay(
        &mut self,
        key: &str,
        ledger: Principal,
        fee: u128,
        create: impl Fn() -> Result<Call>,
    ) -> Result<u64> {
        let (payer, ic) = self.payer().await?;
        let mut payment = match self.payment_journal(key)? {
            Some(payment) => payment,
            None => {
                let payment = Payment {
                    ledger,
                    payer,
                    call: create()?,
                    stage: Stage::Prepared,
                    block: None,
                    error: None,
                };
                self.save_payment(key, &payment)?;
                payment
            }
        };
        if let Some(block) = payment.block {
            return Ok(block);
        }
        ensure(
            payment.ledger == ledger && payment.payer == payer,
            Error::Forbidden,
        )?;
        let reader = Ledger {
            ic: &ic,
            canister: ledger,
        };
        if payment.stage == Stage::Rejected {
            // The ledger refused these exact arguments: nothing was paid.
            self.save_payment(&format!("{key}:rejected:{}", crate::now_ms()), &payment)?;
            payment = Payment {
                call: create()?,
                stage: Stage::Prepared,
                error: None,
                ..payment
            };
        }
        if payment.stage == Stage::Prepared {
            ensure(reader.fee().await? == fee, Error::FeeBlocked)?;
        }
        // A rejection of a resend does not prove the first attempt failed.
        let uncertain = payment.stage == Stage::Unknown;
        payment.stage = Stage::Unknown;
        self.save_payment(key, &payment)?;
        let outcome = match &payment.call {
            Call::Transfer(bytes) => {
                let arg: TransferArg =
                    candid::decode_one(bytes).map_err(|_| Error::IntegrityFailed)?;
                let reply: Result<std::result::Result<Nat, TransferError>> =
                    ic.update(ledger, "icrc1_transfer", (arg,)).await;
                match reply {
                    Ok(Ok(n)) => Ok(block(&n)?),
                    Ok(Err(TransferError::Duplicate { duplicate_of })) => Ok(block(&duplicate_of)?),
                    Ok(Err(error)) => Err(format!("{error:?}")),
                    Err(_) => return Err(Error::ExecutionUnknown),
                }
            }
            Call::Approve(bytes) => {
                let arg: ApproveArgs =
                    candid::decode_one(bytes).map_err(|_| Error::IntegrityFailed)?;
                let reply: Result<std::result::Result<Nat, ApproveError>> =
                    ic.update(ledger, "icrc2_approve", (arg,)).await;
                match reply {
                    Ok(Ok(n)) => Ok(block(&n)?),
                    Ok(Err(ApproveError::Duplicate { duplicate_of })) => Ok(block(&duplicate_of)?),
                    Ok(Err(error)) => Err(format!("{error:?}")),
                    Err(_) => return Err(Error::ExecutionUnknown),
                }
            }
        };
        match outcome {
            Ok(block) => {
                payment.block = Some(block);
                payment.stage = Stage::Confirmed;
                self.save_payment(key, &payment)?;
                Ok(block)
            }
            Err(error) if uncertain => {
                payment.error = Some(error);
                self.save_payment(key, &payment)?;
                Err(Error::ExecutionUnknown)
            }
            Err(error) => {
                payment.stage = Stage::Rejected;
                payment.error = Some(error.clone());
                self.save_payment(key, &payment)?;
                Err(if error.starts_with("BadFee") {
                    Error::FeeBlocked
                } else {
                    Error::InvalidInput(format!("ledger: {error}"))
                })
            }
        }
    }

    /// The payer's Principal and its balance on `ledger`.
    ///
    /// # Errors
    /// Returns `Error::AuthRequired` without a login.
    pub async fn payer_balance(&mut self, ledger: Principal) -> Result<serde_json::Value> {
        let (payer, ic) = self.payer().await?;
        let reader = Ledger {
            ic: &ic,
            canister: ledger,
        };
        Ok(serde_json::json!({
            "principal": payer.to_text(),
            "ledger": ledger.to_text(),
            "balance": reader.balance(payer).await?.to_string(),
            "fee": reader.fee().await?.to_string(),
        }))
    }
}
