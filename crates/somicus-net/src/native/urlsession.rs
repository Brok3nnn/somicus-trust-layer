//! The `NSURLSession` calls, which only macOS compiles (ADR-0049). Each request runs in its own ephemeral session: no
//! cookies, cache or stored credentials, the system's proxy settings and trusted roots, and App Transport Security's
//! TLS 1.2 or later. A delegate answers the session's questions on its own queue: it refuses a redirect the caller does
//! not allow, rejects every sign-in challenge (only the server's certificate is checked, the system's way), and passes
//! the status and each piece of the body to the caller through a channel, so a body over its limit is cancelled while
//! it arrives. Every Objective-C call is behind a safe function here with a `// SAFETY:` comment, and the session is
//! invalidated before the caller returns, which releases the delegate.

use std::ptr;
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Instant;

use block2::DynBlock;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{AllocAnyThread as _, DefinedClass as _, define_class, msg_send};
use objc2_foundation::{
    NSData, NSError, NSHTTPCookieAcceptPolicy, NSHTTPURLResponse, NSMutableURLRequest, NSObject,
    NSObjectProtocol, NSString, NSURL, NSURLAuthenticationChallenge,
    NSURLAuthenticationMethodServerTrust, NSURLCredential, NSURLRequest, NSURLRequestCachePolicy,
    NSURLResponse, NSURLSession, NSURLSessionAuthChallengeDisposition, NSURLSessionConfiguration,
    NSURLSessionDataDelegate, NSURLSessionDataTask, NSURLSessionDelegate,
    NSURLSessionResponseDisposition, NSURLSessionTask, NSURLSessionTaskDelegate,
};

use super::{Answer, Exchange, Failure, Redirects, StreamedBody, status_of};
use crate::error::NetError;

/// The user agent the requests send, as `WinHTTP`'s do.
const AGENT: &str = "Somicus";

/// What the delegate tells the caller, in the order the session reports it.
enum Event {
    /// The answer's status line arrived.
    Response(u16),
    /// A piece of the body arrived.
    Data(Vec<u8>),
    /// The request ended, normally or with why it failed.
    Finished(Option<Failure>),
}

/// The delegate's state: where events go, and the redirects it may still follow.
struct Ivars {
    events: Mutex<Option<Sender<Event>>>,
    redirects: Redirects,
    followed: Mutex<u32>,
}

impl Ivars {
    /// Sends `event` to the caller, who may already have stopped listening.
    fn send(&self, event: Event) {
        if let Ok(events) = self.events.lock()
            && let Some(sender) = events.as_ref()
        {
            let _ = sender.send(event);
        }
    }

    /// Whether to follow a redirect to `scheme`, counting it when it is followed.
    fn follow(&self, scheme: Option<&str>) -> bool {
        let Redirects::HttpsOnly(most) = self.redirects else {
            return false;
        };
        let Ok(mut followed) = self.followed.lock() else {
            return false;
        };
        if scheme == Some("https") && *followed < most {
            *followed += 1;
            true
        } else {
            false
        }
    }
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and `Delegate` does not implement `Drop`. Its ivars are `Send`
    // and `Sync`, as the session's delegate protocols require: the session calls it on its own queue.
    #[unsafe(super = NSObject)]
    #[name = "SomicusURLSessionDelegate"]
    #[ivars = Ivars]
    struct Delegate;

    // SAFETY: `NSObjectProtocol` has no safety requirements.
    unsafe impl NSObjectProtocol for Delegate {}

    // SAFETY: the methods below have the protocol's signatures.
    unsafe impl NSURLSessionDelegate for Delegate {}

    // SAFETY: the methods below have the protocol's signatures.
    unsafe impl NSURLSessionTaskDelegate for Delegate {
        /// Follows a redirect only where the caller allows it, only to HTTPS: a refused redirect's answer is the answer.
        #[unsafe(method(URLSession:task:willPerformHTTPRedirection:newRequest:completionHandler:))]
        fn will_redirect(
            &self,
            _session: &NSURLSession,
            _task: &NSURLSessionTask,
            _response: &NSHTTPURLResponse,
            request: &NSURLRequest,
            completion: &DynBlock<dyn Fn(*mut NSURLRequest)>,
        ) {
            let scheme = request
                .URL()
                .and_then(|url| url.scheme())
                .map(|scheme| scheme.to_string());
            let next = if self.ivars().follow(scheme.as_deref()) {
                ptr::from_ref(request).cast_mut()
            } else {
                ptr::null_mut()
            };
            completion.call((next,));
        }

        /// Lets the system check the server's certificate and rejects every other challenge: no automatic sign-in to a
        /// server or a proxy, no client certificate. A rejected sign-in leaves its 401 or 407 as the answer, as curl
        /// leaves it.
        #[unsafe(method(URLSession:task:didReceiveChallenge:completionHandler:))]
        fn challenge(
            &self,
            _session: &NSURLSession,
            _task: &NSURLSessionTask,
            challenge: &NSURLAuthenticationChallenge,
            completion: &DynBlock<
                dyn Fn(NSURLSessionAuthChallengeDisposition, *mut NSURLCredential),
            >,
        ) {
            let method = challenge.protectionSpace().authenticationMethod();
            // SAFETY: the static is a constant string Foundation defines.
            let server_trust = unsafe { NSURLAuthenticationMethodServerTrust };
            let disposition = if method.isEqualToString(server_trust) {
                NSURLSessionAuthChallengeDisposition::PerformDefaultHandling
            } else {
                NSURLSessionAuthChallengeDisposition::RejectProtectionSpace
            };
            completion.call((disposition, ptr::null_mut()));
        }

        /// Reports the end of the request.
        #[unsafe(method(URLSession:task:didCompleteWithError:))]
        fn completed(
            &self,
            _session: &NSURLSession,
            _task: &NSURLSessionTask,
            error: Option<&NSError>,
        ) {
            self.ivars().send(Event::Finished(error.map(failure_of)));
        }
    }

    // SAFETY: the methods below have the protocol's signatures.
    unsafe impl NSURLSessionDataDelegate for Delegate {
        /// Reports the status, and cancels an answer whose status is not valid or comes from a proxy that wants
        /// sign-in.
        #[unsafe(method(URLSession:dataTask:didReceiveResponse:completionHandler:))]
        fn responded(
            &self,
            _session: &NSURLSession,
            _task: &NSURLSessionDataTask,
            response: &NSURLResponse,
            completion: &DynBlock<dyn Fn(NSURLSessionResponseDisposition)>,
        ) {
            let status = response
                .downcast_ref::<NSHTTPURLResponse>()
                .map_or(Err(Failure::NoStatus), |http| {
                    status_of(u32::try_from(http.statusCode()).unwrap_or(0))
                });
            let disposition = match status {
                Ok(status) => {
                    self.ivars().send(Event::Response(status));
                    NSURLSessionResponseDisposition::Allow
                }
                Err(failure) => {
                    self.ivars().send(Event::Finished(Some(failure)));
                    NSURLSessionResponseDisposition::Cancel
                }
            };
            completion.call((disposition,));
        }

        /// Passes a piece of the body on.
        #[unsafe(method(URLSession:dataTask:didReceiveData:))]
        fn received(&self, _session: &NSURLSession, _task: &NSURLSessionDataTask, data: &NSData) {
            self.ivars().send(Event::Data(data.to_vec()));
        }
    }
);

impl Delegate {
    fn new(events: Sender<Event>, redirects: Redirects) -> Retained<Self> {
        let delegate = Self::alloc().set_ivars(Ivars {
            events: Mutex::new(Some(events)),
            redirects,
            followed: Mutex::new(0),
        });
        // SAFETY: `init` is NSObject's designated initializer, called once on the freshly allocated object.
        unsafe { msg_send![super(delegate), init] }
    }

    /// Stops sending events: the caller has its answer.
    fn close(&self) {
        if let Ok(mut events) = self.ivars().events.lock() {
            *events = None;
        }
    }
}

/// The failure an `NSURLError` code stands for, by its absolute value; codes only, never the error's description.
fn failure_of(error: &NSError) -> Failure {
    Failure::Url(i32::try_from(error.code()).unwrap_or(i32::MIN))
}

/// One running request: its session, its task and the events of its delegate. Dropping it cancels the task and
/// invalidates the session, which releases the delegate.
struct Running {
    session: Retained<NSURLSession>,
    task: Retained<NSURLSessionDataTask>,
    delegate: Retained<Delegate>,
    events: Receiver<Event>,
    deadline: Instant,
}

impl Running {
    /// The next event, or a failure once the time budget has run out.
    fn next(&self) -> Result<Event, Failure> {
        let left = self.deadline.saturating_duration_since(Instant::now());
        match self.events.recv_timeout(left) {
            Ok(event) => Ok(event),
            Err(RecvTimeoutError::Timeout) => Err(Failure::TimedOut),
            Err(RecvTimeoutError::Disconnected) => Err(Failure::NoStatus),
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.delegate.close();
        self.task.cancel();
        self.session.invalidateAndCancel();
    }
}

/// The URL of `exchange` with `scheme`: `https` for every request the app sends.
fn url_of(exchange: &Exchange<'_>, scheme: &str) -> String {
    let target = &exchange.target;
    if target.host.contains(':') {
        format!(
            "{scheme}://[{}]:{}{}",
            target.host, target.port, target.path
        )
    } else {
        format!("{scheme}://{}:{}{}", target.host, target.port, target.path)
    }
}

/// The headers of `exchange` as name and value pairs, from its checked `Name: value` block.
fn headers_of(exchange: &Exchange<'_>) -> Vec<(String, String)> {
    String::from_utf16_lossy(&exchange.headers)
        .split("\r\n")
        .filter_map(|line| line.split_once(": "))
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect()
}

/// The ephemeral configuration every request runs in: nothing kept between requests, nothing taken from a store.
fn configuration(budget_seconds: f64) -> Retained<NSURLSessionConfiguration> {
    let configuration = NSURLSessionConfiguration::ephemeralSessionConfiguration();
    configuration
        .setRequestCachePolicy(NSURLRequestCachePolicy::ReloadIgnoringLocalAndRemoteCacheData);
    configuration.setURLCache(None);
    configuration.setURLCredentialStorage(None);
    configuration.setHTTPCookieStorage(None);
    configuration.setHTTPShouldSetCookies(false);
    configuration.setHTTPCookieAcceptPolicy(NSHTTPCookieAcceptPolicy::Never);
    configuration.setTimeoutIntervalForRequest(budget_seconds);
    configuration.setTimeoutIntervalForResource(budget_seconds);
    configuration.setWaitsForConnectivity(false);
    configuration
}

/// The request for `exchange` at `url`.
fn request_of(
    exchange: &Exchange<'_>,
    url: &str,
    budget_seconds: f64,
) -> Result<Retained<NSMutableURLRequest>, Failure> {
    let url = NSURL::URLWithString(&NSString::from_str(url)).ok_or(Failure::Setup(0))?;
    let request = NSMutableURLRequest::requestWithURL(&url);
    request.setHTTPMethod(&NSString::from_str(exchange.method));
    request.setCachePolicy(NSURLRequestCachePolicy::ReloadIgnoringLocalAndRemoteCacheData);
    request.setHTTPShouldHandleCookies(false);
    request.setTimeoutInterval(budget_seconds);
    request.setValue_forHTTPHeaderField(
        Some(&NSString::from_str(AGENT)),
        &NSString::from_str("User-Agent"),
    );
    for (name, value) in headers_of(exchange) {
        request.setValue_forHTTPHeaderField(
            Some(&NSString::from_str(&value)),
            &NSString::from_str(&name),
        );
    }
    if !exchange.body.is_empty() {
        request.setHTTPBody(Some(&NSData::with_bytes(exchange.body)));
    }
    Ok(request)
}

/// Starts `exchange` at `url` in a session of its own.
fn start(exchange: &Exchange<'_>, url: &str) -> Result<Running, Failure> {
    let budget_seconds = exchange.limits.budget.as_secs_f64();
    let request = request_of(exchange, url, budget_seconds)?;
    let (sender, events) = mpsc::channel();
    let delegate = Delegate::new(sender, exchange.limits.redirects);
    // SAFETY: with no queue the session creates its own serial queue for the delegate, which needs nothing more: its
    // state is behind locks.
    let session = unsafe {
        NSURLSession::sessionWithConfiguration_delegate_delegateQueue(
            &configuration(budget_seconds),
            Some(ProtocolObject::from_ref(&*delegate)),
            None,
        )
    };
    let task = session.dataTaskWithRequest(&request);
    task.resume();
    Ok(Running {
        session,
        task,
        delegate,
        events,
        deadline: Instant::now() + exchange.limits.budget,
    })
}

/// Sends `exchange` and reads its answer. Blocking: run it off the async runtime.
///
/// # Errors
/// Returns [`Failure`] when the request cannot be set up, the exchange fails, the budget runs out, the answer has no
/// valid status, or a proxy asks for sign-in.
pub(crate) fn exchange(exchange: &Exchange<'_>) -> Result<Answer, Failure> {
    run(exchange, "https")
}

fn run(exchange: &Exchange<'_>, scheme: &str) -> Result<Answer, Failure> {
    let running = start(exchange, &url_of(exchange, scheme))?;
    let limit = exchange.limits.bytes;
    let mut status = None;
    let mut body = Vec::new();
    loop {
        match running.next()? {
            Event::Response(code) => status = Some(code),
            Event::Data(chunk) => {
                if chunk.len() > limit.saturating_sub(body.len()) {
                    return Ok(Answer::TooLarge);
                }
                body.extend_from_slice(&chunk);
            }
            Event::Finished(Some(failure)) => return Err(failure),
            Event::Finished(None) => {
                return status
                    .map(|status| Answer::Complete { status, body })
                    .ok_or(Failure::NoStatus);
            }
        }
    }
}

/// Streams a file answer into `sink`, accepting only `expected_status` and never following redirects. A broken read
/// reports the bytes already written so the pinned downloader can retry from that offset.
///
/// # Errors
/// Returns [`NetError`] when the request cannot run or the sink fails.
pub(crate) fn exchange_stream(
    exchange: &Exchange<'_>,
    expected_status: u16,
    sink: &mut dyn FnMut(&[u8]) -> std::io::Result<()>,
) -> Result<StreamedBody, NetError> {
    stream(exchange, "https", expected_status, sink)
}

fn stream(
    exchange: &Exchange<'_>,
    scheme: &str,
    expected_status: u16,
    sink: &mut dyn FnMut(&[u8]) -> std::io::Result<()>,
) -> Result<StreamedBody, NetError> {
    let running = start(exchange, &url_of(exchange, scheme))?;
    let limit = u64::try_from(exchange.limits.bytes).unwrap_or(u64::MAX);
    let mut written = 0_u64;
    loop {
        match running.next() {
            Ok(Event::Response(status)) if status != expected_status => {
                return Ok(StreamedBody::BrokeOff(0));
            }
            Ok(Event::Response(_)) => {}
            Ok(Event::Data(chunk)) => {
                let count = u64::try_from(chunk.len()).unwrap_or(u64::MAX);
                if count > limit.saturating_sub(written) {
                    return Ok(StreamedBody::TooLarge);
                }
                sink(&chunk).map_err(NetError::Io)?;
                written += count;
            }
            Ok(Event::Finished(None)) => return Ok(StreamedBody::Finished(written)),
            Ok(Event::Finished(Some(_))) | Err(_) => return Ok(StreamedBody::BrokeOff(written)),
        }
    }
}

#[cfg(test)]
mod tests;
