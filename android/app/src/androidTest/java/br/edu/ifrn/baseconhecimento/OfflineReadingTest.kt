package br.edu.ifrn.baseconhecimento

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.baseconhecimento_mobile.BaseException
import uniffi.baseconhecimento_mobile.EntryFilter
import uniffi.baseconhecimento_mobile.EntryVisibility
import uniffi.baseconhecimento_mobile.UpdateParams
import uniffi.baseconhecimento_mobile.coreVersion

/** A biblioteca Rust de verdade no aparelho: leitura, filtros, busca e configurações, tudo sem rede. */
@RunWith(AndroidJUnit4::class)
class OfflineReadingTest {
    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext

    @Before
    fun seed() {
        Seed.install(context)
    }

    @After
    fun clean() {
        Seed.clean(context)
    }

    private fun base() = CoreAccess.open(context)

    private val nothing = EntryFilter(null, null, null, emptyList())

    @Test
    fun theNativeLibraryLoads() {
        assertTrue(coreVersion().matches(Regex("""\d+\.\d+\.\d+""")))
    }

    @Test
    fun listsFiltersAndSearchesTheLocalCopy() {
        val base = base()
        val all = base.entries(nothing)
        assertEquals(setOf(Seed.PUBLIC_TITLE, Seed.PRIVATE_TITLE), all.map { it.title }.toSet())
        assertEquals(2, all.size)
        val options = base.filterOptions()
        assertEquals(listOf("Acesso", "Tecnologia"), options.areas)
        assertEquals(listOf("conta", "impressora", "toner"), options.tags)
        val private = base.entries(EntryFilter(null, null, EntryVisibility.PRIVATE, emptyList()))
        assertEquals(listOf(Seed.PRIVATE_TITLE), private.map { it.title })
        assertEquals(1, base.entries(EntryFilter("TECNOLOGIA", null, null, emptyList())).size)
        assertEquals(1, base.entries(EntryFilter(null, "Acesso", null, emptyList())).size)
        assertEquals(1, base.entries(EntryFilter(null, null, null, listOf("toner"))).size)

        val byAttachment = base.search("cartucho", nothing, 10u)
        assertEquals(1, byAttachment.size)
        assertEquals("guia.txt", byAttachment[0].source)
        assertEquals(Seed.PRIVATE_TITLE, byAttachment[0].title)
        assertEquals(1, base.search("matricula", nothing, 10u).size)
        assertTrue(base.search("zzzz", nothing, 10u).isEmpty())
        assertTrue(base.search("cartucho", EntryFilter(null, null, EntryVisibility.PUBLIC, emptyList()), 10u).isEmpty())
    }

    @Test
    fun readsAnEntryWithItsAttachmentsAndResolvesLinksOnlyInsideTheCopy() {
        val detail = base().entry(1uL)
        assertEquals(Seed.PRIVATE_TITLE, detail.summary.title)
        assertTrue(detail.markdown.startsWith("## Resumo"))
        assertTrue(detail.needsCorrection)
        assertEquals("Pessoa Zero", detail.updatedBy)
        assertEquals(listOf("guia.txt"), detail.attachments.map { it.name })
        assertTrue(File(detail.attachments[0].path).readText().contains("cartucho"))

        val entries = File(detail.entriesDir)
        val root = File(detail.rootDir)
        val guide = Links.resolve("../anexos/1/guia.txt", entries, root)
        assertEquals(LinkTarget.Local(File(detail.attachments[0].path).canonicalFile), guide)
        assertTrue(Links.resolve("../anexos/1/imagens/esquema.png", entries, root) is LinkTarget.Local)
        assertEquals(LinkTarget.None, Links.resolve("../../../../../../etc/hosts", entries, root))
        assertEquals(LinkTarget.None, Links.resolve("../anexos/1/ausente.txt", entries, root))

        val blocks = Markdown.parse(detail.markdown)
        assertTrue(blocks.any { it is Block.Picture && it.src == "../anexos/1/imagens/esquema.png" })
        assertTrue(blocks.filterIsInstance<Block.Item>().any { it.depth == 1 })
    }

    @Test
    fun anEntryThatWasNeverDownloadedIsAnError() {
        try {
            base().entry(99uL)
            fail("deveria falhar")
        } catch (error: BaseException.Failed) {
            assertTrue(error.readable().contains("não está na cópia local"))
        }
    }

    @Test
    fun settingsAreKeptAndValidated() {
        val base = base()
        val defaults = base.settings()
        assertTrue(defaults.autoUpdate && defaults.attachments)
        assertEquals(7u, defaults.intervalDays)
        val saved = base.saveSettings(false, 14u, false)
        assertEquals(saved, base.settings())
        assertFalse(saved.autoUpdate)
        try {
            base.saveSettings(true, 0u, true)
            fail("o intervalo 0 deveria ser recusado")
        } catch (_: BaseException.Failed) {
        }
        val status = base.status()
        assertEquals(2u, status.entries)
        assertEquals(1u, status.private)
        assertFalse(status.due)
    }

    @Test
    fun updatingNeedsASessionAndNeverTouchesTheCopyWithout() {
        val base = base()
        assertFalse(base.hasSession())
        assertFalse(base.isAuthenticated())
        try {
            base.update(UpdateParams(false, false, emptyList(), null))
            fail("sem sessão deveria falhar")
        } catch (_: BaseException.SessionExpired) {
        }
        assertEquals(2, base.entries(nothing).size)
    }

    @Test
    fun deletingTheLocalCopyEmptiesTheList() {
        val base = base()
        base.deleteLocalCopy()
        assertTrue(base.entries(nothing).isEmpty())
        assertEquals(0u, base.status().entries)
    }
}
