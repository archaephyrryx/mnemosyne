use std::time::{Duration, Instant};

pub(crate) struct TimedContents<T> {
    contents: T,
    last_updated: Instant,
}

impl<T> TimedContents<T> {
    pub(crate) fn new(contents: T) -> Self {
        let last_updated = Instant::now();
        Self {
            contents,
            last_updated,
        }
    }

    pub(crate) fn replace_contents(&mut self, new_contents: T) {
        self.last_updated = Instant::now();
        self.contents = new_contents;
    }

    pub(crate) fn get_contents(&self) -> &T {
        &self.contents
    }

    pub(crate) fn time_since_last_update(&self) -> Duration {
        self.last_updated.elapsed()
    }
}
