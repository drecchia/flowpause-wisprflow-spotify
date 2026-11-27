//! Tokens no Gerenciador de Credenciais do Windows. Nada vai para arquivo.

use anyhow::{Context, Result};
use keyring::Entry;

use super::auth::AccessToken;

const SERVICE: &str = "flowpause";
const REFRESH_TOKEN_USER: &str = "spotify-refresh-token";
const ACCESS_TOKEN_USER: &str = "spotify-access-token";

pub fn load_refresh_token() -> Result<Option<String>> {
    match entry(REFRESH_TOKEN_USER)?.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(err).context("falha ao ler o refresh token do Gerenciador de Credenciais"),
    }
}

pub fn save_refresh_token(token: &str) -> Result<()> {
    entry(REFRESH_TOKEN_USER)?
        .set_password(token)
        .context("falha ao gravar o refresh token no Gerenciador de Credenciais")
}

/// Access token guardado entre execuções da CLI. Se faltar ou estiver ilegível, o cliente renova.
pub fn load_access_token() -> Option<AccessToken> {
    let json = entry(ACCESS_TOKEN_USER).ok()?.get_password().ok()?;
    serde_json::from_str(&json).ok()
}

pub fn save_access_token(token: &AccessToken) -> Result<()> {
    let json = serde_json::to_string(token)?;
    entry(ACCESS_TOKEN_USER)?
        .set_password(&json)
        .context("falha ao gravar o access token no Gerenciador de Credenciais")
}

fn entry(user: &str) -> Result<Entry> {
    Entry::new(SERVICE, user).context("Gerenciador de Credenciais indisponível")
}
