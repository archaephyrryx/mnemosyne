use futures::Future;
use tokio::sync::Mutex;

use async_trait::async_trait;

#[async_trait]
pub trait AsyncClosure<T, E> {
    async fn call(&mut self) -> Result<T, E>;
}

#[async_trait]
impl<T, E, Fut> AsyncClosure<T, E> for AsyncFn<T, E, Fut>
where
    Fut: Future<Output = Result<T, E>> + Send + Sync,
{
    async fn call(&mut self) -> Result<T, E> {
        (self._inner)().await
    }
}

#[async_trait]
impl<U, T, E, Fut> AsyncClosure<T, E> for AsyncFold<U, T, E, Fut>
where
    U: Send + Sync + 'static,
    Fut: Future<Output = Result<T, E>> + Send,
{
    async fn call(&mut self) -> Result<T, E> {
        let mut acc = self.acc.lock().await;
        (self.f)(&mut acc).await
    }
}

#[macro_export]
macro_rules! async_fn {
    ($x:expr) => {
        $crate::sync::async_fn::AsyncFn::new($x)
    };
}

/// A wrapper around a closure that returns a [`Future`] and can be called more than once, with possible side-effects.
pub struct AsyncFn<T, E, Fut>
where
    Fut: Future<Output = Result<T, E>> + Send + Sync,
{
    _inner: std::pin::Pin<Box<dyn (FnMut() -> Fut) + Sync + Send + 'static + Unpin>>,
}

impl<T, E, Fut> AsyncFn<T, E, Fut>
where
    Fut: Future<Output = Result<T, E>> + Send + Sync,
{
    pub fn new<F>(inner: F) -> Self
    where
        F: FnMut() -> Fut,
        F: Send + Sync + 'static + Unpin,
    {
        Self {
            _inner: Box::pin(inner),
        }
    }
}

#[macro_export]
macro_rules! async_fold {
    ($x:expr, $f:expr) => {
        $crate::sync::async_fn::AsyncFold::new($x, $f)
    };
}

pub struct AsyncFold<U, T, E, Fut>
where
    U: Send + Sync + 'static,
    Fut: Future<Output = Result<T, E>>,
{
    acc: Mutex<U>,
    f: Box<dyn (FnMut(&mut U) -> Fut) + Send + 'static + Unpin>,
}

impl<U, T, E, Fut> AsyncFold<U, T, E, Fut>
where
    U: Send + Sync + 'static,
    Fut: Future<Output = Result<T, E>>,
{
    pub fn new<F>(init: U, f: F) -> Self
    where
        F: FnMut(&mut U) -> Fut,
        F: Send + 'static + Unpin,
    {
        Self {
            acc: Mutex::new(init),
            f: Box::new(f),
        }
    }
}
