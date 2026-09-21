//! Versioned storage envelopes are independent of public certification bytes.
use crate::{model::Subject, store::Config};
use cbor2::Cbor;
use dmsg_runtime::storage::StableCodec;

#[derive(Cbor)]
pub struct Record<T> {
    #[cbor(key = 0)]
    pub schema: u16,
    #[cbor(key = 1)]
    pub value: T,
}

macro_rules! codec {
    ($t:ty) => {
        impl StableCodec for $t {
            type Repr = Record<Self>;

            fn to_repr(&self) -> Self::Repr {
                Record {
                    schema: 1,
                    value: self.clone(),
                }
            }

            fn from_repr(r: Self::Repr) -> Self {
                assert_eq!(r.schema, 1, "incompatible development state");
                r.value
            }
        }
    };
}
codec!(Subject);
codec!(Config);
