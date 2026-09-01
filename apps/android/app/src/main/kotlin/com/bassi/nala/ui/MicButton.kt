package com.bassi.nala.ui

import android.animation.ValueAnimator
import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.util.AttributeSet
import android.view.MotionEvent
import android.view.View
import android.view.animation.LinearInterpolator
import androidx.core.content.ContextCompat
import com.bassi.nala.R

private const val IDLE_COLOR = "#3F51B5"
private const val RECORDING_COLOR = "#F44336"
private const val RING_COLOR = "#663F51B5"
private const val AMPLITUDE_ANIM_MS = 120L
private const val PRESS_SCALE = 0.92f
private const val PRESS_ANIM_MS = 100L
private const val RELEASE_ANIM_MS = 150L
private const val MIN_RING_AMPLITUDE = 0.2f

/**
 * A circular mic button that is also its own level meter: while
 * [recording], a translucent ring around it grows and shrinks with
 * [amplitude] (the RMS level `Recorder` reports). The ring's radius is
 * tweened (not snapped) toward each new amplitude, since raw per-buffer
 * readings arrive in visible jumps — this is what makes the "breathing"
 * look smooth instead of jittery. Also scales down slightly on press for
 * tactile feedback, like a native button.
 */
class MicButton @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : View(context, attrs) {

    private var targetAmplitude = 0f
    private var displayedAmplitude = 0f
    private var amplitudeAnimator: ValueAnimator? = null

    var amplitude: Float
        get() = targetAmplitude
        set(value) {
            targetAmplitude = value.coerceIn(0f, 1f)
            animateAmplitudeTowardTarget()
        }

    var recording: Boolean = false
        set(value) {
            field = value
            invalidate()
        }

    private val buttonPaint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val ringPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.parseColor(RING_COLOR) }
    private val micIcon = ContextCompat.getDrawable(context, R.drawable.ic_mic)?.mutate()?.apply {
        setTint(Color.WHITE)
    }

    private fun animateAmplitudeTowardTarget() {
        amplitudeAnimator?.cancel()
        amplitudeAnimator = ValueAnimator.ofFloat(displayedAmplitude, targetAmplitude).apply {
            duration = AMPLITUDE_ANIM_MS
            interpolator = LinearInterpolator()
            addUpdateListener {
                displayedAmplitude = it.animatedValue as Float
                invalidate()
            }
            start()
        }
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        val centerX = width / 2f
        val centerY = height / 2f
        val maxRadius = minOf(width, height) / 2f
        val buttonRadius = maxRadius * 0.5f

        if (recording) {
            // A visible ring even at very low amplitude (so soft speech
            // still shows *something* moving), growing to the full
            // available radius at max amplitude.
            val ringAmplitude = MIN_RING_AMPLITUDE + (1f - MIN_RING_AMPLITUDE) * displayedAmplitude
            val ringRadius = buttonRadius + (maxRadius - buttonRadius) * ringAmplitude
            canvas.drawCircle(centerX, centerY, ringRadius, ringPaint)
        }

        buttonPaint.color = Color.parseColor(if (recording) RECORDING_COLOR else IDLE_COLOR)
        canvas.drawCircle(centerX, centerY, buttonRadius, buttonPaint)

        micIcon?.let { icon ->
            val iconRadius = (buttonRadius * 0.6f).toInt()
            icon.setBounds(
                (centerX - iconRadius).toInt(),
                (centerY - iconRadius).toInt(),
                (centerX + iconRadius).toInt(),
                (centerY + iconRadius).toInt(),
            )
            icon.draw(canvas)
        }
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        when (event.action) {
            MotionEvent.ACTION_DOWN -> {
                animate().scaleX(PRESS_SCALE).scaleY(PRESS_SCALE).setDuration(PRESS_ANIM_MS).start()
            }
            MotionEvent.ACTION_UP -> {
                animate().scaleX(1f).scaleY(1f).setDuration(RELEASE_ANIM_MS).start()
                performClick()
            }
            MotionEvent.ACTION_CANCEL -> {
                animate().scaleX(1f).scaleY(1f).setDuration(RELEASE_ANIM_MS).start()
            }
        }
        return true
    }

    override fun performClick(): Boolean {
        super.performClick()
        return true
    }
}
