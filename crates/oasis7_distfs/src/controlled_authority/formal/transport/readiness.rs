//! Borrow-scoped socket registration: no polling interval is added to RPC latency.
use polling::{Event, Events, Poller};
use std::{io, net::TcpListener, time::Instant};

pub(super) struct ListenerReadiness<'a> {
    listener: &'a TcpListener,
    poller: Poller,
    events: Events,
    registered: bool,
}
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
#[cfg(test)]
#[path = "readiness_tests.rs"]
mod tests;
