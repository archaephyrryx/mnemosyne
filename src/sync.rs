use std::{convert::Infallible, marker::PhantomData, time::Duration};

use crate::common::TimedContents;

pub mod async_fn;

use async_fn::{AsyncClosure, AsyncFn};
use futures::{future::BoxFuture, Future};

pub type EverReady<T, E = Infallible> = futures::future::Ready<Result<T, E>>;

/// Async-compatible alternative of [`crate::unsync::Mnemosyne`].
///
/// Will not eagerly update, but rather recompute from the same async closure if polled after the internal
/// refresh period has elapsed.
#[allow(clippy::module_name_repetitions)]
pub struct MnemoSync<
    T,
    E = Infallible,
    Fut = BoxFuture<'static, Result<T, E>>,
    F = AsyncFn<T, E, Fut>,
> where
    Fut: Future<Output = Result<T, E>> + Send + Sync,
{
    value: Option<TimedContents<T>>,
    refresh_interval: Duration,
    update: F,
    _proxy: PhantomData<(E, Fut)>,
}

impl<T: Clone, E, Fut, F> MnemoSync<T, E, Fut, F>
where
    Fut: Future<Output = Result<T, E>> + Send + Sync,
{
    /// Construct a new [`MnemoSync`] from an optional initial value `init`, with the specified
    /// value-persistence duration (refresh interval) and update-stream.
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
    pub async fn poll_async(&mut self) -> Result<T, E>
    where
        F: AsyncClosure<T, E>,
    {
        if let Some(ref mut container) = self.value {
            let elapsed = container.time_since_last_update();
            if elapsed >= self.refresh_interval {
                let res = self.update.call().await;
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
        let res = self.update.call().await;
        match res {
            Ok(contents) => {
                self.value = Some(TimedContents::new(contents.clone()));
                Ok(contents)
            }
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures::future::ready;

    use super::*;
    use crate::{async_fn, async_fold, sync::async_fn::AsyncFold};

    #[tokio::test]
    async fn test_peek_empty() {
        type Fut = EverReady<i32>;
        type M = MnemoSync<i32, Infallible, Fut, AsyncFn<i32, Infallible, Fut>>;
        let mnemo: M = MnemoSync::new(None, 1000, async_fn!(|| ready(Ok(13))));
        assert_eq!(mnemo.peek(), None);
    }

    #[tokio::test]
    async fn test_peek_non_empty() {
        type Fut = EverReady<&'static str>;
        type M = MnemoSync<&'static str, Infallible, Fut, AsyncFn<&'static str, Infallible, Fut>>;
        let mut mnemo = M::new(Some("foo"), 1000, async_fn!(|| ready(Ok("bar"))));
        assert_eq!(mnemo.peek(), Some("foo"));
        std::thread::sleep(std::time::Duration::from_millis(2000));
        mnemo.poll_async().await.unwrap();
        assert_eq!(mnemo.peek(), Some("bar"));
    }

    #[tokio::test]
    async fn test_peek_same_as_last_poll() {
        let f = async_fold!(3u8, move |x| {
            if *x == 0 {
                ready(Err(()))
            } else {
                let ret = *x;
                *x -= 1;
                ready(Ok(ret))
            }
        });
        type Fut = EverReady<u8, ()>;
        type M = MnemoSync<u8, (), Fut, AsyncFold<u8, u8, (), Fut>>;
        let mut mnemo = M::new(Some(3u8), 100, f);
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
        type M = MnemoSync<i32, Infallible, Fut, AsyncFn<i32, Infallible, Fut>>;
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
        type M = MnemoSync<i32, (), Fut, AsyncFn<i32, (), Fut>>;
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
