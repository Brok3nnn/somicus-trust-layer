//! Corporate proxy configuration and enterprise authentication modes.
//!
//! Nothing applies this configuration yet (B6.02): [`crate::UrlSessionTransport`] only reports it, and [`crate::send`]
//! does not read it.

use std::fmt;

use crate::debug::Hidden;

/// Authentication credentials for corporate proxies. `Debug` never shows the user name or the password.
#[derive(Clone, PartialEq, Eq)]
pub enum ProxyAuth {
    /// Basic username and password authentication.
    Basic {
        /// Proxy user name.
        username: String,
        /// Proxy password.
        password: String,
    },
    /// Integrated Windows Authentication (`NTLM` / `Negotiate` / `Kerberos`).
    Integrated,
}

impl fmt::Debug for ProxyAuth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Basic { .. } => formatter
                .debug_struct("Basic")
                .field("username", &Hidden)
                .field("password", &Hidden)
                .finish(),
            Self::Integrated => formatter.write_str("Integrated"),
        }
    }
}

/// Configuration for corporate proxy environments.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ProxyConfig {
    /// Direct connection without proxy.
    Direct,
    /// System default proxy settings (`WinHTTP` / macOS System Configuration / Linux env).
    #[default]
    System,
    /// Automatic proxy configuration via `PAC/WPAD` script URL.
    Pac {
        /// URL to the `PAC` (Proxy Auto-Config) file.
        pac_url: String,
    },
    /// Manual `HTTP`/`HTTPS` proxy with optional authentication.
    Manual {
        /// URL of the proxy server.
        proxy_url: String,
        /// Optional authentication credentials.
        auth: Option<ProxyAuth>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_proxy_password_never_prints() {
        let config = ProxyConfig::Manual {
            proxy_url: "http://proxy.example.test:8080".to_owned(),
            auth: Some(ProxyAuth::Basic {
                username: "synthetic-user".to_owned(),
                password: "synthetic-password".to_owned(),
            }),
        };
        let printed = format!("{config:?}");
        assert!(!printed.contains("synthetic-password"), "{printed}");
        assert!(printed.contains("password: <redacted>"), "{printed}");
    }
}
