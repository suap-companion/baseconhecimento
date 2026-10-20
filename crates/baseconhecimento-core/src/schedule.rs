//! Modelos de agendamento da atualização semanal (Agendador de Tarefas, cron e systemd).
//!
//! O agendador roda todo dia `atualizar --se-vencido`; o próprio programa só atualiza quando o
//! intervalo configurado venceu. Assim, um computador desligado na hora certa atualiza na vez seguinte.

use crate::Error;

/// Em que sistema o agendamento será criado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheduler {
    Windows,
    Cron,
    Systemd,
}

impl Scheduler {
    /// Lê `windows`, `cron` ou `systemd`.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "windows" => Some(Self::Windows),
            "cron" => Some(Self::Cron),
            "systemd" => Some(Self::Systemd),
            _ => None,
        }
    }

    /// O sistema em que o programa está rodando.
    pub fn detect() -> Self {
        Self::for_windows(cfg!(windows))
    }

    fn for_windows(windows: bool) -> Self {
        if windows {
            Self::Windows
        } else {
            Self::Systemd
        }
    }
}

fn check_profile(profile: &str) -> Result<(), Error> {
    let valid = !profile.is_empty()
        && profile
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if valid {
        Ok(())
    } else {
        Err(Error::Settings(format!("perfil inválido: {profile:?}")))
    }
}

/// O texto/comando que cria o agendamento diário de `atualizar --se-vencido` para o perfil.
pub fn snippet(scheduler: Scheduler, exe: &str, profile: &str) -> Result<String, Error> {
    check_profile(profile)?;
    if exe.contains(['"', '\n']) {
        return Err(Error::Settings(format!(
            "caminho do programa não aceito: {exe:?}"
        )));
    }
    let command = format!("--profile {profile} atualizar --se-vencido --quiet");
    Ok(match scheduler {
        Scheduler::Windows => format!(
            "schtasks /Create /SC DAILY /ST 12:00 /TN \"baseconhecimento-{profile}\" /TR \"\\\"{exe}\\\" {command}\"\n"
        ),
        Scheduler::Cron => format!("0 12 * * * \"{exe}\" {command}\n"),
        Scheduler::Systemd => format!(
            "# ~/.config/systemd/user/baseconhecimento-{profile}.service\n\
             [Unit]\nDescription=Atualiza a Base de Conhecimento do SUAP ({profile})\n\n\
             [Service]\nType=oneshot\nExecStart=\"{exe}\" {command}\n\n\
             # ~/.config/systemd/user/baseconhecimento-{profile}.timer\n\
             [Unit]\nDescription=Atualização diária da Base de Conhecimento ({profile})\n\n\
             [Timer]\nOnCalendar=daily\nPersistent=true\n\n\
             [Install]\nWantedBy=timers.target\n\n\
             # systemctl --user enable --now baseconhecimento-{profile}.timer\n"
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_names_and_detects_the_system() {
        assert_eq!(Scheduler::parse("windows"), Some(Scheduler::Windows));
        assert_eq!(Scheduler::parse("cron"), Some(Scheduler::Cron));
        assert_eq!(Scheduler::parse("systemd"), Some(Scheduler::Systemd));
        assert_eq!(Scheduler::parse("outro"), None);
        assert_eq!(Scheduler::for_windows(true), Scheduler::Windows);
        assert_eq!(Scheduler::for_windows(false), Scheduler::Systemd);
        assert_eq!(Scheduler::detect(), Scheduler::for_windows(cfg!(windows)));
    }

    #[test]
    fn builds_each_snippet() {
        let windows = snippet(
            Scheduler::Windows,
            "C:\\bin\\baseconhecimento.exe",
            "default",
        )
        .unwrap();
        assert!(windows.starts_with("schtasks /Create /SC DAILY"));
        assert!(windows.contains("atualizar --se-vencido --quiet"));
        let cron = snippet(Scheduler::Cron, "/usr/bin/baseconhecimento", "ti").unwrap();
        assert_eq!(
            cron,
            "0 12 * * * \"/usr/bin/baseconhecimento\" --profile ti atualizar --se-vencido --quiet\n"
        );
        let systemd = snippet(Scheduler::Systemd, "/usr/bin/baseconhecimento", "ti").unwrap();
        assert!(systemd.contains("OnCalendar=daily") && systemd.contains("Persistent=true"));
    }

    #[test]
    fn rejects_unsafe_input() {
        assert!(snippet(Scheduler::Cron, "/bin/x", "a b").is_err());
        assert!(snippet(Scheduler::Cron, "/bin/x", "").is_err());
        assert!(snippet(Scheduler::Cron, "/bin/\"x", "ti").is_err());
    }
}
