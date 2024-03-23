use std::{fmt::Debug, hash::Hash};

use futures::{stream, FutureExt, Stream, StreamExt, TryFutureExt, TryStreamExt};
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube_client::{discovery, Api, Client, Discovery};
use kube_core::{params::WatchParams, ApiResource, Resource};
use kube_runtime::{reflector, watcher, WatchStreamExt};
use serde::de::DeserializeOwned;

use super::{Event, QualifiedName};

/// Subscriber for starting a subcontroller for each object of the type `K`.
pub fn objects<K: Resource>(
    client: Api<K>,
    watcher_config: watcher::Config,
) -> impl Stream<Item = Result<Event<QualifiedName, K>, watcher::Error>>
where
    K: 'static + Debug + Clone + DeserializeOwned + Send,
    K::DynamicType: Clone + Eq + Hash + Default,
{
    with(client, watcher_config, <_>::default())
}

/// Subscriber for starting a subcontroller for each object of the type `dyntype`.
pub fn with<K: Resource>(
    client: Api<K>,
    watcher_config: watcher::Config,
    dyntype: K::DynamicType,
) -> impl Stream<Item = Result<Event<QualifiedName, K>, watcher::Error>>
where
    K: 'static + Debug + Clone + DeserializeOwned + Send,
    K::DynamicType: Clone + Eq + Hash,
{
    let writer = reflector::store::Writer::new(dyntype.clone());
    let store = writer.as_reader();

    reflector(writer, watcher(client, watcher_config))
        .touched_objects()
        .map_ok(move |resource| {
            let store_value = store.get(&reflector::ObjectRef::from_obj_with(
                &resource,
                dyntype.clone(),
            ));
            Event {
                key: QualifiedName::from_resource(&resource),
                entry: resource,
                exists: store_value.is_some(),
            }
        })
}
