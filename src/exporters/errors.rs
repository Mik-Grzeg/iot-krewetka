use rdkafka::{message::OwnedMessage, error::KafkaError, producer::future_producer::OwnedDeliveryResult};

#[derive(Debug)]
pub enum ExporterError {
    KafkaErr((KafkaError, OwnedMessage)),    
}

impl From<(KafkaError, OwnedMessage)> for ExporterError {
    fn from(error: (KafkaError, OwnedMessage)) -> ExporterError {
        ExporterError::KafkaErr(error)
    }
}