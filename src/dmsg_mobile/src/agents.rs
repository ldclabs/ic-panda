use super::*;
use dmsg_client::agent;
use dmsg_types::agent::{DelegationAuthority, PrincipalType};

#[uniffi::export]
impl MobileEngine {
    pub async fn agents(&self) -> Result<ObjectSnapshot> {
        self.object(|c| Box::pin(async { c.agent_list() })).await
    }

    pub async fn create_agent(&self, label: String) -> Result<ObjectSnapshot> {
        self.object(move |c| Box::pin(async move { c.agent_create(&label).map(|a| a.view()) }))
            .await
    }

    pub async fn pause_agent(&self, label: String, paused: bool) -> Result<ObjectSnapshot> {
        self.object(move |c| {
            Box::pin(async move { c.agent_pause(&label, paused).map(|a| a.view()) })
        })
        .await
    }

    pub async fn remove_agent(&self, label: String) -> Result<ObjectSnapshot> {
        self.object(move |c| {
            Box::pin(async move { c.agent_remove("gui", &label).map(|a| a.view()) })
        })
        .await
    }

    pub async fn agent_grants(&self) -> Result<ObjectSnapshot> {
        self.object(|c| Box::pin(async { c.capability_grants() }))
            .await
    }

    pub async fn request_agent_permissions(
        &self,
        label: String,
        capabilities: Vec<String>,
        days: u64,
    ) -> Result<ObjectSnapshot> {
        self.object(move |c| {
            Box::pin(async move { c.capability_request(&label, capabilities, Some(days)) })
        })
        .await
    }

    pub async fn revoke_agent_permissions(&self, id: String) -> Result<ObjectSnapshot> {
        self.object(move |c| Box::pin(async move { c.capability_revoke("gui", &id) }))
            .await
    }

    pub async fn agent_principal(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| Box::pin(async { agent::principal_view(h).await }))
            .await
    }

    pub async fn enable_agent_principal(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| {
            Box::pin(async {
                agent::principal_request(
                    h,
                    "gui",
                    agent::PrincipalChange::Enable {
                        principal_type: PrincipalType::Person,
                    },
                )
                .await
            })
        })
        .await
    }

    pub async fn register_agent_controller(
        &self,
        name: String,
        scopes: Vec<String>,
        audiences: Vec<String>,
        unrestricted: bool,
    ) -> Result<ObjectSnapshot> {
        let authority = if unrestricted {
            DelegationAuthority::Unrestricted
        } else {
            DelegationAuthority::Restricted { scopes, audiences }
        };
        self.held_object(move |h| {
            Box::pin(
                async move { agent::controller_request(h, "gui", Some(name), authority).await },
            )
        })
        .await
    }

    pub async fn retire_agent_controller(&self, generation: u32) -> Result<ObjectSnapshot> {
        self.held_object(move |h| {
            Box::pin(async move {
                agent::principal_request(h, "gui", agent::PrincipalChange::Retire { generation })
                    .await
            })
        })
        .await
    }

    pub async fn agent_delegations(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| Box::pin(async { agent::delegation_credentials(h).await }))
            .await
    }

    pub async fn grant_agent_delegation(
        &self,
        subject: String,
        scopes: Vec<String>,
        audiences: Vec<String>,
        days: u64,
    ) -> Result<ObjectSnapshot> {
        self.held_object(move |h| {
            Box::pin(async move {
                agent::delegation_grant_request(
                    h, "gui", None, &subject, scopes, audiences, days, None, None,
                )
                .await
            })
        })
        .await
    }

    pub async fn revoke_agent_delegation(
        &self,
        id: String,
        reason: Option<String>,
    ) -> Result<ObjectSnapshot> {
        self.held_object(move |h| {
            Box::pin(
                async move { agent::delegation_revoke_request(h, "gui", None, &id, reason).await },
            )
        })
        .await
    }
}
