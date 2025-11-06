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
use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use anyhow::{Result, anyhow};
use zenoh::{
    Session,
    key_expr::{
        OwnedKeyExpr,
        format::{kedefine, keformat},
    },
    pubsub::Subscriber,
    sample::SampleKind,
};

kedefine!(
    // There is no similar issue liveliness token, because `/` is transformed into `%` in the key expression.
    pub(crate) ke_graphcache: "@ros2_lv/${domain:*}/${zid:*}/${node:*}/${entity:*}/${entity_kind:*}/${enclave:*}/${namespace:*}/${node_name:*}/${topic:*}/${rostype:*}/${hash:*}/${qos:*}",
);

pub(crate) type HashSetKeyExprs = Arc<RwLock<HashSet<OwnedKeyExpr>>>;

pub(crate) struct KeyExprMonitor {
    liveliness_subscriber: Option<Subscriber<()>>,
    hashset_key_exprs: HashSetKeyExprs,
}

impl KeyExprMonitor {
    pub(crate) fn new() -> Self {
        Self {
            liveliness_subscriber: None,
            hashset_key_exprs: Arc::new(RwLock::new(HashSet::new())),
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
        let hashset_key_exprs = self.hashset_key_exprs.clone();
        let liveliness_subscriber = session
            .liveliness()
            .declare_subscriber(&key_expr)
            .history(true)
            .callback(move |sample| {
                tracing::trace!(
                    "Received liveliness token: kind='{}', key_expr='{}'",
                    sample.kind(),
                    sample.key_expr(),
                );
                // Update the hashset of key_exprs
                match sample.kind() {
                    SampleKind::Put => {
                        hashset_key_exprs
                            .clone()
                            .write()
                            .unwrap()
                            .insert(OwnedKeyExpr::from(sample.key_expr().clone()));
                    }
                    SampleKind::Delete => {
                        hashset_key_exprs
                            .clone()
                            .write()
                            .unwrap()
                            .remove(&OwnedKeyExpr::from(sample.key_expr().clone()));
                    }
                }
            })
            .await
            .map_err(|e| anyhow!("Unable to declare the liveliness_subscriber: {e}"))?;
        self.liveliness_subscriber = Some(liveliness_subscriber);
        Ok(())
    }

    pub(crate) fn get_hashset_key_exprs(&self) -> HashSetKeyExprs {
        self.hashset_key_exprs.clone()
    }
}
