package com.bassi.nala.audio

import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.concurrent.thread
import kotlin.math.log10
import kotlin.math.sqrt

/**
 * Records mono PCM16 at [SAMPLE_RATE] — exactly what `voice --serve`
 * expects, so nothing has to resample on either end. `onAmplitude` fires
 * once per read chunk with the RMS level normalized to `[0, 1]`, off the
 * calling thread, so a UI can drive a waveform without polling.
 */
class Recorder(private val onAmplitude: (Float) -> Unit) {

    private var audioRecord: AudioRecord? = null
    private val isRecording = AtomicBoolean(false)
    private var recordThread: Thread? = null
    private val samples = ArrayList<Short>()

    @Suppress("MissingPermission") // caller checks RECORD_AUDIO before calling start()
    fun start() {
        if (isRecording.getAndSet(true)) return
        samples.clear()

        val minBufferSize = AudioRecord.getMinBufferSize(SAMPLE_RATE, CHANNEL_CONFIG, AUDIO_FORMAT)
        val record = AudioRecord(
            MediaRecorder.AudioSource.VOICE_RECOGNITION,
            SAMPLE_RATE,
            CHANNEL_CONFIG,
            AUDIO_FORMAT,
            minBufferSize * 4,
        )
        audioRecord = record
        record.startRecording()

        recordThread = thread(name = "nala-record") {
            val buffer = ShortArray(minBufferSize)
            while (isRecording.get()) {
                val read = record.read(buffer, 0, buffer.size)
                if (read > 0) {
                    var sumSquares = 0.0
                    for (i in 0 until read) {
                        samples.add(buffer[i])
                        sumSquares += buffer[i].toDouble() * buffer[i].toDouble()
                    }
                    val rms = sqrt(sumSquares / read)
                    onAmplitude(amplitudeFromRms(rms))
                }
            }
        }
    }

    /** Stops recording, waits for the capture thread to drain, and returns the PCM captured. */
    fun stop(): ShortArray {
        if (!isRecording.getAndSet(false)) return ShortArray(0)
        recordThread?.join()
        recordThread = null
        audioRecord?.stop()
        audioRecord?.release()
        audioRecord = null
        return samples.toShortArray()
    }

    companion object {
        const val SAMPLE_RATE = 16_000
        private const val CHANNEL_CONFIG = AudioFormat.CHANNEL_IN_MONO
        private const val AUDIO_FORMAT = AudioFormat.ENCODING_PCM_16BIT

        // Quiet speech sits far below full scale, so a plain rms/32767
        // ratio barely nudges the meter. dB is how loudness is actually
        // perceived: `SILENCE_FLOOR_DB` is the level mapped to 0 (below it
        // is "nothing happening"), full scale (0 dBFS) maps to 1.
        private const val SILENCE_FLOOR_DB = -50.0

        /** Maps an RMS sample level to `[0, 1]` on a dB scale, not linear. */
        fun amplitudeFromRms(rms: Double): Float {
            if (rms <= 0.0) return 0f
            val db = 20.0 * log10(rms / Short.MAX_VALUE)
            return ((db - SILENCE_FLOOR_DB) / -SILENCE_FLOOR_DB).coerceIn(0.0, 1.0).toFloat()
        }
    }
}
