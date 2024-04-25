use crate::{api, erring::HTTPError};
use http::header;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use url::Url;

// https://cloud.google.com/recaptcha-enterprise/docs/create-assessment-website
pub struct ReCAPTCHA {
    api_url: url::Url,
    pub hostnames: Vec<String>,
    pub site_key: String,
}

#[derive(Deserialize, Serialize)]
pub struct ReCAPTCHAOutput {
    pub name: String,
    pub event: Event,
    #[serde(rename = "tokenProperties")]
    pub token_properties: TokenProperties,
    #[serde(rename = "riskAnalysis")]
    pub risk_analysis: RiskAnalysis,
}

impl ReCAPTCHAOutput {
    pub fn is_valid(&self, score: f32, event: &Event, hostnames: &Vec<String>) -> bool {
        if !self.token_properties.valid
            || self.risk_analysis.score < score
            || self.token_properties.action != event.expected_action
        {
            return false;
        }

        hostnames.contains(&self.token_properties.hostname)
    }
}

#[derive(Deserialize, Serialize)]
pub struct Event {
    pub token: String,
    #[serde(rename = "siteKey")]
    pub site_key: String,
    #[serde(rename = "userAgent")]
    pub user_agent: String,
    #[serde(rename = "userIpAddress")]
    pub user_ip_address: String,
    #[serde(rename = "expectedAction")]
    pub expected_action: String,
    // ja3: String,
}

#[derive(Serialize, Deserialize)]
pub struct RiskAnalysis {
    pub score: f32,
    pub reasons: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct TokenProperties {
    pub valid: bool,
    pub hostname: String,
    pub action: String,
    #[serde(rename = "createTime")]
    pub create_time: String,
}

#[derive(Serialize)]
struct ReCAPTCHAInput<'a> {
    event: &'a Event,
}

impl ReCAPTCHA {
    pub fn new(project: String, site_key: String, api_key: String, hostnames: Vec<String>) -> Self {
        Self {
            api_url: Url::parse(&format!(
                "https://recaptchaenterprise.googleapis.com/v1/projects/{}/assessments?key={}",
                project, api_key
            ))
            .expect("invalid Google reCAPTCHA API url"),
            hostnames,
            site_key,
        }
    }

    pub async fn verify(
        &self,
        http_cli: &Client,
        event: &Event,
    ) -> Result<ReCAPTCHAOutput, HTTPError> {
        let res = match http_cli
            .post(self.api_url.clone())
            .header(header::ACCEPT, "application/json")
            .header(header::USER_AGENT, api::USER_AGENT)
            .json(&ReCAPTCHAInput { event })
            .send()
            .await
        {
            Ok(res) => {
                if res.status().is_success() {
                    res
                } else {
                    let status = res.status().as_u16();
                    let err = res
                        .text()
                        .await
                        .unwrap_or("read response failed".to_string());

                    return Err(HTTPError::new(status, err));
                }
            }
            Err(err) => {
                return Err(HTTPError::new(
                    500,
                    format!("failed to verify Google reCAPTCHA token: {:?}", err),
                ));
            }
        };

        res.json().await.map_err(|err| {
            HTTPError::new(
                500,
                format!("failed to parse Google reCAPTCHA response: {:?}", err),
            )
        })
    }
}
