package com.example.riftmobile

import android.Manifest
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    private val viewModel: MainViewModel by viewModels()

    private val requestMicPermission = registerForActivityResult(
        ActivityResultContracts.RequestPermission()
    ) { granted ->
        viewModel.onMicPermission(granted)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        viewModel.initHandle()
        setContent {
            MaterialTheme {
                Surface {
                    RiftApp(viewModel, onRequestMic = {
                        requestMicPermission.launch(Manifest.permission.RECORD_AUDIO)
                    })
                }
            }
        }
    }
}

data class ChatItem(
    val time: String,
    val from: String,
    val text: String,
)

class MainViewModel : ViewModel() {
    var handle by mutableStateOf(0L)
        private set
    var channelName by mutableStateOf("")
    var password by mutableStateOf("")
    var connected by mutableStateOf(false)
    var statusText by mutableStateOf("disconnected")
    var peerCount by mutableStateOf(0)
    var messages by mutableStateOf(listOf<ChatItem>())
    var inputText by mutableStateOf("")
    var pttPressed by mutableStateOf(false)
    var micPermission by mutableStateOf(false)

    fun initHandle() {
        if (handle != 0L) return
        handle = RiftNative.init(null)
    }

    fun onMicPermission(granted: Boolean) {
        micPermission = granted
    }

    fun connect() {
        if (handle == 0L) return
        statusText = "connecting"
        val passwordValue = password.ifEmpty { null }
        RiftNative.joinChannel(handle, channelName, passwordValue, true, true)
        connected = true
        statusText = "connected"
        startPolling()
    }

    fun disconnect() {
        if (handle == 0L) return
        RiftNative.leaveChannel(handle, channelName)
        connected = false
        statusText = "disconnected"
    }

    fun sendChat() {
        val text = inputText.trim()
        if (text.isEmpty() || handle == 0L) return
        RiftNative.sendChat(handle, text)
        inputText = ""
    }

    fun startPtt() {
        if (!micPermission || handle == 0L) return
        RiftNative.startPtt(handle)
        pttPressed = true
    }

    fun stopPtt() {
        if (handle == 0L) return
        RiftNative.stopPtt(handle)
        pttPressed = false
    }

    private fun startPolling() {
        viewModelScope.launch(Dispatchers.IO) {
            while (connected) {
                val event = RiftNative.pollEvent(handle)
                if (event != null) {
                    when (event.type) {
                        "chat" -> {
                            val item = ChatItem(
                                time = event.status ?: "",
                                from = event.from ?: "peer",
                                text = event.text ?: ""
                            )
                            messages = messages + item
                        }
                        "stats" -> {
                            if (event.peers != null) {
                                peerCount = event.peers
                            }
                        }
                        "peer_joined" -> {
                            peerCount = (event.peers ?: peerCount)
                        }
                        "peer_left" -> {
                            peerCount = (event.peers ?: peerCount)
                        }
                        "status" -> {
                            statusText = event.status ?: statusText
                        }
                    }
                }
                delay(50)
            }
        }
    }
}

@Composable
fun RiftApp(viewModel: MainViewModel, onRequestMic: () -> Unit) {
    Column(modifier = Modifier.fillMaxSize().padding(16.dp)) {
        ConnectionPanel(viewModel, onRequestMic)
        Spacer(modifier = Modifier.height(12.dp))
        ChatList(viewModel.messages, modifier = Modifier.weight(1f))
        Spacer(modifier = Modifier.height(12.dp))
        ChatInput(
            value = viewModel.inputText,
            onValueChange = { viewModel.inputText = it },
            onSend = { viewModel.sendChat() }
        )
        Spacer(modifier = Modifier.height(12.dp))
        PttButton(
            pressed = viewModel.pttPressed,
            onDown = { viewModel.startPtt() },
            onUp = { viewModel.stopPtt() }
        )
    }
}

@Composable
fun ConnectionPanel(viewModel: MainViewModel, onRequestMic: () -> Unit) {
    Column {
        Text(
            text = "Status: ${viewModel.statusText} | Peers: ${viewModel.peerCount}",
            fontWeight = FontWeight.Bold
        )
        Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedTextField(
                value = viewModel.channelName,
                onValueChange = { viewModel.channelName = it },
                label = { Text("Channel") },
                modifier = Modifier.weight(1f)
            )
            OutlinedTextField(
                value = viewModel.password,
                onValueChange = { viewModel.password = it },
                label = { Text("Password") },
                modifier = Modifier.weight(1f)
            )
        }
        Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(onClick = {
                if (!viewModel.micPermission) {
                    onRequestMic()
                }
                viewModel.connect()
            }) {
                Text("Connect")
            }
            Button(onClick = { viewModel.disconnect() }) {
                Text("Disconnect")
            }
        }
    }
}

@Composable
fun ChatList(messages: List<ChatItem>, modifier: Modifier = Modifier) {
    LazyColumn(modifier = modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surfaceVariant)) {
        items(messages) { item ->
            Text("[${item.time}] ${item.from}: ${item.text}", modifier = Modifier.padding(4.dp))
        }
    }
}

@Composable
fun ChatInput(value: String, onValueChange: (String) -> Unit, onSend: () -> Unit) {
    Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedTextField(
            value = value,
            onValueChange = onValueChange,
            label = { Text("Message") },
            modifier = Modifier.weight(1f)
        )
        Button(onClick = onSend) {
            Text("Send")
        }
    }
}

@Composable
fun PttButton(pressed: Boolean, onDown: () -> Unit, onUp: () -> Unit) {
    val color = if (pressed) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.secondary
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .height(80.dp)
            .background(color)
            .pointerInput(Unit) {
                detectTapGestures(
                    onPress = {
                        onDown()
                        tryAwaitRelease()
                        onUp()
                    }
                )
            },
        contentAlignment = androidx.compose.ui.Alignment.Center
    ) {
        Text(text = if (pressed) "Talking..." else "Hold to Talk", color = MaterialTheme.colorScheme.onPrimary)
    }
}
