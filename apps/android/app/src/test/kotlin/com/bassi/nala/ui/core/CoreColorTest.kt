package com.bassi.nala.ui.core

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class CoreColorTest {

    private val all = CoreStatus.values().toList()

    @Test
    fun everyStatusMapsToADistinctColor() {
        val colors = all.map { CoreColor.statusColor(it) }
        for (i in colors.indices) {
            for (j in colors.indices) {
                if (i != j) assertNotEquals("statuses at $i and $j share a color", colors[i], colors[j])
            }
        }
    }

    @Test
    fun errorIsAShadeOfRed() {
        val color = CoreColor.statusColor(CoreStatus.ERROR)
        assertTrue(color.red > color.green && color.red > color.blue)
    }

    @Test
    fun statusColorIsDeterministic() {
        assertEquals(CoreColor.statusColor(CoreStatus.SPEAKING), CoreColor.statusColor(CoreStatus.SPEAKING))
    }

    @Test
    fun idleIsAShadeOfBlue() {
        val color = CoreColor.statusColor(CoreStatus.IDLE)
        assertTrue(color.blue > color.red && color.blue > color.green)
    }

    @Test
    fun everyStatusMapsToADistinctAccentColor() {
        val colors = all.map { CoreColor.accentColor(it) }
        for (i in colors.indices) {
            for (j in colors.indices) {
                if (i != j) assertNotEquals("accents at $i and $j share a color", colors[i], colors[j])
            }
        }
    }

    @Test
    fun glowColorIsFaint() {
        for (status in all) {
            assertTrue(CoreColor.glowColor(status).alpha < 128)
        }
    }
}
