package br.edu.ifrn.baseconhecimento

import android.app.Application
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import androidx.work.WorkInfo
import androidx.work.WorkManager
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.baseconhecimento_mobile.Base
import uniffi.baseconhecimento_mobile.BaseException
import uniffi.baseconhecimento_mobile.EntryDetail
import uniffi.baseconhecimento_mobile.EntryFilter
import uniffi.baseconhecimento_mobile.EntrySummary
import uniffi.baseconhecimento_mobile.EntryVisibility
import uniffi.baseconhecimento_mobile.FilterOptions
import uniffi.baseconhecimento_mobile.SearchHit
import uniffi.baseconhecimento_mobile.Status
import uniffi.baseconhecimento_mobile.UpdateProgress
import uniffi.baseconhecimento_mobile.updateProgress

/** Em qual tela o app está. */
sealed interface Screen {
    data object Start : Screen
    data object Login : Screen
    data object Entries : Screen
    data class Entry(val id: ULong) : Screen
    data object Settings : Screen
}

/** Quantas respostas da busca a tela mostra. */
private const val SEARCH_LIMIT = 50u

/** O texto de um erro da biblioteca Rust, como a pessoa deve ler. */
fun Throwable.readable(): String =
    (this as? BaseException.Failed)?.detail ?: (message ?: toString())

/** O filtro da lista a partir do que a tela escolheu. */
data class Filters(
    val area: String? = null,
    val visibility: EntryVisibility? = null,
    val tags: Set<String> = emptySet(),
    val inTitle: String = "",
) {
    val active: Boolean get() = area != null || visibility != null || tags.isNotEmpty()

    fun toCore(): EntryFilter = EntryFilter(inTitle.ifBlank { null }, area, visibility, tags.toList())
}

class MainViewModel(application: Application) : AndroidViewModel(application) {
    private var core: Base? = null

    var screen by mutableStateOf<Screen>(Screen.Start)
        private set
    var profiles by mutableStateOf(Profiles.list(application))
        private set
    var activeProfile by mutableStateOf(Profiles.active(application))
        private set
    var profileError by mutableStateOf<String?>(null)

    // Lista, filtros e busca (tudo lido do aparelho; nada disso usa a rede)
    var entries by mutableStateOf<List<EntrySummary>>(emptyList())
        private set
    var filters by mutableStateOf(Filters())
        private set
    var options by mutableStateOf(FilterOptions(emptyList(), emptyList()))
        private set
    var query by mutableStateOf("")
        private set
    var hits by mutableStateOf<List<SearchHit>?>(null)
        private set
    var listError by mutableStateOf<String?>(null)
        private set
    var hasSession by mutableStateOf(false)
        private set

    // Leitura de uma entrada
    var detail by mutableStateOf<EntryDetail?>(null)
        private set
    var detailError by mutableStateOf<String?>(null)
        private set

    // Atualização
    var status by mutableStateOf<Status?>(null)
        private set
    var progress by mutableStateOf<UpdateProgress?>(null)
        private set
    var queued by mutableStateOf(false)
        private set
    var outcome by mutableStateOf<Outcome?>(null)
        private set
    var unmeteredOnly by mutableStateOf(CoreAccess.unmeteredOnly(application))
        private set
    var message by mutableStateOf<String?>(null)

    private var searchJob: Job? = null
    private var wasRunning = false

    init {
        start()
        watchUpdates()
    }

    private fun open(): Base {
        core?.let { return it }
        return CoreAccess.open(getApplication()).also { core = it }
    }

    /** Offline primeiro: se já há cópia local, abre a lista sem rede; sem cópia e sem sessão, vai ao login. */
    fun start() {
        screen = Screen.Start
        viewModelScope.launch {
            try {
                val (session, total) = withContext(Dispatchers.IO) { open().hasSession() to open().status().entries }
                hasSession = session
                if (total > 0u || session) openEntries() else screen = Screen.Login
            } catch (error: Throwable) {
                listError = error.readable()
                screen = Screen.Settings
            }
        }
    }

    val baseUrl: String get() = activeProfile.url

    /** A tela de login importa os cookies da WebView para a biblioteca. */
    suspend fun importCookies(header: String): Boolean = withContext(Dispatchers.IO) {
        open().importSessionCookies(header)
        open().isAuthenticated()
    }

    /** Login concluído: abre a lista e, se a cópia local está vazia, já pede a primeira atualização. */
    fun loggedIn() {
        hasSession = true
        openEntries()
        viewModelScope.launch {
            val empty = withContext(Dispatchers.IO) { open().status().entries == 0u }
            if (empty) Updater.runNow(getApplication())
        }
    }

    fun openLogin() {
        screen = Screen.Login
    }

    fun openEntries() {
        screen = Screen.Entries
        reload()
    }

    /** Relê do aparelho a lista, as opções de filtro, o resumo e a busca em andamento. */
    fun reload() {
        viewModelScope.launch {
            try {
                val (list, opts, summary) = withContext(Dispatchers.IO) {
                    val base = open()
                    Triple(base.entries(filters.toCore()), base.filterOptions(), base.status())
                }
                entries = list
                options = opts
                status = summary
                listError = null
            } catch (error: Throwable) {
                listError = error.readable()
            }
        }
        runSearch(query, immediately = true)
    }

    fun applyFilters(next: Filters) {
        filters = next
        reload()
    }

    /** Muda o texto da busca; a busca roda um instante depois que a pessoa para de digitar. */
    fun search(text: String) {
        query = text
        runSearch(text, immediately = false)
    }

    private fun runSearch(text: String, immediately: Boolean) {
        searchJob?.cancel()
        if (text.isBlank()) {
            hits = null
            return
        }
        searchJob = viewModelScope.launch {
            if (!immediately) delay(300)
            try {
                hits = withContext(Dispatchers.IO) { open().search(text, filters.toCore(), SEARCH_LIMIT) }
                listError = null
            } catch (error: Throwable) {
                hits = emptyList()
                listError = error.readable()
            }
        }
    }

    fun openEntry(id: ULong) {
        detail = null
        detailError = null
        screen = Screen.Entry(id)
        viewModelScope.launch {
            try {
                detail = withContext(Dispatchers.IO) { open().entry(id) }
            } catch (error: Throwable) {
                detailError = error.readable()
            }
        }
    }

    fun back() {
        when (screen) {
            is Screen.Entry -> screen = Screen.Entries
            Screen.Settings -> if (status?.entries == 0u && !hasSession) openLogin() else openEntries()
            else -> Unit
        }
    }

    fun openSettings() {
        screen = Screen.Settings
        refreshSettings()
    }

    fun refreshSettings() {
        viewModelScope.launch {
            runCatching { withContext(Dispatchers.IO) { open().status() } }.onSuccess { status = it }
            hasSession = withContext(Dispatchers.IO) { runCatching { open().hasSession() }.getOrDefault(false) }
            outcome = Updater.lastOutcome(getApplication())
        }
    }

    /** Grava a configuração da atualização e liga ou desliga a tarefa agendada. */
    fun saveSettings(auto: Boolean, days: Int, attachments: Boolean) {
        viewModelScope.launch {
            try {
                val saved = withContext(Dispatchers.IO) { open().saveSettings(auto, days.toUInt(), attachments) }
                status = status?.copy(settings = saved)
                Updater.schedule(getApplication(), auto, unmeteredOnly)
                message = null
                refreshSettings()
            } catch (error: Throwable) {
                message = error.readable()
            }
        }
    }

    fun saveUnmeteredOnly(value: Boolean) {
        unmeteredOnly = value
        CoreAccess.saveUnmeteredOnly(getApplication(), value)
        Updater.schedule(getApplication(), status?.settings?.autoUpdate == true, value)
    }

    fun updateNow() {
        message = null
        Updater.runNow(getApplication())
        queued = true
    }

    fun cancelUpdate() {
        Updater.cancel(getApplication())
        queued = false
    }

    /** Esquece a sessão (a cópia local continua legível sem rede). */
    fun logout() {
        viewModelScope.launch {
            withContext(Dispatchers.IO) { open().logout() }
            hasSession = false
            openSettings()
        }
    }

    /** Apaga as entradas, anexos e índice deste perfil; a sessão fica. */
    fun deleteLocalCopy() {
        viewModelScope.launch {
            try {
                withContext(Dispatchers.IO) { open().deleteLocalCopy() }
                entries = emptyList()
                hits = null
                message = "Cópia local apagada."
            } catch (error: Throwable) {
                message = error.readable()
            }
            refreshSettings()
        }
    }

    fun switchProfile(name: String) {
        Profiles.activate(getApplication(), name)
        reloadProfile()
    }

    /** Cadastra um perfil novo (nome e endereço do SUAP) e passa a usá-lo; devolve se deu certo. */
    fun addProfile(name: String, url: String): Boolean {
        val error = Profiles.add(getApplication(), name, url)
        profileError = error
        if (error == null) reloadProfile()
        return error == null
    }

    private fun reloadProfile() {
        activeProfile = Profiles.active(getApplication())
        profiles = Profiles.list(getApplication())
        core = null
        entries = emptyList()
        hits = null
        query = ""
        filters = Filters()
        status = null
        detail = null
        message = null
        start()
    }

    /** Acompanha a atualização (em andamento ou na fila) para a tela mostrar o andamento. */
    private fun watchUpdates() {
        viewModelScope.launch {
            WorkManager.getInstance(getApplication()).getWorkInfosForUniqueWorkFlow("atualizacao-agora").collect { infos ->
                queued = infos.any { it.state == WorkInfo.State.ENQUEUED || it.state == WorkInfo.State.BLOCKED }
            }
        }
        viewModelScope.launch {
            while (true) {
                val now = withContext(Dispatchers.IO) { updateProgress() }
                progress = now
                if (wasRunning && !now.running) onUpdateFinished()
                wasRunning = now.running
                delay(if (now.running) 500 else 2000)
            }
        }
    }

    private fun onUpdateFinished() {
        outcome = Updater.lastOutcome(getApplication())
        if (screen == Screen.Entries || screen == Screen.Settings) {
            reload()
            if (screen == Screen.Settings) refreshSettings()
        }
    }
}
