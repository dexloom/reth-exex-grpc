use std::collections::{BTreeMap, HashMap};
use std::convert::Infallible;
use std::pin::Pin;

use alloy::primitives::{Address, BlockHash, U256};
use alloy::rpc;
use alloy::rpc::types::BlockTransactionsKind;
use alloy::rpc::types::trace::geth::AccountState;
use async_stream::stream;
use eyre::{eyre, Result};
use futures_util::pin_mut;
use reth::primitives::{Header, TransactionSigned, TxHash};
use reth::revm::db::{BundleAccount, BundleState, StorageWithOriginalValues};
use reth::revm::db::states::StorageSlot;
use reth_exex::ExExNotification;
use reth_primitives::{SealedBlock, SealedBlockWithSenders};
use reth_tracing::tracing::error;
use tokio_stream::Stream;
use tonic::transport::Channel;

use crate::helpers::append_all_matching_block_logs_sealed;
use crate::proto::{Block, SubscribeRequest};
use crate::proto::remote_ex_ex_client::RemoteExExClient;

pub struct ExExClient {
    client: RemoteExExClient<Channel>,
}

impl ExExClient {
    pub async fn connect(url: String) -> eyre::Result<ExExClient> {
        let client = RemoteExExClient::connect(url).await?;
        Ok(ExExClient {
            client
        })
    }

    pub async fn subscribe_mempool_tx(&self) -> Result<impl Stream<Item=alloy::rpc::types::eth::Transaction> + '_> {
        let stream = self.client.clone().subscribe_mempool_tx(SubscribeRequest {}).await;
        let mut stream = match stream {
            Ok(stream) => {
                stream.into_inner()
            }
            Err(e) => {
                error!(error=?e, "subscribe header");
                return Err(eyre!("ERROR"));
            }
        };

        Ok(stream! {
            loop {
                match stream.message().await {
                    Ok(Some(transaction_proto)) => {
                        if let Ok(transaction) = TransactionSigned::try_from(&transaction_proto){
                            if let Some(transaction) = transaction.into_ecrecovered() {
                                let transaction = reth_rpc_types_compat::transaction::from_recovered(transaction);
                                yield transaction;
                            }
                        }
                    }
                    Ok(None) => break, // Stream has ended
                    Err(err) => {
                        eprintln!("Error receiving message: {:?}", err);
                        break;
                    }
                }
            }
        })
    }

    pub async fn subscribe_header(&self) -> Result<impl Stream<Item=alloy::rpc::types::Header> + '_> {
        let stream = self.client.clone()
            .subscribe_header(SubscribeRequest {})
            .await;

        let mut stream = match stream {
            Ok(stream) => {
                stream.into_inner()
            }
            Err(e) => {
                error!(error=?e, "subscribe header");
                return Err(eyre!("ERROR"));
            }
        };
        Ok(stream! {
            loop {
                match stream.message().await {
                    Ok(Some(notification)) => {
                        if let Some(header) = notification.header {
                            match ( Header::try_from(&header), BlockHash::try_from(notification.hash.as_slice()) ) {
                                ( Ok(header), Ok(hash) )=> {
                                    let sealed_header = reth::primitives::SealedHeader::new(
                                        header,
                                        hash
                                    );
                                    let header = reth_rpc_types_compat::block::from_primitive_with_hash(sealed_header);
                                    yield header;
                                }
                                _=>{}
                            }
                        }
                    },
                    Ok(None) => break, // Stream has ended
                    Err(err) => {
                        eprintln!("Error receiving message: {:?}", err);
                        break;
                    }
                }
            }
        })
    }


    pub async fn subscribe_block(&self) -> Result<impl Stream<Item=alloy::rpc::types::Block>> {
        let stream = self.client.clone()
            .subscribe_block(SubscribeRequest {})
            .await;

        let mut stream = match stream {
            Ok(stream) => {
                stream.into_inner()
            }
            Err(e) => {
                error!(error=?e, "subscribe header");
                return Err(eyre!("ERROR"));
            }
        };

        Ok(stream! {
            loop {
                match stream.message().await {
                    Ok(Some(block_msg)) => {
                        if let Ok(sealed_block)  = SealedBlockWithSenders::try_from(&block_msg) {
                            let diff = sealed_block.difficulty;
                            let hash = sealed_block.hash();

                            if let Ok(block) = reth_rpc_types_compat::block::from_block(
                                sealed_block.unseal(),
                                diff,
                                BlockTransactionsKind::Full,
                                Some(hash))
                            {
                                yield block
                            }
                        }
                    },
                    Ok(None) => break, // Stream has ended
                    Err(err) => {
                        eprintln!("Error receiving message: {:?}", err);
                        break;
                    }
                }
            }
        })
    }
    pub async fn subscribe_logs(&self) -> Result<impl Stream<Item=(BlockHash, Vec<alloy::rpc::types::Log>)>> {
        let stream = self.client.clone()
            .subscribe_receipts(SubscribeRequest {})
            .await;

        let mut stream = match stream {
            Ok(stream) => {
                stream.into_inner()
            }
            Err(e) => {
                error!(error=?e, "subscribe receipts");
                return Err(eyre!("ERROR"));
            }
        };
        Ok(stream! {
            loop {
                match stream.message().await {
                    Ok(Some(notification)) => {
                        if let Some(receipts) = notification.receipts {
                            if let Some(sealed_block) = notification.block {
                                if let Ok((block_hash, logvec)) = append_all_matching_block_logs_sealed(
                                    receipts,
                                    false,
                                    sealed_block,
                                ){
                                    yield (block_hash, logvec);
                                }
                            }
                        }

                    },
                    Ok(None) => break, // Stream has ended
                    Err(err) => {
                        eprintln!("Error receiving message: {:?}", err);
                        break;
                    }
                }
            }
        })
    }

    pub async fn subscribe_stata_update(&self) -> Result<impl Stream<Item=(BlockHash, BTreeMap<Address, AccountState>)>> {
        let stream = self.client.clone()
            .subscribe_state_update(SubscribeRequest {})
            .await;

        let mut stream = match stream {
            Ok(stream) => {
                stream.into_inner()
            }
            Err(e) => {
                error!(error=?e, "subscribe receipts");
                return Err(eyre!("ERROR"));
            }
        };
        Ok(stream! {
            loop {
                match stream.message().await {
                    Ok(Some(state_update)) => {
                        if let Ok(block_hash) = TxHash::try_from(state_update.hash.as_slice()) {
                            if let Some(bundle_proto) = state_update.bundle {

                                if let Ok(bundle_state) = reth::revm::db::BundleState::try_from(&bundle_proto){
                                    let mut state_update : BTreeMap<Address, AccountState> = BTreeMap::new();

                                    let state_ref: &HashMap<Address, BundleAccount> = &bundle_state.state;

                                    for (address, accounts) in state_ref.iter() {
                                        let mut account_state = state_update.entry(*address).or_default();
                                        if let Some(account_info) = accounts.info.clone() {
                                            account_state.code = account_info.code.map(|c| c.bytecode().clone());
                                            account_state.balance = Some(account_info.balance);
                                            account_state.nonce = Some(account_info.nonce);
                                        }

                                        let storage: &StorageWithOriginalValues = &accounts.storage;

                                        for (key, storage_slot) in storage.iter() {
                                            let (key, storage_slot): (&U256, &StorageSlot) = (key, storage_slot);
                                            account_state
                                                .storage
                                                .insert((*key).into(), storage_slot.present_value.into());
                                        }
                                    }
                                    yield (block_hash, state_update);
                                }
                            }

                        }
                    },
                    Ok(None) => break, // Stream has ended
                    Err(err) => {
                        eprintln!("Error receiving message: {:?}", err);
                        break;
                    }
                }
            }
        })
    }

    pub async fn subscribe_exex(&self) -> Result<impl Stream<Item=ExExNotification> + '_> {
        let mut stream = self.client.clone()
            .subscribe_ex_ex(SubscribeRequest {})
            .await;

        let mut stream = match stream {
            Ok(stream) => {
                stream.into_inner()
            }
            Err(e) => {
                error!(error=?e, "subscribe exex");
                return Err(eyre!("ERROR"));
            }
        };

        Ok(stream! {


            loop {
                match stream.message().await {
                    Ok(Some(notification)) => {
                            match ExExNotification::try_from(&notification) {
                                Ok(notification) => yield notification,
                                Err(err) => eprintln!("Error converting notification: {:?}", err),
                            }
                        },
                    Ok(None) => break, // Stream has ended
                    Err(err) => {
                        eprintln!("Error receiving message: {:?}", err);
                        break;
                    }
                }
            }
        })
    }
}

