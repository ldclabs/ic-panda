use dmsg_types::Error;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MobileError {
    #[error("{code}")]
    Failure { code: String },
}

pub type Result<T> = std::result::Result<T, MobileError>;

impl From<Error> for MobileError {
    fn from(error: Error) -> Self {
        if let Error::Unavailable(message) = &error
            && message == dmsg_client::approval_key::HARDWARE_AUTHORIZATION_REQUIRED
        {
            return Self::Failure {
                code: "HARDWARE_AUTHORIZATION_REQUIRED".into(),
            };
        }
        if let Error::Unavailable(message) = &error
            && let Some(reason) = message.strip_prefix("hardware: ")
        {
            return Self::Failure {
                code: match reason {
                    "BIOMETRIC_LOCKOUT"
                    | "BIOMETRIC_LOCKOUT_PERMANENT"
                    | "BIOMETRIC_NOT_ENROLLED"
                    | "DEVICE_CREDENTIAL_NOT_SET"
                    | "HARDWARE_SECURITY_UPDATE_REQUIRED"
                    | "AUTHENTICATION_TIMEOUT" => reason,
                    _ => "HARDWARE_UNAVAILABLE",
                }
                .into(),
            };
        }
        Self::Failure {
            code: dmsg_client::error_code(&error),
        }
    }
}
