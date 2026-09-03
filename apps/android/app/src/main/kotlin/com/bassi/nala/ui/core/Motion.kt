package com.bassi.nala.ui.core

import kotlin.math.exp
import kotlin.math.sin

/**
 * Pure time-based helpers for animating the core: framerate-independent
 * smoothing and a slow idle "breathing" pulse. Ported 1:1 from
 * `apps/nala-overlay/src/motion.rs`. The caller supplies `dt`/`elapsed`,
 * which keeps this testable without a real animation loop.
 */
object Motion {

    /** How long, in seconds, a "breathe" cycle takes at rest. */
    const val BREATHE_PERIOD: Float = 3.2f

    private const val TAU = (2.0 * Math.PI).toFloat()

    /**
     * Exponentially smooths `current` toward `target` over `dt` seconds,
     * with `halfLife` controlling how quickly it catches up. Framerate
     * independent.
     */
    fun smooth(current: Float, target: Float, dt: Float, halfLife: Float): Float {
        if (halfLife <= 0.0f) return target
        val decay = exp(-dt / halfLife)
        return target + (current - target) * decay
    }

    /** A slow, `[0.0, 1.0]`-normalized pulse for the idle state. */
    fun breathe(elapsed: Float): Float {
        val phase = (elapsed / BREATHE_PERIOD) * TAU
        return (sin(phase) + 1.0f) / 2.0f
    }
}
