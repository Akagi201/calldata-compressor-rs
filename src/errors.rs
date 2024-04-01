use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq, Clone)]
pub enum CompressorError {
    #[error("Dict not init")]
    DictNotInit,
    #[error("Invalid range")]
    InvalidRange,
    #[error("Lookup not found")]
    LookupNotFound,
    #[error("Unsupported method: `{0}`")]
    UnsupportedMethod(u8),
}
