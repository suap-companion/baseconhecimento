package br.edu.ifrn.baseconhecimento

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.work.BackoffPolicy
import androidx.work.Constraints
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import androidx.work.Worker
import androidx.work.workDataOf
import java.util.concurrent.TimeUnit
import uniffi.baseconhecimento_mobile.BaseException
import uniffi.baseconhecimento_mobile.UpdateParams
import uniffi.baseconhecimento_mobile.UpdateSummary
import uniffi.baseconhecimento_mobile.cancelUpdate

/** Como terminou a última atualização (guardado só no aparelho; nunca traz títulos nem textos). */
data class Outcome(val text: String, val error: Boolean, val at: Long)

/**
 * A atualização em segundo plano. A tarefa agendada confere todo dia, e a biblioteca só baixa quando o intervalo
 * configurado (padrão: uma semana) venceu; "Atualizar agora" roda a mesma tarefa uma vez, sem esperar o intervalo.
 * O WorkManager sobrevive a reinício, respeita bateria fraca, rede e o Doze.
 */
object Updater {
    private const val PERIODIC = "atualizacao-agendada"
    private const val NOW = "atualizacao-agora"
    private const val CHANNEL = "atualizacao"
    private const val NOTIFICATION_ID = 1
    private const val PREFS = "baseconhecimento"
    const val MANUAL = "manual"

    /** Texto curto do resultado; as contagens vêm da biblioteca e não identificam nenhuma entrada. */
    fun describe(added: Int, updated: Int, inaccessible: Int, failed: Int, cancelled: Boolean): String {
        val parts = buildList {
            add(if (added == 1) "1 nova" else "$added novas")
            add(if (updated == 1) "1 atualizada" else "$updated atualizadas")
            if (inaccessible > 0) add("$inaccessible sem acesso")
            if (failed > 0) add(if (failed == 1) "1 falha" else "$failed falhas")
        }
        return parts.joinToString(", ") + if (cancelled) " (interrompida)" else ""
    }

    fun describe(summary: UpdateSummary): String = describe(
        summary.added.toInt(), summary.updated.toInt(), summary.inaccessible.size, summary.failed.size, summary.cancelled,
    )

    /** O intervalo de conferência da tarefa agendada: diário, e a biblioteca decide se já venceu. */
    private const val CHECK_HOURS = 24L

    /** Liga ou desliga a atualização agendada (ao mudar a configuração ou a regra de rede). */
    fun schedule(context: Context, enabled: Boolean, unmeteredOnly: Boolean) {
        val work = WorkManager.getInstance(context)
        if (!enabled) {
            work.cancelUniqueWork(PERIODIC)
            return
        }
        val constraints = Constraints.Builder()
            .setRequiredNetworkType(if (unmeteredOnly) NetworkType.UNMETERED else NetworkType.CONNECTED)
            .setRequiresBatteryNotLow(true)
            .setRequiresStorageNotLow(true)
            .build()
        val request = PeriodicWorkRequestBuilder<UpdateWorker>(CHECK_HOURS, TimeUnit.HOURS)
            .setConstraints(constraints)
            .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 1, TimeUnit.HOURS)
            .build()
        // UPDATE: mudar a regra de rede vale já, sem perder o horário da próxima conferência.
        work.enqueueUniquePeriodicWork(PERIODIC, ExistingPeriodicWorkPolicy.UPDATE, request)
    }

    /** Pede uma atualização agora (qualquer rede); o andamento aparece na tela pelo `updateProgress`. */
    fun runNow(context: Context) {
        val request = OneTimeWorkRequestBuilder<UpdateWorker>()
            .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
            .setInputData(workDataOf(MANUAL to true))
            .build()
        WorkManager.getInstance(context).enqueueUniqueWork(NOW, ExistingWorkPolicy.KEEP, request)
    }

    /** Para a atualização em andamento (o que já veio fica salvo) e tira da fila a que ainda não começou. */
    fun cancel(context: Context) {
        cancelUpdate()
        WorkManager.getInstance(context).cancelUniqueWork(NOW)
    }

    fun lastOutcome(context: Context): Outcome? {
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val text = prefs.getString("outcome_text", null) ?: return null
        return Outcome(text, prefs.getBoolean("outcome_error", false), prefs.getLong("outcome_at", 0))
    }

    fun remember(context: Context, text: String, error: Boolean) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
            .putString("outcome_text", text)
            .putBoolean("outcome_error", error)
            .putLong("outcome_at", System.currentTimeMillis() / 1000)
            .apply()
    }

    private fun canNotify(context: Context): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED

    /** Avisa o fim da atualização agendada. Na tela bloqueada não aparece o texto, só um aviso genérico. */
    @Suppress("MissingPermission")
    fun notify(context: Context, title: String, text: String) {
        if (!canNotify(context)) return
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(CHANNEL, "Atualização da base", NotificationManager.IMPORTANCE_LOW)
            context.getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
        }
        val open = PendingIntent.getActivity(
            context, 0, Intent(context, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val publicVersion = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentTitle("Base de Conhecimento")
            .build()
        val notification = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentTitle(title)
            .setContentText(text)
            .setContentIntent(open)
            .setAutoCancel(true)
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
            .setPublicVersion(publicVersion)
            .build()
        NotificationManagerCompat.from(context).notify(NOTIFICATION_ID, notification)
    }

    fun canAskNotifications(): Boolean = Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU
}

/** Uma rodada de atualização: a agendada confere o intervalo de cada perfil; a manual roda o perfil em uso. */
class UpdateWorker(context: Context, params: WorkerParameters) : Worker(context, params) {
    override fun doWork(): Result {
        val context = applicationContext
        val manual = inputData.getBoolean(Updater.MANUAL, false)
        val profiles = if (manual) listOf(Profiles.active(context)) else Profiles.list(context)
        var retry = false
        for (profile in profiles) {
            try {
                val base = uniffi.baseconhecimento_mobile.Base(CoreAccess.dataDir(context, profile).apply { mkdirs() }.absolutePath, profile.url)
                val summary = if (manual) base.update(UpdateParams(false, false, emptyList(), null)) else base.updateIfDue()
                if (summary != null) report(context, profile, summary, manual)
            } catch (_: BaseException.SessionExpired) {
                Updater.remember(context, "Sessão do SUAP expirada: entre de novo no app.", true)
                if (!manual) Updater.notify(context, "Base de Conhecimento: sessão expirada", "Abra o app e entre de novo para continuar atualizando.")
            } catch (error: Throwable) {
                // Sem rede ou SUAP fora do ar: a mensagem não traz conteúdo da base; a agendada tenta de novo mais tarde.
                Updater.remember(context, error.readable(), true)
                retry = retry || !manual
            }
        }
        return if (retry) Result.retry() else Result.success()
    }

    private fun report(context: Context, profile: AppProfile, summary: UpdateSummary, manual: Boolean) {
        val text = Updater.describe(summary)
        Updater.remember(context, text, false)
        val changed = summary.added > 0u || summary.updated > 0u
        if (!manual && changed && !summary.cancelled) {
            Updater.notify(context, "Base de Conhecimento atualizada (${profile.name})", text)
        }
    }

    override fun onStopped() {
        // A tarefa foi parada pelo sistema (ou cancelada): a rodada nativa termina antes da próxima entrada.
        cancelUpdate()
    }
}
