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
    refresh_interval: chrono::Duration,
    refresh_fn: Box<dyn FnMut() -> Result<T, E>>,
}

impl<T: Clone, E> Mnemosyne<T, E> {
    /// Construct a new [`Mnemosyne<T, E>`] from an optional initial value `init`, with the specified
    /// value-persistence duration (refresh interval) and boxed computation (refresh function).
    ///
    /// The refresh interval is specified by the parameter `refresh_millis` as a number of milliseconds.
    /// If this value is zero, the closure will be called every time the value would be polled.
    pub fn new(init: Option<T>, refresh_millis: u64, f: Box<dyn FnMut() -> Result<T, E>>) -> Self {
        let refresh_interval = chrono::Duration::milliseconds(refresh_millis as i64);
        let value = init.map(|x| TimedContents::new(x));
        Self { value, refresh_interval, refresh_fn: f }
    }

    /// Returns a copy of the current value held by this [`Mnemosyne<T, E>`], if one exists.
    ///
    /// Unlike [`poll`], this method will not interact with the chronometry of the received object,
    /// will never call the stored closure, and will not change the value stored internally. It is therefore
    /// idempotent, and should generally return the same value as the last successful call to `poll`, if one
    /// was made.
    ///
    /// If this [`Mnemosyne<T, E>`] was not initialized and never successfully updated, this method will return
    /// `None`.
    ///
    /// ```
    /// use std::time::Duration;
    /// use mnemosyne::Mnemosyne;
    /// let mut mem : Mnemosyne<i32> = Mnemosyne::new(Some(42), 1000, Box::new(|| Ok(13)));
    ///
    /// // Test that `peek` returns the initial value when no updates have been made
    /// assert_eq!(mem.peek(), Some(42));
    ///
    /// // Wait longer than the refresh interval without calling `poll` and test that `peek` still returns the initial value
    /// std::thread::sleep(Duration::from_millis(2000));
    /// assert_eq!(mem.peek(), Some(42));
    ///
    /// // Call `poll` and check that `peek` returns the updated value
    /// mem.poll().unwrap();
    /// assert_eq!(mem.peek(), Some(13));
    /// ```
    pub fn peek(&self) -> Option<T> {
        if let Some(v) = &self.value { Some(v.get_contents().clone()) } else { None }
    }

    /// Polls the current value stored in this [`Mnemosyne<T, E>`] and updates it if it is determined to be out-of-date.
    ///
    /// This method calls the internal closure at most once per invocation and may not call it at all if the refresh interval has not yet elapsed.
    ///
    /// If the closure returns an `Err` value, this method returns an `Err` value without modifying the value stored in the [`Mnemosyne<T, E>`].
    ///
    /// If an error occurs, the immediate value that would have been overwritten upon a successful callback can be safely interrogated using the [`peek`] method.
    ///
    /// # Returns
    ///
    /// Returns a copy of the most up-to-date value held by this [`Mnemosyne<T, E>`], if one exists, or an `Err` value if the closure returns an error.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    /// use std::thread::sleep;
    /// use mnemosyne::Mnemosyne;
    ///
    /// let mut mnemosyne : Mnemosyne<i32> = Mnemosyne::new(Some(42), 100, Box::new(|| Ok(1337)));
    /// assert!(matches!(mnemosyne.poll(), Ok(42)));
    ///
    /// sleep(Duration::from_millis(200));
    ///
    /// assert!(matches!(mnemosyne.poll(), Ok(1337)));
    /// ```
    ///
    /// [`peek`]: Mnemosyne::peek
    pub fn poll(&mut self) -> Result<T, E> {
        if let Some(ref mut container) = self.value {
            let elapsed = container.time_since_last_update();
            if elapsed >= self.refresh_interval {
                let res = self.refresh_fn.as_mut()();
                match res {
                    Ok(new_contents) => {
                        container.replace_contents(new_contents.clone());
                        return Ok(new_contents);
                    }
                    Err(err) => {
                        return Err(err);
                    }
                }
            } else {
                return Ok(container.get_contents().clone());
            }
        } else {
            let res = self.refresh_fn.as_mut()();
            match res {
                Ok(contents) => {
                    self.value = Some(TimedContents::new(contents.clone()));
                    Ok(contents)
                }
                Err(e) => Err(e),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn test_peek_empty() {
        let mnemo: Mnemosyne<u32> = Mnemosyne::new(None, 1000, Box::new(|| Ok(13)));
        assert_eq!(mnemo.peek(), None);
    }

    #[test]
    fn test_peek_non_empty() {
        let mut mnemo = Mnemosyne::<&'static str>::new(Some("foo"), 1000, Box::new(|| Ok("bar")));
        assert_eq!(mnemo.peek(), Some("foo"));
        std::thread::sleep(std::time::Duration::from_millis(2000));
        mnemo.poll().unwrap();
        assert_eq!(mnemo.peek(), Some("bar"));
    }

    #[test]
    fn test_peek_same_as_last_poll() {
        let mut countdown : u8 = 3;
        let f = Box::new(move || {
            if countdown == 0 {
                Err(())
            } else {
                let ret = countdown;
                countdown -= 1;
                Ok(ret)
            }
        });
        let mut mnemo = Mnemosyne::new(Some(3u8), 100, f);
        std::thread::sleep(Duration::from_millis(100));
        assert!(matches!(mnemo.poll(), Ok(3)));
        std::thread::sleep(Duration::from_millis(100));
        assert!(matches!(mnemo.poll(), Ok(2)));
        std::thread::sleep(Duration::from_millis(100));
        assert!(matches!(mnemo.poll(), Ok(1)));
        std::thread::sleep(Duration::from_millis(100));
        assert!(matches!(mnemo.poll(), Err(..)));
        assert_eq!(mnemo.peek(), Some(1));
    }

    #[test]
    fn test_poll() {
        let mut x = 0;
        let f = Box::new(move || {
            let ret = x;
            x += 1;
            Ok(ret)
        });
        let mut mnem: Mnemosyne<i32, Infallible> = Mnemosyne::new(Some(42), 2000, f);
        assert_eq!(mnem.peek(), Some(42));
        std::thread::sleep(std::time::Duration::from_millis(1000));
        assert!(matches!(mnem.poll(), Ok(42)));
        std::thread::sleep(std::time::Duration::from_millis(2000));
        assert!(matches!(mnem.poll(), Ok(0)));
        std::thread::sleep(std::time::Duration::from_millis(3000));
        assert!(matches!(mnem.poll(), Ok(1)));
    }

    #[test]
    fn test_persist_value_on_failure() {
        let mut ctr = 1;
        let f = Box::new(move || {
            let ret = if ctr % 2 == 0 { Err(()) } else { Ok(ctr) };
            ctr += 1;
            ret
        });
        let mut mnem: Mnemosyne<i32, ()> = Mnemosyne::new(None, 0, f);
        for i in 0..10 {
            let j = 2 * i + 1;
            let p = mnem.poll().unwrap();
            let _ = mnem.poll().unwrap_err();
            let q = mnem.peek().unwrap();
            assert_eq!(j, p);
            assert_eq!(j, q);
        }
    }
}