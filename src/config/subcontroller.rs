use std::fmt;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::{marker::PhantomData, panic};

use std::future::Future;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub trait Subcontroller<Key, Entry> {
    fn start(&mut self, key: Key, entry: &Entry);

    fn stop(&mut self, key: Key);
}

pub struct SubcontrollerBuilder<Key, Entry, Extracted, Run, If, Extract> {
    run: Arc<Run>,
    if_fn: If,
    extract: Extract,
    current_task: Option<CancellableTask<()>>,
    next_task: AtomicU64,
    _ph: PhantomData<(Key, Entry, Extracted)>,
}

pub fn subcontroller<Key, Entry, Extracted, Run>(
    run: Run,
) -> SubcontrollerBuilder<Key, Entry, Extracted, Run, DefaultIfFn, DefaultExtractFn> {
    SubcontrollerBuilder {
        run: Arc::new(run),
        if_fn: DefaultIfFn,
        extract: DefaultExtractFn,
        current_task: None,
        next_task: AtomicU64::new(0),
        _ph: PhantomData,
    }
}

impl<Key, Entry, Extracted, Fut, Run, If, Extract, ControllerErr>
    SubcontrollerBuilder<Key, Entry, Extracted, Run, If, Extract>
where
    Key: Send + Clone + 'static,
    Entry: Send,
    Fut: Future<Output = Result<(), ControllerErr>> + Send + 'static,
    Run: Send + Sync + Fn(CancellationToken, Key, Extract::Extracted) -> Fut + 'static,
    If: Send + IfFn<Key, Entry>,
    Extract: Send + ExtractFn<Key, Entry, Extracted = Extracted>,
    Extracted: Send + 'static,
    ControllerErr: Send + fmt::Debug,
{
    fn set_task(&mut self, key: Key, entry: Option<&Entry>) {
        let prev = self.current_task.take();
        let cancel = CancellationToken::new();

        let extracted = entry
            .filter(|entry| self.if_fn.call(&key, entry))
            .map(|entry| self.extract.call(key.clone(), entry));

        let fut = {
            let cancel = cancel.clone();
            let run = Arc::clone(&self.run);

            async move {
                if let Some(extracted) = extracted {
                    run(cancel, key, extracted).await
                } else {
                    Ok(())
                }
            }
        };

        let handle = tokio::spawn({
            let cancel = cancel.clone();
            async move {
                if let Some(prev) = prev {
                    prev.cancel().await;
                }

                if cancel.is_cancelled() {
                    return;
                }

                if let Err(err) = fut.await {
                    // TODO expose error properly
                    log::error!("Subcontrller failed with error: {err:?}");
                }
            }
        });
        self.current_task = Some(CancellableTask {
            join_handle: handle,
            token: cancel,
        });
    }
}

impl<Key, Entry, Extracted, Fut, Run, If, Extract, ControllerErr> Subcontroller<Key, Entry>
    for SubcontrollerBuilder<Key, Entry, Extracted, Run, If, Extract>
where
    Key: Send + Clone + 'static,
    Entry: Send,
    Fut: Future<Output = Result<(), ControllerErr>> + Send + 'static,
    Run: Send + Sync + Fn(CancellationToken, Key, Extract::Extracted) -> Fut + 'static,
    If: Send + IfFn<Key, Entry>,
    Extract: Send + ExtractFn<Key, Entry, Extracted = Extracted>,
    Extracted: Send + 'static,
    ControllerErr: Send + fmt::Debug,
{
    fn start(&mut self, key: Key, entry: &Entry) {
        self.set_task(key, Some(entry));
    }

    fn stop(&mut self, key: Key) {
        self.set_task(key, None);
    }
}

impl<Key, Entry, Extracted, Run, Extract>
    SubcontrollerBuilder<Key, Entry, Extracted, Run, DefaultIfFn, Extract>
{
    pub fn if_<F: Fn(&Key, &Entry) -> bool>(
        self,
        f: F,
    ) -> SubcontrollerBuilder<Key, Entry, Extracted, Run, impl IfFn<Key, Entry>, Extract> {
        SubcontrollerBuilder {
            run: self.run,
            if_fn: AssignedIfFn(f),
            extract: self.extract,
            current_task: None,
            next_task: self.next_task,
            _ph: PhantomData,
        }
    }
}

pub trait IfFn<Key, Entry> {
    fn call(&self, key: &Key, entry: &Entry) -> bool;
}

pub struct DefaultIfFn;
impl<Key, Entry> IfFn<Key, Entry> for DefaultIfFn {
    fn call(&self, _key: &Key, _entry: &Entry) -> bool {
        true
    }
}

struct AssignedIfFn<F>(F);
impl<Key, Entry, F: Fn(&Key, &Entry) -> bool> IfFn<Key, Entry> for AssignedIfFn<F> {
    fn call(&self, key: &Key, entry: &Entry) -> bool {
        (self.0)(key, entry)
    }
}

impl<Key, Entry, Extracted, Fut, Run, If>
    SubcontrollerBuilder<Key, Entry, Extracted, Run, If, DefaultExtractFn>
where
    Run: Fn(Key, Extracted) -> Fut,
    If: IfFn<Key, Entry>,
{
    pub fn using<F>(
        self,
        f: F,
    ) -> SubcontrollerBuilder<
        Key,
        Entry,
        Extracted,
        Run,
        If,
        impl ExtractFn<Key, Entry, Extracted = Extracted>,
    >
    where
        F: Fn(Key, &Entry) -> Extracted,
    {
        SubcontrollerBuilder {
            run: self.run,
            if_fn: self.if_fn,
            extract: AssignedExtractFn(f),
            current_task: None,
            next_task: self.next_task,
            _ph: PhantomData,
        }
    }
}

pub trait ExtractFn<Key, Entry> {
    type Extracted: Sized;

    fn call(&self, key: Key, entry: &Entry) -> Self::Extracted;
}

pub struct DefaultExtractFn;
impl<Key, Entry: Clone> ExtractFn<Key, Entry> for DefaultExtractFn {
    type Extracted = Entry;

    fn call(&self, _key: Key, entry: &Entry) -> Self::Extracted {
        entry.clone()
    }
}

pub struct AssignedExtractFn<F>(F);
impl<Key, Entry, Extracted, F> ExtractFn<Key, Entry> for AssignedExtractFn<F>
where
    F: Fn(Key, &Entry) -> Extracted,
{
    type Extracted = Extracted;

    fn call(&self, key: Key, entry: &Entry) -> Self::Extracted {
        (self.0)(key, entry)
    }
}

struct CancellableTask<T> {
    join_handle: JoinHandle<T>,
    token: CancellationToken,
}

impl<T> CancellableTask<T> {
    async fn cancel(self) {
        self.token.cancel();
        if let Err(err) = self.join_handle.await {
            if let Ok(panic) = err.try_into_panic() {
                panic::resume_unwind(panic)
            }
        }
    }
}
