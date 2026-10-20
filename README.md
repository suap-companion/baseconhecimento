# baseconhecimento

Espelha a **Base de Conhecimento** da Central de Serviços do SUAP numa pasta local, em **Markdown**, com os anexos, e a indexa para **busca offline** (SQLite/FTS5). É uma **biblioteca** Rust e um **CLI**, no mesmo repositório (`suap-companion/baseconhecimento`), irmão do [`chamados`](https://github.com/suap-companion/chamados).

> **Nota:** Este NÃO é um projeto oficial do IFRN, é um projeto pessoal. Construído sem qualquer apoio ou incentivo institucional.

## Por que

A busca da Base de Conhecimento do SUAP é lenta e pouco útil. Aqui você baixa tudo o que a sua conta enxerga, lê em qualquer editor, pesquisa sem rede e deixa a cópia se atualizar sozinha toda semana.

## Privacidade (regra dura)

- Entradas **privadas** e **sigilosas** são baixadas e indexadas **só nesta máquina**, e só as que a pessoa logada enxerga (o SUAP responde 403/404 para o resto, e isso é apenas contado).
- Nada é enviado a servidor algum além do próprio SUAP; os anexos só são baixados do mesmo endereço do SUAP do perfil.
- O progresso e as mensagens mostram ids e números, **nunca títulos nem textos**. Repositório, issues, PRs e testes usam apenas dados sintéticos.
- A senha vem da variável `SUAP_PASSWORD` (nunca por argumento) e não é salva; só os cookies da sessão ficam no diretório de dados.

## Instalação

Baixe o binário da [última release](https://github.com/suap-companion/baseconhecimento/releases) (Windows, Linux x86_64/arm64 e macOS Apple Silicon), extraia e ponha no `PATH`. Ou compile (Rust estável):

```bash
cargo build --release -p baseconhecimento-cli   # target/release/baseconhecimento
```

## Primeiros passos

```bash
baseconhecimento profile init --base-url https://suap.ifrn.edu.br/ --username seu_usuario
SUAP_PASSWORD=... baseconhecimento login        # PowerShell: $env:SUAP_PASSWORD='...'
baseconhecimento atualizar                      # primeira cópia completa
baseconhecimento buscar impressora toner        # busca offline
baseconhecimento mostrar 123                    # a entrada em Markdown
```

O arquivo `~/.config/suap/config.toml` (perfis) é o mesmo do `chamados`: um perfil criado em um serve ao outro. A sessão e a cópia local são separadas (`baseconhecimento` tem a própria pasta de dados).

## Comandos

| Comando | O que faz |
| --- | --- |
| `paths` | onde ficam configuração, sessão e cópia local do perfil |
| `profile init\|update\|remove\|show\|list` | perfis (ambientes SUAP), como no `chamados` |
| `login`, `session-status` | entra no SUAP e confere a sessão |
| `atualizar` | baixa o que mudou (incremental) e reindexa; `--forcar`, `--podar`, `--sem-anexos`, `--id N`, `--se-vencido`, `--quiet` |
| `listar` | entradas baixadas; filtros `--area`, `--visibilidade`; `--json` |
| `mostrar <id>` | a entrada em Markdown (ou `--json`) |
| `buscar <palavras>` | busca por palavras (BM25) em entradas e anexos TXT/MD/PDF; `--limite`, `--json` |
| `reindexar` | refaz o índice a partir dos arquivos locais |
| `status` | totais por visibilidade, última atualização e se está vencida |
| `config` | liga/desliga a atualização automática, intervalo em dias, anexos |
| `agendar` | imprime o comando de agendamento (Windows, cron ou systemd) |

## Atualização incremental, semanal e sob demanda

- **Sob demanda:** `baseconhecimento atualizar`. Só baixa de novo as entradas cujo conteúdo mudou (comparado por um resumo do conteúdo, já que o `atualizado_em` do SUAP só é gravado na criação da entrada).
- **Semanal:** o agendador roda todo dia `atualizar --se-vencido --quiet`; o programa só atualiza quando o intervalo configurado (padrão 7 dias) venceu. Computador desligado na hora certa atualiza na vez seguinte. `baseconhecimento agendar` imprime o comando; veja [docs/agendamento.md](docs/agendamento.md).
- **Settings:** `baseconhecimento config --auto-atualizar true --intervalo-dias 7 --anexos true`. A biblioteca expõe a mesma `Settings` para uma tela de configurações futura.

## Como fica no disco

```text
<dados>/base/<perfil>/
  entradas/<id>-<titulo>.md   frontmatter (id, titulo, area, visibilidade, tags, ...) + Resumo + Solução + Anexos
  anexos/<id>/<arquivo>       anexos, com nomes sanitizados
  manifesto.json              o que foi baixado e o resumo do conteúdo
  indice.sqlite               índice de busca (FTS5)
  settings.json               configurações da atualização
```

Veja [docs/arquitetura.md](docs/arquitetura.md) para os detalhes do rastreamento, da conversão e das limitações.

## Arquitetura

- `baseconhecimento-core`: leitura do HTML do SUAP (`parse`), conversão para Markdown (`markdown`), nomes seguros (`sanitize`), cópia local (`store`), índice (`index`), atualização incremental (`crawl`), configurações (`settings`) e agendamento (`schedule`).
- `baseconhecimento-cli`: a interface de linha de comando (binário `baseconhecimento`).
- **Núcleo compartilhado** ([`suap-companion/suap-kit`](https://github.com/suap-companion/suap-kit), dependência Git fixada em tag): `suap-core` (login, sessão, perfis, diretórios) e `suap-rag` (SQLite/FTS5, trechos). A sincronização cifrada (`suap-sync`) ainda não é usada: a regra de privacidade mantém o conteúdo só local, e sincronizar apenas perfis fica para uma próxima issue.

## Desenvolvimento

Sem Rust local, use o Docker: `sh scripts/validar-docker.sh` (formatação, `clippy -D warnings` e cobertura de **100% das linhas**). Com Rust: `sh scripts/validar.sh`. Detalhes em [AGENTS.md](AGENTS.md).

## Licença

MIT. A fonte Rawline da central de ajuda tem licença própria em `site/assets/fonts/Rawline-OFL.txt`.
