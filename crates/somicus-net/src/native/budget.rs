//! The time budget of one request: a deadline fixed when it starts, and the `WinHTTP` time-outs taken from what is
//! left of it before every blocking call.

use std::time::{Duration, Instant};

use super::Failure;

/// The four `WinHTTP` time-outs in milliseconds, for `WinHttpSetTimeouts`. Never 0, which `WinHTTP` reads as "wait
/// forever".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Timeouts {
    pub(super) resolve: i32,
    pub(super) connect: i32,
    pub(super) send: i32,
    pub(super) receive: i32,
}

impl Timeouts {
    /// For sending, where name resolution, connecting, the TLS handshake and sending happen in one call: each gets a
    /// quarter of what is left, so together they cannot outlast it.
    pub(super) fn sending(left: Duration) -> Self {
        let quarter = milliseconds(left / 4);
        Self {
            resolve: quarter,
            connect: quarter,
            send: quarter,
            receive: quarter,
        }
    }

    /// For receiving: what is left goes to waiting for the answer; a redirect's own resolution, connection and sending
    /// get a quarter each.
    pub(super) fn receiving(left: Duration) -> Self {
        let quarter = milliseconds(left / 4);
        Self {
            resolve: quarter,
            connect: quarter,
            send: quarter,
            receive: milliseconds(left),
        }
    }
}

/// `duration` in whole milliseconds for `WinHTTP`, at least 1 and at most `i32::MAX`.
fn milliseconds(duration: Duration) -> i32 {
    i32::try_from(duration.as_millis())
        .unwrap_or(i32::MAX)
        .max(1)
}

/// The time a request has left, from a deadline fixed when it starts. Before every blocking `WinHTTP` call, the call's
/// time-outs are taken from what is left, and a request whose time is up fails.
pub(super) struct Budget {
    deadline: Instant,
}

impl Budget {
    /// A budget of `total` from now; one too large to represent is spent at once, so the request fails closed.
    pub(super) fn new(total: Duration) -> Self {
        let now = Instant::now();
        Self {
            deadline: now.checked_add(total).unwrap_or(now),
        }
    }

    fn left(&self) -> Result<Duration, Failure> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|left| !left.is_zero())
            .ok_or(Failure::TimedOut)
    }

    pub(super) fn for_sending(&self) -> Result<Timeouts, Failure> {
        self.left().map(Timeouts::sending)
    }

    pub(super) fn for_receiving(&self) -> Result<Timeouts, Failure> {
        self.left().map(Timeouts::receiving)
    }
}
