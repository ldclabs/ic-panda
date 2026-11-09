//! AuthenticationServices transports the same challenge/response as the browser.
use super::*;
use dmsg_client::hold::Hold;
use dmsg_types::auth::{AuthAction, PasskeyCallback, PasskeyResponse};

#[uniffi::export]
impl MobileEngine {
    pub async fn begin_passkey(&self, operation: PasskeyOperation) -> Result<PasskeyStart> {
        self.cancel_login();
        let revision = self.login.lock().map_err(|_| Error::Locked)?.revision;
        let generation = self.host.generation();
        let action = match operation {
            PasskeyOperation::Login | PasskeyOperation::BindExisting => AuthAction::Login,
            PasskeyOperation::Register | PasskeyOperation::Bind => AuthAction::Register,
            PasskeyOperation::LogoutAll => AuthAction::LogoutAll,
        };
        let session = self
            .held(move |h| {
                Box::pin(async move {
                    if matches!(
                        operation,
                        PasskeyOperation::Bind | PasskeyOperation::BindExisting
                    ) {
                        h.ensure_login().await?;
                        h.with(|c| ensure(c.session_state().state == "unlocked", Error::Locked))
                            .await?;
                    }
                    let setup = h.with(|c| c.passkey_start(action)).await?;
                    setup.start(dmsg_protocol::login::IOS_CALLBACK.into()).await
                })
            })
            .await?;
        let started = PasskeyStart {
            state: session.request().state.to_vec(),
            challenge: session.request().challenge.challenge.to_vec(),
            user_id: session.user_handle(),
            credential_id: session.credential(),
            operation,
            deadline_ms: session.deadline,
        };
        let mut login = self.login.lock().map_err(|_| Error::Locked)?;
        ensure(
            login.revision == revision && self.host.inner.generation() == generation,
            Error::Locked,
        )?;
        login.pending = Some(PendingLogin {
            session: InteractiveLogin::Passkey(Box::new(session)),
            binding_generation: matches!(
                operation,
                PasskeyOperation::Bind | PasskeyOperation::BindExisting
            )
            .then_some(generation),
            callback: None,
            delegated: None,
        });
        Ok(started)
    }

    pub async fn complete_passkey(
        &self,
        state: Vec<u8>,
        response: NativePasskeyResponse,
    ) -> Result<SessionSnapshot> {
        let callback = self.native_passkey_callback(state, response)?;
        self.complete_login(callback).await
    }

    pub async fn complete_passkey_binding(
        &self,
        state: Vec<u8>,
        response: NativePasskeyResponse,
    ) -> Result<ObjectSnapshot> {
        let callback = self.native_passkey_callback(state, response)?;
        self.complete_binding(callback).await
    }
}

impl MobileEngine {
    fn native_passkey_callback(
        &self,
        state: Vec<u8>,
        response: NativePasskeyResponse,
    ) -> Result<String> {
        let state = dmsg_types::Hash::new(state.try_into().map_err(|_| Error::IntegrityFailed)?);
        let callback = PasskeyCallback {
            protocol: dmsg_client::passkey::PROTOCOL.into(),
            state,
            response: Some(PasskeyResponse {
                credential_id: response.credential_id.into(),
                client_data_json: response.client_data_json.into(),
                authenticator_data: response.authenticator_data.into(),
                signature: response.signature.into(),
            }),
            error: None,
        };
        let bytes = dmsg_client::codec::canonical(&callback);
        let login = self.login.lock().map_err(|_| Error::Locked)?;
        let pending = login.pending.as_ref().ok_or(Error::Expired)?;
        ensure(
            matches!(pending.session, InteractiveLogin::Passkey(_)),
            Error::Forbidden,
        )?;
        if let Some(accepted) = &pending.callback {
            ensure(*accepted == bytes, Error::IntegrityFailed)?;
        } else {
            pending.session.accept(&bytes, dmsg_client::now_ms())?;
        }
        Ok(format!(
            "{}#{}",
            dmsg_protocol::login::IOS_CALLBACK,
            dmsg_client::codec::b64(&bytes)
        ))
    }
}
