use std::time::Duration;

use rdkafka::{
    ClientConfig,
    consumer::{BaseConsumer, Consumer},
    error::KafkaError,
};

use crate::models::{Connection, ConnectionSecurity};

/// Own the topic data needed after the native metadata is dropped.
#[derive(Debug)]
pub(crate) struct TopicSummary {
    pub(crate) name: String,
    pub(crate) error: Option<String>,
}

/// Carry the metadata snapshot from the worker to the UI.
#[derive(Debug)]
pub(crate) struct MetadataSummary {
    pub(crate) broker_count: usize,
    pub(crate) topics: Vec<TopicSummary>,
}

/// Preserve the failure stage and the original Kafka error.
pub(crate) enum MetadataError {
    ClientCreation(KafkaError),
    Fetch(KafkaError),
}

/// Map supported saved settings into a Kafka client configuration.
/// Checks required fields without reading certificates or opening a connection.
pub(crate) fn build_client_config(connection: &Connection) -> Result<ClientConfig, &'static str> {
    validate_kafka_features()?;
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
    // Metadata inspection must not create topics or commit consumer offsets.
    config.set("allow.auto.create.topics", "false");
    config.set("enable.auto.commit", "false");

    // Validate property names and values without starting a Kafka client.
    config.create_native_config().map_err(|error| {
        // Native configuration errors can contain passwords; log only the key and code.
        if let KafkaError::ClientConfig(code, _, key, _) = &error {
            eprintln!("[kafka-studio] event=metadata_config_rejected key={key} code={code:?}");
        }
        "The Kafka library rejected a configuration setting."
    })?;
    Ok(config)
}

/// Fetch broker count and topic entries on a blocking background worker.
/// The timeout bounds metadata fetching, not the entire client lifecycle.
pub(crate) fn fetch_metadata_summary(
    config: ClientConfig,
) -> Result<MetadataSummary, MetadataError> {
    let consumer = config
        .create::<BaseConsumer>()
        .inspect_err(|err| {
            // Record the failed stage without exposing connection details.
            eprintln!(
                "[kafka-studio] event=metadata_client_creation_failed code={:?}",
                err.rdkafka_error_code()
            );
        })
        .map_err(MetadataError::ClientCreation)?;
    // None requests metadata without restricting it to one named topic.
    let metadata = consumer
        .fetch_metadata(None, Duration::from_secs(5))
        .inspect_err(|err| {
            // Record the failed stage without exposing connection details.
            eprintln!(
                "[kafka-studio] event=metadata_fetch_failed code={:?}",
                err.rdkafka_error_code()
            );
        })
        .map_err(MetadataError::Fetch)?;
    let topics = metadata
        .topics()
        .iter()
        .map(|topic| TopicSummary {
            name: topic.name().to_owned(),
            error: topic.error().map(|err| format!("{err:?}")),
        })
        .collect();
    Ok(MetadataSummary {
        broker_count: metadata.brokers().len(),
        topics,
    })
}

/// Check required native-library capabilities without creating a Kafka client.
fn validate_kafka_features() -> Result<(), &'static str> {
    let mut config = ClientConfig::new();
    config.set("builtin.features", "ssl,sasl,sasl_scram");
    config
        .create_native_config()
        .map(|_| ())
        .map_err(|_| "The Kafka library requires TLS and SASL/SCRAM build support.")
}
