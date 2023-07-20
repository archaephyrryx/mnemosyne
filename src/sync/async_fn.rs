use std::pin::Pin;

use futures::Future;

pub trait AsyncClosure<'a, T, E> {
    type Fut: Future<Output = Result<T, E>> + 'a;

    fn call<'c>(self: Pin<&'c mut Self>) -> Self::Fut
    where
        'c: 'a;
}

/// A wrapper around a closure that returns a [`Future`] and can be called more than once, with possible side-effects.
pub struct AsyncFn<T, E, Fut>
where
    Fut: Future<Output = Result<T, E>> + Send + Sync,
{
    inner: std::pin::Pin<Box<dyn (FnMut() -> Fut) + Sync + Send + 'static + Unpin>>,
}

#[macro_export]
macro_rules! async_fn {
    ($x:expr) => {
        $crate::sync::async_fn::AsyncFn::new($x)
    };
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
            inner: Box::pin(inner),
        }
    }
}

impl<'a, T, E, Fut> AsyncClosure<'a, T, E> for AsyncFn<T, E, Fut>
where
    Fut: Future<Output = Result<T, E>> + Send + Sync + 'a,
{
    type Fut = Fut;

    fn call<'c>(mut self: Pin<&'c mut Self>) -> Fut
    where
        'c: 'a,
    {
        (self.inner)()
    }
}
