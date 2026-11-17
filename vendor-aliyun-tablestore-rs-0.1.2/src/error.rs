use std::{
    fmt::{Display, Formatter},
    string::FromUtf8Error,
};

use reqwest::StatusCode;
use thiserror::Error;

use crate::protos;

impl Display for protos::Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "API response error. code: {}, message: {}",
            self.code,
            self.message.as_ref().unwrap_or(&"".to_string())
        )
    }
}

#[derive(Error, Debug)]
pub enum OtsError {
    #[error("{0}")]
    ReqwestError(#[from] reqwest::Error),

    #[error("{0}")]
    ProtobufDecodeError(#[from] prost::DecodeError),

    #[error("Decode simple row matrix data failed: {0}")]
    SrmDecodeError(String),

    #[error("Validation failed: {0}")]
    ValidationFailed(String),

    /// This is error for OTS API response.
    #[error("{0}")]
    ApiError(Box<protos::Error>),

    #[error("{0}")]
    FromUtf8Error(#[from] FromUtf8Error),

    #[error("{0}")]
    ReadError(#[from] std::io::Error),

    #[error("Aliyun ots api response with non-successful code: {0}. response message is: {1}")]
    StatusError(StatusCode, String),

    #[error("{0}")]
    PlainBufferError(String),
}
