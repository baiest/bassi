package com.bassi.nala.ui.core

import kotlin.math.abs
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class MotionTest {

    @Test
    fun smoothMovesTowardTheTarget() {
        val result = Motion.smooth(0.0f, 1.0f, 0.1f, 0.2f)
        assertTrue(result > 0.0f && result < 1.0f)
    }

    @Test
    fun smoothWithZeroDtDoesNotChange() {
        assertEquals(0.3f, Motion.smooth(0.3f, 1.0f, 0.0f, 0.2f))
    }

    @Test
    fun smoothNeverOvershootsTheTarget() {
        val result = Motion.smooth(0.0f, 1.0f, 100.0f, 0.2f)
        assertTrue(result <= 1.0f)
        assertTrue(abs(result - 1.0f) < 1e-3f)
    }

    @Test
    fun smoothConvergesFromAboveToo() {
        val result = Motion.smooth(1.0f, 0.0f, 100.0f, 0.2f)
        assertTrue(result >= 0.0f)
        assertTrue(abs(result) < 1e-3f)
    }

    @Test
    fun smoothIsFramerateIndependent() {
        val halfLife = 0.2f
        val oneStep = Motion.smooth(0.0f, 1.0f, 0.1f, halfLife)

        var twoSteps = 0.0f
        twoSteps = Motion.smooth(twoSteps, 1.0f, 0.05f, halfLife)
        twoSteps = Motion.smooth(twoSteps, 1.0f, 0.05f, halfLife)

        assertTrue(abs(oneStep - twoSteps) < 1e-4f)
    }

    @Test
    fun breatheStaysInUnitRange() {
        for (i in 0 until 100) {
            val elapsed = i * 0.1f
            val value = Motion.breathe(elapsed)
            assertTrue(value in 0.0f..1.0f)
        }
    }

    @Test
    fun breatheIsPeriodic() {
        val a = Motion.breathe(0.5f)
        val b = Motion.breathe(0.5f + Motion.BREATHE_PERIOD)
        assertTrue(abs(a - b) < 1e-3f)
    }
}
