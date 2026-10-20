//! Configurações da atualização, por perfil (`settings.json` na pasta do perfil).

use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::Error;

const FILE: &str = "settings.json";
const DAY: i64 = 86_400;
/// Intervalo padrão entre atualizações automáticas: uma semana.
pub const DEFAULT_INTERVAL_DAYS: u32 = 7;
const MAX_INTERVAL_DAYS: u32 = 365;

fn yes() -> bool {
    true
}

fn week() -> u32 {
    DEFAULT_INTERVAL_DAYS
}

/// O que a pessoa pode ajustar (na CLI: `baseconhecimento config`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Se `atualizar --se-vencido` (o agendador) pode atualizar sozinho.
    #[serde(default = "yes")]
    pub auto_update: bool,
    /// Dias entre uma atualização automática e a seguinte.
    #[serde(default = "week")]
    pub interval_days: u32,
    /// Se os anexos são baixados junto com as entradas.
    #[serde(default = "yes")]
    pub attachments: bool,
    /// Segundos Unix da última atualização completa.
    #[serde(default)]
    pub last_update: Option<i64>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_update: true,
            interval_days: DEFAULT_INTERVAL_DAYS,
            attachments: true,
            last_update: None,
        }
    }
}

impl Settings {
    /// Lê as configurações da pasta do perfil (os padrões se ainda não existem).
    pub fn load(root: &Path) -> Result<Self, Error> {
        let file = root.join(FILE);
        if !file.exists() {
            return Ok(Self::default());
        }
        Ok(serde_json::from_str(&fs::read_to_string(file)?)?)
    }

    /// Grava as configurações, depois de validar o intervalo.
    pub fn save(&self, root: &Path) -> Result<(), Error> {
        if !(1..=MAX_INTERVAL_DAYS).contains(&self.interval_days) {
            return Err(Error::Settings(format!(
                "o intervalo deve ficar entre 1 e {MAX_INTERVAL_DAYS} dias"
            )));
        }
        fs::create_dir_all(root)?;
        let temporary = root.join("settings.json.tmp");
        fs::write(&temporary, serde_json::to_string_pretty(self)?)?;
        fs::rename(temporary, root.join(FILE))?;
        Ok(())
    }

    /// Se já passou o intervalo desde a última atualização (ou se nunca houve uma).
    pub fn is_due(&self, now: i64) -> bool {
        self.last_update
            .is_none_or(|last| now - last >= i64::from(self.interval_days) * DAY)
    }

    /// Quando a próxima atualização automática vence (`None` se nunca houve uma).
    pub fn next_due(&self) -> Option<i64> {
        self.last_update
            .map(|last| last + i64::from(self.interval_days) * DAY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_missing_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Settings::load(dir.path()).unwrap(), Settings::default());
        let settings = Settings {
            auto_update: false,
            interval_days: 3,
            attachments: false,
            last_update: Some(10),
        };
        settings.save(&dir.path().join("novo")).unwrap();
        assert_eq!(Settings::load(&dir.path().join("novo")).unwrap(), settings);
    }

    #[test]
    fn missing_fields_take_defaults() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("settings.json"), "{}").unwrap();
        assert_eq!(Settings::load(dir.path()).unwrap(), Settings::default());
        fs::write(dir.path().join("settings.json"), "quebrado").unwrap();
        assert!(Settings::load(dir.path()).is_err());
    }

    #[test]
    fn rejects_bad_intervals() {
        let dir = tempfile::tempdir().unwrap();
        for days in [0, 366] {
            let settings = Settings {
                interval_days: days,
                ..Settings::default()
            };
            assert!(settings.save(dir.path()).is_err());
        }
    }

    #[test]
    fn knows_when_it_is_due() {
        let mut settings = Settings::default();
        assert!(settings.is_due(0));
        assert_eq!(settings.next_due(), None);
        settings.last_update = Some(1_000);
        assert!(!settings.is_due(1_000 + 7 * DAY - 1));
        assert!(settings.is_due(1_000 + 7 * DAY));
        assert_eq!(settings.next_due(), Some(1_000 + 7 * DAY));
    }
}
