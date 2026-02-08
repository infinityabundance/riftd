package com.example.riftmobile

object RiftNative {
    init {
        System.loadLibrary("rift_sdk")
    }

    external fun init(context: android.content.Context, configPath: String?): Long
    external fun joinChannel(handle: Long, name: String, password: String?, internet: Boolean, dht: Boolean): Int
    external fun leaveChannel(handle: Long, name: String): Int
    external fun sendChat(handle: Long, text: String): Int
    external fun startPtt(handle: Long): Int
    external fun stopPtt(handle: Long): Int
    external fun pollEvent(handle: Long): RiftEventDto?
    external fun lastError(): String?
    external fun setBootstrapNodes(handle: Long, nodes: String)
    external fun setDhtEnabled(handle: Long, enabled: Boolean)
}

data class RiftEventDto(
    val type: String,
    val from: String?,
    val text: String?,
    val peers: Int?,
    val status: String?
)
