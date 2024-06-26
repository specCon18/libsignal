//
// Copyright 2023 Signal Messenger, LLC.
// SPDX-License-Identifier: AGPL-3.0-only
//

use base64::prelude::{Engine as _, BASE64_STANDARD};
use futures_util::stream::FuturesUnordered;
use futures_util::StreamExt;
use http::HeaderValue;
use std::future;
use std::future::Future;
use std::time::Duration;

/// Constructs the value of the `Authorization` header for the `Basic` auth scheme.
pub(crate) fn basic_authorization(username: &str, password: &str) -> HeaderValue {
    let auth = BASE64_STANDARD.encode(format!("{}:{}", username, password).as_bytes());
    let auth = format!("Basic {}", auth);
    HeaderValue::try_from(auth).expect("valid header value")
}

/// Requires a `Future` to complete before the specified duration has elapsed.
///
/// Takes in a future whose return type is `Result<T, E>`, a `duration` timeout,
/// and a `timeout_error` of type `E`. Internally, a [tokio::time::timeout] is called,
/// but the return type of this method is the same as the return type of the given `future`,
/// i.e. `Result<T, E>`, which in the case of timing out will be `Err(timeout_error)`.
pub async fn timeout<T, E, F>(duration: Duration, timeout_error: E, future: F) -> Result<T, E>
where
    F: Future<Output = Result<T, E>>,
{
    match tokio::time::timeout(duration, future).await {
        Ok(r) => r,
        Err(_) => Err(timeout_error),
    }
}

/// Takes a series of `Future` objects that all return a `Result<T, E>`
/// and returns when the first of them completes successfully.
///
/// Errors from the failed futures are deliberately ignored by this helper method.
/// If error processing is needed, the caller should pass futures that inspect their errors.
pub async fn first_ok<T, E, F, I>(futures: I) -> Option<T>
where
    F: Future<Output = Result<T, E>>,
    I: IntoIterator<Item = F>,
{
    FuturesUnordered::from_iter(futures)
        .filter_map(|result| future::ready(result.ok()))
        .next()
        .await
}

/// In the tokio time paused test mode, if some logic is supposed to wake up at specific time
/// and a test wants to make sure it observes the result of that logic without moving
/// the time past that point, it's not enough to call `sleep()` or `advance()` alone.
/// The combination of sleeping and advancing by 0 makes sure that all events
/// (in all tokio threads) scheduled to run at (or before) that specific time are processed.
///
/// `sleep_and_catch_up_showcase()` test demonstrates this behavior.
#[cfg(test)]
pub(crate) async fn sleep_and_catch_up(duration: Duration) {
    tokio::time::sleep(duration).await;
    tokio::time::advance(Duration::ZERO).await
}

/// See [`sleep_and_catch_up`]
#[cfg(test)]
pub(crate) async fn sleep_until_and_catch_up(time: tokio::time::Instant) {
    tokio::time::sleep_until(time).await;
    tokio::time::advance(Duration::ZERO).await
}

#[cfg(test)]
mod test {
    use super::*;
    use std::future::Future;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time;

    #[tokio::test(start_paused = true)]
    async fn first_ok_picks_the_result_from_earliest_finished_future() {
        let future_1 = future(30, Ok(1));
        let future_2 = future(10, Ok(2));
        let future_3 = future(20, Ok(3));
        let result = first_ok(vec![future_1, future_2, future_3]).await.unwrap();
        assert_eq!(2, result);
    }

    #[tokio::test(start_paused = true)]
    async fn first_ok_ignores_failed_futures() {
        let future_1 = future(30, Ok(1));
        let future_2 = future(10, Err("error"));
        let future_3 = future(20, Ok(3));
        let result = first_ok(vec![future_1, future_2, future_3]).await.unwrap();
        assert_eq!(3, result);
    }

    #[tokio::test(start_paused = true)]
    async fn first_ok_returns_none_if_all_failed() {
        let future_1 = future(30, Err("error 1"));
        let future_2 = future(10, Err("error 2"));
        let future_3 = future(20, Err("error 3"));
        assert!(first_ok(vec![future_1, future_2, future_3]).await.is_none())
    }

    #[tokio::test(start_paused = true)]
    async fn sleep_and_catch_up_showcase() {
        const DURATION: Duration = Duration::from_millis(100);

        async fn test<F: Future<Output = ()>>(sleep_variant: F) -> bool {
            let flag = Arc::new(AtomicBool::new(false));
            let flag_clone = flag.clone();
            tokio::spawn(async move {
                time::sleep(DURATION).await;
                flag_clone.store(true, Ordering::Relaxed);
            });
            sleep_variant.await;
            flag.load(Ordering::Relaxed)
        }

        assert!(!test(time::sleep(DURATION)).await);
        assert!(!test(time::advance(DURATION)).await);
        assert!(test(sleep_and_catch_up(DURATION)).await);
    }

    async fn future(delay: u64, result: Result<u32, &str>) -> Result<u32, &str> {
        tokio::time::sleep(Duration::from_millis(delay)).await;
        result
    }
}
