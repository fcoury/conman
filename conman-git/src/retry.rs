use std::time::Duration;

use conman_core::ConmanError;
use tonic::{Code, Status};

const MAX_RETRIES: u32 = 3;
const BASE_DELAY_MS: u64 = 50;

pub fn map_grpc_error(status: Status) -> ConmanError {
    ConmanError::Git {
        message: format!("gRPC {:?}: {}", status.code(), status.message()),
    }
}

fn is_retryable(code: Code) -> bool {
    matches!(code, Code::Unavailable | Code::DeadlineExceeded)
}

pub async fn retry_grpc<F, Fut, T>(f: F) -> Result<T, ConmanError>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, Status>>,
{
    let mut attempt = 0;
    loop {
        match f().await {
            Ok(v) => return Ok(v),
            Err(status) => {
                attempt += 1;
                if attempt >= MAX_RETRIES || !is_retryable(status.code()) {
                    return Err(map_grpc_error(status));
                }

                let backoff = BASE_DELAY_MS * (1 << (attempt - 1));
                tokio::time::sleep(Duration::from_millis(backoff)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[tokio::test]
    async fn retry_succeeds_after_transient_failures() {
        let attempts = AtomicUsize::new(0);

        let result = retry_grpc(|| async {
            let current = attempts.fetch_add(1, Ordering::SeqCst);
            if current < 2 {
                Err(Status::new(Code::Unavailable, "temp"))
            } else {
                Ok("ok")
            }
        })
        .await;

        assert_eq!(result.expect("ok"), "ok");
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_does_not_retry_non_retryable() {
        let attempts = AtomicUsize::new(0);

        let result = retry_grpc(|| async {
            attempts.fetch_add(1, Ordering::SeqCst);
            Err::<(), _>(Status::new(Code::InvalidArgument, "bad"))
        })
        .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }
}
