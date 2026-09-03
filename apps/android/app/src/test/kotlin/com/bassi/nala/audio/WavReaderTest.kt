package com.bassi.nala.audio

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class WavReaderTest {

    @Test
    fun decodesMonoSamplesAndFormat() {
        val samples = shortArrayOf(0, 1000, -1000, Short.MAX_VALUE)
        val wav = WavWriter.wrap(samples, sampleRate = 22_050, channels = 1)

        val decoded = WavReader.decode(wav)

        assertArrayEquals(samples, decoded.samples)
        assertEquals(22_050, decoded.sampleRate)
        assertEquals(1, decoded.channels)
    }

    @Test
    fun decodesStereoWithoutRejectingIt() {
        val samples = shortArrayOf(0, 0, 100, 100)
        val wav = WavWriter.wrap(samples, sampleRate = 44_100, channels = 2)

        val decoded = WavReader.decode(wav)

        assertEquals(2, decoded.channels)
        assertEquals(4, decoded.samples.size)
    }

    @Test
    fun rejectsGarbageBytes() {
        assertThrows(WavReader.DecodeError::class.java) {
            WavReader.decode("not a wav file".toByteArray())
        }
    }

    @Test
    fun rejectsTruncatedData() {
        val wav = WavWriter.wrap(shortArrayOf(1, 2, 3, 4), sampleRate = 16_000, channels = 1)
        assertThrows(WavReader.DecodeError::class.java) {
            WavReader.decode(wav.copyOf(20))
        }
    }
}
