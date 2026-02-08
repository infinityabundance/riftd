package com.example.riftmobile.sdk

sealed class RiftEvent {
    data class Chat(val from: String, val text: String, val timestamp: String) : RiftEvent()
    data class PeerJoined(val peerId: String) : RiftEvent()
    data class PeerLeft(val peerId: String) : RiftEvent()
    data class Stats(val peers: Int) : RiftEvent()
    data class Security(val message: String) : RiftEvent()
    data class Fingerprint(val peerId: String, val fingerprint: String) : RiftEvent()
    data class Status(val text: String) : RiftEvent()
}
