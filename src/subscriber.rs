//! Subscribers to construct `Config` with.

use std::{fmt::Debug, hash::Hash};

use futures::{stream, FutureExt, Stream, StreamExt, TryFutureExt, TryStreamExt};
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube_client::{discovery, Api, Client, Discovery};
use kube_core::{params::WatchParams, ApiResource, Resource};
use kube_runtime::{reflector, watcher, WatchStreamExt};
use serde::de::DeserializeOwned;

/// Uniquely identifies an object of a known type by namespace and name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QualifiedName {
    /// The object namespace, if any
    pub namespace: Option<String>,
    /// The object name
    pub name: String,
}

impl QualifiedName {
    pub fn from_resource<K: Resource>(resource: &K) -> Self {
        Self {
            namespace: resource.meta().namespace.clone(),
            name: resource.meta().name.clone().unwrap(),
        }
    }
}

pub struct Event<Key, Entry> {
    pub key: Key,
    pub entry: Entry,
    pub exists: bool,
}

pub mod objects;
pub use objects::objects;

/// Uniquely identifies a type of resources in a cluster.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GroupResource {
    /// The group of the resource type, or empty string for the core group.
    pub group: String,
    /// The plural name of the resource type.
    pub resource: String,
}

impl GroupResource {
    pub fn from_api_resource(resource: &ApiResource) -> Self {
        Self {
            group: resource.group.clone(),
            resource: resource.plural.clone(),
        }
    }
}

pub mod types;
pub use types::types;
