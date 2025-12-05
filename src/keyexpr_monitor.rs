//
// Copyright (c) 2025 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: Apache-2.0
//
// Contributors:
//   ChenYing Kuo, <cy@zettascale.tech>
//
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use anyhow::{Result, anyhow};
use tokio::sync::{Notify, futures::Notified};
use zenoh::{
    Session,
    key_expr::{
        OwnedKeyExpr,
        format::{kedefine, keformat},
    },
    pubsub::Subscriber,
    sample::SampleKind,
};

use crate::utils;

kedefine!(
    // There is no similar issue liveliness token, because `/` is transformed into `%` in the key expression.
    pub(crate) ke_graphcache: "@ros2_lv/${domain:*}/${zid:*}/${node:*}/${entity:*}/${entity_kind:*}/${enclave:*}/${namespace:*}/${node_name:*}/${topic:*}/${rostype:*}/${hash:*}/${qos:*}",
);

#[derive(Debug, Clone)]
pub(crate) struct KeyExprInfo {
    pub(crate) original_topic: String,
    pub(crate) domain: u32,
    pub(crate) enclave: String,
    pub(crate) namespace: String,
    pub(crate) topic: String,
    pub(crate) rostype: String,
    pub(crate) hash: String,
    pub(crate) qos: String,
}

pub(crate) struct HashMapKeyExprs {
    storage_key_exprs: Arc<RwLock<HashMap<OwnedKeyExpr, KeyExprInfo>>>,
    notify: Arc<Notify>,
}

impl HashMapKeyExprs {
    pub(crate) fn new() -> Self {
        Self {
            storage_key_exprs: Arc::new(RwLock::new(HashMap::new())),
            notify: Arc::new(Notify::new()),
        }
    }

    pub(crate) fn insert(&self, key_expr: OwnedKeyExpr) {
        if let Ok(ke) = utils::ke_graphcache::parse(&key_expr) {
            let domain = if let Ok(domain) = ke.domain().to_string().parse::<u32>() {
                domain
            } else {
                tracing::warn!("Something wrong when parsing the domain: {}", ke.domain());
                return;
            };

            // Transform the topic name from % to /, e.g. %camera%image_raw -> /camera/image_raw
            let original_topic = ke.topic().replace("%", "/").to_string();
            let keyexpr_info = KeyExprInfo {
                original_topic,
                domain,
                enclave: ke.enclave().to_string(),
                namespace: ke.namespace().to_string(),
                topic: ke.topic().to_string(),
                rostype: ke.rostype().to_string(),
                hash: ke.hash().to_string(),
                qos: ke.qos().to_string(),
            };
            tracing::trace!("Parse the received liveliness token: keyexpr_info={keyexpr_info:?}",);

            self.storage_key_exprs
                .write()
                .unwrap()
                .insert(key_expr, keyexpr_info);
            self.notify.notify_one();
        } else {
            tracing::warn!(
                "Something wrong when parsing the liveliness token name: {}",
                key_expr
            );
        }
    }

    pub(crate) fn remove(&self, key_expr: &OwnedKeyExpr) {
        self.storage_key_exprs.write().unwrap().remove(key_expr);
        self.notify.notify_one();
    }

    pub(crate) fn to_vec(&self) -> Vec<(OwnedKeyExpr, KeyExprInfo)> {
        self.storage_key_exprs
            .read()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    pub(crate) fn notified(&self) -> Notified<'_> {
        self.notify.notified()
    }
}

pub(crate) type StorageKeyExprs = Arc<HashMapKeyExprs>;

pub(crate) struct KeyExprMonitor {
    liveliness_subscriber: Option<Subscriber<()>>,
    storage_key_exprs: StorageKeyExprs,
}

impl KeyExprMonitor {
    pub(crate) fn new() -> Self {
        Self {
            liveliness_subscriber: None,
            storage_key_exprs: Arc::new(HashMapKeyExprs::new()),
        }
    }

    pub(crate) async fn start(&mut self, session: Session) -> Result<()> {
        // Subscribe to the liveliness
        let key_expr = keformat!(
            ke_graphcache::formatter(),
            domain = "*",
            zid = "*",
            node = "*",
            entity = "*",
            entity_kind = "MP",
            enclave = "*",
            namespace = "*",
            node_name = "*",
            topic = "*",
            rostype = "*",
            hash = "*",
            qos = "*",
        )
        .map_err(|e| anyhow!("Unable to format the key expression: {e}"))?;
        tracing::debug!("Subscribing to liveliness key expression: {}", key_expr);
        let storage_key_exprs = self.storage_key_exprs.clone();
        let liveliness_subscriber = session
            .liveliness()
            .declare_subscriber(&key_expr)
            .history(true)
            .callback(move |sample| {
                tracing::debug!(
                    "Received liveliness token: kind='{}', key_expr='{}'",
                    sample.kind(),
                    sample.key_expr(),
                );
                // Update the hashset of key_exprs
                match sample.kind() {
                    SampleKind::Put => {
                        storage_key_exprs.insert(OwnedKeyExpr::from(sample.key_expr().clone()));
                    }
                    SampleKind::Delete => {
                        storage_key_exprs.remove(&OwnedKeyExpr::from(sample.key_expr().clone()));
                    }
                }
            })
            .await
            .map_err(|e| anyhow!("Unable to declare the liveliness_subscriber: {e}"))?;
        self.liveliness_subscriber = Some(liveliness_subscriber);
        Ok(())
    }

    pub(crate) fn get_hashset_key_exprs(&self) -> StorageKeyExprs {
        self.storage_key_exprs.clone()
    }
}
