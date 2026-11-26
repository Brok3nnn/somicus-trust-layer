//! The `WinHTTP` calls, which only Windows compiles. Each one is behind a safe function here with a `// SAFETY:`
//! comment. Handles close when dropped, the request before its connection and the connection before its session, and
//! everything the calls read is borrowed for the whole exchange, which closes its handles before it returns.

use std::ffi::c_void;

use windows::Win32::Foundation::GetLastError;
use windows::Win32::Networking::WinHttp::{
    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_AUTOLOGON_SECURITY_LEVEL_HIGH,
    WINHTTP_DISABLE_AUTHENTICATION, WINHTTP_DISABLE_COOKIES, WINHTTP_DISABLE_REDIRECTS,
    WINHTTP_FLAG_ESCAPE_DISABLE, WINHTTP_FLAG_ESCAPE_DISABLE_QUERY, WINHTTP_FLAG_SECURE,
    WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2, WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3,
    WINHTTP_OPTION_AUTOLOGON_POLICY, WINHTTP_OPTION_DISABLE_FEATURE,
    WINHTTP_OPTION_MAX_HTTP_AUTOMATIC_REDIRECTS, WINHTTP_OPTION_RECEIVE_RESPONSE_TIMEOUT,
    WINHTTP_OPTION_REDIRECT_POLICY, WINHTTP_OPTION_REDIRECT_POLICY_DISALLOW_HTTPS_TO_HTTP,
    WINHTTP_OPTION_SECURE_PROTOCOLS, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
    WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
    WinHttpSetOption, WinHttpSetTimeouts,
};
use windows::core::PCWSTR;

use super::{
    Answer, Budget, Exchange, Failure, INVALID_SERVER_RESPONSE, Redirects, StreamedBody, Timeouts,
    read_limited, read_streamed, status_of, wide, win32_code,
};
use crate::error::NetError;

/// The user agent `WinHTTP` sends.
const AGENT: &str = "Somicus";

/// Sends `exchange` and reads its answer. Blocking: run it off the async runtime.
///
/// # Errors
/// Returns [`Failure`] when `WinHTTP` cannot set the request up, the exchange fails, the budget runs out, the answer
/// has no valid status, or a proxy asks for sign-in.
pub(crate) fn exchange(exchange: &Exchange<'_>) -> Result<Answer, Failure> {
    with_response(exchange, |request, budget, status| {
        let body = read_limited(
            |chunk| read_chunk(request, budget, chunk),
            exchange.limits.bytes,
        )?;
        Ok(body.map_or(Answer::TooLarge, |body| Answer::Complete { status, body }))
    })
}

/// Streams a file answer into `sink`, accepting only `expected_status` and never following redirects. A broken read
/// reports the bytes already written so the pinned downloader can retry from that offset.
pub(crate) fn exchange_stream(
    exchange: &Exchange<'_>,
    expected_status: u16,
    sink: &mut dyn FnMut(&[u8]) -> std::io::Result<()>,
) -> Result<StreamedBody, NetError> {
    with_response(exchange, |request, budget, status| {
        if status != expected_status {
            return Ok(StreamedBody::BrokeOff(0));
        }
        read_streamed(
            |chunk| read_chunk(request, budget, chunk),
            u64::try_from(exchange.limits.bytes).unwrap_or(u64::MAX),
            sink,
        )
        .map_err(NetError::from)
    })
}

/// Opens a request, sends it and passes its response to `read` while all three `WinHTTP` handles remain alive.
fn with_response<T, E: From<Failure>>(
    exchange: &Exchange<'_>,
    read: impl FnOnce(&Handle, &Budget, u16) -> Result<T, E>,
) -> Result<T, E> {
    let budget = Budget::new(exchange.limits.budget);
    let session = open_session().map_err(Failure::Setup)?;
    let connection = connect(&session, exchange).map_err(Failure::Setup)?;
    let request = open_request(&connection, exchange).map_err(Failure::Setup)?;
    restrict(&request, exchange.limits.redirects).map_err(Failure::Setup)?;
    set_timeouts(&request, budget.for_sending()?).map_err(Failure::Setup)?;
    send(&request, exchange).map_err(Failure::Exchange)?;
    let timeouts = budget.for_receiving()?;
    set_timeouts(&request, timeouts).map_err(Failure::Setup)?;
    set_option(
        &request,
        WINHTTP_OPTION_RECEIVE_RESPONSE_TIMEOUT,
        timeouts.receive.unsigned_abs(),
    )
    .map_err(Failure::Setup)?;
    receive(&request).map_err(Failure::Exchange)?;
    let status = status_of(status(&request).map_err(Failure::Exchange)?)?;
    read(&request, &budget, status)
}

/// An open `WinHTTP` handle (`HINTERNET`), closed when dropped.
struct Handle(*mut c_void);

impl Handle {
    /// Takes `raw`, or the thread's last error when `WinHTTP` returned no handle.
    fn take(raw: *mut c_void) -> Result<Self, u32> {
        if raw.is_null() {
            Err(last_error())
        } else {
            Ok(Self(raw))
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a handle `WinHTTP` opened, which nothing else closes; dropping closes it exactly once.
        // Closing fails only for a handle that is not valid, so the result tells nothing to act on.
        let _ = unsafe { WinHttpCloseHandle(self.0) };
    }
}

/// The calling thread's last error code.
fn last_error() -> u32 {
    // SAFETY: `GetLastError` reads the calling thread's last error and has no preconditions.
    unsafe { GetLastError() }.0
}

/// The Win32 error code of a failed call.
fn code_of(error: &windows::core::Error) -> u32 {
    win32_code(error.code().0)
}

/// Sets the `u32` option `option` on `handle`.
fn set_option(handle: &Handle, option: u32, value: u32) -> Result<(), u32> {
    let bytes = value.to_ne_bytes();
    // SAFETY: `handle` is open, and the buffer is a `u32` in native byte order with its length, as `WinHTTP`'s `u32`
    // options require; the call copies it before it returns.
    unsafe { WinHttpSetOption(Some(handle.0.cast_const()), option, Some(&bytes)) }
        .map_err(|error| code_of(&error))
}

/// Sets the four time-outs of `request`.
fn set_timeouts(request: &Handle, timeouts: Timeouts) -> Result<(), u32> {
    // SAFETY: `request` is open and the time-outs are plain integers; none is 0 (no limit), as `Timeouts` guarantees.
    unsafe {
        WinHttpSetTimeouts(
            request.0,
            timeouts.resolve,
            timeouts.connect,
            timeouts.send,
            timeouts.receive,
        )
    }
    .map_err(|error| code_of(&error))
}

/// A synchronous session that uses the system's proxy settings and allows TLS 1.2 and 1.3 only, or TLS 1.2 only on a
/// Windows whose `WinHTTP` has no TLS 1.3. Never an older protocol: the option replaces `WinHTTP`'s default set.
fn open_session() -> Result<Handle, u32> {
    let agent = wide(AGENT);
    // SAFETY: `agent` is a NUL-terminated UTF-16 string that lives through the call, which copies it; the automatic
    // proxy type takes no proxy name or bypass list, so both are null; no flags makes the session synchronous.
    let session = Handle::take(unsafe {
        WinHttpOpen(
            PCWSTR(agent.as_ptr()),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        )
    })?;
    set_option(
        &session,
        WINHTTP_OPTION_SECURE_PROTOCOLS,
        WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2 | WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3,
    )
    .or_else(|_| {
        set_option(
            &session,
            WINHTTP_OPTION_SECURE_PROTOCOLS,
            WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2,
        )
    })?;
    Ok(session)
}

/// The connection to the exchange's host and port.
fn connect(session: &Handle, exchange: &Exchange<'_>) -> Result<Handle, u32> {
    let host = wide(&exchange.target.host);
    // SAFETY: `session` is open; `host` is a NUL-terminated UTF-16 string that lives through the call, which copies
    // it; the reserved argument is 0.
    Handle::take(unsafe {
        WinHttpConnect(session.0, PCWSTR(host.as_ptr()), exchange.target.port, 0)
    })
}

/// The HTTPS request on `connection`. The path goes out exactly as given (no escaping), as curl sends it: it is
/// visible ASCII only, so it cannot end the request line.
fn open_request(connection: &Handle, exchange: &Exchange<'_>) -> Result<Handle, u32> {
    let verb = wide(exchange.method);
    let object = wide(&exchange.target.path);
    let flags =
        WINHTTP_FLAG_SECURE | WINHTTP_FLAG_ESCAPE_DISABLE | WINHTTP_FLAG_ESCAPE_DISABLE_QUERY;
    // SAFETY: `connection` is open; `verb` and `object` are NUL-terminated UTF-16 strings that live through the call,
    // which copies them; a null version means HTTP/1.1, a null referrer and null accept types send neither.
    Handle::take(unsafe {
        WinHttpOpenRequest(
            connection.0,
            PCWSTR(verb.as_ptr()),
            PCWSTR(object.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            flags,
        )
    })
}

/// No cookies, no automatic sign-in and never the person's Windows credentials; redirects as `redirects` allows.
fn restrict(request: &Handle, redirects: Redirects) -> Result<(), u32> {
    let mut disabled = WINHTTP_DISABLE_COOKIES | WINHTTP_DISABLE_AUTHENTICATION;
    match redirects {
        Redirects::Never => disabled |= WINHTTP_DISABLE_REDIRECTS,
        Redirects::HttpsOnly(most) => {
            set_option(
                request,
                WINHTTP_OPTION_REDIRECT_POLICY,
                WINHTTP_OPTION_REDIRECT_POLICY_DISALLOW_HTTPS_TO_HTTP,
            )?;
            set_option(request, WINHTTP_OPTION_MAX_HTTP_AUTOMATIC_REDIRECTS, most)?;
        }
    }
    set_option(request, WINHTTP_OPTION_DISABLE_FEATURE, disabled)?;
    set_option(
        request,
        WINHTTP_OPTION_AUTOLOGON_POLICY,
        WINHTTP_AUTOLOGON_SECURITY_LEVEL_HIGH,
    )
}

/// Sends the headers and the body.
fn send(request: &Handle, exchange: &Exchange<'_>) -> Result<(), u32> {
    let headers = (!exchange.headers.is_empty()).then_some(exchange.headers.as_slice());
    let body = (!exchange.body.is_empty()).then(|| exchange.body.as_ptr().cast::<c_void>());
    // SAFETY: `request` is open. `headers` goes with its length in UTF-16 units, so it needs no NUL, and `body` points
    // to `body_length` readable bytes (`Exchange::with_body` set both from the same slice). Both are borrowed for the
    // whole exchange, which closes this handle before it returns, so they outlive every use `WinHTTP` makes of them
    // (the body must stay until `WinHttpReceiveResponse` completes). The context value is unused in a synchronous
    // session.
    unsafe {
        WinHttpSendRequest(
            request.0,
            headers,
            body,
            exchange.body_length,
            exchange.body_length,
            0,
        )
    }
    .map_err(|error| code_of(&error))
}

/// Waits for the answer's headers.
fn receive(request: &Handle) -> Result<(), u32> {
    // SAFETY: `request` is open and sent; the reserved argument must be null.
    unsafe { WinHttpReceiveResponse(request.0, std::ptr::null_mut()) }
        .map_err(|error| code_of(&error))
}

/// The answer's status code.
fn status(request: &Handle) -> Result<u32, u32> {
    let mut status: u32 = 0;
    let mut size = u32::BITS / 8;
    // SAFETY: `request` has its answer's headers. With `WINHTTP_QUERY_FLAG_NUMBER` the status is written as a `u32`
    // into `status`, whose size in bytes `size` gives; no header name and no index are used, so both are null.
    unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&raw mut status).cast::<c_void>()),
            &raw mut size,
            std::ptr::null_mut(),
        )
    }
    .map_err(|error| code_of(&error))?;
    Ok(status)
}

/// Reads the next part of the body into `chunk`, waiting no longer than the budget allows; 0 bytes is the end.
fn read_chunk(request: &Handle, budget: &Budget, chunk: &mut [u8]) -> Result<usize, Failure> {
    set_timeouts(request, budget.for_receiving()?).map_err(Failure::Setup)?;
    let mut available: u32 = 0;
    // SAFETY: `request` has its answer's headers; `available` is a writable `u32`.
    unsafe { WinHttpQueryDataAvailable(request.0, &raw mut available) }
        .map_err(|error| Failure::Exchange(code_of(&error)))?;
    let wanted = available.min(u32::try_from(chunk.len()).unwrap_or(u32::MAX));
    if wanted == 0 {
        return Ok(0);
    }
    let mut read: u32 = 0;
    // SAFETY: `chunk` is writable for `wanted` bytes, at most its length, through the call; `read` is a writable `u32`.
    unsafe {
        WinHttpReadData(
            request.0,
            chunk.as_mut_ptr().cast::<c_void>(),
            wanted,
            &raw mut read,
        )
    }
    .map_err(|error| Failure::Exchange(code_of(&error)))?;
    usize::try_from(read).map_err(|_| Failure::Exchange(INVALID_SERVER_RESPONSE))
}

// The tests run only on Windows and have not run yet: run them on a Windows machine with
// `cargo nextest run -p somicus-net` (and `--run-ignored only` for the one that needs the internet).
#[cfg(test)]
mod tests;
