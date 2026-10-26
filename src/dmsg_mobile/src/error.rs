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
            && message.starts_with("hardware: ")
        {
            return Self::Failure {
                code: "HARDWARE_UNAVAILABLE".into(),
            };
        }
        Self::Failure {
            code: dmsg_client::error_code(&error),
        }
    }
}
