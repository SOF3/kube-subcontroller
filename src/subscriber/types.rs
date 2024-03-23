use futures::{stream, FutureExt, Stream, StreamExt, TryFutureExt, TryStreamExt};
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube_client::{api::WatchParams, discovery, Api, Client, Discovery};
use kube_core::ApiResource;

use super::GroupResource;

/// Subscriber for starting a subcontroller for each GVR.
pub fn types(client: Client) -> impl Stream<Item = Result<(GroupResource, ApiResource), Error>> {
    let crd_client = Api::<CustomResourceDefinition>::all(client.clone());

    fn flatten_groups<'a>(
        groups: impl Iterator<Item = &'a discovery::ApiGroup> + 'a,
    ) -> Vec<(GroupResource, ApiResource)> {
        groups
            .flat_map(|group| {
                group
                    .recommended_resources()
                    .into_iter()
                    .map(|(resource, _capab)| {
                        (GroupResource::from_api_resource(&resource), resource)
                    })
            })
            .collect()
    }

    async { Discovery::new(client).run().await.map_err(Error::Discovery) }
        .and_then(|mut discovery| async move {
            let resources = flatten_groups(discovery.groups());
            Ok(stream::iter(resources).map(Ok).chain(
                stream::repeat_with(move || {
                    crd_client
                        .watch(&WatchParams::default(), "0")
                        .map_err(Error::Watch)
                })
                .then(|watcher| watcher.map_ok(|watch| watch.map_err(Error::Watch)))
                .try_flatten() // Stream<Item = Result<WatchEvent, Error>>
                .and_then(|_watch_event| async move {
                    discovery = discovery.run().await.map_err(Error::Discovery)?;
                    let resources = flatten_groups(discovery.groups());
                    Ok(stream::iter(resources).map(Ok))
                })
                .try_flatten(),
            ))
        })
        .into_stream()
        .try_flatten()
}

/// The error type returned by [`types`].
pub enum Error {
    Discovery(kube_client::Error),
    Watch(kube_client::Error),
}
