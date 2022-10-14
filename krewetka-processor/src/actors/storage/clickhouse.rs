use super::astorage::{AStorage, StorageError};
use crate::consts::STORAGE_BUFFER_SIZE;
use clickhouse_rs::{row, types::Block, Pool};
use log::debug;
use log::info;
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::actors::messages::PersistFlowMessageWithMetadata;
use async_trait::async_trait;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ClickhouseSettings {
    host: String,
    port: u16,
    user: String,
    password: String,
}

impl From<ClickhouseSettings> for ClickhouseState {
    fn from(settings: ClickhouseSettings) -> ClickhouseState {
        ClickhouseState::new(settings)
    }
}

impl std::fmt::Display for ClickhouseSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "tcp://{}:{}@{}:{}/default?compression=lz4",
            self.user, self.password, self.host, self.port
        )
    }
}

pub struct ClickhouseState {
    pub settings: ClickhouseSettings,
    pub pool: Arc<Pool>,
    pub buffer_sender: mpsc::Sender<PersistFlowMessageWithMetadata>,
}

impl ClickhouseState {
    pub fn new(settings: ClickhouseSettings) -> Self {
        let dsn = settings.to_string();

        let pool = Arc::new(Pool::new(dsn));
        // let buffer = Arc::new(Mutex::new(Vec::with_capacity(STORAGE_BUFFER_SIZE)));
        let (buffer_sender, _buffer_recv) =
            mpsc::channel::<PersistFlowMessageWithMetadata>(STORAGE_BUFFER_SIZE);
        Self {
            settings,
            pool,
            buffer_sender,
        }
    }

    async fn stash(&self, msgs: &Vec<PersistFlowMessageWithMetadata>) -> Result<(), StorageError> {
        let mut client = self
            .pool
            .as_ref()
            .get_handle()
            .await
            .map_err(|e| StorageError::Database(Box::new(e)))?;

        let mut block = Block::with_capacity(msgs.len());
        for f in msgs {
            block.push(row! {
               host: f.host.as_str(),
               out_bytes: f.flow_message.out_bytes,
               out_pkts:   f.flow_message.out_pkts,
               in_bytes:   f.flow_message.in_bytes,
               in_pkts:    f.flow_message.in_pkts,
               ipv4_src_addr: f.flow_message.ipv4_src_addr.as_str(),
               ipv4_dst_addr: f.flow_message.ipv4_dst_addr.as_str(),
               l7_proto:       f.flow_message.l7_proto,
               l4_dst_port:    f.flow_message.l4_dst_port,
               l4_src_port:    f.flow_message.l4_src_port,
               flow_duration_milliseconds: f.flow_message.flow_duration_milliseconds,
               protocol:       f.flow_message.protocol,
               tcp_flags:      f.flow_message.tcp_flags,
               timestamp:      f.timestamp
            })?;
        }

        info!("Saving to clickhouse {} messages", msgs.len());

        client.insert("messages", block).await?;
        Ok(())
    }
}

#[async_trait]
impl AStorage for ClickhouseState {
    async fn stash_on_buffer(&self, msg: PersistFlowMessageWithMetadata) {
        info!("Sending message to stash buffer");
        self.buffer_sender.send(msg).await;
    }

    async fn flush_buffer(&self, mut buffer_recv: mpsc::Receiver<PersistFlowMessageWithMetadata>) {
        let mut buffer: Vec<PersistFlowMessageWithMetadata> =
            Vec::with_capacity(STORAGE_BUFFER_SIZE);

        while let Some(msg) = buffer_recv.recv().await {
            debug!("Got msg: {:?}", msg);
            buffer.push(msg);
            if buffer.len() == STORAGE_BUFFER_SIZE {
                match self.stash(&buffer).await {
                    Ok(_) => {
                        // TODO should pass those messages to acking actor
                        debug!("Saved messages");
                        buffer.clear();
                    }
                    Err(e) => {
                        debug!("Failed to save messages: {:?}", e);
                        // TODO should pass those messages to nacking actor
                        buffer.clear();
                    }
                }
            }
        }
    }

    // async fn stash(&self, msgs: &Vec<FlowMessageWithMetadata>) -> Result<(), StorageError> {
    //     let mut client = self
    //         .pool
    //         .as_ref()
    //         .get_handle()
    //         .await
    //         .map_err(|e| StorageError::Database(Box::new(e)))?;

    //     let mut block = Block::with_capacity(msgs.len());
    //     for f in msgs {
    //         block.push(row! {
    //            host: f.host.as_str(),
    //            out_bytes: f.flow_message.out_bytes,
    //            out_pkts:   f.flow_message.out_pkts,
    //            in_bytes:   f.flow_message.in_bytes,
    //            in_pkts:    f.flow_message.in_pkts,
    //            ipv4_src_addr: f.flow_message.ipv4_src_addr.as_str(),
    //            ipv4_dst_addr: f.flow_message.ipv4_dst_addr.as_str(),
    //            l7_proto:       f.flow_message.l7_proto,
    //            l4_dst_port:    f.flow_message.l4_dst_port,
    //            l4_src_port:    f.flow_message.l4_src_port,
    //            flow_duration_milliseconds: f.flow_message.flow_duration_milliseconds,
    //            protocol:       f.flow_message.protocol,
    //            tcp_flags:      f.flow_message.tcp_flags,
    //            timestamp:      f.timestamp
    //         })?;
    //     }

    //     info!("Saving to clickhouse {} messages", msgs.len());

    //     client.insert("messages", block).await?;
    //     Ok(())
    // }
}
