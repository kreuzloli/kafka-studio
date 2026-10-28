//! 模型模块：对外暴露。

mod connection;
pub(crate) use connection::Connection;
mod connection_folder;
pub(crate) use connection_folder::{ConnectionFolder, ConnectionFolderId};
mod connection_import;
pub(crate) use connection_import::{ConnectionImport, ImportedConnection, ImportedFolder};
mod scram_credentials;
pub(crate) use scram_credentials::ScramCredentials;
