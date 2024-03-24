use std::{
    collections::{HashMap, HashSet},
    mem,
    pin::pin,
};

use futures::{
    channel::mpsc, stream, FutureExt, Sink, SinkExt, Stream, StreamExt, TryFutureExt, TryStreamExt,
};
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube_client::{api::WatchParams, discovery, Api, Client, Discovery};
use kube_core::ApiResource;

use super::{Event, GroupResource};

pub fn types(
    client: Client,
) -> impl Stream<Item = Result<Event<GroupResource, ApiResource>, Error>> {
    async fn flush_groups<E>(
        discovery: &Discovery,
        delivered: &mut HashSet<GroupResource>,
        mut sink: impl Sink<Result<Event<GroupResource, ApiResource>, Error>, Error = E> + Unpin,
    ) -> Result<(), E> {
        let mut remaining = mem::take(delivered);
        let mut additions = Vec::new();

        for group in discovery.groups() {
            for (resource, _capab) in group.recommended_resources() {
                let gr = GroupResource::from_api_resource(&resource);
                if !remaining.remove(&gr) {
                    additions.push(Event {
                        key: gr.clone(),
                        entry: Some(resource),
                    });
                }
                delivered.insert(gr);
            }
        }

        sink.send_all(&mut stream::iter(additions.into_iter().map(Ok).map(Ok)))
            .await?;
        sink.send_all(&mut stream::iter(remaining.into_iter().map(|gr| {
            Ok(Ok(Event {
                key: gr,
                entry: None,
            }))
        })))
        .await?;

        Ok(())
    }

    async fn run<E>(
        client: Client,
        mut sink: impl Sink<Result<Event<GroupResource, ApiResource>, Error>, Error = E> + Unpin,
    ) -> Result<Result<(), E>, Error> {
        let crd_client = Api::<CustomResourceDefinition>::all(client.clone());
        let mut discovery = Discovery::new(client)
            .run()
            .await
            .map_err(Error::Discovery)?;
        let mut delivered = HashSet::new();

        if let Err(err) = flush_groups(&discovery, &mut delivered, &mut sink).await {
            return Ok(Err(err));
        }

        let watcher = crd_client
            .watch(&WatchParams::default(), "0")
            .await
            .map_err(Error::Watch)?;
        let mut watcher = Box::pin(watcher);
        while let Some(_event) = watcher.try_next().await.map_err(Error::Watch)? {
            discovery = discovery.run().await.map_err(Error::Discovery)?;

            if let Err(err) = flush_groups(&discovery, &mut delivered, &mut sink).await {
                return Ok(Err(err));
            }
        }

        Ok(Ok(()))
    }

    let (mut send, recv) = mpsc::unbounded::<Result<Event<GroupResource, ApiResource>, Error>>();

    tokio::spawn(async move {
        let send_err = match run(client, &mut send).await {
            Ok(send_result) => send_result,
            Err(err) => send.send(Err(err)).await,
        };
        drop(send_err); // the subscriber is no longer reqiured.
    });

    recv
}

/// The error type returned by [`types`].
pub enum Error {
    Discovery(kube_client::Error),
    Watch(kube_client::Error),
}
