use std::{convert::Infallible, marker::PhantomData, pin::Pin, time::Duration};

use crate::common::{TimedContents, self};

pub mod async_fn;

use async_fn::{AsyncClosure, AsyncFn};
use futures::future::BoxFuture;

pub type EverReady<T, E = Infallible> = futures::future::Ready<Result<T, E>>;

/// Async-compatible alternative of [`crate::unsync::Mnemosyne`].
///
/// Will not eagerly update, but rather recompute from the same async closure if polled after the internal
/// refresh period has elapsed.
#[allow(clippy::module_name_repetitions)]
pub struct MnemoSync<T, E = Infallible, F = AsyncFn<T, E, BoxFuture<'static, Result<T, E>>>>
where
    E: 'static,
    F: for<'a> AsyncClosure<'a, T, E>,
{
    value: Option<TimedContents<T>>,
    refresh_interval: Duration,
    update: F,
    _proxy: PhantomData<E>,
}

impl<T: Clone, E, F> MnemoSync<T, E, F>
where
    E: 'static,
    F: for<'a> AsyncClosure<'a, T, E> + Unpin,
{
    /// Construct a new [`MnemoSync`] from an optional initial value `init`, with the specified
    /// value-persistence duration (refresh interval) and (async) update-closure.
    ///
    /// The refresh interval is specified by the parameter `refresh_millis` as a number of milliseconds.
    /// If this value is zero, the stream will be polled every time the inner value is requested.
    pub fn new(init: Option<T>, refresh_millis: u64, update: F) -> Self {
        let refresh_interval = Duration::from_millis(refresh_millis);
        let value = init.map(|x| TimedContents::new(x));
        Self {
            value,
            refresh_interval,
            update,
            _proxy: PhantomData,
        }
    }

    /// Returns a copy of the current value held by this [`MnemoSync`], if one exists.
    ///
    /// Unlike [`poll`], this method will not interact with the chronometry of the received object,
    /// will never call the stored closure, and will not change the value stored internally. It is therefore
    /// idempotent, and should generally return the same value as the last successful call to `poll`, if one
    /// was made.
    ///
    /// If this [`MnemoSync<T, E>`] was not initialized and never successfully updated, this method will return
    /// `None`.
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main {
    /// use std::time::Duration;
    /// use mnemosyne::MnemoSync;
    /// use mnemosyne::async_fn;
    /// let mut mem : MnemoSync<i32> = MnemoSync::new(Some(42), 1000, async_fn![async { Ok(13) }]);
    ///
    /// // Test that `peek` returns the initial value when no updates have been made
    /// assert_eq!(mem.peek(), Some(42));
    ///
    /// // Wait longer than the refresh interval without calling `poll` and test that `peek` still returns the initial value
    /// std::thread::sleep(Duration::from_millis(2000));
    /// assert_eq!(mem.peek(), Some(42));
    ///
    /// // Call `poll_async` and check that `peek` returns the updated value
    /// mem.poll_async().await.unwrap();
    /// assert_eq!(mem.peek(), Some(13));
    /// # }
    /// ```
    pub fn peek(&self) -> Option<T> {
        self.value.as_ref().map(|v| v.get_contents().clone())
    }

    /// Polls the current value stored in this [`MnemoSync`] and updates it if it is determined to be out-of-date.
    ///
    /// This method calls the internal closure at most once per invocation and may not call it at all if the refresh interval has not yet elapsed.
    ///
    /// # Errors
    /// If the closure returns an `Err` value, this method returns an `Err` value without modifying the value stored in the [`MnemoSync`].
    /// If an error occurs, the immediate value that would have been overwritten upon a successful callback can be safely interrogated using the [`peek`] method.
    ///
    /// # Returns
    ///
    /// Returns a copy of the most up-to-date value held by this [`MnemoSync`], if one exists, or an `Err` value if the closure returns an error.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main {
    /// use std::time::Duration;
    /// use mnemosyne::sync::MnemoSync;
    ///
    /// let f = async_fn!(async { Ok(13) });
    /// let mut mnemosync : MnemoSync<i32> = MnemoSync::new(Some(42), 100, f);
    /// assert!(matches!(mnemosync.poll_async().await, Ok(42)));
    ///
    /// tokio::time::sleep(Duration::from_millis(200)).await;
    ///
    /// assert!(matches!(mnemosync.poll_async().await, Ok(1337)));
    /// # }
    /// ```
    ///
    /// [`peek`]: MnemoSync::peek
    pub async fn poll_async(&mut self) -> Result<T, E> {
        if let Some(ref mut container) = self.value {
            let elapsed = container.time_since_last_update();
            if elapsed >= self.refresh_interval {
                let f = Pin::new(&mut self.update);
                let res = f.call().await;
                match res {
                    Ok(new_contents) => {
                        container.replace_contents(new_contents.clone());
                        return Ok(new_contents);
                    }
                    Err(err) => {
                        return Err(err);
                    }
                }
            }
            return Ok(container.get_contents().clone());
        }
        let f = Pin::new(&mut self.update);
        let res = f.call().await;
        match res {
            Ok(contents) => {
                self.value = Some(TimedContents::new(contents.clone()));
                Ok(contents)
            }
            Err(e) => Err(e),
        }
    }

    pub fn time_since_last_update(&self) -> Option<std::time::Duration> {
        self.value.as_ref().map(common::TimedContents::time_since_last_update)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::atomic::{AtomicBool, AtomicU8, Ordering},
        time::Duration,
    };

    use futures::future::ready;

    use super::*;
    use crate::async_fn;

    #[tokio::test]
    async fn test_peek_empty() {
        type Fut = EverReady<i32>;
        type M = MnemoSync<i32, Infallible, AsyncFn<i32, Infallible, Fut>>;
        let mnemo: M = MnemoSync::new(None, 1000, async_fn!(|| ready(Ok(13))));
        assert_eq!(mnemo.peek(), None);
    }

    #[tokio::test]
    async fn test_peek_non_empty() {
        type Fut = EverReady<&'static str>;
        type M = MnemoSync<&'static str, Infallible, AsyncFn<&'static str, Infallible, Fut>>;
        let mut mnemo = M::new(Some("foo"), 1000, async_fn!(|| ready(Ok("bar"))));
        assert_eq!(mnemo.peek(), Some("foo"));
        std::thread::sleep(std::time::Duration::from_millis(2000));
        mnemo.poll_async().await.unwrap();
        assert_eq!(mnemo.peek(), Some("bar"));
    }

    #[tokio::test]
    async fn test_peek_same_as_last_poll() {
        struct Countdown {
            count: AtomicU8,
            _done: AtomicBool,
        }
        impl Countdown {
            fn new(count: u8) -> Self {
                Self {
                    count: AtomicU8::new(count),
                    _done: AtomicBool::new(false),
                }
            }
        }
        impl<'a> AsyncClosure<'a, u8, ()> for Countdown {
            type Fut = EverReady<u8, ()>;

            fn call<'c>(self: Pin<&'c mut Self>) -> Self::Fut
            where
                'c: 'a,
            {
                // Once self.count reaches 0, we should always return an error
                // self._done starts at false and is set to true once count reaches 0
                // regardless of what count is, if _done is true, we should return an error
                // otherwise, we decrement count and return the old value
                if self._done.load(Ordering::SeqCst) {
                    return ready(Err(()));
                }

                let count = self.count.fetch_sub(1, Ordering::SeqCst);
                if count == 0 {
                    self._done.store(true, Ordering::SeqCst);
                }
                ready(Ok(count))
            }
        }

        type M = MnemoSync<u8, (), Countdown>;
        let f = Countdown::new(3);
        let mut mnemo = M::new(None, 100, f);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(matches!(mnemo.poll_async().await, Ok(3)));
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(matches!(mnemo.poll_async().await, Ok(2)));
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(matches!(mnemo.poll_async().await, Ok(1)));
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(matches!(mnemo.poll_async().await, Err(..)));
        assert_eq!(mnemo.peek(), Some(1));
    }

    #[tokio::test]
    async fn test_poll() {
        let mut x = 0;
        let f = async_fn!(move || {
            let ret = x;
            x += 1;
            ready(Ok(ret))
        });
        type Fut = EverReady<i32>;
        type M = MnemoSync<i32, Infallible, AsyncFn<i32, Infallible, Fut>>;
        let mut mnem = M::new(Some(42), 2000, f);
        assert_eq!(mnem.peek(), Some(42));
        std::thread::sleep(std::time::Duration::from_millis(1000));
        assert!(matches!(mnem.poll_async().await, Ok(42)));
        std::thread::sleep(std::time::Duration::from_millis(2000));
        assert!(matches!(mnem.poll_async().await, Ok(0)));
        std::thread::sleep(std::time::Duration::from_millis(3000));
        assert!(matches!(mnem.poll_async().await, Ok(1)));
    }

    #[tokio::test]
    async fn test_persist_value_on_failure() {
        let mut x = 0;
        let f = async_fn!(move || {
            let ret = if x % 2 == 0 { Err(()) } else { Ok(x) };
            x += 1;
            ready(ret)
        });
        type Fut = EverReady<i32, ()>;
        type M = MnemoSync<i32, (), AsyncFn<i32, (), Fut>>;
        let mut mnem = M::new(None, 0, f);
        for i in 0..10 {
            let j = 2 * i + 1;
            let p = mnem.poll_async().await.unwrap();
            let _ = mnem.poll_async().await.unwrap_err();
            let q = mnem.peek().unwrap();
            assert_eq!(j, p);
            assert_eq!(j, q);
        }
    }
}
