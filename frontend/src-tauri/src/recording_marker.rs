//! Marca em disco que há uma gravação em andamento.
//!
//! O script `scripts/atualizar-mac.sh` lê `recording.active` na pasta de dados
//! do app antes de fechar o app para instalar uma versão nova. O arquivo fica
//! vazio: não guarda título nem conteúdo da reunião.

use std::path::PathBuf;
use std::time::Duration;

use tauri::{AppHandle, Manager, Runtime};

const FILE_NAME: &str = "recording.active";
const POLL: Duration = Duration::from_secs(2);

fn marker_path<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|dir| dir.join(FILE_NAME))
}

/// Remove um marcador que sobrou de uma execução anterior e passa a manter o
/// arquivo em sincronia com o estado de gravação.
pub fn start<R: Runtime>(app: &AppHandle<R>) {
    let Some(path) = marker_path(app) else {
        return;
    };
    let _ = std::fs::remove_file(&path);

    tauri::async_runtime::spawn(async move {
        let mut marked = false;
        loop {
            let recording = crate::audio::recording_commands::is_recording().await;
            if recording != marked {
                let result = if recording {
                    std::fs::write(&path, b"")
                } else {
                    match std::fs::remove_file(&path) {
                        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
                        _ => Ok(()),
                    }
                };
                match result {
                    Ok(()) => marked = recording,
                    Err(error) => log::warn!("Recording marker could not be updated: {}", error),
                }
            }
            tokio::time::sleep(POLL).await;
        }
    });
}
