use candid::{define_function, CandidType};
use serde::Deserialize;
use serde_bytes::ByteBuf;

use crate::store;

#[derive(CandidType, Deserialize, Clone)]
pub struct HeaderField(pub String, pub String);

#[derive(CandidType, Deserialize, Clone)]
pub struct HttpRequest {
    pub method: String,
    pub url: String, // url path
    pub headers: Vec<HeaderField>,
    pub body: ByteBuf,
}

#[derive(CandidType, Deserialize, Clone)]
pub struct HttpResponse {
    pub status_code: u16,
    pub headers: Vec<HeaderField>,
    pub body: ByteBuf,
    pub streaming_strategy: Option<StreamingStrategy>,
}

#[derive(CandidType, Deserialize, Clone)]
pub struct StreamingCallbackToken {
    pub file_id: u32,
    pub chunk_id: u32,
    pub chunks: u32,
}

impl StreamingCallbackToken {
    pub fn next(&self) -> Option<StreamingCallbackToken> {
        if self.chunk_id + 1 >= self.chunks {
            None
        } else {
            Some(StreamingCallbackToken {
                file_id: self.file_id,
                chunk_id: self.chunk_id + 1,
                chunks: self.chunks,
            })
        }
    }
}

define_function!(pub CallbackFunc : (StreamingCallbackToken) -> (StreamingCallbackHttpResponse) query);
#[derive(CandidType, Deserialize, Clone)]
pub enum StreamingStrategy {
    Callback {
        token: StreamingCallbackToken,
        callback: CallbackFunc,
    },
}

#[derive(CandidType, Deserialize, Clone)]
pub struct StreamingCallbackHttpResponse {
    pub body: ByteBuf,
    pub token: Option<StreamingCallbackToken>,
}

fn get_file_id(path: String) -> Result<u32, String> {
    let path = path
        .strip_prefix("/file/")
        .ok_or_else(|| "invalid path".to_string())?;

    path.parse().map_err(|_| "invalid file id".to_string())
}

fn create_strategy(arg: StreamingCallbackToken) -> Option<StreamingStrategy> {
    match arg.next() {
        None => None,
        Some(token) => Some(StreamingStrategy::Callback {
            token,
            callback: CallbackFunc::new(
                ic_cdk::id(),
                "http_request_streaming_callback".to_string(),
            ),
        }),
    }
}

// https://bwwuq-byaaa-aaaan-qmk4q-cai.raw.icp0.io/file/1
#[ic_cdk::query]
fn http_request(request: HttpRequest) -> HttpResponse {
    match get_file_id(request.url) {
        Err(err) => HttpResponse {
            body: ByteBuf::from(err.as_bytes()),
            status_code: 400,
            headers: vec![],
            streaming_strategy: None,
        },
        Ok(file_id) => match store::fs::get_file(file_id) {
            None => HttpResponse {
                body: ByteBuf::from("file not found".as_bytes()),
                status_code: 404,
                headers: vec![],
                streaming_strategy: None,
            },
            Some(metadata) => {
                // todo: escape filename
                let filename = metadata.name.strip_prefix("/").unwrap_or(&metadata.name);
                let filename = format!("attachment; filename={}", filename);
                HttpResponse {
                    body: ByteBuf::from(
                        store::fs::get_chunk(file_id, 0)
                            .map(|chunk| chunk.0)
                            .unwrap_or_default(),
                    ),
                    status_code: 200,
                    headers: vec![
                        HeaderField("content-type".to_string(), metadata.content_type.clone()),
                        HeaderField("accept-ranges".to_string(), "bytes".to_string()),
                        HeaderField("content-disposition".to_string(), filename),
                        HeaderField(
                            "cache-control".to_string(),
                            "private, max-age=0".to_string(),
                        ),
                    ],
                    streaming_strategy: create_strategy(StreamingCallbackToken {
                        file_id,
                        chunk_id: 0,
                        chunks: metadata.chunks,
                    }),
                }
            }
        },
    }
}

#[ic_cdk::query]
fn http_request_streaming_callback(token: StreamingCallbackToken) -> StreamingCallbackHttpResponse {
    match store::fs::get_chunk(token.file_id, token.chunk_id) {
        None => ic_cdk::trap("chunk not found"),
        Some(chunk) => StreamingCallbackHttpResponse {
            body: ByteBuf::from(chunk.0),
            token: token.next(),
        },
    }
}
