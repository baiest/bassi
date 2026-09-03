package com.bassi.nala.ui

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.util.AttributeSet
import android.view.Choreographer
import android.view.MotionEvent
import android.view.View
import com.bassi.nala.ui.core.CoreColor
import com.bassi.nala.ui.core.CoreStatus
import com.bassi.nala.ui.core.Motion
import com.bassi.nala.ui.core.Point3
import com.bassi.nala.ui.core.RING_POINTS
import com.bassi.nala.ui.core.RING_TILTS
import com.bassi.nala.ui.core.RgbaColor
import com.bassi.nala.ui.core.SPHERE_POINTS
import com.bassi.nala.ui.core.Scene

private const val SAFE_MARGIN = 0.14f
private const val PULSE_GAIN = 1.2f
private const val CORE_RADIUS_FRACTION = 0.28f
private const val HALO_RADIUS_FACTOR = 1.6f
private const val SCENE_PULSE_GAIN = 0.1f
private const val AMPLITUDE_HALF_LIFE = 0.08f
private const val YAW_SPEED = 0.35f
private const val PITCH_SPEED = 0.13f
private const val MIN_POINT_RADIUS_DP = 1.0f
private const val MAX_POINT_RADIUS_DP = 2.6f
private const val PRESS_SCALE = 0.92f
private const val PRESS_ANIM_MS = 100L
private const val RELEASE_ANIM_MS = 150L
private const val NANOS_PER_SECOND = 1_000_000_000.0f
private const val MAX_DT = 0.1f

/**
 * The "Jarvis" core: a 3D point cloud + halo + solid center, reactive to
 * voice, replacing the old flat mic button. A Canvas-drawn port of
 * `apps/nala-overlay`'s egui rendering (`overlay.rs`) — same geometry
 * ([Scene]), same colors ([CoreColor]), same motion ([Motion]), so the
 * phone and the desktop overlay read as the same object.
 *
 * Tap toggles recording (wired by the caller via [setOnClickListener]); this
 * view only owns the animation and painting. Unlike the desktop overlay,
 * which repaints continuously forever, the animation loop here is tied to
 * the window's attach state — it must not keep the CPU busy while the
 * activity (and the screen) is off.
 */
class NalaCoreView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : View(context, attrs) {

    var status: CoreStatus = CoreStatus.IDLE
        set(value) {
            field = value
            invalidate()
        }

    /** Raw amplitude in `[0, 1]`; smoothed internally before it drives the pulse. */
    var amplitude: Float = 0f

    private val density = resources.displayMetrics.density
    private val sphere = Scene.spherePoints(SPHERE_POINTS)
    private val rings = RING_TILTS.map { tilt -> Scene.ringPoints(RING_POINTS, tilt) }

    private var smoothedAmplitude = 0f
    private var yaw = 0f
    private var pitch = 0f
    private var elapsed = 0f
    private var lastFrameNanos = 0L

    private val corePaint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val haloPaint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val pointPaint = Paint(Paint.ANTI_ALIAS_FLAG)

    private val frameCallback = object : Choreographer.FrameCallback {
        override fun doFrame(frameTimeNanos: Long) {
            val dt = if (lastFrameNanos == 0L) {
                0f
            } else {
                ((frameTimeNanos - lastFrameNanos) / NANOS_PER_SECOND).coerceAtMost(MAX_DT)
            }
            lastFrameNanos = frameTimeNanos

            elapsed += dt
            smoothedAmplitude = Motion.smooth(smoothedAmplitude, amplitude, dt, AMPLITUDE_HALF_LIFE)
            yaw += YAW_SPEED * dt
            pitch = kotlin.math.sin(PITCH_SPEED * elapsed) * 0.3f

            invalidate()
            Choreographer.getInstance().postFrameCallback(this)
        }
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        lastFrameNanos = 0L
        Choreographer.getInstance().postFrameCallback(frameCallback)
    }

    override fun onDetachedFromWindow() {
        Choreographer.getInstance().removeFrameCallback(frameCallback)
        super.onDetachedFromWindow()
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)

        val center = width / 2f to height / 2f
        val sceneRadius = minOf(width, height) / 2f * (1.0f - SAFE_MARGIN)

        val pulse = if (status == CoreStatus.IDLE) {
            Motion.breathe(elapsed) * 0.5f
        } else {
            smoothedAmplitude
        }
        val coreRadius = sceneRadius * CORE_RADIUS_FRACTION * (1.0f + pulse * PULSE_GAIN)
        val pointsRadius = sceneRadius * (1.0f + pulse * SCENE_PULSE_GAIN)

        drawPoints(canvas, center, pointsRadius, CoreColor.accentColor(status))

        haloPaint.color = toArgb(CoreColor.glowColor(status))
        canvas.drawCircle(center.first, center.second, coreRadius * HALO_RADIUS_FACTOR, haloPaint)

        corePaint.color = toArgb(CoreColor.statusColor(status))
        canvas.drawCircle(center.first, center.second, coreRadius, corePaint)
    }

    private fun drawPoints(canvas: Canvas, center: Pair<Float, Float>, radius: Float, color: RgbaColor) {
        val minRadiusPx = MIN_POINT_RADIUS_DP * density
        val maxRadiusPx = MAX_POINT_RADIUS_DP * density

        val projected = (sphere.asSequence() + rings.asSequence().flatten())
            .map { Scene.rotate(it, yaw, pitch) }
            .map { Scene.project(it, radius, Scene.PERSPECTIVE) }
            .toList()

        for (point in Scene.depthSorted(projected)) {
            val pointRadius = (minRadiusPx * point.scale).coerceIn(minRadiusPx, maxRadiusPx)
            val alpha = (((point.scale - 0.5f).coerceIn(0f, 1f)) * 255).toInt().coerceAtLeast(40)
            pointPaint.color = Color.argb(alpha, color.red, color.green, color.blue)
            canvas.drawCircle(
                center.first + point.pos.first,
                center.second + point.pos.second,
                pointRadius,
                pointPaint,
            )
        }
    }

    private fun toArgb(color: RgbaColor): Int = Color.argb(color.alpha, color.red, color.green, color.blue)

    override fun onTouchEvent(event: MotionEvent): Boolean {
        when (event.action) {
            MotionEvent.ACTION_DOWN ->
                animate().scaleX(PRESS_SCALE).scaleY(PRESS_SCALE).setDuration(PRESS_ANIM_MS).start()
            MotionEvent.ACTION_UP -> {
                animate().scaleX(1f).scaleY(1f).setDuration(RELEASE_ANIM_MS).start()
                performClick()
            }
            MotionEvent.ACTION_CANCEL ->
                animate().scaleX(1f).scaleY(1f).setDuration(RELEASE_ANIM_MS).start()
        }
        return true
    }

    override fun performClick(): Boolean {
        super.performClick()
        return true
    }
}
