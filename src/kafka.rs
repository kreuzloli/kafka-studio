use rdkafka::ClientConfig;

use crate::models::{Connection, ConnectionSecurity};

/// Map supported saved settings into a Kafka client configuration.
/// Checks required fields without reading certificates or opening a connection.
pub(crate) fn build_client_config(connection: &Connection) -> Result<ClientConfig, &'static str> {
    let mut config = ClientConfig::new();
    match connection.security {
        Some(ConnectionSecurity::SaslSslScramSha512) => {
            let credentials = connection
                .credentials
                .as_ref()
                .ok_or("Provide credentials before reading metadata.")?;
            // Use the selected local CA, not the imported JKS source path.
            let ca_path = connection
                .ca_location
                .as_deref()
                .ok_or("Select a local PEM CA file before reading metadata.")?;
            let ca_location = ca_path
                .to_str()
                .ok_or("The CA file path must be valid UTF-8.")?;

            config.set("ssl.ca.location", ca_location);
            config.set("security.protocol", "SASL_SSL");
            config.set("sasl.mechanisms", "SCRAM-SHA-512");
            // Preserve credential whitespace and case; never log these values.
            config.set("sasl.username", credentials.username.as_str());
            config.set("sasl.password", credentials.password.expose());
        }
        None => {
            return Err("Specify the security mode before reading metadata.");
        }
    }
    config.set("bootstrap.servers", connection.bootstrap_servers.as_str());
    Ok(config)
}
