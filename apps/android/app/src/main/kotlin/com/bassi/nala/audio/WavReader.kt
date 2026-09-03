package com.bassi.nala.audio

import java.nio.ByteBuffer
import java.nio.ByteOrder

/** A decoded WAV clip: whatever sample rate/channel count its header reports. */
data class DecodedClip(val samples: ShortArray, val sampleRate: Int, val channels: Int)

/**
 * Decodes a 16-bit PCM WAV file, keeping whatever sample rate/channel count
 * its header reports — a reply clip's format depends on whatever TTS backend
 * `voice --serve` is running, so this never assumes [WavWriter]'s own fixed
 * 44-byte layout and instead walks RIFF chunks, tolerating extra chunks
 * (e.g. `LIST`) between `fmt ` and `data`.
 */
object WavReader {

    class DecodeError(message: String) : Exception(message)

    fun decode(bytes: ByteArray): DecodedClip {
        if (bytes.size < 12) throw DecodeError("file too short to be a WAV")

        val buffer = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
        val riff = readAscii(buffer, 4)
        buffer.int // RIFF chunk size, unused
        val wave = readAscii(buffer, 4)
        if (riff != "RIFF" || wave != "WAVE") throw DecodeError("not a RIFF/WAVE file")

        var channels = -1
        var sampleRate = -1
        var bitsPerSample = -1
        var samples: ShortArray? = null

        while (buffer.remaining() >= 8) {
            val chunkId = readAscii(buffer, 4)
            val chunkSize = buffer.int
            if (chunkSize < 0 || chunkSize > buffer.remaining()) {
                throw DecodeError("chunk '$chunkId' size exceeds remaining data")
            }
            val chunkEnd = buffer.position() + chunkSize

            when (chunkId) {
                "fmt " -> {
                    if (chunkSize < 16) throw DecodeError("fmt chunk too short")
                    buffer.short // audio format, unused
                    channels = buffer.short.toInt()
                    sampleRate = buffer.int
                    buffer.int // byte rate, unused
                    buffer.short // block align, unused
                    bitsPerSample = buffer.short.toInt()
                }
                "data" -> {
                    if (bitsPerSample != 16) {
                        throw DecodeError("unsupported bits per sample: $bitsPerSample")
                    }
                    val sampleCount = chunkSize / 2
                    samples = ShortArray(sampleCount) { buffer.short }
                }
            }

            // Chunks are word-aligned; the padding byte may be absent past
            // the last chunk, so never seek beyond what's actually there.
            buffer.position((chunkEnd + (chunkSize and 1)).coerceAtMost(buffer.limit()))
        }

        if (channels <= 0 || sampleRate <= 0) throw DecodeError("missing or invalid fmt chunk")
        val decodedSamples = samples ?: throw DecodeError("missing data chunk")

        return DecodedClip(decodedSamples, sampleRate, channels)
    }

    private fun readAscii(buffer: ByteBuffer, length: Int): String {
        val bytes = ByteArray(length)
        buffer.get(bytes)
        return String(bytes, Charsets.US_ASCII)
    }
}
