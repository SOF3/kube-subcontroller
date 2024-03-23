use std::{
    collections::{hash_map, HashMap},
    marker::PhantomData,
    mem,
    sync::Arc,
};

use futures::TryStreamExt;
use parking_lot::Mutex;

use crate::config::Subcontroller;

use super::config::Config;

pub async fn run<ConfigT: Config>(config: ConfigT) -> Result<(), RunError<ConfigT>> {
    // TODO delete handle from map when there is no active task
    let mut states = HashMap::<ConfigT::TriggerKey, Handle<ConfigT>>::new();

    let mut sub = config.subscribe();
    while let Some(event) = sub.try_next().await.map_err(RunError::Subscribe)? {
        if let Some(entry) = event.entry.as_ref() {
            match states.entry(event.key.clone()) {
                hash_map::Entry::Vacant(state) => {
                    let mut subcontrollers = config.subcontrollers();
                    for subcontroller in &mut subcontrollers {
                        subcontroller.start(event.key.clone(), entry);
                    }

                    state.insert(Handle {
                        subcontrollers,
                        _ph: PhantomData,
                    });
                }
                hash_map::Entry::Occupied(mut state) => {
                    let handle = state.get_mut();
                    for subcontroller in &mut handle.subcontrollers {
                        subcontroller.start(event.key.clone(), entry);
                    }
                }
            }
        } else if let Some(handle) = states.get_mut(&event.key) {
            for subcontroller in &mut handle.subcontrollers {
                subcontroller.stop(event.key.clone());
            }
        }
    }

    Ok(())
}

pub enum RunError<ConfigT: Config> {
    Subscribe(ConfigT::SubscribeErr),
}

struct Handle<ConfigT: Config> {
    subcontrollers: Vec<Box<dyn Subcontroller<ConfigT::TriggerKey, ConfigT::Entry>>>,
    _ph: PhantomData<ConfigT>,
}
