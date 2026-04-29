//! Auth helpers — OAuth state signing, proxy token exchange/refresh, Vault
//! writes (mint/finalize/mark/fanout credentials).
//!
//! Called by core HTTP handlers in `core/src/api/auth.rs` (Phase 3) and by
//! the `credential_refresh` cron action (Phase 4).

pub mod error;
pub mod proxy;
pub mod state;
pub mod vault;

pub use error::{AuthError, Result};
pub use proxy::{proxy_exchange, proxy_refresh, ProxyExchangeResponse};
pub use state::{sign_oauth_state, verify_oauth_state};
pub use vault::{
    fanout_action_ids, finalize_apikey_credential, finalize_credential,
    finalize_self_issued_bearer, mark_credential_status, mint_pending_credential,
    read_credential_secrets, update_credential_secrets, CredentialStatus,
};
