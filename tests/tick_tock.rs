#![cfg(test)]
use std::{sync::{Arc, atomic::Ordering}, pin::Pin};
use futures::{StreamExt, future::BoxFuture};
use mnemosyne::{MnemoSync, sync::async_fn::AsyncClosure};
use tokio::{time::interval, sync::Mutex};
use tokio_stream::wrappers::IntervalStream;
use std::{time::Duration, sync::atomic::AtomicU32};

struct DelayedCallbackCounter {
    count: Arc<AtomicU32>,
    ticker: Arc<Mutex<IntervalStream>>,
}

impl<'a> AsyncClosure<'a, u32, ()> for DelayedCallbackCounter {
    type Fut = BoxFuture<'a, Result<u32, ()>>;

    fn call<'c>(self: Pin<&'c mut Self>) -> Self::Fut where 'c: 'a
    {
        let this = self.ticker.clone();
        Box::pin(async move {
            let _ = this.lock().await.next().await;
            self.count.fetch_add(1, Ordering::SeqCst);
            Ok(self.count.load(Ordering::SeqCst))
        })
    }
}


#[tokio::test]
async fn clock_test() {
    let ticker = IntervalStream::new(interval(Duration::from_secs(1)));
    let ticks = Arc::new(AtomicU32::new(0));
    let dcc = DelayedCallbackCounter { count: ticks.clone(), ticker: Arc::new(Mutex::new(ticker)) };
    let mut mnemosync : MnemoSync<u32, (), DelayedCallbackCounter> = MnemoSync::new(Some(0), 2000, dcc);
    let init_val = mnemosync.peek();
    assert_eq!(init_val, Some(0));
    let mut poll_count_total = 0;
    let mut poll_count_after_2s = 0;
    loop {
        let elapsed = mnemosync.time_since_last_update();
        let res = mnemosync.poll_async().await;
        match res {
            Ok(_) => {
                poll_count_total += 1;
                if let Some(span) = elapsed {
                    if span >= Duration::from_millis(2000) {
                        poll_count_after_2s += 1;
                    }
                }
                let val = mnemosync.peek();
                assert_eq!(val, Some(poll_count_after_2s), "val: {:?}, poll_count_after_2s: {}, poll_count_total: {}", val, poll_count_after_2s, poll_count_total);
                if poll_count_after_2s == 5 {
                    break;
                } else if poll_count_total == 10 {
                    break;
                }
            }
            Err(_) => {
                panic!("poll_async returned an error");
            }
        }
        match elapsed {
            None => tokio::time::sleep(Duration::from_millis(1000)).await,
            Some(span) => {
                if span < Duration::from_millis(2000) {
                    if span > Duration::from_millis(1000) {
                        tokio::time::sleep(Duration::from_millis(2000) - span).await;
                    } else {
                        tokio::time::sleep(Duration::from_millis(1000) - span).await;
                    }
                } else {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }

        }
    }


}