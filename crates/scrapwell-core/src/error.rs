#[derive(thiserror::Error, Debug)]
pub enum ScrapwellError {
    #[error("invalid path: {0}")]
    InvalidPath(String),

    #[error("entry not found: {0}")]
    NotFound(String),

    #[error("duplicate name: {0}")]
    DuplicateName(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, ScrapwellError>;
