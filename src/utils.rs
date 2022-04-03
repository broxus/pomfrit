use std::collections::{hash_map, HashMap};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

pub fn trigger() -> (Trigger, TriggerReceiver) {
    let state = Arc::new(State {
        complete: AtomicBool::new(false),
        wakers: Default::default(),
        next_id: AtomicUsize::new(1),
    });
    (
        Trigger {
            state: state.clone(),
        },
        TriggerReceiver { id: 0, state },
    )
}

#[derive(Clone)]
pub struct Trigger {
    state: Arc<State>,
}

impl Trigger {
    pub fn trigger(&self) {
        if self.state.complete.swap(true, Ordering::AcqRel) {
            return;
        }

        let wakers = std::mem::take(&mut *self.state.wakers.lock());
        for waker in wakers.into_values() {
            waker.wake();
        }
    }
}

pub struct TriggerReceiver {
    id: usize,
    state: Arc<State>,
}

impl Drop for TriggerReceiver {
    fn drop(&mut self) {
        if !self.state.complete.load(Ordering::Acquire) {
            self.state.wakers.lock().remove(&self.id);
        }
    }
}

impl Clone for TriggerReceiver {
    fn clone(&self) -> Self {
        Self {
            id: self.state.next_id.fetch_add(1, Ordering::AcqRel),
            state: self.state.clone(),
        }
    }
}

impl Future for TriggerReceiver {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.state.complete.load(Ordering::Acquire) {
            return Poll::Ready(());
        }

        let mut wakers = self.state.wakers.lock();
        if self.state.complete.load(Ordering::Acquire) {
            Poll::Ready(())
        } else {
            match wakers.entry(self.id) {
                hash_map::Entry::Occupied(mut entry) => {
                    if !cx.waker().will_wake(entry.get()) {
                        entry.insert(cx.waker().clone());
                    }
                }
                hash_map::Entry::Vacant(entry) => {
                    entry.insert(cx.waker().clone());
                }
            };

            Poll::Pending
        }
    }
}

struct State {
    complete: AtomicBool,
    wakers: parking_lot::Mutex<HashMap<usize, Waker>>,
    next_id: AtomicUsize,
}

#[cfg(feature = "serde")]
pub mod serde_url {
    use std::str::FromStr;

    use hyper::http::uri::PathAndQuery;
    use serde::de::Error;
    use serde::Deserialize;

    pub fn serialize<S>(data: &PathAndQuery, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(data.as_str())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<PathAndQuery, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let data = String::deserialize(deserializer)?;
        let data = match data.as_bytes().first() {
            None => "/".to_owned(),
            Some(b'/') => data,
            Some(_) => format!("/{}", data),
        };
        PathAndQuery::from_str(&data).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use futures_test::task::new_count_waker;

    use super::*;

    #[test]
    fn correct_trigger_behaviour() {
        let (waker, wake_counter) = new_count_waker();

        let (trigger, receiver) = trigger();
        let mut receiver = receiver.clone();

        assert!(Pin::new(&mut receiver)
            .poll(&mut Context::from_waker(&waker))
            .is_pending());
        assert_eq!(wake_counter.get(), 0);

        trigger.trigger();
        assert!(Pin::new(&mut receiver)
            .poll(&mut Context::from_waker(&waker))
            .is_ready());
        assert_eq!(wake_counter.get(), 1);
    }

    #[test]
    fn correct_trigger_behaviour_with_multiple_wakes() {
        let (waker, wake_counter) = new_count_waker();

        let (trigger, receiver) = trigger();
        let mut receiver = receiver.clone();

        assert!(Pin::new(&mut receiver)
            .poll(&mut Context::from_waker(&waker))
            .is_pending());
        assert_eq!(wake_counter.get(), 0);

        assert!(Pin::new(&mut receiver)
            .poll(&mut Context::from_waker(&waker))
            .is_pending());
        assert_eq!(wake_counter.get(), 0);

        trigger.trigger();
        assert!(Pin::new(&mut receiver)
            .poll(&mut Context::from_waker(&waker))
            .is_ready());
        assert_eq!(wake_counter.get(), 1);
    }
}
