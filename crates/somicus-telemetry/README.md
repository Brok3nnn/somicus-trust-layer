# somicus-telemetry

Content-free diagnostics for the Somicus Rust crates: `tracing` spans and events limited to an allow-list of fields.

## What it does

- Owns the only way the other crates record a span or an event, and the field allow-list that decides what may be
  recorded.
- `init()` installs the process-wide subscriber: in **debug builds** one line per event and per closed span at `INFO`
  and above goes to standard error; **release builds install nothing**. There is no exporter.
- Never records screen text, field values, prompts, model output, URLs, file paths, tokens, keys, e-mail addresses or
  any message text, and never uses the network, writes files or reads the environment.

## What may be recorded

| Kind  | Fields                                                                              | Holds                                                   |
| ----- | ----------------------------------------------------------------------------------- | ------------------------------------------------------- |
| label | `command`, `kind`, `outcome`, `reason`, `transport`, `decision`, `risk`, `autonomy` | a fixed name: an enum variant, an error kind, a command |
| count | `status`, `bytes`, `seq`, `nodes`, `redactions`                                     | an unsigned number                                      |

A span's duration is measured by the layer when it closes. The list lives in `src/field.rs`; adding a name is a
security-sensitive change.

## How the allow-list is enforced

1. **At compile time.** `span!`, `record!` and `event!` accept only the names above. A label field takes a `Label`: a
   `&'static str` or a type whose `label()` returns fixed names, so a `String` or a borrowed `&str` read at runtime
   does not compile. A count field takes an unsigned integer.
2. **At run time.** The `AllowList` layer admits only spans and events whose target is a Somicus crate, drops every
   field that is not on the list or whose value does not fit its kind (a label must be at most 48 lower-case ASCII
   letters, digits and underscores, starting with a letter; `Debug`, `Display`, error, boolean and float values are
   never read), and hands a `Record` of what is left to a `Sink`. A raw `tracing` call cannot add content either.

Residual risks: a single lower-case word could pass the layer's shape check if code turned runtime text into a
`&'static str` (for example by leaking it); the compile-time bound makes that a deliberate act a reviewer sees. And
`tracing`'s `log` feature would also hand events to a `log` logger; no dependency enables it and none is installed.

## Public API

`span!`, `record!`, `event!`; `Span` (`record_result`, `in_scope`, `instrument`); `init()`; `Label`, `Count`,
`Outcome`; `AllowList`, `Record`, `Sink`. The `testing` feature (dev-dependencies only) adds `testing::capture`, which
each crate's tests use to prove its spans carry only allowed fields and none of the synthetic content it handled.

## Dependencies

`tracing` and `tracing-subscriber` (registry only, no formatter). No Somicus crate.

## Testing

`cargo test -p somicus-telemetry`.
