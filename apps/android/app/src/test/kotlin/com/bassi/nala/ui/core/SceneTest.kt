package com.bassi.nala.ui.core

import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.sqrt
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

private const val EPSILON = 1e-4f

private fun Point3.norm(): Float = sqrt(x * x + y * y + z * z)

class SceneTest {

    @Test
    fun spherePointsReturnsTheRequestedCount() {
        assertEquals(90, Scene.spherePoints(90).size)
    }

    @Test
    fun spherePointsOfZeroIsEmptyNotACrash() {
        assertTrue(Scene.spherePoints(0).isEmpty())
    }

    @Test
    fun everySpherePointLiesOnTheUnitSphere() {
        for (p in Scene.spherePoints(50)) {
            assertTrue("point off the sphere: $p", abs(p.norm() - 1.0f) < 1e-3f)
        }
    }

    @Test
    fun ringPointsOfZeroIsEmptyNotACrash() {
        assertTrue(Scene.ringPoints(0, 0.3f).isEmpty())
    }

    @Test
    fun everyRingPointLiesOnTheUnitCircle() {
        for (p in Scene.ringPoints(32, 0.4f)) {
            assertTrue("point off the ring: $p", abs(p.norm() - 1.0f) < EPSILON)
        }
    }

    @Test
    fun anUntiltedRingStaysInTheXzPlane() {
        for (p in Scene.ringPoints(16, 0.0f)) {
            assertTrue(abs(p.y) < EPSILON)
        }
    }

    @Test
    fun rotatingByZeroIsIdentity() {
        val p = Point3(0.3f, 0.5f, 0.8f)
        val rotated = Scene.rotate(p, 0.0f, 0.0f)

        assertTrue(abs(rotated.x - p.x) < EPSILON)
        assertTrue(abs(rotated.y - p.y) < EPSILON)
        assertTrue(abs(rotated.z - p.z) < EPSILON)
    }

    @Test
    fun rotationPreservesDistanceFromTheOrigin() {
        val p = Point3(0.2f, -0.6f, 0.7f)
        val rotated = Scene.rotate(p, 1.234f, -0.876f)

        assertTrue(abs(rotated.norm() - p.norm()) < EPSILON)
    }

    @Test
    fun aFullTurnReturnsToTheStart() {
        val p = Point3(0.4f, 0.1f, 0.9f)
        val rotated = Scene.rotate(p, (2.0 * PI).toFloat(), (2.0 * PI).toFloat())

        assertTrue(abs(rotated.x - p.x) < 1e-3f)
        assertTrue(abs(rotated.y - p.y) < 1e-3f)
        assertTrue(abs(rotated.z - p.z) < 1e-3f)
    }

    @Test
    fun aCloserPointProjectsWithABiggerScale() {
        val near = Scene.project(Point3(0.0f, 0.0f, 0.5f), 100.0f, Scene.PERSPECTIVE)
        val far = Scene.project(Point3(0.0f, 0.0f, -0.5f), 100.0f, Scene.PERSPECTIVE)

        assertTrue(near.scale > far.scale)
    }

    @Test
    fun aPointOnTheAxisProjectsToTheCenter() {
        val projected = Scene.project(Point3(0.0f, 0.0f, 0.3f), 100.0f, Scene.PERSPECTIVE)

        assertTrue(abs(projected.pos.first) < EPSILON)
        assertTrue(abs(projected.pos.second) < EPSILON)
    }

    @Test
    fun projectionNeverDividesByZeroForAnyPointOnTheUnitSphere() {
        for (p in Scene.spherePoints(200)) {
            val projected = Scene.project(p, 100.0f, Scene.PERSPECTIVE)
            assertTrue(projected.scale.isFinite())
        }
    }

    @Test
    fun depthSortedOrdersAscendingByDepth() {
        val points = listOf(
            Projected(0.0f to 0.0f, depth = 0.5f, scale = 1.0f),
            Projected(0.0f to 0.0f, depth = -0.5f, scale = 1.0f),
            Projected(0.0f to 0.0f, depth = 0.0f, scale = 1.0f),
        )

        val sorted = Scene.depthSorted(points)

        assertEquals(-0.5f, sorted[0].depth)
        assertEquals(0.0f, sorted[1].depth)
        assertEquals(0.5f, sorted[2].depth)
    }

    @Test
    fun depthSortedIsStableForTies() {
        val points = listOf(
            Projected(1.0f to 0.0f, depth = 0.0f, scale = 1.0f),
            Projected(2.0f to 0.0f, depth = 0.0f, scale = 1.0f),
        )

        val sorted = Scene.depthSorted(points)

        assertEquals(1.0f to 0.0f, sorted[0].pos)
        assertEquals(2.0f to 0.0f, sorted[1].pos)
    }
}
