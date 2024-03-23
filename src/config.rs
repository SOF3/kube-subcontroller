use std::{hash::Hash, marker::PhantomData, panic};

use futures::{Future, Stream};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Parameters for [`run`].
pub trait Config {
    /// The type of key used to map an event to its subcontroller set.
    type TriggerKey: Sized + Clone + Eq + Hash + 'static;
    /// The data associated with an event, which can be resolved into parameters for the subcontroller set.
    type Entry: Sized + 'static;
    /// Critical errors during [`subscribe`](Self::subscribe).
    type SubscribeErr: Sized + 'static;

    /// Subscribe to events indicating subcontroller updates.
    fn subscribe(
        &self,
    ) -> impl Stream<Item = Result<Event<Self::TriggerKey, Self::Entry>, Self::SubscribeErr>> + Unpin;

    /// The set of subcontrollers
    fn subcontrollers(&self) -> Vec<Box<dyn Subcontroller<Self::TriggerKey, Self::Entry>>>;
}

mod subcontroller;
pub use subcontroller::*;

use crate::subscriber::Event;

type SubcontrollerCtor<TriggerKey, Entry> = dyn Fn() -> Box<dyn Subcontroller<TriggerKey, Entry>>;

pub struct Builder<TriggerKey, Entry, StreamErr, Subscriber, SubscriberFn>
where
    Subscriber: Stream<Item = Result<Event<TriggerKey, Entry>, StreamErr>>,
    SubscriberFn: Fn() -> Subscriber,
{
    subscriber: SubscriberFn,
    subcontrollers: Vec<Box<SubcontrollerCtor<TriggerKey, Entry>>>,
}

impl<TriggerKey, Entry, StreamErr, Subscriber, SubscriberFn>
    Builder<TriggerKey, Entry, StreamErr, Subscriber, SubscriberFn>
where
    Subscriber: Stream<Item = Result<Event<TriggerKey, Entry>, StreamErr>>,
    SubscriberFn: Fn() -> Subscriber,
{
    /// Adds a subcontroller to this config.
    pub fn with(
        mut self,
        ctor: impl Fn() -> Box<dyn Subcontroller<TriggerKey, Entry>> + 'static,
    ) -> Self {
        self.subcontrollers.push(Box::new(ctor));
        self
    }
}

impl<TriggerKey, Entry, StreamErr, Subscriber, SubscriberFn> Config
    for Builder<TriggerKey, Entry, StreamErr, Subscriber, SubscriberFn>
where
    TriggerKey: Clone + Eq + Hash + 'static,
    Entry: 'static,
    StreamErr: 'static,
    Subscriber: Stream<Item = Result<Event<TriggerKey, Entry>, StreamErr>> + Unpin,
    SubscriberFn: Fn() -> Subscriber,
{
    type TriggerKey = TriggerKey;
    type Entry = Entry;
    type SubscribeErr = StreamErr;

    fn subscribe(
        &self,
    ) -> impl Stream<Item = Result<Event<Self::TriggerKey, Self::Entry>, Self::SubscribeErr>> + Unpin
    {
        (self.subscriber)()
    }

    fn subcontrollers(&self) -> Vec<Box<dyn Subcontroller<Self::TriggerKey, Self::Entry>>> {
        let subcontrollers: Vec<_> = self.subcontrollers.iter().map(|ctor| ctor()).collect();
        subcontrollers
    }
}

pub fn on<TriggerKey, Entry, StreamErr, Subscriber, SubscriberFn>(
    subscriber: SubscriberFn,
) -> Builder<TriggerKey, Entry, StreamErr, Subscriber, SubscriberFn>
where
    Subscriber: Stream<Item = Result<Event<TriggerKey, Entry>, StreamErr>>,
    SubscriberFn: Fn() -> Subscriber,
{
    Builder {
        subscriber,
        subcontrollers: Vec::new(),
    }
}
