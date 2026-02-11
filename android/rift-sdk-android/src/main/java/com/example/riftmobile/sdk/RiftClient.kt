package com.example.riftmobile.sdk

import android.content.Context
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.launch
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

class RiftClient(private val context: Context, private val configPath: String?) {
    private val scope = CoroutineScope(Dispatchers.IO)
    private val _events = MutableSharedFlow<RiftEvent>(extraBufferCapacity = 64)
    val events = _events.asSharedFlow()

    private var handle: Long = 0
    private var pollJob: Job? = null
    private val timeFormat = SimpleDateFormat("HH:mm:ss", Locale.US)

    fun init(): Boolean {
        if (handle != 0L) return true
        handle = RiftNative.init(context, configPath)
        return handle != 0L
    }

    fun lastError(): String? = RiftNative.lastError()

    fun createIdentity(): Boolean {
        return init()
    }

    fun setInvite(invite: String?) {
        if (handle == 0L) return
        RiftNative.setInvite(handle, invite)
    }

    fun setBootstrapNodes(nodes: String) {
        if (handle == 0L) return
        RiftNative.setBootstrapNodes(handle, nodes)
    }

    fun setDhtEnabled(enabled: Boolean) {
        if (handle == 0L) return
        RiftNative.setDhtEnabled(handle, enabled)
    }

    fun setTurnServers(servers: String) {
        if (handle == 0L) return
        RiftNative.setTurnServers(handle, servers)
    }

    fun setAudioQuality(quality: String?) {
        if (handle == 0L) return
        RiftNative.setAudioQuality(handle, quality)
    }

    fun generateInvite(channel: String, password: String?, knownPeers: String): String? {
        return RiftNative.generateInvite(channel, password, knownPeers)
    }

    suspend fun joinChannel(name: String, password: String?, internet: Boolean, dht: Boolean): Boolean {
        if (handle == 0L) return false
        val result = RiftNative.joinChannel(handle, name, password, internet, dht)
        if (result == 0) {
            startPolling()
        }
        return result == 0
    }

    suspend fun leaveChannel(name: String) {
        if (handle == 0L) return
        RiftNative.leaveChannel(handle, name)
        stopPolling()
    }

    fun sendChat(text: String): Boolean {
        if (handle == 0L) return false
        return RiftNative.sendChat(handle, text) == 0
    }

    fun startPtt(): Boolean {
        if (handle == 0L) return false
        return RiftNative.startPtt(handle) == 0
    }

    fun stopPtt(): Boolean {
        if (handle == 0L) return false
        return RiftNative.stopPtt(handle) == 0
    }

    fun setMute(muted: Boolean): Boolean {
        if (handle == 0L) return false
        return RiftNative.setMute(handle, muted) == 0
    }

    fun startCall(peerHex: String): String? {
        if (handle == 0L) return null
        return RiftNative.startCall(handle, peerHex)
    }

    fun acceptCall(sessionHex: String): Boolean {
        if (handle == 0L) return false
        return RiftNative.acceptCall(handle, sessionHex) == 0
    }

    fun declineCall(sessionHex: String, reason: String? = null): Boolean {
        if (handle == 0L) return false
        return RiftNative.declineCall(handle, sessionHex, reason) == 0
    }

    fun endCall(sessionHex: String): Boolean {
        if (handle == 0L) return false
        return RiftNative.endCall(handle, sessionHex) == 0
    }

    private fun startPolling() {
        if (pollJob != null) return
        pollJob = scope.launch {
            while (true) {
                val event = RiftNative.pollEvent(handle)
                if (event != null) {
                    handleEvent(event)
                } else {
                    delay(50)
                }
            }
        }
    }

    private fun stopPolling() {
        pollJob?.cancel()
        pollJob = null
    }

    private fun handleEvent(event: RiftEventDto) {
        when (event.type) {
            "chat" -> {
                val timestamp = event.status ?: timeFormat.format(Date())
                _events.tryEmit(RiftEvent.Chat(event.from ?: "peer", event.text ?: "", timestamp))
            }
            "peer_joined" -> _events.tryEmit(RiftEvent.PeerJoined(event.from ?: ""))
            "peer_left" -> _events.tryEmit(RiftEvent.PeerLeft(event.from ?: ""))
            "incoming_call" -> _events.tryEmit(
                RiftEvent.IncomingCall(
                    sessionId = event.status ?: "",
                    from = event.from ?: "",
                    rndzvSrtUri = event.text
                )
            )
            "call_state" -> _events.tryEmit(
                RiftEvent.CallStateChanged(
                    sessionId = event.status ?: "",
                    state = event.text ?: "unknown"
                )
            )
            "audio_level" -> {
                val level = event.text?.toFloatOrNull() ?: 0f
                _events.tryEmit(RiftEvent.AudioLevel(event.from ?: "", level))
            }
            "stats" -> _events.tryEmit(RiftEvent.Stats(event.peers ?: 0))
            "security" -> _events.tryEmit(RiftEvent.Security(event.text ?: ""))
            "fingerprint" -> _events.tryEmit(
                RiftEvent.Fingerprint(event.from ?: "", event.text ?: "")
            )
            "status" -> _events.tryEmit(RiftEvent.Status(event.status ?: ""))
        }
    }
}
