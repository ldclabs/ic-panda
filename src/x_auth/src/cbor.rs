use crate::erring::HTTPError;
use async_trait::async_trait;
use axum::{
    body::Body,
    extract::FromRequest,
    http::{
        header::{self, HeaderValue},
        request::Request,
        StatusCode,
    },
    response::{IntoResponse, Response},
};
use bytes::{BufMut, Bytes, BytesMut};
use serde::{de::DeserializeOwned, Serialize};
use std::{error::Error, ops::Deref};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cbor<T>(pub T);

impl<T: Default> Default for Cbor<T> {
    fn default() -> Self {
        Cbor(T::default())
    }
}

impl<T> AsRef<T> for Cbor<T> {
    #[inline]
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T> Deref for Cbor<T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[async_trait]
impl<T, S> FromRequest<S> for Cbor<T>
where
    T: DeserializeOwned + Send + Sync,
    S: Send + Sync,
{
    type Rejection = HTTPError;

    async fn from_request(req: Request<Body>, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = Bytes::from_request(req, state).await.map_err(|err| {
            HTTPError::new(
                StatusCode::BAD_REQUEST.as_u16(),
                format!("Invalid body, {}", err),
            )
        })?;

        let value: T = ciborium::from_reader(&bytes[..]).map_err(|err| HTTPError {
            code: StatusCode::BAD_REQUEST.as_u16(),
            message: format!("Invalid CBOR body, {}", err),
            data: None,
        })?;
        Ok(Cbor(value))
    }
}

impl<T> IntoResponse for Cbor<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        // Use a small initial capacity of 128 bytes like serde_json::to_vec
        // https://docs.rs/serde_json/1.0.82/src/serde_json/ser.rs.html#2189
        let mut buf = BytesMut::with_capacity(128).writer();
        let res: Result<Response, Box<dyn Error>> = match ciborium::into_writer(&self.0, &mut buf) {
            Ok(()) => Ok((
                [(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/cbor"),
                )],
                buf.into_inner().freeze(),
            )
                .into_response()),
            Err(err) => Err(Box::new(err)),
        };

        match res {
            Ok(res) => res,
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("text/plain; charset=utf-8"),
                )],
                err.to_string(),
            )
                .into_response(),
        }
    }
}
