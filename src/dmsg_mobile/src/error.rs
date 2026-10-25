use dmsg_types::Error;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MobileError {
    #[error("{code}")]
    Failure { code: String },
}

pub type Result<T> = std::result::Result<T, MobileError>;

impl From<Error> for MobileError {
    fn from(error: Error) -> Self {
        Self::Failure {
            code: dmsg_client::error_code(&error),
        }
    }
}
