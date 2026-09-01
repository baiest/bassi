package com.bassi.nala.audio

import org.junit.Assert.assertEquals
import org.junit.Test
import java.nio.ByteBuffer
import java.nio.ByteOrder

class WavWriterTest {

    @Test
    fun `wraps pcm with a valid RIFF WAVE header`() {
        val pcm = shortArrayOf(0, 100, -100, Short.MAX_VALUE)
        val wav = WavWriter.wrap(pcm, sampleRate = 16000, channels = 1)

        val buffer = ByteBuffer.wrap(wav).order(ByteOrder.LITTLE_ENDIAN)
        val riff = ByteArray(4).also { buffer.get(it) }
        assertEquals("RIFF", String(riff, Charsets.US_ASCII))

        val chunkSize = buffer.int
        assertEquals(36 + pcm.size * 2, chunkSize)

        val wave = ByteArray(4).also { buffer.get(it) }
        assertEquals("WAVE", String(wave, Charsets.US_ASCII))

        val fmt = ByteArray(4).also { buffer.get(it) }
        assertEquals("fmt ", String(fmt, Charsets.US_ASCII))
    }

    @Test
    fun `reflects sample rate and channel count in the fmt chunk`() {
        val wav = WavWriter.wrap(shortArrayOf(1, 2, 3), sampleRate = 22050, channels = 2)

        val buffer = ByteBuffer.wrap(wav).order(ByteOrder.LITTLE_ENDIAN)
        buffer.position(22) // channels field offset
        val channels = buffer.short
        val sampleRate = buffer.int

        assertEquals(2, channels.toInt())
        assertEquals(22050, sampleRate)
    }

    @Test
    fun `data chunk size matches the number of pcm bytes`() {
        val pcm = shortArrayOf(1, 2, 3)
        val wav = WavWriter.wrap(pcm, sampleRate = 16000, channels = 1)

        val buffer = ByteBuffer.wrap(wav).order(ByteOrder.LITTLE_ENDIAN)
        buffer.position(40) // data chunk size field offset
        val dataSize = buffer.int

        assertEquals(pcm.size * 2, dataSize)
        assertEquals(44 + pcm.size * 2, wav.size)
    }

    @Test
    fun `produces just the header for empty pcm`() {
        val wav = WavWriter.wrap(ShortArray(0), sampleRate = 16000, channels = 1)

        assertEquals(44, wav.size)
    }

    @Test
    fun `preserves sample values through the byte layout`() {
        val pcm = shortArrayOf(Short.MIN_VALUE, -1, 0, 1, Short.MAX_VALUE)
        val wav = WavWriter.wrap(pcm, sampleRate = 16000, channels = 1)

        val buffer = ByteBuffer.wrap(wav).order(ByteOrder.LITTLE_ENDIAN)
        buffer.position(44)
        val decoded = ShortArray(pcm.size) { buffer.short }

        assertEquals(pcm.toList(), decoded.toList())
    }
}
