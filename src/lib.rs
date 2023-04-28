extern crate chrono;

use chrono::{ DateTime, Utc };
use std::{ convert::Infallible };

pub(crate) struct TimedContents<T> {
    contents: T,
    last_updated: DateTime<Utc>,
}

impl<T> TimedContents<T> {
    fn new(contents: T) -> Self {
        let last_updated = chrono::Utc::now();
        Self { contents, last_updated }
    }

    fn replace_contents(&mut self, new_contents: T) -> () {
        self.last_updated = chrono::Utc::now();
        self.contents = new_contents;
    }

    pub fn get_contents(&self) -> &T {
        &self.contents
    }

    pub fn time_since_last_update(&self) -> chrono::Duration {
        chrono::Utc::now() - self.last_updated
    }
}

/// Memoization type that 'forgets' the last value it computed after a set interval has passed.
///
/// Will not eagerly update, but rather recompute from the same closure if polled after the internal
/// refresh period has elapsed.
pub struct Mnemosyne<T, E = Infallible> {
    value: Option<TimedContents<T>>,
    refresh_rate: chrono::Duration,
    refresh_fn: Box<dyn FnMut() -> Result<T, E>>,
}

#[derive(Debug)]
pub enum MnemosyneError<E> {
    ClosureError(E),
    UninitializedPoll,
}

impl<E> std::fmt::Display for MnemosyneError<E> where E: std::error::Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MnemosyneError::ClosureError(e) => {
                write!(
                    f,
                    "mnemosyne-internal update-function returned error (keeping old value): {}",
                    e
                )
            }
            MnemosyneError::UninitializedPoll => {
                write!(
                    f,
                    "cannot poll uninitialized mnemosyne immutably, as it has not been initialized"
                )
            }
        }
    }
}

impl<T: Clone, E> Mnemosyne<T, E> {
    /// Construct a new [`Mnemosyne<T, E>`] from an optional initial value `init`, with the specified
    /// stickiness duration and reusable computation.
    pub fn new(
        init: Option<T>,
        refresh_millis: u64,
        f: Box<dyn FnMut() -> Result<T, E>>
    ) -> Self {
        let refresh_rate = chrono::Duration::milliseconds(refresh_millis as i64);
        let value = init.map(|x| TimedContents::new(x));
        Self { value, refresh_rate, refresh_fn: f }
    }

    /// Returns, without modifying any state or blocking, the current value held, if available.
    ///
    /// If there is no value stored at the time, returns `None`.
    pub fn peek(&self) -> Option<T> {
        if let Some(v) = &self.value { Some(v.get_contents().clone()) } else { None }
    }

    /// Polls the current value stored in this [`Mnemosyne<T, E>`], updating it according to the internal
    /// closure if it is determined to be out-of-date.
    pub fn poll(&mut self) -> Result<T, MnemosyneError<E>> {
        if self.value.is_none() {
            let res = self.refresh_fn.as_mut()();
            match res {
                Ok(contents) => {
                    self.value = Some(TimedContents::new(contents));
                }
                Err(e) => {
                    return Err(MnemosyneError::ClosureError(e));
                }
            }
        }
        match self.value.as_mut() {
            Some(container) => {
                let elapsed = container.time_since_last_update();
                if elapsed >= self.refresh_rate {
                    let res = self.refresh_fn.as_mut()();
                    match res {
                        Ok(new_contents) => {
                            container.replace_contents(new_contents.clone());
                            return Ok(new_contents);
                        }
                        Err(err) => {
                            return Err(MnemosyneError::ClosureError(err));
                        }
                    }
                } else {
                    return Ok(container.get_contents().clone());
                }
            }
            None => unreachable!("poll ensures value is initialized"),
        }
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_poll() {
        let mut x = 0;
        let f = Box::new(move || { let ret = x; x += 1; Ok(ret) });
        let mut mnem : Mnemosyne<i32, Infallible> = Mnemosyne::new(Some(42), 2000, f);
        assert_eq!(mnem.peek(), Some(42));
        std::thread::sleep(std::time::Duration::from_millis(1000));
        assert!(matches!(mnem.poll(), Ok(42)));
        std::thread::sleep(std::time::Duration::from_millis(2000));
        assert!(matches!(mnem.poll(), Ok(0)));
        std::thread::sleep(std::time::Duration::from_millis(3000));
        assert!(matches!(mnem.poll(), Ok(1)));
    }
}