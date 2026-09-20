use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{message}")]
    User {
        message: String,
        technical: Option<String>,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Error {
    pub fn user(message: impl Into<String>) -> Self {
        Self::User {
            message: message.into(),
            technical: None,
        }
    }

    pub fn detailed(message: impl Into<String>, technical: impl Into<String>) -> Self {
        Self::User {
            message: message.into(),
            technical: Some(technical.into()),
        }
    }

    pub fn human_message(&self) -> String {
        match self {
            Self::User { message, .. } => message.clone(),
            Self::Io(err) => format!("A file or disk error occurred: {err}"),
        }
    }

    pub fn technical_details(&self) -> Option<String> {
        match self {
            Self::User { technical, .. } => technical.clone(),
            Self::Io(err) => Some(err.to_string()),
        }
    }
}
