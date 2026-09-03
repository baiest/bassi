package com.bassi.nala.audio

import kotlin.math.log10
import kotlin.math.sqrt

/**
 * Turns raw PCM16 samples into a loudness curve the core can pulse to.
 * Pure — no audio device — so both live mic capture ([Recorder]) and clip
 * playback ([com.bassi.nala.playback.ClipPlayer]) can share it. Ported 1:1
 * from `apps/nala-overlay/src/amplitude.rs`, same -50 dBFS silence floor as
 * [Recorder.amplitudeFromRms] so a clip sounds/looks the same played on the
 * phone or on the desktop overlay.
 */
object Amplitude {

    private const val SILENCE_FLOOR_DB = -50.0

    /** RMS loudness of `samples`, dB-mapped into `[0, 1]`. Empty input is silence. */
    fun fromSamples(samples: ShortArray): Float {
        if (samples.isEmpty()) return 0f

        val sumSquares = samples.sumOf { it.toDouble() * it.toDouble() }
        val rms = sqrt(sumSquares / samples.size)
        if (rms <= 0.0) return 0f

        val db = 20.0 * log10(rms / Short.MAX_VALUE)
        return ((db - SILENCE_FLOOR_DB) / -SILENCE_FLOOR_DB).coerceIn(0.0, 1.0).toFloat()
    }

    /**
     * Splits `samples` into fixed-size windows (the last one possibly
     * shorter) and computes [fromSamples] for each — the loudness-over-time
     * curve a playback loop walks in real time to drive the core.
     */
    fun windows(samples: ShortArray, windowLen: Int): List<Float> {
        if (windowLen == 0 || samples.isEmpty()) return emptyList()
        return samples.toList().chunked(windowLen) { chunk -> fromSamples(chunk.toShortArray()) }
    }
}
