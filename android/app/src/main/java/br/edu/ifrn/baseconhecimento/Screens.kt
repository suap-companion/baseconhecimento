package br.edu.ifrn.baseconhecimento

import android.annotation.SuppressLint
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.webkit.CookieManager
import android.webkit.MimeTypeMap
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SuggestionChip
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.FileProvider
import java.io.File
import java.net.URI
import kotlinx.coroutines.launch
import uniffi.baseconhecimento_mobile.EntrySummary
import uniffi.baseconhecimento_mobile.EntryVisibility
import uniffi.baseconhecimento_mobile.SearchHit
import uniffi.baseconhecimento_mobile.coreVersion

@Composable
fun BaseApp(model: MainViewModel) {
    when (val screen = model.screen) {
        Screen.Start -> Centered { CircularProgressIndicator() }
        Screen.Login -> LoginScreen(model)
        Screen.Entries -> EntriesScreen(model)
        is Screen.Entry -> EntryScreen(model)
        Screen.Settings -> SettingsScreen(model)
    }
}

@Composable
private fun Centered(content: @Composable () -> Unit) {
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.Center, horizontalAlignment = Alignment.CenterHorizontally) {
        content()
    }
}

/** O nome da visibilidade como o SUAP a mostra. */
fun EntryVisibility.label(): String = when (this) {
    EntryVisibility.PUBLIC -> "Pública"
    EntryVisibility.PRIVATE -> "Privada"
    EntryVisibility.CONFIDENTIAL -> "Sigilosa"
}

/** O menu da conta: trocar de perfil, cadastrar outro e ir às configurações. */
@Composable
private fun accountOf(model: MainViewModel, onNewProfile: () -> Unit): HeaderAccount = HeaderAccount(
    name = model.activeProfile.name,
    enrollment = null,
    profile = model.baseUrl.removePrefix("https://").removeSuffix("/"),
    photo = null,
    items = model.profiles.filter { it.name != model.activeProfile.name }
        .map { other -> HeaderItem("Trocar para ${other.name}") { model.switchProfile(other.name) } } +
        listOf(HeaderItem("Cadastrar novo perfil…", onClick = onNewProfile), HeaderItem("Configurações") { model.openSettings() }),
)

/** Login no próprio site do SUAP: o app só recebe os cookies, nunca a senha. */
@SuppressLint("SetJavaScriptEnabled")
@Composable
fun LoginScreen(model: MainViewModel) {
    val scope = rememberCoroutineScope()
    val base = model.baseUrl
    var message by remember { mutableStateOf<String?>(null) }
    var entering by remember { mutableStateOf(false) }
    val hasCopy = (model.status?.entries ?: 0u) > 0u
    BackHandler(enabled = hasCopy) { model.openEntries() }
    Scaffold(
        topBar = {
            GovBrHeader(
                "Entrar no SUAP",
                onBack = if (hasCopy) ({ model.openEntries() }) else null,
                quick = listOf(HeaderItem("Configurações (endereço do SUAP)") { model.openSettings() }),
            )
        },
    ) { padding ->
        Column(Modifier.padding(padding)) {
            message?.let { Text(it, Modifier.padding(16.dp), color = MaterialTheme.colorScheme.error) }
            if (entering) Centered { CircularProgressIndicator() }
            AndroidView(
                modifier = if (entering) Modifier else Modifier.fillMaxSize(),
                factory = { context ->
                    WebView(context).apply {
                        settings.javaScriptEnabled = true
                        settings.domStorageEnabled = true
                        webViewClient = object : WebViewClient() {
                            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest) = false

                            override fun onPageFinished(view: WebView, url: String) {
                                if (!isAfterLogin(base, url)) return
                                val cookies = CookieManager.getInstance().getCookie(base) ?: return
                                entering = true
                                scope.launch {
                                    try {
                                        if (model.importCookies(cookies)) model.loggedIn() else entering = false
                                    } catch (error: Throwable) {
                                        entering = false
                                        message = error.readable()
                                    }
                                }
                            }
                        }
                        loadUrl(base + "accounts/login/?next=/centralservicos/baseconhecimento/")
                    }
                },
            )
        }
    }
}

/** A lista: busca e filtros sobre a cópia local, sem rede. Puxar a lista pede uma atualização. */
@OptIn(ExperimentalLayoutApi::class, ExperimentalMaterial3Api::class)
@Composable
fun EntriesScreen(model: MainViewModel) {
    var newProfile by remember { mutableStateOf(false) }
    val updating = model.progress?.running == true || model.queued
    Scaffold(
        topBar = {
            GovBrHeader(
                title = "Base de Conhecimento",
                subtitle = model.activeProfile.name,
                quick = listOf(HeaderItem("Configurações da atualização") { model.openSettings() }),
                search = HeaderSearch(
                    label = "Buscar nas entradas e anexos",
                    value = model.query,
                    onChange = { model.search(it) },
                    onSubmit = { model.search(model.query) },
                    onClose = { model.search("") },
                ),
                account = accountOf(model) { newProfile = true },
            )
        },
    ) { padding ->
        PullToRefreshBox(
            isRefreshing = updating,
            onRefresh = { if (model.hasSession) model.updateNow() else model.openLogin() },
            modifier = Modifier.padding(padding),
        ) {
            Column(Modifier.fillMaxSize()) {
                StatusBanner(model)
                FilterBar(model)
                model.listError?.let { Text(it, Modifier.padding(16.dp), color = MaterialTheme.colorScheme.error) }
                val hits = model.hits
                if (hits != null) HitsList(hits, model) else EntriesList(model.entries, model)
            }
        }
    }
    if (newProfile) NewProfileDialog(model) { newProfile = false }
}

/** O aviso do topo da lista: atualizando, sem sessão ou sem nada baixado ainda. */
@Composable
private fun StatusBanner(model: MainViewModel) {
    val progress = model.progress
    val total = model.status?.entries ?: 0u
    if (progress?.running == true) {
        val listed = progress.listed
        Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp)) {
            Text(
                if (listed != null) "Atualizando: ${progress.done} de $listed entradas" else "Atualizando: lendo as áreas…",
                style = MaterialTheme.typography.bodyMedium,
            )
            if (listed != null && listed > 0u) {
                LinearProgressIndicator(progress = { progress.done.toFloat() / listed.toFloat() }, modifier = Modifier.fillMaxWidth())
            } else {
                LinearProgressIndicator(Modifier.fillMaxWidth())
            }
        }
    } else if (model.queued) {
        Text("Atualização na fila (espera rede).", Modifier.padding(16.dp), style = MaterialTheme.typography.bodyMedium)
    } else if (!model.hasSession) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("Sem sessão do SUAP: lendo só a cópia local.", Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium)
            TextButton(onClick = { model.openLogin() }) { Text("Entrar") }
        }
    } else if (total == 0u) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("Nada baixado ainda.", Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium)
            TextButton(onClick = { model.updateNow() }) { Text("Baixar a base") }
        }
    }
}

/** Filtros por área, visibilidade e tags (listas das que existem na cópia local). */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun FilterBar(model: MainViewModel) {
    val filters = model.filters
    var menu by remember { mutableStateOf<String?>(null) }
    FlowRow(
        Modifier.fillMaxWidth().padding(horizontal = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        FilterMenu("Área", filters.area, menu == "area", { menu = if (it) "area" else null }, model.options.areas, { it }) {
            model.applyFilters(filters.copy(area = it))
        }
        FilterMenu(
            "Visibilidade", filters.visibility?.label(), menu == "vis", { menu = if (it) "vis" else null },
            EntryVisibility.entries, { it.label() },
        ) { model.applyFilters(filters.copy(visibility = it)) }
        FilterMenu("Tag", filters.tags.firstOrNull(), menu == "tag", { menu = if (it) "tag" else null }, model.options.tags, { it }) {
            model.applyFilters(filters.copy(tags = if (it == null) emptySet() else setOf(it)))
        }
        if (filters.active) {
            TextButton(onClick = { model.applyFilters(Filters()) }) { Text("Limpar filtros") }
        }
    }
}

@Composable
private fun <T> FilterMenu(
    label: String,
    selected: String?,
    expanded: Boolean,
    onExpand: (Boolean) -> Unit,
    values: List<T>,
    text: (T) -> String,
    onPick: (T?) -> Unit,
) {
    androidx.compose.foundation.layout.Box {
        FilterChip(selected = selected != null, onClick = { onExpand(true) }, label = { Text(selected ?: label) })
        DropdownMenu(expanded = expanded, onDismissRequest = { onExpand(false) }) {
            DropdownMenuItem(text = { Text("Todas") }, onClick = { onExpand(false); onPick(null) })
            values.forEach { value ->
                DropdownMenuItem(text = { Text(text(value)) }, onClick = { onExpand(false); onPick(value) })
            }
        }
    }
}

@Composable
private fun EntriesList(entries: List<EntrySummary>, model: MainViewModel) {
    if (entries.isEmpty()) {
        Text(
            if (model.filters.active) "Nenhuma entrada com esses filtros." else "Nenhuma entrada na cópia local.",
            Modifier.padding(16.dp),
            style = MaterialTheme.typography.bodyLarge,
        )
        return
    }
    LazyColumn(Modifier.fillMaxSize().semantics { contentDescription = "Lista de entradas" }) {
        item {
            Text(Format.entries(entries.size), Modifier.padding(horizontal = 16.dp, vertical = 4.dp), style = MaterialTheme.typography.labelMedium)
        }
        items(entries, key = { it.id.toLong() }) { entry ->
            Card(
                Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp).clickable { model.openEntry(entry.id) },
            ) {
                Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(entry.title, style = MaterialTheme.typography.titleMedium, maxLines = 3, overflow = TextOverflow.Ellipsis)
                    Text(
                        listOfNotNull(entry.area, entry.visibility?.label()).joinToString(" · "),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    if (entry.tags.isNotEmpty()) {
                        Text(entry.tags.joinToString("  ") { "#$it" }, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.primary)
                    }
                }
            }
        }
    }
}

@Composable
private fun HitsList(hits: List<SearchHit>, model: MainViewModel) {
    if (hits.isEmpty()) {
        Text("Nada encontrado.", Modifier.padding(16.dp), style = MaterialTheme.typography.bodyLarge)
        return
    }
    LazyColumn(Modifier.fillMaxSize()) {
        items(hits, key = { it.id.toLong() }) { hit ->
            Card(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp).clickable { model.openEntry(hit.id) }) {
                Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(hit.title, style = MaterialTheme.typography.titleMedium, maxLines = 2, overflow = TextOverflow.Ellipsis)
                    val from = if (hit.source == "entrada") "no texto" else "no anexo ${hit.source}" + (hit.page?.let { ", p. $it" } ?: "")
                    Text(
                        listOfNotNull(hit.area, hit.visibility?.label(), from).joinToString(" · "),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Text(hit.snippet, style = MaterialTheme.typography.bodyMedium, maxLines = 4, overflow = TextOverflow.Ellipsis)
                }
            }
        }
    }
}

/** A entrada, lida do arquivo Markdown da cópia local; anexos e imagens abrem do aparelho. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun EntryScreen(model: MainViewModel) {
    val context = LocalContext.current
    val detail = model.detail
    BackHandler { model.back() }
    Scaffold(
        topBar = { GovBrHeader(title = detail?.summary?.title ?: "Entrada", onBack = { model.back() }) },
    ) { padding ->
        Column(Modifier.padding(padding).fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            model.detailError?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            if (detail == null) {
                if (model.detailError == null) CircularProgressIndicator()
                return@Column
            }
            Text(detail.summary.title, style = MaterialTheme.typography.headlineSmall)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                detail.summary.visibility?.let { SuggestionChip(onClick = {}, label = { Text(it.label()) }) }
                detail.summary.area?.let { SuggestionChip(onClick = {}, label = { Text(it) }) }
                if (!detail.active) SuggestionChip(onClick = {}, label = { Text("Inativo") })
                if (detail.needsCorrection) SuggestionChip(onClick = {}, label = { Text("Pendente de correção") })
                if (detail.pendingReview) SuggestionChip(onClick = {}, label = { Text("Supervisão pendente") })
            }
            if (detail.summary.tags.isNotEmpty()) {
                Text(detail.summary.tags.joinToString("  ") { "#$it" }, color = MaterialTheme.colorScheme.primary)
            }
            val by = detail.updatedBy?.let { " por $it" }.orEmpty()
            Text(
                "Baixada em ${Format.whenText(detail.summary.fetchedAt)}" + (detail.summary.updatedAt?.let { " · SUAP: $it$by" } ?: ""),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            HorizontalDivider()
            MarkdownView(detail.markdown, File(detail.entriesDir), File(detail.rootDir), { open(context, it) })
            if (detail.attachments.isNotEmpty()) {
                HorizontalDivider()
                Text("Anexos", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
                detail.attachments.forEach { file ->
                    OutlinedButton(onClick = { open(context, LinkTarget.Local(File(file.path))) }, Modifier.fillMaxWidth()) {
                        Text("${file.name} (${Format.size(file.size.toLong())})", maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                }
            }
            detail.url?.let { url ->
                TextButton(onClick = { open(context, LinkTarget.Web(url)) }) { Text("Abrir no SUAP") }
            }
        }
    }
}

/** Abre um link: a web no navegador, um arquivo da cópia local no aplicativo que o entenda. */
fun open(context: Context, target: LinkTarget) {
    val intent = when (target) {
        is LinkTarget.Web -> Intent(Intent.ACTION_VIEW, Uri.parse(target.url))
        is LinkTarget.Local -> {
            val uri = FileProvider.getUriForFile(context, "${context.packageName}.files", target.file)
            val type = MimeTypeMap.getSingleton().getMimeTypeFromExtension(target.file.extension.lowercase()) ?: "*/*"
            Intent(Intent.ACTION_VIEW).setDataAndType(uri, type).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        LinkTarget.None -> {
            Toast.makeText(context, "Esse link não está disponível offline.", Toast.LENGTH_SHORT).show()
            return
        }
    }
    try {
        context.startActivity(intent)
    } catch (_: Exception) {
        Toast.makeText(context, "Nenhum aplicativo abre este arquivo.", Toast.LENGTH_SHORT).show()
    }
}

/** Cadastra um perfil (outro SUAP, por exemplo o local do emulador). */
@Composable
fun NewProfileDialog(model: MainViewModel, onClose: () -> Unit) {
    var name by remember { mutableStateOf("") }
    var url by remember { mutableStateOf("https://") }
    AlertDialog(
        onDismissRequest = onClose,
        title = { Text("Novo perfil") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(name, { name = it }, label = { Text("Nome") }, singleLine = true)
                OutlinedTextField(url, { url = it }, label = { Text("Endereço do SUAP") }, singleLine = true)
                model.profileError?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            }
        },
        confirmButton = { Button(onClick = { if (model.addProfile(name, url)) onClose() }) { Text("Cadastrar") } },
        dismissButton = { TextButton(onClick = onClose) { Text("Cancelar") } },
    )
}

/** Os intervalos oferecidos para a atualização automática, em dias. */
val INTERVALS = listOf(1, 3, 7, 14, 30)

/** Configurações: atualização sob demanda e agendada, conta, perfis e dados locais. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun SettingsScreen(model: MainViewModel) {
    val context = LocalContext.current
    BackHandler { model.back() }
    var newProfile by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }
    var askNotifications by remember { mutableStateOf(false) }
    val settings = model.status?.settings
    val progress = model.progress
    val running = progress?.running == true
    val notificationsLauncher = androidx.activity.compose.rememberLauncherForActivityResult(
        androidx.activity.result.contract.ActivityResultContracts.RequestPermission(),
    ) { }
    Scaffold(topBar = { GovBrHeader(title = "Configurações", onBack = { model.back() }) }) { padding ->
        Column(Modifier.padding(padding).fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            model.listError?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            model.message?.let { Text(it, color = MaterialTheme.colorScheme.error) }

            SectionTitle("Atualização")
            val status = model.status
            if (status != null) {
                Text("${Format.entries(status.entries.toInt())} na cópia local (${status.public} públicas, ${status.`private`} privadas, ${status.confidential} sigilosas).")
                Text("Última atualização: ${Format.whenText(status.settings.lastUpdate)}")
                status.settings.nextDue?.let { Text("Próxima automática a partir de: ${Format.whenText(it)}") }
            }
            model.outcome?.let {
                Text(
                    "Último resultado: ${it.text}",
                    color = if (it.error) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurface,
                )
            }
            if (running) {
                val listed = progress?.listed
                Text(if (listed != null) "Atualizando: ${progress.done} de $listed entradas" else "Atualizando: lendo as áreas…")
                if (listed != null && listed > 0u) {
                    LinearProgressIndicator(progress = { progress.done.toFloat() / listed.toFloat() }, modifier = Modifier.fillMaxWidth())
                } else {
                    LinearProgressIndicator(Modifier.fillMaxWidth())
                }
                progress?.recent?.takeLast(5)?.forEach { Text(it, style = MaterialTheme.typography.bodySmall) }
                OutlinedButton(onClick = { model.cancelUpdate() }) { Text("Cancelar atualização") }
            } else if (model.queued) {
                Text("Na fila: começa quando houver rede.")
                OutlinedButton(onClick = { model.cancelUpdate() }) { Text("Cancelar") }
            } else {
                Button(onClick = { if (model.hasSession) model.updateNow() else model.openLogin() }) {
                    Text(if (model.hasSession) "Atualizar agora" else "Entrar para atualizar")
                }
            }

            HorizontalDivider()
            SectionTitle("Atualização automática")
            if (settings != null) {
                SwitchRow("Atualizar sozinho", settings.autoUpdate) { on ->
                    model.saveSettings(on, settings.intervalDays.toInt(), settings.attachments)
                    if (on && Updater.canAskNotifications()) askNotifications = true
                }
                Text("A cada:")
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    INTERVALS.forEach { days ->
                        FilterChip(
                            selected = settings.intervalDays.toInt() == days,
                            onClick = { model.saveSettings(settings.autoUpdate, days, settings.attachments) },
                            label = { Text(if (days == 1) "1 dia" else "$days dias") },
                        )
                    }
                }
                SwitchRow("Baixar anexos e imagens", settings.attachments) { model.saveSettings(settings.autoUpdate, settings.intervalDays.toInt(), it) }
                SwitchRow("Só com Wi-Fi (rede sem franquia)", model.unmeteredOnly) { model.saveUnmeteredOnly(it) }
                Text(
                    "A conferência roda uma vez por dia, só com bateria não fraca e armazenamento livre, e baixa quando o intervalo vence.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }

            HorizontalDivider()
            SectionTitle("Conta e perfis")
            Text("Perfil em uso: ${model.activeProfile.name} (${model.baseUrl})")
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                model.profiles.filter { it.name != model.activeProfile.name }.forEach { other ->
                    OutlinedButton(onClick = { model.switchProfile(other.name) }) { Text("Usar ${other.name}") }
                }
                OutlinedButton(onClick = { newProfile = true }) { Text("Novo perfil…") }
            }
            Text(if (model.hasSession) "Sessão do SUAP guardada." else "Sem sessão do SUAP.")
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = { model.openLogin() }) { Text(if (model.hasSession) "Entrar de novo" else "Entrar") }
                if (model.hasSession) OutlinedButton(onClick = { model.logout() }) { Text("Sair") }
            }

            HorizontalDivider()
            SectionTitle("Dados no aparelho")
            Text(
                "As entradas, os anexos e o índice de busca ficam só neste aparelho, na pasta privada do app, e não vão para " +
                    "backup. Entradas privadas e sigilosas incluídas.",
                style = MaterialTheme.typography.bodySmall,
            )
            OutlinedButton(onClick = { confirmDelete = true }) { Text("Apagar a cópia local") }
            Text("Versão ${coreVersion()}", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
    if (newProfile) NewProfileDialog(model) { newProfile = false }
    if (confirmDelete) {
        AlertDialog(
            onDismissRequest = { confirmDelete = false },
            title = { Text("Apagar a cópia local?") },
            text = { Text("Todas as entradas e anexos deste perfil saem do aparelho. Para lê-los de novo é preciso atualizar com rede.") },
            confirmButton = { Button(onClick = { confirmDelete = false; model.deleteLocalCopy() }) { Text("Apagar") } },
            dismissButton = { TextButton(onClick = { confirmDelete = false }) { Text("Cancelar") } },
        )
    }
    if (askNotifications) {
        askNotifications = false
        if (androidx.core.content.ContextCompat.checkSelfPermission(context, android.Manifest.permission.POST_NOTIFICATIONS) != android.content.pm.PackageManager.PERMISSION_GRANTED) {
            notificationsLauncher.launch(android.Manifest.permission.POST_NOTIFICATIONS)
        }
    }
}

@Composable
private fun SectionTitle(text: String) {
    Text(text, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
}

@Composable
private fun SwitchRow(label: String, checked: Boolean, onChange: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(label, Modifier.weight(1f))
        Switch(checked = checked, onCheckedChange = onChange)
    }
}
