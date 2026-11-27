//! Início com o Windows pela chave `Run` do usuário. O valor aponta para `flowpause-bg.exe`,
//! que inicia `flowpause run` sem janela e com a saída no arquivo de log.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW,
};
use windows::core::w;

const RUN_KEY: windows::core::PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const VALUE_NAME: windows::core::PCWSTR = w!("flowpause");
const LAUNCHER: &str = "flowpause-bg.exe";

/// Grava o valor na chave `Run` e devolve o caminho do lançador.
pub fn enable() -> Result<PathBuf> {
    let launcher = std::env::current_exe()
        .context("não foi possível achar o caminho do flowpause.exe")?
        .with_file_name(LAUNCHER);
    if !launcher.exists() {
        bail!(
            "{} não existe; reinstale com `cargo install --path . --locked`",
            launcher.display()
        );
    }
    let command: Vec<u16> = format!("\"{}\"", launcher.display())
        .encode_utf16()
        .chain(Some(0))
        .collect();
    // SAFETY: `command` é uma string UTF-16 terminada em zero, com o tamanho em bytes informado.
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE_NAME,
            REG_SZ.0,
            Some(command.as_ptr().cast()),
            (command.len() * 2) as u32,
        )
    };
    if status != ERROR_SUCCESS {
        bail!("falha ao gravar na chave Run: {}", status.to_hresult().message());
    }
    Ok(launcher)
}

/// Apaga o valor da chave `Run`. Devolve `false` se ele já não existia.
pub fn disable() -> Result<bool> {
    // SAFETY: chave e nome são strings constantes terminadas em zero.
    let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE_NAME) };
    match status {
        ERROR_SUCCESS => Ok(true),
        ERROR_FILE_NOT_FOUND => Ok(false),
        _ => bail!("falha ao apagar da chave Run: {}", status.to_hresult().message()),
    }
}
