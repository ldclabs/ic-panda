//! Irreversible operations are never replayed by the approval reconciler.

use super::*;
use crate::hold::{Hold, Unlocked};
use futures_util::future::{AbortHandle, AbortRegistration, Abortable};

pub(crate) struct ActiveUse {
    pub(crate) actor: String,
    pub(crate) policy: String,
    pub(crate) abort: AbortHandle,
}

/// Approved work to execute outside the client lock.
pub struct SecretRun {
    id: String,
    registration: AbortRegistration,
}

impl Client {
    pub(crate) fn secret_abort(&mut self, actor: Option<&str>, policy: Option<&str>) {
        self.secret_active.retain(|id, active| {
            if actor.is_none_or(|a| a == active.actor) && policy.is_none_or(|p| p == active.policy)
            {
                active.abort.abort();
                self.running.remove(id);
                false
            } else {
                true
            }
        });
    }

    pub(crate) fn secret_run(&mut self, request: &mut Request, now: u64) -> Result<SecretRun> {
        let Action::SecretUse { plan } = &request.action else {
            return Err(Error::Forbidden);
        };
        self.secret_plan_check(plan, now)?;
        ensure(
            self.secret_active.len() < 4
                && !self.secret_active.values().any(|a| a.actor == plan.actor),
            Error::QuotaExceeded,
        )?;
        let (abort, registration) = AbortHandle::new_pair();
        self.secret_active.insert(
            request.id.clone(),
            ActiveUse {
                actor: plan.actor.clone(),
                policy: plan.input.policy_id.clone(),
                abort,
            },
        );
        self.running.insert(request.id.clone(), now);
        Ok(SecretRun {
            id: request.id.clone(),
            registration,
        })
    }

    pub(crate) fn secret_fail_before_send(&mut self, id: &str, error: Error) -> Result<Value> {
        self.secret_settle(id, State::Failed, None, Some(crate::error_code(&error)))
    }

    pub(crate) fn secret_finish(
        &mut self,
        id: &str,
        outcome: Result<github::IssueResult>,
    ) -> Result<Value> {
        match outcome {
            Ok(result) => self.secret_settle(id, State::Done, Some(result), None),
            // Only an explicit adapter-level HTTP rejection proves no operation.
            Err(Error::Forbidden) => {
                self.secret_settle(id, State::Failed, None, Some("FORBIDDEN".into()))
            }
            Err(error) => {
                let code = if crate::error_code(&error) == "SECRET_OUTPUT_BLOCKED" {
                    "SECRET_OUTPUT_BLOCKED"
                } else {
                    "EXECUTION_UNKNOWN"
                };
                self.secret_settle(id, State::Lost, None, Some(code.into()))
            }
        }
    }

    fn secret_settle(
        &mut self,
        id: &str,
        state: State,
        result: Option<github::IssueResult>,
        error: Option<String>,
    ) -> Result<Value> {
        let mut request = self.approval_record(id)?;
        self.secret_active.remove(id);
        self.running.remove(id);
        if request.state != State::Executing {
            return self.approval_view(&request);
        }
        request.state = state;
        request.secret_result = result;
        request.error = error;
        request.updated_at = now_ms();
        self.save_approval(&request)?;
        self.approval_view(&request)
    }

    pub(crate) fn secret_reconcile(&mut self, request: &mut Request) -> Result<()> {
        if let Some(active) = self.secret_active.remove(&request.id) {
            active.abort.abort();
        }
        self.running.remove(&request.id);
        request.state = if request.started {
            State::Lost
        } else {
            State::Failed
        };
        request.error = Some(
            if request.started {
                "EXECUTION_UNKNOWN"
            } else {
                "SECRET_SESSION_ENDED"
            }
            .into(),
        );
        request.updated_at = now_ms();
        self.save_approval(request)
    }
}

impl SecretRun {
    /// Send once; only session-bound settlement can publish a result.
    pub async fn run<H: Hold>(self, h: &H) -> Result<Value> {
        self.run_with(h, |http, request, target, token| async move {
            github::send(http, request, &target, &token).await
        })
        .await
    }

    pub(crate) async fn run_with<H: Hold, F, Fut>(self, h: &H, send: F) -> Result<Value>
    where
        F: FnOnce(reqwest::Client, reqwest::Request, Target, Zeroizing<String>) -> Fut,
        Fut: std::future::Future<Output = Result<github::IssueResult>>,
    {
        let bound = Unlocked::new(h).await?;
        // Transport setup may fail before the irreversible point.
        let prepared = bound
            .with(|c| {
                let now = now_ms();
                let mut request = c.approval_record(&self.id)?;
                ensure(
                    request.state == State::Executing && !request.started,
                    Error::Expired,
                )?;
                ensure(now < request.expires_at, Error::Expired)?;
                let Action::SecretUse { plan } = &request.action else {
                    return Err(Error::Forbidden);
                };
                c.secret_plan_check(plan, now)?;
                ensure(
                    c.secret_active.contains_key(&self.id),
                    failure("SECRET_SESSION_ENDED"),
                )?;
                let http = github::transport()?;
                // Mark before extracting the executable credential, in this same
                // synchronous section. A crash from here on is conservatively lost.
                request.started = true;
                c.save_approval(&request)?;
                let token = c.secret_value(&plan.resolved.reference)?;
                let outbound = github::request(&http, &plan.target, &plan.input.arguments, &token)?;
                Ok((http, outbound, plan.target.clone(), token))
            })
            .await;
        let (http, request, target, token) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                return bound
                    .with(|c| c.secret_fail_before_send(&self.id, error))
                    .await;
            }
        };
        let result = Abortable::new(send(http, request, target, token), self.registration)
            .await
            .unwrap_or(Err(Error::ExecutionUnknown));
        bound.with(|c| c.secret_finish(&self.id, result)).await
    }
}
