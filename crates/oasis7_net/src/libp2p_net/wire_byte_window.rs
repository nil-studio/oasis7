//! One shared 60-second directional and total admission window for substream I/O.
use std::{
    collections::VecDeque,
    io,
    sync::{Arc, Mutex},
    task::{Context, Poll, ready},
    time::Duration,
};

use futures::{FutureExt, future::BoxFuture};
use tokio::{
    sync::Notify,
    time::{Instant, sleep_until},
};

const BUCKET_RETENTION_SECS: u64 = 61;

#[derive(Debug, Clone, Copy)]
pub(super) struct WireByteLimits {
    pub inbound: u64,
    pub outbound: u64,
    pub total: u64,
}

#[derive(Clone, Copy)]
pub(super) enum Direction {
    Read = 0,
    Write = 1,
}

#[derive(Clone, Copy, Debug, Default)]
struct Charge {
    actual: u64,
    conservative: u64,
}

impl Charge {
    fn total(self) -> io::Result<u64> {
        self.actual
            .checked_add(self.conservative)
            .ok_or_else(invalid)
    }

    fn with_charge(self, next: Self) -> io::Result<Self> {
        Ok(Self {
            actual: self.actual.checked_add(next.actual).ok_or_else(invalid)?,
            conservative: self
                .conservative
                .checked_add(next.conservative)
                .ok_or_else(invalid)?,
        })
    }

    fn without_charge(self, expired: Self) -> io::Result<Self> {
        Ok(Self {
            actual: self
                .actual
                .checked_sub(expired.actual)
                .ok_or_else(invalid)?,
            conservative: self
                .conservative
                .checked_sub(expired.conservative)
                .ok_or_else(invalid)?,
        })
    }
}

#[derive(Debug)]
struct Bucket {
    expires: Instant,
    charges: [Charge; 2],
}

#[derive(Debug, Default)]
struct State {
    buckets: VecDeque<Bucket>,
    used: [Charge; 2],
    reserved: [u64; 2],
    failed: bool,
}

impl State {
    fn prune(&mut self, now: Instant) -> io::Result<()> {
        while self
            .buckets
            .front()
            .is_some_and(|bucket| now >= bucket.expires)
        {
            let bucket = self.buckets.pop_front().ok_or_else(invalid)?;
            self.used[0] = self.used[0].without_charge(bucket.charges[0])?;
            self.used[1] = self.used[1].without_charge(bucket.charges[1])?;
        }
        Ok(())
    }

    fn record(&mut self, lane: usize, charge: Charge, expires: Instant) -> io::Result<()> {
        if self
            .buckets
            .back()
            .is_none_or(|bucket| bucket.expires != expires)
        {
            if self.buckets.len() == BUCKET_RETENTION_SECS as usize
                || self
                    .buckets
                    .back()
                    .is_some_and(|bucket| expires < bucket.expires)
            {
                return Err(invalid());
            }
            self.buckets.push_back(Bucket {
                expires,
                charges: [Charge::default(); 2],
            });
        }
        let bucket = self.buckets.back_mut().ok_or_else(invalid)?;
        let bucket_charge = bucket.charges[lane].with_charge(charge)?;
        self.used[lane] = self.used[lane].with_charge(charge)?;
        bucket.charges[lane] = bucket_charge;
        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct WireByteWindow {
    origin: Instant,
    limits: WireByteLimits,
    state: Mutex<State>,
    released: Arc<Notify>,
}

fn invalid() -> io::Error {
    io::Error::other("invalid substream byte budget state")
}

impl WireByteWindow {
    #[cfg(test)]
    pub(super) fn new(limits: WireByteLimits) -> io::Result<Self> {
        if limits.inbound == 0 || limits.outbound == 0 || limits.total == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "zero substream byte limit",
            ));
        }
        Ok(Self {
            origin: Instant::now(),
            limits,
            state: Mutex::new(State::default()),
            released: Arc::new(Notify::new()),
        })
    }

    fn bucket_expiry(&self, now: Instant) -> io::Result<Instant> {
        let seconds = now
            .checked_duration_since(self.origin)
            .ok_or_else(invalid)?
            .as_secs()
            .checked_add(BUCKET_RETENTION_SECS)
            .ok_or_else(invalid)?;
        self.origin
            .checked_add(Duration::from_secs(seconds))
            .ok_or_else(invalid)
    }

    fn reserve(self: &Arc<Self>, direction: Direction, requested: usize) -> io::Result<Admission> {
        let requested = u64::try_from(requested).map_err(|_| invalid())?;
        let lane = direction as usize;
        let mut state = self.state.lock().map_err(|_| invalid())?;
        if state.failed {
            return Err(invalid());
        }
        // Sample after taking the lock to keep completion buckets ordered.
        if let Err(error) = state.prune(Instant::now()) {
            state.failed = true;
            return Err(error);
        }
        let used = [state.used[0].total()?, state.used[1].total()?];
        let used_total = used[0].checked_add(used[1]).ok_or_else(invalid)?;
        let directional_limit = [self.limits.inbound, self.limits.outbound][lane];
        let directional_free = directional_limit
            .checked_sub(used[lane])
            .ok_or_else(invalid)?;
        let total_free = self
            .limits
            .total
            .checked_sub(used_total)
            .ok_or_else(invalid)?;
        let reserved_total = state.reserved[0]
            .checked_add(state.reserved[1])
            .ok_or_else(invalid)?;
        let allowance = requested
            .min(
                directional_free
                    .checked_sub(state.reserved[lane])
                    .ok_or_else(invalid)?,
            )
            .min(total_free.checked_sub(reserved_total).ok_or_else(invalid)?);
        if allowance != 0 {
            state.reserved[lane] = state.reserved[lane]
                .checked_add(allowance)
                .ok_or_else(invalid)?;
            return Ok(Admission::Granted(Reservation {
                window: Arc::clone(self),
                lane,
                amount: allowance,
                submitted: false,
            }));
        }

        // Register the notification while the reservation state is locked. A
        // settlement after this point is visible even before the first poll.
        let release = (directional_free != 0 && total_free != 0)
            .then(|| Arc::clone(&self.released).notified_owned());
        let deadline = state.buckets.front().map(|bucket| bucket.expires);
        let wait = async move {
            match (release, deadline) {
                (Some(release), Some(deadline)) => {
                    futures::future::select(Box::pin(release), Box::pin(sleep_until(deadline)))
                        .await;
                }
                (Some(release), None) => release.await,
                (None, Some(deadline)) => sleep_until(deadline).await,
                (None, None) => {}
            }
        }
        .boxed();
        if directional_free == 0 || total_free == 0 {
            deadline.ok_or_else(invalid)?;
        }
        Ok(Admission::Stalled(wait))
    }

    fn settle(&self, lane: usize, amount: u64, charge: Charge) -> io::Result<()> {
        let mut state = self.state.lock().map_err(|_| invalid())?;
        if state.failed {
            return Err(invalid());
        }
        let settled = (|| -> io::Result<bool> {
            let now = Instant::now();
            let used = charge.total()?;
            if used > amount {
                return Err(invalid());
            }
            let remaining = state.reserved[lane]
                .checked_sub(amount)
                .ok_or_else(invalid)?;
            state.prune(now)?;
            if used != 0 {
                state.record(lane, charge, self.bucket_expiry(now)?)?;
            }
            state.reserved[lane] = remaining;
            Ok(amount != used)
        })();
        let released = match settled {
            Ok(released) => released,
            Err(error) => {
                state.failed = true;
                return Err(error);
            }
        };
        drop(state);
        if released {
            self.released.notify_waiters();
        }
        Ok(())
    }
}

enum Admission {
    Granted(Reservation),
    Stalled(BoxFuture<'static, ()>),
}

struct Reservation {
    window: Arc<WireByteWindow>,
    lane: usize,
    amount: u64,
    submitted: bool,
}

impl Reservation {
    fn apply(
        mut self,
        operation: impl FnOnce(usize) -> Poll<io::Result<usize>>,
    ) -> Poll<io::Result<usize>> {
        let allowance = usize::try_from(self.amount).map_err(|_| invalid())?;
        self.submitted = true;
        let result = operation(allowance);
        let charge = match result {
            Poll::Ready(Ok(n)) if n <= allowance => Charge {
                actual: u64::try_from(n).map_err(|_| invalid())?,
                conservative: 0,
            },
            Poll::Pending => Charge::default(),
            // The erased transport does not expose a reliable progress count on error.
            Poll::Ready(_) => Charge {
                actual: 0,
                conservative: self.amount,
            },
        };
        let amount = std::mem::take(&mut self.amount);
        self.window.settle(self.lane, amount, charge)?;
        if matches!(result, Poll::Ready(Ok(n)) if n > allowance) {
            return Poll::Ready(Err(invalid()));
        }
        result
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        if self.amount == 0 {
            return;
        }
        let charge = Charge {
            actual: 0,
            conservative: if self.submitted { self.amount } else { 0 },
        };
        let _ = self.window.settle(self.lane, self.amount, charge);
    }
}

#[derive(Default)]
pub(super) struct BudgetWait(Option<BoxFuture<'static, ()>>);

impl BudgetWait {
    pub(super) fn poll(
        &mut self,
        window: &Arc<WireByteWindow>,
        cx: &mut Context<'_>,
        direction: Direction,
        requested: usize,
        operation: impl FnOnce(usize, &mut Context<'_>) -> Poll<io::Result<usize>>,
    ) -> Poll<io::Result<usize>> {
        if requested == 0 {
            return Poll::Ready(Ok(0));
        }
        if let Some(wait) = &mut self.0 {
            ready!(wait.as_mut().poll(cx));
            self.0 = None;
        }
        match window.reserve(direction, requested)? {
            Admission::Granted(permit) => permit.apply(|len| operation(len, cx)),
            Admission::Stalled(mut wait) => {
                if wait.as_mut().poll(cx).is_ready() {
                    cx.waker().wake_by_ref();
                } else {
                    self.0 = Some(wait);
                }
                Poll::Pending
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::poll_fn;

    fn limits() -> WireByteLimits {
        WireByteLimits {
            inbound: 4,
            outbound: 4,
            total: 4,
        }
    }

    #[test]
    fn reservations_share_directions_and_release_without_phantom_actual_bytes() {
        let window = Arc::new(WireByteWindow::new(limits()).unwrap());
        let Admission::Granted(permit) = window.reserve(Direction::Read, 4).unwrap() else {
            panic!("initial reservation did not succeed");
        };
        let mut wait = BudgetWait::default();
        let cx = &mut Context::from_waker(futures::task::noop_waker_ref());
        assert!(
            wait.poll(&window, cx, Direction::Write, 4, |_, _| panic!(
                "quota unavailable"
            ))
            .is_pending()
        );
        assert!(permit.apply(|_| Poll::Pending).is_pending());
        let uncertain = wait.poll(&window, cx, Direction::Write, 2, |len, _| {
            assert_eq!(len, 2);
            Poll::Ready(Err(io::Error::other("unobserved transport progress")))
        });
        assert!(matches!(uncertain, Poll::Ready(Err(_))));
        let Admission::Granted(permit) = window.reserve(Direction::Read, 4).unwrap() else {
            panic!("remaining bytes did not become available");
        };
        assert_eq!(permit.amount, 2);
        assert!(matches!(
            permit.apply(|_| Poll::Ready(Ok(1))),
            Poll::Ready(Ok(1))
        ));
        let state = window.state.lock().unwrap();
        assert_eq!(state.reserved, [0, 0]);
        assert_eq!(state.used[0].actual, 1);
        assert_eq!(state.used[1].actual, 0);
        assert_eq!(state.used[1].conservative, 2);
    }

    #[test]
    fn charged_bytes_remain_until_bucket_expiry_and_waiter_resumes() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let mut window = WireByteWindow::new(limits()).unwrap();
                window.origin -= Duration::from_secs(60);
                let expiry = window.bucket_expiry(window.origin).unwrap();
                let charge = Charge {
                    actual: 4,
                    conservative: 0,
                };
                let state = window.state.get_mut().unwrap();
                state
                    .record(Direction::Read as usize, charge, expiry)
                    .unwrap();
                state.prune(expiry - Duration::from_nanos(1)).unwrap();
                assert_eq!(state.used[0].actual, 4);

                let window = Arc::new(window);
                let mut wait = BudgetWait::default();
                let n = tokio::time::timeout(
                    Duration::from_secs(4),
                    poll_fn(|cx| {
                        wait.poll(&window, cx, Direction::Write, 4, |allowed, _| {
                            assert!(Instant::now() >= expiry);
                            Poll::Ready(Ok(allowed))
                        })
                    }),
                )
                .await
                .unwrap()
                .unwrap();
                assert_eq!(n, 4);
                let state = window.state.lock().unwrap();
                assert_eq!(state.used[0].actual, 0);
                assert_eq!(state.used[1].actual, 4);
            });
    }
}
