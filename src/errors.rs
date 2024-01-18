use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq, Clone)]
pub enum CompressorError {
    #[error("Dict not init")]
    DictNotInit,
    #[error("Invalid range")]
    InvalidRange,
    #[error("Unsuported method: `{0}`")]
    UnsuportedMethod(u8),
}
