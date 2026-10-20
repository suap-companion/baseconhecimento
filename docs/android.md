# App Android

Kotlin + Jetpack Compose (Material 3, tema do Design System gov.br), `minSdk` 26, falando com a biblioteca Rust por uma fachada UniFFI. Mesmo modelo do app do [`chamados`](https://github.com/suap-companion/chamados). Plano e decisões: issue #6.

```text
android/                      projeto Gradle (módulo app, pacote br.edu.ifrn.baseconhecimento)
crates/baseconhecimento-mobile/   fachada Rust (cdylib + UniFFI): objeto Base
scripts/build-android.sh      compila as 3 ABIs (Docker + NDK) e gera as ligações Kotlin
scripts/suap_falso.py         SUAP sintético para testar a atualização no emulador
.github/workflows/android.yml build do APK (CI, release e execução manual)
```

## Pré-requisitos (uma vez)

- Android Studio com SDK (plataforma 37, build-tools 36, `cmdline-tools`), JDK 21 e Docker.
- NDK r29 **Linux** (usado dentro do Docker) e **Windows** (usado pelo Gradle):

  ```bash
  android --sdk=$HOME/.android-sdk-linux sdk --platform=linux_x86_64 install ndk/29.0.14206865
  android sdk install ndk/29.0.14206865
  ```
- `android/local.properties` com `sdk.dir=C\:/Users/<você>/AppData/Local/Android/Sdk` (não vai para o repositório).
- Emulador: `system-images;android-36;google_apis;x86_64` e um AVD (o dos testes locais é o `chamados-spike`).

## Compilar

```bash
sh scripts/build-android.sh           # .so das 3 ABIs + ligações Kotlin (android/app/src/main/{jniLibs,java/uniffi})
cd android
./gradlew testDebugUnitTest lintDebug assembleDebug
```

Os arquivos gerados (`jniLibs/`, `java/uniffi/`) não são versionados. O perfil `mobile` do `Cargo.toml` otimiza o tamanho (`opt-level = "s"`, LTO). A versão do app é a de `[workspace.package]` no `Cargo.toml` (`versionCode = maior*10000 + menor*100 + correção`).

## A fachada `baseconhecimento-mobile`

O objeto `Base(dataDir, baseUrl)` é o que o app usa; nada nele é específico do Android. As funções são **bloqueantes** (chame fora da thread principal).

| Função | O que faz |
| --- | --- |
| `importSessionCookies(header)` / `hasSession()` / `isAuthenticated()` / `logout()` | sessão a partir dos cookies da WebView de login; a senha nunca passa pelo app |
| `entries(filtro)` / `filterOptions()` | lista da cópia local (texto, área, visibilidade, tags) e as áreas/tags que existem |
| `entry(id)` | a entrada: Markdown (sem frontmatter nem título), metadados, anexos baixados e as pastas que ancoram os caminhos relativos |
| `search(consulta, filtro, limite)` | busca FTS5/BM25 em entradas e anexos TXT/MD/PDF, uma ocorrência por entrada |
| `settings()` / `saveSettings(auto, dias, anexos)` / `status()` | as mesmas `Settings` da biblioteca (validadas) e os totais por visibilidade |
| `update(params)` / `updateIfDue()` | `crawl::update`; a segunda só roda com a atualização automática ligada e o intervalo vencido (é a que a tarefa agendada chama) |
| `updateProgress()` / `cancelUpdate()` / `updateRunning()` | andamento (`listed`, `done`, últimas linhas) e cancelamento antes da próxima entrada |
| `rebuildIndex()` / `deleteLocalCopy()` | refaz o índice; apaga a cópia local do perfil |

Decisões:

- **Uma atualização por vez** no processo (a da tela e a agendada não se atropelam); o cancelamento é global e vale para a que estiver rodando. Uma rodada cancelada devolve `cancelled = true` e **não** marca `last_update`.
- **Progresso por consulta** (`updateProgress`), não por callback: o andamento só tem ids e contagens, nunca títulos nem textos, e a tela o consulta a cada 0,5 s.
- Pastas: `<dataDir>/config` e `<dataDir>/data`; a cópia fica em `<dataDir>/data/base/default/` (o layout da CLI).
- **Sem sincronização entre aparelhos**: a Base de Conhecimento não sai da máquina (regra de privacidade). Os perfis do app ficam nas preferências do app.
- Erros: `BaseException.SessionExpired` (o app volta ao login) e `BaseException.Failed(detail)`.

## O app

- **Offline primeiro.** Ao abrir, se há cópia local, a lista aparece sem rede, com ou sem sessão. Sem cópia e sem sessão, vai ao login.
- **Login:** a página de login do SUAP numa WebView; quando a navegação sai de `/accounts/`, o app lê os cookies, entrega a `importSessionCookies` e confirma com `isAuthenticated`. gov.br e dois fatores continuam funcionando. Se a cópia está vazia, a primeira atualização começa sozinha.
- **Lista e busca:** filtros por área, visibilidade e tag; busca pela lupa (toda a base e os anexos de texto); puxar a lista pede uma atualização.
- **Leitura:** um leitor próprio do Markdown que o `baseconhecimento` grava (títulos, ênfases, listas aninhadas, citações, código, tabelas, imagens e links), sem biblioteca externa. Imagens e anexos vêm **só da cópia local**: os caminhos relativos (`../anexos/<id>/...`) são resolvidos para dentro da cópia (nada que saia dela abre) e abertos com `FileProvider`; links web abrem no navegador; imagem que não pôde ser baixada aparece como texto.
- **Configurações:** atualização sob demanda com andamento (`entradas conferidas / total`) e cancelamento; atualização automática (liga/desliga, 1/3/7/14/30 dias, anexos, só Wi-Fi); perfis (outro SUAP, como `http://10.0.2.2:8000/` no emulador); entrar de novo/sair; apagar a cópia local.
- **Agendamento (WorkManager):** a tarefa periódica roda **uma vez por dia**, com bateria não fraca, armazenamento livre e rede (sem franquia, se a opção estiver ligada), e chama `updateIfDue()` de cada perfil: só baixa quando o intervalo configurado (padrão 7 dias) venceu. "Atualizar agora" enfileira a mesma tarefa uma vez (qualquer rede). No fim de uma rodada agendada com novidades, uma notificação mostra só contagens e fica oculta na tela bloqueada; sessão expirada também avisa. `POST_NOTIFICATIONS` (Android 13+) é pedida ao ligar a atualização automática; sem ela tudo funciona, só não há aviso.
- **Privacidade:** `allowBackup="false"` e regras de extração que excluem tudo (backup na nuvem e transferência de aparelho); a cópia fica em `filesDir`; erros e logs não trazem títulos nem textos; HTTP sem TLS só para `10.0.2.2` (emulador).

### Limites desta versão

- O WorkManager encerra tarefas longas (cerca de 10 minutos); como a atualização é incremental e salva entrada a entrada, a próxima rodada continua do ponto em que parou (as páginas já baixadas são conferidas de novo, mas não baixadas outra vez).
- A varredura de ids fora das áreas (`--varrer`) e a poda (`--podar`) existem na fachada de biblioteca mas ainda não têm botão no app.
- O APK universal inclui as 3 ABIs e o código sem minificar (R8 desligado, porque JNA e UniFFI usam reflexão); veja o tamanho abaixo.
- Não há iOS.

## Testes

```bash
cd android
./gradlew testDebugUnitTest lintDebug      # unitários (JVM) e lint
./gradlew connectedDebugAndroidTest         # no emulador ou aparelho (o CI não roda)
```

- **Unitários:** leitor de Markdown, resolução segura de links, formatação e texto do resultado.
- **Instrumentação (`connectedDebugAndroidTest`):** `OfflineReadingTest` e `OfflineScreensTest` gravam uma cópia local **sintética** no formato da CLI e exercitam a biblioteca Rust de verdade e as telas (lista, filtros, busca, leitura, configurações) sem rede. `UpdateFlowTest` atualiza pela rede do emulador contra o SUAP falso e roda o `Worker`; ele é **ignorado** se o servidor não estiver no ar:

  ```bash
  python -I scripts/suap_falso.py     # sobe em 0.0.0.0:8765; o emulador o vê em http://10.0.2.2:8765/
  ```

Os testes mexem nos dados do app instalado (apagam e recriam o perfil em uso): rode no emulador, não no aparelho do dia a dia.

## Release e assinatura

O `android.yml` compila as 3 ABIs, roda testes unitários e lint e monta o APK de release. A chave **nunca** fica no repositório: o `release.yml` o assina com os segredos `ANDROID_KEYSTORE_B64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` e `ANDROID_KEY_PASSWORD`. Sem eles o build roda, avisa, e o APK **não** é publicado (um APK sem assinatura não instala). Com a chave, a release anexa `baseconhecimento-<tag>-android.apk` e o `SHA256SUMS.txt` inclui o APK.

Crie a chave uma vez e guarde o arquivo e as senhas num cofre (quem a perde não consegue publicar atualizações que instalem por cima):

```bash
keytool -genkeypair -v -keystore baseconhecimento-release.jks -alias baseconhecimento -keyalg RSA -keysize 4096 -validity 10000
base64 -w0 baseconhecimento-release.jks | gh secret set ANDROID_KEYSTORE_B64 --repo suap-companion/baseconhecimento
gh secret set ANDROID_KEYSTORE_PASSWORD --repo suap-companion/baseconhecimento
gh secret set ANDROID_KEY_ALIAS --repo suap-companion/baseconhecimento --body baseconhecimento
gh secret set ANDROID_KEY_PASSWORD --repo suap-companion/baseconhecimento
```

## Tamanho (medido)

Compilado em 2026-10-20 (v0.4.0), `assembleRelease`, universal (arm64-v8a, armeabi-v7a e x86_64):

| Item | Tamanho |
| --- | --- |
| APK de release (universal, sem assinar) | 27,4 MiB |
| `libbaseconhecimento_mobile.so` (arm64-v8a / armeabi-v7a / x86_64) | 6,6 / 4,6 / 7,0 MiB |
| `classes*.dex` (Compose + Material 3, sem minificar) | 21,2 MiB |

A biblioteca é maior que a do `chamados` (~3,2 MiB em arm64) por causa do SQLite embutido e do leitor de PDF (`suap-rag`), que o chamados não usa. Dependências novas em relação ao Kotlin/Compose do chamados: nenhuma além do `androidx.work:work-runtime-ktx` (agendamento que sobrevive a reinício e respeita bateria, rede e Doze; o `chamados` já o usa) e, só nos testes, `androidx.work:work-testing`. O app **não** inclui CameraX, ML Kit nem modelos de IA. Melhorias possíveis: R8 com regras para JNA/UniFFI e `splits` por ABI.

## Não validado

Tudo foi testado com dados sintéticos: o SUAP real (HTML, login do gov.br com dois fatores, volume de anexos, tempo de uma primeira atualização completa e o limite de 10 minutos do WorkManager) **não foi** exercitado, assim como a leitura de PDFs reais e o comportamento do agendamento em aparelhos de fabricantes que encerram apps em segundo plano.
