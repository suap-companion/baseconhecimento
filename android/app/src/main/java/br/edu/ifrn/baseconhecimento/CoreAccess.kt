package br.edu.ifrn.baseconhecimento

import android.content.Context
import java.io.File
import java.net.URI
import uniffi.baseconhecimento_mobile.Base

const val PRODUCTION_URL = "https://suap.ifrn.edu.br/"
const val LOCAL_URL = "http://10.0.2.2:8000/"

/** Como as telas e a tarefa agendada chegam à biblioteca Rust: o perfil em uso, o endereço dele e a pasta dos dados. */
object CoreAccess {
    private const val PREFS = "baseconhecimento"

    /** O endereço do SUAP do perfil em uso. */
    fun baseUrl(context: Context): String = Profiles.active(context).url

    /** Muda o endereço do perfil em uso. */
    fun saveBaseUrl(context: Context, url: String) {
        Profiles.changeUrl(context, url)
    }

    /** O nome da pasta que as versões anteriores davam a um endereço (o primeiro perfil a mantém). */
    fun legacyDirName(url: String): String {
        val host = runCatching { URI(url).authority }.getOrNull() ?: "suap"
        return "perfil-" + host.replace(Regex("[^A-Za-z0-9.-]"), "_")
    }

    /** A pasta da sessão e dos dados de um perfil (no armazenamento privado do app). */
    fun dataDir(context: Context, profile: AppProfile): File = File(context.filesDir, profile.dir)

    /** Abre a cópia local e a sessão guardada do perfil em uso. */
    fun open(context: Context): Base {
        val profile = Profiles.active(context)
        val dir = dataDir(context, profile).apply { mkdirs() }
        return Base(dir.absolutePath, profile.url)
    }

    /** Só atualiza com Wi-Fi (rede sem franquia)? Padrão: sim, porque a base pode ter centenas de anexos. */
    fun unmeteredOnly(context: Context): Boolean =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean("unmetered_only", true)

    fun saveUnmeteredOnly(context: Context, value: Boolean) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean("unmetered_only", value).apply()
    }
}
