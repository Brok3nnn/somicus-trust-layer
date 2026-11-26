//! The macros that are the only way Somicus code records a span or an event.
//!
//! Each field is written `name = value`. A name that is not on the allow-list does not compile, and each name takes
//! only its kind of value: a label field takes a [`crate::Label`] (a `&'static str` or a type with fixed names), a
//! count field an unsigned integer. A `String`, a borrowed `&str` or a `Debug` value does not compile.

/// Checks one field against the allow-list and converts its value. Used by the other macros.
#[doc(hidden)]
#[macro_export]
macro_rules! __field_value {
    (command, $value:expr) => {
        $crate::field::label(&$value)
    };
    (kind, $value:expr) => {
        $crate::field::label(&$value)
    };
    (outcome, $value:expr) => {
        $crate::field::label(&$value)
    };
    (reason, $value:expr) => {
        $crate::field::label(&$value)
    };
    (transport, $value:expr) => {
        $crate::field::label(&$value)
    };
    (decision, $value:expr) => {
        $crate::field::label(&$value)
    };
    (risk, $value:expr) => {
        $crate::field::label(&$value)
    };
    (autonomy, $value:expr) => {
        $crate::field::label(&$value)
    };
    (status, $value:expr) => {
        $crate::field::count($value)
    };
    (bytes, $value:expr) => {
        $crate::field::count($value)
    };
    (seq, $value:expr) => {
        $crate::field::count($value)
    };
    (nodes, $value:expr) => {
        $crate::field::count($value)
    };
    (redactions, $value:expr) => {
        $crate::field::count($value)
    };
    ($other:ident, $value:expr) => {
        ::core::compile_error!(::core::concat!(
            "`",
            ::core::stringify!($other),
            "` is not on the somicus-telemetry field allow-list"
        ))
    };
}

/// Records an event at a level (`ERROR`, `WARN`, `INFO`, `DEBUG` or `TRACE`) with a fixed name and allow-listed
/// fields only.
///
/// ```ignore
/// somicus_telemetry::event!(ERROR, "desktop_run_failed", reason = "tauri_setup");
/// ```
#[macro_export]
macro_rules! event {
    ($level:ident, $name:literal $(, $field:ident = $value:expr)* $(,)?) => {
        $crate::__private::tracing::event!(
            name: $name,
            $crate::__private::tracing::Level::$level,
            { $( $field = $crate::__field_value!($field, $value) ),* }
        )
    };
}

/// Opens a [`crate::Span`] at a level with a fixed name and allow-listed fields. Every allow-listed field is declared
/// on the span, so [`crate::record!`] can set any of them later; nothing else can be set.
///
/// ```ignore
/// let span = somicus_telemetry::span!(INFO, "net_request", kind = method, transport = "https");
/// ```
#[macro_export]
macro_rules! span {
    ($level:ident, $name:literal $(, $field:ident = $value:expr)* $(,)?) => {{
        let span = $crate::Span::__new($crate::__private::tracing::span!(
            $crate::__private::tracing::Level::$level,
            $name,
            command = $crate::__private::tracing::field::Empty,
            kind = $crate::__private::tracing::field::Empty,
            outcome = $crate::__private::tracing::field::Empty,
            reason = $crate::__private::tracing::field::Empty,
            transport = $crate::__private::tracing::field::Empty,
            decision = $crate::__private::tracing::field::Empty,
            risk = $crate::__private::tracing::field::Empty,
            autonomy = $crate::__private::tracing::field::Empty,
            status = $crate::__private::tracing::field::Empty,
            bytes = $crate::__private::tracing::field::Empty,
            seq = $crate::__private::tracing::field::Empty,
            nodes = $crate::__private::tracing::field::Empty,
            redactions = $crate::__private::tracing::field::Empty,
        ));
        $( $crate::record!(span, $field = $value); )*
        span
    }};
}

/// Sets allow-listed fields on a [`crate::Span`].
///
/// ```ignore
/// somicus_telemetry::record!(span, status = response.status, bytes = response.body.len());
/// ```
#[macro_export]
macro_rules! record {
    ($span:expr, $($field:ident = $value:expr),+ $(,)?) => {{
        let span: &$crate::Span = &$span;
        $( span.__record(::core::stringify!($field), $crate::__field_value!($field, $value)); )+
    }};
}
