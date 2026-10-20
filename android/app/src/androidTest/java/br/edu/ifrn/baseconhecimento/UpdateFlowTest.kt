package br.edu.ifrn.baseconhecimento

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.net.HttpURLConnection
import java.net.URL
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.baseconhecimento_mobile.Base
import uniffi.baseconhecimento_mobile.EntryFilter
import uniffi.baseconhecimento_mobile.UpdateParams
import uniffi.baseconhecimento_mobile.updateProgress

/**
 * A atualização de verdade pela rede do aparelho, contra o SUAP falso do computador (`python -I scripts/suap_falso.py`;
 * o emulador o enxerga em 10.0.2.2). Sem esse servidor no ar o teste é ignorado.
 */
@RunWith(AndroidJUnit4::class)
class UpdateFlowTest {
    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext
    private val url = "http://10.0.2.2:8765/"

    @Before
    fun requireTheFakeSuap() {
        val up = runCatching {
            (URL(url).openConnection() as HttpURLConnection).run {
                connectTimeout = 1500
                responseCode == 200
            }
        }.getOrDefault(false)
        assumeTrue("SUAP falso fora do ar (scripts/suap_falso.py)", up)
        Seed.clean(context)
    }

    @After
    fun clean() {
        Seed.clean(context)
    }

    @Test
    fun logsInByCookiesDownloadsEverythingAndReadsItOffline() {
        val base = Base(Seed.dataDir(context).absolutePath, url)
        assertEquals(2u, base.importSessionCookies("sessionid=abc; csrftoken=xyz"))
        assertTrue(base.isAuthenticated())

        val summary = base.update(UpdateParams(false, false, emptyList(), null))
        assertEquals(2u, summary.added)
        assertEquals(listOf(3uL), summary.inaccessible)
        assertEquals(1u, summary.attachments)
        assertEquals(1u, summary.images)
        assertFalse(summary.cancelled)
        val progress = updateProgress()
        assertFalse(progress.running)
        assertEquals(3u, progress.done)

        val nothing = EntryFilter(null, null, null, emptyList())
        assertEquals(2, base.entries(nothing).size)
        val hits = base.search("cartucho", nothing, 10u)
        assertEquals(1, hits.size)
        assertEquals("guia.txt", hits[0].source)

        val detail = base.entry(1uL)
        assertEquals(1, detail.attachments.size)
        assertTrue(detail.markdown.contains("](../anexos/1/imagens/"))
        assertNotNull(base.status().settings.lastUpdate)

        // Segunda rodada: nada mudou.
        val again = base.update(UpdateParams(false, false, emptyList(), null))
        assertEquals(2u, again.unchanged)

        // Sem rede de novo (logout não apaga nada): a leitura continua.
        base.logout()
        assertFalse(base.hasSession())
        assertEquals(2, base.entries(nothing).size)
    }

    @Test
    fun theBackgroundWorkerRunsTheSameRoundAndRecordsTheOutcome() {
        // O perfil em uso passa a ser o do SUAP falso: o Worker abre o perfil em uso.
        assertEquals(null, Profiles.add(context, "Falso", url))
        val base = CoreAccess.open(context)
        base.importSessionCookies("sessionid=abc")
        val worker = androidx.work.testing.TestListenableWorkerBuilder<UpdateWorker>(context)
            .setInputData(androidx.work.workDataOf(Updater.MANUAL to true))
            .build()
        val result = worker.doWork()
        assertEquals(androidx.work.ListenableWorker.Result.success(), result)
        val outcome = Updater.lastOutcome(context)
        assertNotNull(outcome)
        assertFalse(outcome!!.error)
        assertEquals("2 novas, 0 atualizadas, 1 sem acesso", outcome.text)
    }
}
