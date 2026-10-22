package br.edu.ifrn.baseconhecimento

import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.waitUntilAtLeastOneExists
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Before
import org.junit.Rule
import org.junit.Test

/** O app inteiro com a biblioteca Rust de verdade e uma cópia local sintética: tudo se lê sem rede. */
@OptIn(ExperimentalTestApi::class)
class OfflineScreensTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext
    private var scenario: ActivityScenario<MainActivity>? = null

    @Before
    fun seed() {
        Seed.install(context)
        scenario = ActivityScenario.launch(MainActivity::class.java)
    }

    @After
    fun clean() {
        scenario?.close()
        Seed.clean(context)
    }

    private fun waitFor(text: String, substring: Boolean = false) {
        compose.waitUntilAtLeastOneExists(hasText(text, substring = substring), 10_000)
    }

    @Test
    fun theListOpensOfflineWithTheCopyEvenWithoutASession() {
        waitFor(Seed.PRIVATE_TITLE)
        compose.onNodeWithText(Seed.PRIVATE_TITLE).assertIsDisplayed()
        compose.onNodeWithText(Seed.PUBLIC_TITLE).assertIsDisplayed()
        compose.onNodeWithText("Sem sessão do SUAP: lendo só a cópia local.").assertIsDisplayed()
        compose.onNodeWithText("2 entradas").assertIsDisplayed()
    }

    @Test
    fun filteringByVisibilityNarrowsTheList() {
        waitFor(Seed.PRIVATE_TITLE)
        compose.onNodeWithText("Visibilidade").performClick()
        compose.onNodeWithText("Sigilosa").assertIsDisplayed()
        compose.onNodeWithText("Pública").performClick()
        waitFor("1 entrada")
        compose.onNodeWithText(Seed.PUBLIC_TITLE).assertIsDisplayed()
        compose.onNodeWithText("Limpar filtros").performClick()
        waitFor("2 entradas")
    }

    @Test
    fun searchFindsTheAttachmentText() {
        waitFor(Seed.PRIVATE_TITLE)
        compose.onNodeWithContentDescription("Buscar").performClick()
        compose.onNodeWithText("Buscar nas entradas e anexos").performTextInput("cartucho")
        waitFor("no anexo guia.txt", substring = true)
        compose.onNodeWithText(Seed.PRIVATE_TITLE).assertIsDisplayed()
    }

    @Test
    fun anEntryIsReadWithItsMarkdownAndAttachments() {
        waitFor(Seed.PRIVATE_TITLE)
        compose.onNodeWithText(Seed.PRIVATE_TITLE).performClick()
        waitFor("Resumo")
        compose.onNodeWithText("Pendente de correção").assertIsDisplayed()
        compose.onNodeWithText("retirar o cartucho").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("guia.txt (", substring = true).performScrollTo().assertIsDisplayed()
    }

    @Test
    fun settingsShowTheCopyAndTheSchedule() {
        waitFor(Seed.PRIVATE_TITLE)
        compose.onNodeWithContentDescription("Acesso rápido").performClick()
        compose.onNodeWithText("Configurações da atualização").performClick()
        waitFor("Atualização automática")
        compose.onNodeWithText("Última atualização: nunca").assertIsDisplayed()
        compose.onNodeWithText("Entrar para atualizar").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("7 dias").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Apagar a cópia local").performScrollTo().assertIsDisplayed()
    }
}
