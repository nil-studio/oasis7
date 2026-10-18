//! Borrow-scoped socket registration: no polling interval is added to RPC latency.
#[cfg(any(unix, windows))]
use polling::{Event, Events, Poller};
use std::{io, net::TcpListener, time::Instant};

#[cfg(any(unix, windows))]
pub(super) struct ListenerReadiness<'a> {
    listener: &'a TcpListener,
    poller: Poller,
    events: Events,
    registered: bool,
}
#[cfg(any(unix, windows))]
impl<'a> ListenerReadiness<'a> {
    pub(super) fn new(listener: &'a TcpListener) -> io::Result<Self> {
        let poller = Poller::new()?;
        // SAFETY: the borrowed listener outlives this registration. Every exit
        // deletes it before the poller and borrow are released, including errors.
        unsafe { poller.add(listener, Event::readable(0))? };
        Ok(Self {
            listener,
            poller,
            events: Events::new(),
            registered: true,
        })
    }
    pub(super) fn wait_until(&mut self, deadline: Instant) -> io::Result<bool> {
        // Portable oneshot registration must be rearmed after each readiness.
        self.poller.modify(self.listener, Event::readable(0))?;
        loop {
            self.events.clear();
            // polling retries EINTR against this same absolute deadline.
            self.poller.wait_deadline(&mut self.events, deadline)?;
            if self.events.iter().any(|event| event.key == 0) {
                // Error/hangup readiness also returns to accept, which reports
                // the original socket error rather than treating it as idle.
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
        }
    }
    pub(super) fn close(mut self) -> io::Result<()> {
        self.poller.delete(self.listener)?;
        self.registered = false;
        Ok(())
    }
}
#[cfg(any(unix, windows))]
impl Drop for ListenerReadiness<'_> {
    fn drop(&mut self) {
        if self.registered
            && let Err(error) = self.poller.delete(self.listener)
        {
            // The poller is destroyed while the listener borrow is still
            // live; a cleanup error cannot leave a dangling registration.
            eprintln!("formal_endpoint_readiness_cleanup error={error}");
        }
    }
}
// Protocol types remain available on targets without native socket readiness.
// Such targets must reject the server operation rather than poll or fake success.
#[cfg(not(any(unix, windows)))]
pub(super) struct ListenerReadiness<'a> {
    _listener: std::marker::PhantomData<&'a TcpListener>,
}
#[cfg(not(any(unix, windows)))]
impl<'a> ListenerReadiness<'a> {
    pub(super) fn new(_listener: &'a TcpListener) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "formal endpoint socket readiness requires Unix or Windows",
        ))
    }
    pub(super) fn wait_until(&mut self, _deadline: Instant) -> io::Result<bool> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "formal endpoint socket readiness requires Unix or Windows",
        ))
    }
    pub(super) fn close(self) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "formal endpoint socket readiness requires Unix or Windows",
        ))
    }
}
#[cfg(all(test, any(unix, windows)))]
#[path = "readiness_tests.rs"]
mod tests;
