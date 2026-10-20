package br.edu.ifrn.baseconhecimento

import java.time.ZoneOffset
import org.junit.Assert.assertEquals
import org.junit.Test

class FormatTest {
    @Test
    fun sizes() {
        assertEquals("12 B", Format.size(12))
        assertEquals("2 KB", Format.size(2048))
        assertEquals("2,5 MB", Format.size(2_621_440))
    }

    @Test
    fun moments() {
        assertEquals("nunca", Format.whenText(null))
        assertEquals("20/10/2026 14:05", Format.whenText(1_792_505_100, ZoneOffset.UTC))
    }

    @Test
    fun entryCounts() {
        assertEquals("1 entrada", Format.entries(1))
        assertEquals("0 entradas", Format.entries(0))
        assertEquals("7 entradas", Format.entries(7))
    }

    @Test
    fun updateOutcomeText() {
        assertEquals("3 novas, 1 atualizada", Updater.describe(3, 1, 0, 0, false))
        assertEquals("1 nova, 0 atualizadas, 2 sem acesso, 1 falha (interrompida)", Updater.describe(1, 0, 2, 1, true))
        assertEquals("0 novas, 2 atualizadas, 3 falhas", Updater.describe(0, 2, 0, 3, false))
    }

    @Test
    fun loginIsOverWhenTheSiteLeavesTheAccountPages() {
        val base = "https://suap.exemplo.br/"
        assertEquals(false, isAfterLogin(base, "https://suap.exemplo.br/accounts/login/"))
        assertEquals(false, isAfterLogin(base, "https://acesso.gov.br/login"))
        assertEquals(true, isAfterLogin(base, "https://suap.exemplo.br/centralservicos/baseconhecimento/"))
        assertEquals(false, isAfterLogin("não é endereço", "https://suap.exemplo.br/"))
    }
}
