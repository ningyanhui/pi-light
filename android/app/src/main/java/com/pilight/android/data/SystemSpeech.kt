package com.pilight.android.data

import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.speech.RecognitionListener
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import java.util.Locale

/** 系统 SpeechRecognizer 错误码 → 可展示文案（数字与 Android SDK 对齐，便于 JVM 单测）。 */
fun describeSpeechError(code: Int): String =
    when (code) {
        1, 2 -> "系统语音识别需要网络（多数机型走云端 ASR）"
        3 -> "麦克风不可用"
        4, 11 -> "系统语音识别服务不可用"
        5 -> "语音识别客户端出错"
        6 -> "没有听到说话"
        7 -> "没有听清，请再试一次"
        8 -> "语音识别正忙，请稍后再试"
        9 -> "需要麦克风权限才能语音输入"
        12, 13 -> "当前系统语言不受语音识别支持"
        else -> "语音识别失败（$code）"
    }

interface SpeechInputListener {
    fun onPartial(text: String)

    fun onFinal(text: String)

    fun onError(message: String)

    fun onUnavailable(message: String)

    fun onEnded()
}

/**
 * 优先走平台 [SpeechRecognizer]（系统 ASR，通常是 Google 语音服务）。
 * 不把音频送到 Pi / 中继，只回传转写文本。
 */
class SystemSpeechInput(
    context: Context,
    private val listener: SpeechInputListener,
) {
    private val appContext = context.applicationContext
    private val main = Handler(Looper.getMainLooper())
    private var recognizer: SpeechRecognizer? = null
    var listening: Boolean = false
        private set

    fun available(): Boolean = SpeechRecognizer.isRecognitionAvailable(appContext)

    fun start() {
        main.post {
            if (listening) return@post
            if (!available()) {
                listener.onUnavailable("这台设备没有可用的系统语音识别。")
                return@post
            }
            val engine = recognizer ?: SpeechRecognizer.createSpeechRecognizer(appContext).also {
                it.setRecognitionListener(Listener())
                recognizer = it
            }
            val intent =
                Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).apply {
                    putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
                    putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, true)
                    putExtra(RecognizerIntent.EXTRA_MAX_RESULTS, 1)
                    putExtra(RecognizerIntent.EXTRA_LANGUAGE, Locale.getDefault().toLanguageTag())
                }
            listening = true
            runCatching { engine.startListening(intent) }.onFailure { e ->
                listening = false
                listener.onError(e.message ?: "无法启动系统语音识别")
            }
        }
    }

    fun cancel() {
        main.post {
            listening = false
            runCatching { recognizer?.cancel() }
            listener.onEnded()
        }
    }

    fun destroy() {
        main.post {
            listening = false
            runCatching { recognizer?.destroy() }
            recognizer = null
        }
    }

    private inner class Listener : RecognitionListener {
        override fun onReadyForSpeech(params: Bundle?) = Unit

        override fun onBeginningOfSpeech() = Unit

        override fun onRmsChanged(rmsdB: Float) = Unit

        override fun onBufferReceived(buffer: ByteArray?) = Unit

        override fun onEndOfSpeech() = Unit

        override fun onError(error: Int) {
            listening = false
            listener.onError(describeSpeechError(error))
            listener.onEnded()
        }

        override fun onResults(results: Bundle?) {
            listening = false
            val text = firstResult(results)
            if (!text.isNullOrBlank()) {
                listener.onFinal(text)
            } else {
                listener.onError(describeSpeechError(7))
            }
            listener.onEnded()
        }

        override fun onPartialResults(partialResults: Bundle?) {
            val text = firstResult(partialResults)
            if (!text.isNullOrBlank()) {
                listener.onPartial(text)
            }
        }

        override fun onEvent(eventType: Int, params: Bundle?) = Unit

        private fun firstResult(bundle: Bundle?): String? =
            bundle
                ?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)
                ?.firstOrNull()
                ?.trim()
                ?.ifBlank { null }
    }
}
