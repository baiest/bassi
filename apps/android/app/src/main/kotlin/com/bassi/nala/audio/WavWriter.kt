package com.bassi.nala.audio

import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * Wraps raw 16-bit PCM samples in a 44-byte WAV header — the exact format
 * `voice --serve` expects (mono, 16-bit, matching sample rate), so the
 * server never has to resample or reject a well-formed recording.
 */
object WavWriter {
    private const val HEADER_SIZE = 44
    private const val BITS_PER_SAMPLE = 16

    fun wrap(pcm: ShortArray, sampleRate: Int, channels: Int): ByteArray {
        val byteRate = sampleRate * channels * BITS_PER_SAMPLE / 8
        val blockAlign = channels * BITS_PER_SAMPLE / 8
        val dataSize = pcm.size * 2
        val riffChunkSize = 36 + dataSize

        val header = ByteBuffer.allocate(HEADER_SIZE).order(ByteOrder.LITTLE_ENDIAN)
        header.put("RIFF".toByteArray(Charsets.US_ASCII))
        header.putInt(riffChunkSize)
        header.put("WAVE".toByteArray(Charsets.US_ASCII))
        header.put("fmt ".toByteArray(Charsets.US_ASCII))
        header.putInt(16) // fmt chunk size for PCM
        header.putShort(1) // audio format: PCM
        header.putShort(channels.toShort())
        header.putInt(sampleRate)
        header.putInt(byteRate)
        header.putShort(blockAlign.toShort())
        header.putShort(BITS_PER_SAMPLE.toShort())
        header.put("data".toByteArray(Charsets.US_ASCII))
        header.putInt(dataSize)

        val data = ByteBuffer.allocate(dataSize).order(ByteOrder.LITTLE_ENDIAN)
        for (sample in pcm) {
            data.putShort(sample)
        }

        val output = ByteArrayOutputStream(HEADER_SIZE + dataSize)
        output.write(header.array())
        output.write(data.array())
        return output.toByteArray()
    }
}
