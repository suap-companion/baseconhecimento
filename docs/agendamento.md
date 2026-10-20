# Atualização semanal automática

O `baseconhecimento` não roda como serviço. Um agendador do sistema executa todo dia:

```text
baseconhecimento --profile <perfil> atualizar --se-vencido --quiet
```

`--se-vencido` faz o programa conferir as configurações do perfil (`baseconhecimento config`): se a atualização automática está desligada, ou se o intervalo (padrão 7 dias) ainda não venceu desde a última atualização completa, não faz nada. Assim o computador desligado no horário atualiza no próximo dia em que ligar. `--quiet` só mostra avisos (entradas que falharam, áreas negadas pelo SUAP).

Pré-requisito: uma sessão válida (`baseconhecimento login`). Se a sessão expirar, a execução falha com o pedido de novo login (código de saída 1); o agendador não guarda a senha.

`baseconhecimento agendar [--sistema windows|cron|systemd]` imprime o comando pronto para o seu sistema, já com o caminho do programa e o perfil.

## Windows (Agendador de Tarefas)

```powershell
schtasks /Create /SC DAILY /ST 12:00 /TN "baseconhecimento-default" /TR "\"C:\caminho\baseconhecimento.exe\" --profile default atualizar --se-vencido --quiet"
```

## Linux/macOS (cron)

```cron
0 12 * * * "/caminho/baseconhecimento" --profile default atualizar --se-vencido --quiet
```

## Linux (systemd, por usuário)

`~/.config/systemd/user/baseconhecimento-default.service` e `.timer` (use `baseconhecimento agendar --sistema systemd` para o conteúdo), depois:

```bash
systemctl --user enable --now baseconhecimento-default.timer
```

`Persistent=true` no timer recupera a execução perdida com a máquina desligada.

## Ajustes

```bash
baseconhecimento config --auto-atualizar false        # pausa a atualização automática
baseconhecimento config --intervalo-dias 3            # a cada 3 dias
baseconhecimento config --anexos false                # não baixa anexos
baseconhecimento status                               # última atualização e se está vencida
```

A atualização sob demanda (`baseconhecimento atualizar`) ignora o intervalo e funciona mesmo com a automática desligada.
