use std::path::PathBuf;

#[derive(Debug)]
#[allow(dead_code)]
pub enum CiError {
    ConfigIo {
        path: PathBuf,
        source: std::io::Error,
    },
    ConfigParse {
        source: toml::de::Error,
    },
    ConfigValidation {
        detail: String,
    },

    Git {
        context: &'static str,
        detail: String,
    },

    JobSpawn {
        job_name: String,
        source: std::io::Error,
    },
    JobIo {
        job_name: String,
        source: std::io::Error,
    },

    Ipc {
        detail: String,
    },
    SocketBind {
        path: PathBuf,
        source: std::io::Error,
    },
    SocketConnect {
        path: PathBuf,
        source: std::io::Error,
    },

    Internal {
        detail: String,
    },
}

impl std::fmt::Display for CiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CiError::ConfigIo { path, source } => {
                write!(f, "failed to read config '{}': {}", path.display(), source)
            }
            CiError::ConfigParse { source } => {
                write!(f, "failed to parse config: {source}")
            }
            CiError::ConfigValidation { detail } => {
                write!(f, "config validation error: {detail}")
            }
            CiError::Git { context, detail } => {
                write!(f, "git error ({context}): {detail}")
            }
            CiError::JobSpawn { job_name, source } => {
                write!(f, "failed to spawn job '{job_name}': {source}")
            }
            CiError::JobIo { job_name, source } => {
                write!(f, "I/O error in job '{job_name}': {source}")
            }
            CiError::Ipc { detail } => {
                write!(f, "IPC error: {detail}")
            }
            CiError::SocketBind { path, source } => {
                write!(
                    f,
                    "failed to bind unix socket '{}': {source}",
                    path.display()
                )
            }
            CiError::SocketConnect { path, source } => {
                write!(
                    f,
                    "failed to connect to daemon socket '{}': {source}",
                    path.display()
                )
            }
            CiError::Internal { detail } => {
                write!(f, "internal error: {detail}")
            }
        }
    }
}

impl std::error::Error for CiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CiError::ConfigIo { source, .. } => Some(source),
            CiError::ConfigParse { source } => Some(source),
            CiError::JobSpawn { source, .. } => Some(source),
            CiError::JobIo { source, .. } => Some(source),
            CiError::SocketBind { source, .. } => Some(source),
            CiError::SocketConnect { source, .. } => Some(source),
            _ => None,
        }
    }
}
