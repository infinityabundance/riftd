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
import androidx.compose.material3.Switch
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
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import android.util.Log

class MainActivity : ComponentActivity() {
    private val viewModel: MainViewModel by viewModels()

    private val requestMicPermission = registerForActivityResult(
        ActivityResultContracts.RequestPermission()
    ) { granted ->
        viewModel.onMicPermission(granted)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        viewModel.initHandle(this, filesDir.absolutePath)
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
    var channelName by mutableStateOf("gaming")
    var password by mutableStateOf("")
    var bootstrapNodes by mutableStateOf("")
    var dhtEnabled by mutableStateOf(true)
    var connected by mutableStateOf(false)
    var statusText by mutableStateOf("disconnected")
    var peerCount by mutableStateOf(0)
    var messages by mutableStateOf(listOf<ChatItem>())
    var inputText by mutableStateOf("")
    var pttPressed by mutableStateOf(false)
    var micPermission by mutableStateOf(false)
    private var pollJob: Job? = null
    private val timeFormat = SimpleDateFormat("HH:mm:ss", Locale.US)
    var debugText by mutableStateOf("")

    fun initHandle(context: android.content.Context, basePath: String) {
        if (handle != 0L) return
        handle = RiftNative.init(context, basePath)
        Log.d("RiftMobile", "initHandle handle=$handle")
        if (handle == 0L) {
            statusText = "init failed"
            debugText = RiftNative.lastError() ?: "init failed"
        }
    }

    fun onMicPermission(granted: Boolean) {
        micPermission = granted
    }

    fun connect() {
        val name = channelName.trim()
        if (handle == 0L || name.isEmpty()) {
            statusText = if (handle == 0L) "init failed" else "enter channel"
            debugText = "connect blocked handle=$handle name='$name'"
            return
        }
        if (dhtEnabled && bootstrapNodes.trim().isEmpty()) {
            statusText = "bootstrap required"
            debugText = "dht enabled but no bootstrap"
            return
        }
        val passwordValue = password.ifEmpty { null }
        statusText = "connecting"
        debugText = "connecting..."
        Log.d("RiftMobile", "connect start name=$name")
        RiftNative.setDhtEnabled(handle, dhtEnabled)
        RiftNative.setBootstrapNodes(handle, bootstrapNodes)
        viewModelScope.launch(Dispatchers.IO) {
            val result = RiftNative.joinChannel(handle, name, passwordValue, true, true)
            withContext(Dispatchers.Main) {
                if (result == 0) {
                    connected = true
                    statusText = "connected"
                    debugText = "connected"
                    startPolling()
                } else {
                    connected = false
                    statusText = "connect failed"
                    debugText = "connect failed code=$result"
                }
            }
        }
    }

    fun disconnect() {
        if (handle == 0L) return
        val name = channelName.trim()
        viewModelScope.launch(Dispatchers.IO) {
            if (name.isNotEmpty()) {
                RiftNative.leaveChannel(handle, name)
            }
            withContext(Dispatchers.Main) {
                connected = false
                statusText = "disconnected"
                debugText = "disconnected"
                pollJob?.cancel()
                pollJob = null
            }
        }
    }

    fun sendChat() {
        val text = inputText.trim()
        if (text.isEmpty()) return
        if (handle == 0L) {
            statusText = "init failed"
            debugText = "send blocked handle=0"
            return
        }
        val time = timeFormat.format(Date())
        messages = messages + ChatItem(time = time, from = "me", text = text)
        inputText = ""
        viewModelScope.launch(Dispatchers.IO) {
            val result = RiftNative.sendChat(handle, text)
            if (result != 0) {
                withContext(Dispatchers.Main) {
                    statusText = "send failed"
                    debugText = "send failed code=$result"
                }
            }
        }
    }

    fun startPtt() {
        if (!micPermission || handle == 0L) return
        viewModelScope.launch(Dispatchers.IO) {
            RiftNative.startPtt(handle)
        }
        pttPressed = true
        debugText = "ptt start"
    }

    fun stopPtt() {
        if (handle == 0L) return
        viewModelScope.launch(Dispatchers.IO) {
            RiftNative.stopPtt(handle)
        }
        pttPressed = false
        debugText = "ptt stop"
    }

    private fun startPolling() {
        if (pollJob != null) return
        pollJob = viewModelScope.launch(Dispatchers.IO) {
            while (connected) {
                val event = RiftNative.pollEvent(handle)
                if (event != null) {
                    withContext(Dispatchers.Main) {
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
        if (viewModel.debugText.isNotEmpty()) {
            Text(
                text = "Debug: ${viewModel.debugText}",
                style = MaterialTheme.typography.bodySmall
            )
        }
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
        OutlinedTextField(
            value = viewModel.bootstrapNodes,
            onValueChange = { viewModel.bootstrapNodes = it },
            label = { Text("DHT Bootstrap (ip:port, comma)") },
            modifier = Modifier.fillMaxWidth()
        )
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            Text("Use DHT")
            Switch(
                checked = viewModel.dhtEnabled,
                onCheckedChange = { viewModel.dhtEnabled = it }
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
