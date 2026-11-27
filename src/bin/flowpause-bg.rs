//! Lançador do início automático: roda `flowpause run` sem janela, com a saída anexada a
//! `%LOCALAPPDATA%\flowpause\logs\flowpause.log`, e termina em seguida.
//! É um binário à parte porque um app de console abriria uma janela no login.

#![windows_subsystem = "windows"]

use std::fs::{self, OpenOptions};
use std::io;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn main() {
    // Sem console, não há onde mostrar uma falha aqui.
    let _ = launch();
}

fn launch() -> io::Result<()> {
    let exe = std::env::current_exe()?.with_file_name("flowpause.exe");
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| io::Error::other("LOCALAPPDATA não definida"))?;
    let logs = PathBuf::from(local_app_data).join("flowpause").join("logs");
    fs::create_dir_all(&logs)?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(logs.join("flowpause.log"))?;
    Command::new(exe)
        .arg("run")
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()?;
    Ok(())
}
