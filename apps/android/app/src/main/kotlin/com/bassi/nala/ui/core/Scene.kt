package com.bassi.nala.ui.core

import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * Pure 3D geometry for the core's "Jarvis" point cloud: a sphere plus a few
 * tilted orbital rings, rotated and projected to 2D with simple perspective.
 * Ported 1:1 from `apps/nala-overlay/src/scene.rs` — same constants, same
 * math — so the desktop overlay and this view read as the same object. No
 * Android types here, so the math is testable off-device.
 */

/** How many points make up the sphere's point cloud. */
const val SPHERE_POINTS = 90

/** How many points make up each orbital ring. */
const val RING_POINTS = 48

/** Tilt (radians, rotation around X) of each orbital ring. */
val RING_TILTS = floatArrayOf(0.35f, -0.55f)

/** A point in the scene's local 3D space, before rotation/projection. */
data class Point3(val x: Float, val y: Float, val z: Float)

/** A `Point3` after rotation and perspective projection, ready to paint. */
data class Projected(
    /** Position in the projection plane, in units of the caller's radius. */
    val pos: Pair<Float, Float>,
    /** Original (rotated) depth — more positive is closer to the camera. */
    val depth: Float,
    /** Perspective scale factor: > 1.0 closer than the origin, < 1.0 farther. */
    val scale: Float,
)

object Scene {

    /**
     * Distance from the camera to the projection plane, in units of the
     * scene's radius (always 1.0 before projection). Must stay greater than
     * 1.0 so no point (at most 1.0 from the origin) can ever reach the
     * camera and divide by zero.
     */
    const val PERSPECTIVE: Float = 2.6f

    /**
     * Spreads `count` points evenly over the unit sphere using the
     * Fibonacci (golden angle) lattice — deterministic, no clustering at
     * the poles. `count == 0` returns an empty list.
     */
    fun spherePoints(count: Int): List<Point3> {
        if (count == 0) return emptyList()

        val goldenAngle = PI.toFloat() * (3.0f - sqrt(5.0f))
        return (0 until count).map { i ->
            val n = count.toFloat()
            val y = 1.0f - (i / max(n - 1.0f, 1.0f)) * 2.0f
            val radiusAtY = sqrt(max(1.0f - y * y, 0.0f))
            val theta = goldenAngle * i
            Point3(cos(theta) * radiusAtY, y, sin(theta) * radiusAtY)
        }
    }

    /**
     * Builds a unit circle in the XZ plane, then tilts it by `tilt` radians
     * around the X axis — one orbital ring. `count == 0` returns an empty
     * list.
     */
    fun ringPoints(count: Int, tilt: Float): List<Point3> {
        if (count == 0) return emptyList()

        return (0 until count).map { i ->
            val angle = 2.0f * PI.toFloat() * i / count
            val x = cos(angle)
            val zBase = sin(angle)
            val y = -zBase * sin(tilt)
            val z = zBase * cos(tilt)
            Point3(x, y, z)
        }
    }

    /** Rotates `p` around the Y axis by `yaw`, then around the X axis by `pitch`. */
    fun rotate(p: Point3, yaw: Float, pitch: Float): Point3 {
        val sinYaw = sin(yaw)
        val cosYaw = cos(yaw)
        val x1 = p.x * cosYaw + p.z * sinYaw
        val z1 = -p.x * sinYaw + p.z * cosYaw
        val y1 = p.y

        val sinPitch = sin(pitch)
        val cosPitch = cos(pitch)
        val y2 = y1 * cosPitch - z1 * sinPitch
        val z2 = y1 * sinPitch + z1 * cosPitch

        return Point3(x1, y2, z2)
    }

    /**
     * Projects `p` (assumed already rotated) onto a 2D plane with simple
     * perspective, scaling by `radius`. `perspective` must be > 1.0.
     */
    fun project(p: Point3, radius: Float, perspective: Float): Projected {
        val k = perspective / (perspective - p.z)
        return Projected(pos = (p.x * k * radius) to (p.y * k * radius), depth = p.z, scale = k)
    }

    /**
     * Sorts projected points back-to-front (ascending depth) so painting
     * them in order gives a correct painter's-algorithm overlap.
     */
    fun depthSorted(points: List<Projected>): List<Projected> = points.sortedBy { it.depth }
}
