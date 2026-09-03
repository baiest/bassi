package com.bassi.nala.audio

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AmplitudeTest {

    @Test
    fun silenceIsZeroAmplitude() {
        assertEquals(0f, Amplitude.fromSamples(ShortArray(100)))
    }

    @Test
    fun emptyInputIsZeroNotACrash() {
        assertEquals(0f, Amplitude.fromSamples(ShortArray(0)))
    }

    @Test
    fun fullScaleIsAtOrNearFullAmplitude() {
        val samples = ShortArray(100) { Short.MAX_VALUE }
        assertTrue(Amplitude.fromSamples(samples) > 0.95f)
    }

    @Test
    fun aQuieterSignalHasLowerAmplitudeThanALouderOne() {
        val quiet = ShortArray(100) { 1000 }
        val loud = ShortArray(100) { 20000 }
        assertTrue(Amplitude.fromSamples(quiet) < Amplitude.fromSamples(loud))
    }

    @Test
    fun amplitudeIsNeverNegativeOrAboveOne() {
        for (value in shortArrayOf(0, 1, 100, 1000, Short.MAX_VALUE, Short.MIN_VALUE)) {
            val amplitude = Amplitude.fromSamples(ShortArray(10) { value })
            assertTrue(amplitude in 0f..1f)
        }
    }

    @Test
    fun windowsSplitsIntoTheExpectedNumberOfChunks() {
        val samples = ShortArray(250) { 1000 }
        val windows = Amplitude.windows(samples, 100)
        assertEquals(3, windows.size) // 100, 100, 50
    }

    @Test
    fun windowsWithZeroWindowLenReturnsNothing() {
        assertTrue(Amplitude.windows(shortArrayOf(1, 2, 3), 0).isEmpty())
    }

    @Test
    fun windowsOfEmptySamplesIsEmpty() {
        assertTrue(Amplitude.windows(ShortArray(0), 100).isEmpty())
    }
}
