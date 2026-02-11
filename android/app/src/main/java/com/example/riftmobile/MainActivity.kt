package com.example.riftmobile

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.core.content.ContextCompat
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Divider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.example.riftmobile.sdk.RiftClient
import com.example.riftmobile.sdk.RiftEvent
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

class MainActivity : ComponentActivity() {
    private val viewModel: MainViewModel by viewModels()

    private val requestMicPermission = registerForActivityResult(
        ActivityResultContracts.RequestPermission()
    ) { granted ->
        viewModel.onMicPermission(granted)
    }

    private val requestNotificationPermission = registerForActivityResult(
        ActivityResultContracts.RequestPermission()
    ) { granted ->
        viewModel.onNotificationPermission(granted)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        viewModel.initHandle(this, filesDir.absolutePath)

        // Request notification permission on Android 13+
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            if (ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS)
                != PackageManager.PERMISSION_GRANTED) {
                requestNotificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
            } else {
                viewModel.onNotificationPermission(true)
            }
        } else {
            viewModel.onNotificationPermission(true)
        }

        setContent {
            MaterialTheme {
                Surface {
                    RiftApp(viewModel) {
                        requestMicPermission.launch(Manifest.permission.RECORD_AUDIO)
                    }
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

data class PeerItem(
    val id: String,
)

data class IncomingCallState(
    val sessionId: String,
    val from: String,
    val rndzvSrtUri: String? = null
)

data class ActiveCallState(
    val sessionId: String,
    val peerId: String,
    val state: String
)

enum class Screen {
    Home,
    Call,
    Settings
}

class MainViewModel : ViewModel() {
    private val timeFormat = SimpleDateFormat("HH:mm:ss", Locale.US)
    private var appContext: android.content.Context? = null

    var screen by mutableStateOf(Screen.Home)
        private set
    var client: RiftClient? = null
        private set

    var channelName by mutableStateOf("gaming")
    var password by mutableStateOf("")
    var inviteInput by mutableStateOf("")
    var generatedInvite by mutableStateOf("")
    var knownPeers by mutableStateOf("")
    var bootstrapNodes by mutableStateOf("")
    var dhtEnabled by mutableStateOf(false)
    var internetEnabled by mutableStateOf(true)
    var connected by mutableStateOf(false)
    var statusText by mutableStateOf("disconnected")
    var peerCount by mutableStateOf(0)
    var peers by mutableStateOf(listOf<PeerItem>())
    var messages by mutableStateOf(listOf<ChatItem>())
    var inputText by mutableStateOf("")
    var pttPressed by mutableStateOf(false)
    var micPermission by mutableStateOf(false)
    var notificationPermission by mutableStateOf(false)
    var debugText by mutableStateOf("")

    var audioQuality by mutableStateOf("medium")
    var turnServers by mutableStateOf("")

    // Call state
    var incomingCall by mutableStateOf<IncomingCallState?>(null)
        private set
    var activeCall by mutableStateOf<ActiveCallState?>(null)
        private set
    var muted by mutableStateOf(false)
        private set

    fun initHandle(context: android.content.Context, basePath: String) {
        if (client != null) return
        appContext = context.applicationContext
        val newClient = RiftClient(context, basePath)
        val ok = newClient.init()
        client = newClient
        Log.d("RiftMobile", "initHandle ok=$ok")
        if (!ok) {
            statusText = "init failed"
            debugText = newClient.lastError() ?: "init failed"
            return
        }
        startEventCollector(newClient)
    }

    private fun startEventCollector(newClient: RiftClient) {
        viewModelScope.launch(Dispatchers.IO) {
            newClient.events.collect { event ->
                withContext(Dispatchers.Main) {
                    when (event) {
                        is RiftEvent.Chat -> {
                            messages = messages + ChatItem(
                                time = event.timestamp,
                                from = event.from,
                                text = event.text
                            )
                        }
                        is RiftEvent.PeerJoined -> {
                            peers = (peers + PeerItem(event.peerId)).distinctBy { it.id }
                            peerCount = peers.size
                        }
                        is RiftEvent.PeerLeft -> {
                            peers = peers.filterNot { it.id == event.peerId }
                            peerCount = peers.size
                        }
                        is RiftEvent.Stats -> {
                            peerCount = event.peers
                        }
                        is RiftEvent.Security -> {
                            messages = messages + ChatItem(
                                time = timeFormat.format(Date()),
                                from = "security",
                                text = event.message
                            )
                        }
                        is RiftEvent.Fingerprint -> {
                            messages = messages + ChatItem(
                                time = timeFormat.format(Date()),
                                from = "fingerprint",
                                text = "${event.peerId} ${event.fingerprint}"
                            )
                        }
                        is RiftEvent.Status -> {
                            statusText = event.text
                        }
                        is RiftEvent.IncomingCall -> {
                            incomingCall = IncomingCallState(
                                sessionId = event.sessionId,
                                from = event.from,
                                rndzvSrtUri = event.rndzvSrtUri
                            )
                        }
                        is RiftEvent.CallStateChanged -> {
                            when (event.state.lowercase()) {
                                "active", "connected" -> {
                                    // Call became active
                                    val incoming = incomingCall
                                    if (incoming != null && incoming.sessionId == event.sessionId) {
                                        activeCall = ActiveCallState(
                                            sessionId = event.sessionId,
                                            peerId = incoming.from,
                                            state = event.state
                                        )
                                        incomingCall = null
                                    } else if (activeCall?.sessionId == event.sessionId) {
                                        activeCall = activeCall?.copy(state = event.state)
                                    }
                                }
                                "ended", "terminated", "failed" -> {
                                    if (activeCall?.sessionId == event.sessionId) {
                                        activeCall = null
                                        stopForegroundService()
                                    }
                                    if (incomingCall?.sessionId == event.sessionId) {
                                        incomingCall = null
                                    }
                                }
                                else -> {
                                    activeCall = activeCall?.copy(state = event.state)
                                }
                            }
                        }
                        is RiftEvent.AudioLevel -> {
                            // Could use this to show audio level indicators
                        }
                    }
                }
            }
        }
    }

    fun onMicPermission(granted: Boolean) {
        micPermission = granted
    }

    fun onNotificationPermission(granted: Boolean) {
        notificationPermission = granted
    }

    private fun startForegroundService(sessionId: String, peerId: String) {
        appContext?.let { ctx ->
            RiftForegroundService.startCall(ctx, sessionId, peerId)
        }
    }

    private fun stopForegroundService() {
        appContext?.let { ctx ->
            RiftForegroundService.endCall(ctx)
        }
    }

    fun navigateTo(next: Screen) {
        screen = next
    }

    fun connect() {
        val name = channelName.trim()
        val client = client ?: return
        if (name.isEmpty()) {
            statusText = "enter channel"
            return
        }
        val passwordValue = password.ifEmpty { null }
        statusText = "connecting"
        debugText = "connecting..."
        client.setDhtEnabled(dhtEnabled)
        client.setBootstrapNodes(bootstrapNodes)
        client.setTurnServers(turnServers)
        client.setAudioQuality(audioQuality)
        viewModelScope.launch(Dispatchers.IO) {
            val ok = client.joinChannel(name, passwordValue, internetEnabled, dhtEnabled)
            withContext(Dispatchers.Main) {
                if (ok) {
                    connected = true
                    statusText = "connected"
                    screen = Screen.Call
                } else {
                    connected = false
                    statusText = "connect failed"
                    debugText = "connect failed"
                }
            }
        }
    }

    fun connectWithInvite() {
        val invite = inviteInput.trim()
        val name = channelName.trim()
        val client = client ?: return
        if (invite.isEmpty() || name.isEmpty()) {
            statusText = "enter invite + channel"
            return
        }
        client.setInvite(invite)
        connect()
    }

    fun disconnect() {
        val client = client ?: return
        val name = channelName.trim()
        viewModelScope.launch(Dispatchers.IO) {
            if (name.isNotEmpty()) {
                client.leaveChannel(name)
            }
            withContext(Dispatchers.Main) {
                connected = false
                statusText = "disconnected"
                pttPressed = false
                peers = emptyList()
                peerCount = 0
                screen = Screen.Home
            }
        }
    }

    fun sendChat() {
        val text = inputText.trim()
        val client = client ?: return
        if (text.isEmpty()) return
        if (!connected) {
            statusText = "not connected"
            return
        }
        val time = timeFormat.format(Date())
        messages = messages + ChatItem(time = time, from = "me", text = text)
        inputText = ""
        viewModelScope.launch(Dispatchers.IO) {
            val ok = client.sendChat(text)
            if (!ok) {
                withContext(Dispatchers.Main) {
                    statusText = "send failed"
                }
            }
        }
    }

    fun startPtt() {
        val client = client ?: return
        if (!micPermission || !connected) return
        client.startPtt()
        pttPressed = true
    }

    fun stopPtt() {
        val client = client ?: return
        client.stopPtt()
        pttPressed = false
    }

    fun generateInvite() {
        val client = client ?: return
        val channel = channelName.trim()
        if (channel.isEmpty()) return
        generatedInvite = client.generateInvite(channel, password.ifEmpty { null }, knownPeers) ?: ""
    }

    fun startCall(peerId: String) {
        val client = client ?: return
        if (!connected) return
        viewModelScope.launch(Dispatchers.IO) {
            val sessionId = client.startCall(peerId)
            if (sessionId != null) {
                withContext(Dispatchers.Main) {
                    activeCall = ActiveCallState(
                        sessionId = sessionId,
                        peerId = peerId,
                        state = "connecting"
                    )
                    startForegroundService(sessionId, peerId)
                }
            } else {
                withContext(Dispatchers.Main) {
                    statusText = "call failed"
                }
            }
        }
    }

    fun acceptCall() {
        val call = incomingCall ?: return
        val client = client ?: return
        viewModelScope.launch(Dispatchers.IO) {
            val ok = client.acceptCall(call.sessionId)
            if (ok) {
                withContext(Dispatchers.Main) {
                    activeCall = ActiveCallState(
                        sessionId = call.sessionId,
                        peerId = call.from,
                        state = "active"
                    )
                    incomingCall = null
                    startForegroundService(call.sessionId, call.from)
                }
            }
        }
    }

    fun declineCall() {
        val call = incomingCall ?: return
        val client = client ?: return
        viewModelScope.launch(Dispatchers.IO) {
            client.declineCall(call.sessionId)
            withContext(Dispatchers.Main) {
                incomingCall = null
            }
        }
    }

    fun endCall() {
        val call = activeCall ?: return
        val client = client ?: return
        viewModelScope.launch(Dispatchers.IO) {
            client.endCall(call.sessionId)
            withContext(Dispatchers.Main) {
                activeCall = null
                stopForegroundService()
            }
        }
    }

    fun toggleMute() {
        val client = client ?: return
        val newMuted = !muted
        if (client.setMute(newMuted)) {
            muted = newMuted
        }
    }
}

@Composable
fun RiftApp(viewModel: MainViewModel, onRequestMic: () -> Unit) {
    // Show incoming call dialog
    viewModel.incomingCall?.let { call ->
        IncomingCallDialog(
            from = call.from.take(16) + "...",
            onAccept = { viewModel.acceptCall() },
            onDecline = { viewModel.declineCall() }
        )
    }

    when (viewModel.screen) {
        Screen.Home -> HomeScreen(viewModel, onRequestMic)
        Screen.Call -> CallScreen(viewModel)
        Screen.Settings -> SettingsScreen(viewModel)
    }
}

@Composable
fun IncomingCallDialog(
    from: String,
    onAccept: () -> Unit,
    onDecline: () -> Unit
) {
    AlertDialog(
        onDismissRequest = onDecline,
        title = { Text("Incoming Call") },
        text = { Text("Call from $from") },
        confirmButton = {
            Button(onClick = onAccept) {
                Text("Accept")
            }
        },
        dismissButton = {
            TextButton(onClick = onDecline) {
                Text("Decline")
            }
        }
    )
}

@Composable
fun HomeScreen(viewModel: MainViewModel, onRequestMic: () -> Unit) {
    Column(modifier = Modifier.fillMaxSize().padding(16.dp)) {
        Text(
            text = "Status: ${viewModel.statusText} | Peers: ${viewModel.peerCount}",
            fontWeight = FontWeight.Bold
        )
        if (viewModel.debugText.isNotEmpty()) {
            Text(text = "Debug: ${viewModel.debugText}", style = MaterialTheme.typography.bodySmall)
        }
        Spacer(modifier = Modifier.height(8.dp))
        OutlinedTextField(
            value = viewModel.channelName,
            onValueChange = { viewModel.channelName = it },
            label = { Text("Channel") },
            modifier = Modifier.fillMaxWidth()
        )
        Spacer(modifier = Modifier.height(8.dp))
        OutlinedTextField(
            value = viewModel.password,
            onValueChange = { viewModel.password = it },
            label = { Text("Password") },
            modifier = Modifier.fillMaxWidth()
        )
        Spacer(modifier = Modifier.height(8.dp))
        OutlinedTextField(
            value = viewModel.inviteInput,
            onValueChange = { viewModel.inviteInput = it },
            label = { Text("Invite Link") },
            modifier = Modifier.fillMaxWidth()
        )
        Spacer(modifier = Modifier.height(8.dp))
        OutlinedTextField(
            value = viewModel.knownPeers,
            onValueChange = { viewModel.knownPeers = it },
            label = { Text("Known Peers (ip:port, comma)") },
            modifier = Modifier.fillMaxWidth()
        )
        Spacer(modifier = Modifier.height(8.dp))
        OutlinedTextField(
            value = viewModel.bootstrapNodes,
            onValueChange = { viewModel.bootstrapNodes = it },
            label = { Text("DHT Bootstrap (ip:port, comma)") },
            modifier = Modifier.fillMaxWidth()
        )
        Spacer(modifier = Modifier.height(8.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("Internet")
            Spacer(modifier = Modifier.width(8.dp))
            Switch(checked = viewModel.internetEnabled, onCheckedChange = { viewModel.internetEnabled = it })
            Spacer(modifier = Modifier.width(16.dp))
            Text("DHT")
            Spacer(modifier = Modifier.width(8.dp))
            Switch(checked = viewModel.dhtEnabled, onCheckedChange = { viewModel.dhtEnabled = it })
        }
        Spacer(modifier = Modifier.height(12.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(onClick = {
                if (!viewModel.micPermission) {
                    onRequestMic()
                }
                viewModel.connect()
            }) { Text("Join Channel") }
            OutlinedButton(onClick = { viewModel.connectWithInvite() }) { Text("Join Invite") }
        }
        Spacer(modifier = Modifier.height(8.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(onClick = { viewModel.generateInvite() }) { Text("Generate Invite") }
            OutlinedButton(onClick = { viewModel.navigateTo(Screen.Settings) }) { Text("Settings") }
        }
        if (viewModel.generatedInvite.isNotEmpty()) {
            Spacer(modifier = Modifier.height(8.dp))
            Text("Invite:")
            Text(viewModel.generatedInvite, style = MaterialTheme.typography.bodySmall)
        }
    }
}

@Composable
fun CallScreen(viewModel: MainViewModel) {
    val listState = rememberLazyListState()
    Column(modifier = Modifier.fillMaxSize().padding(16.dp)) {
        Row(horizontalArrangement = Arrangement.SpaceBetween, modifier = Modifier.fillMaxWidth()) {
            Text("Channel: ${viewModel.channelName}", fontWeight = FontWeight.Bold)
            Text("Peers: ${viewModel.peerCount}")
        }

        // Active call banner
        viewModel.activeCall?.let { call ->
            Spacer(modifier = Modifier.height(8.dp))
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(MaterialTheme.colorScheme.primaryContainer)
                    .padding(12.dp),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Column {
                    Text("In call with: ${call.peerId.take(12)}...", fontWeight = FontWeight.Bold)
                    Text("Status: ${call.state}", style = MaterialTheme.typography.bodySmall)
                }
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedButton(onClick = { viewModel.toggleMute() }) {
                        Text(if (viewModel.muted) "Unmute" else "Mute")
                    }
                    Button(onClick = { viewModel.endCall() }) {
                        Text("End")
                    }
                }
            }
        }

        Spacer(modifier = Modifier.height(8.dp))
        Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Column(modifier = Modifier.weight(0.35f)) {
                Text("Peers", fontWeight = FontWeight.Bold)
                LazyColumn(
                    modifier = Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surfaceVariant)
                ) {
                    items(viewModel.peers) { peer ->
                        Row(
                            modifier = Modifier.fillMaxWidth().padding(6.dp),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Text(peer.id.take(12) + "...", modifier = Modifier.weight(1f))
                            if (viewModel.activeCall == null) {
                                TextButton(
                                    onClick = { viewModel.startCall(peer.id) },
                                    modifier = Modifier.padding(0.dp)
                                ) {
                                    Text("Call", style = MaterialTheme.typography.bodySmall)
                                }
                            }
                        }
                    }
                }
            }
            Column(modifier = Modifier.weight(0.65f)) {
                Text("Chat", fontWeight = FontWeight.Bold)
                ChatList(viewModel.messages, listState, modifier = Modifier.weight(1f))
                Spacer(modifier = Modifier.height(8.dp))
                ChatInput(
                    value = viewModel.inputText,
                    onValueChange = { viewModel.inputText = it },
                    onSend = { viewModel.sendChat() },
                    enabled = viewModel.connected
                )
            }
        }
        Spacer(modifier = Modifier.height(12.dp))
        PttButton(
            pressed = viewModel.pttPressed,
            onDown = { viewModel.startPtt() },
            onUp = { viewModel.stopPtt() },
            enabled = viewModel.connected
        )
        Spacer(modifier = Modifier.height(12.dp))
        Divider()
        Spacer(modifier = Modifier.height(8.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = { viewModel.navigateTo(Screen.Settings) }) { Text("Settings") }
            OutlinedButton(onClick = { viewModel.disconnect() }) { Text("Leave") }
        }
    }

    LaunchedEffect(viewModel.messages.size) {
        if (viewModel.messages.isNotEmpty()) {
            listState.animateScrollToItem(viewModel.messages.size - 1)
        }
    }
}

@Composable
fun SettingsScreen(viewModel: MainViewModel) {
    Column(modifier = Modifier.fillMaxSize().padding(16.dp)) {
        Text("Settings", fontWeight = FontWeight.Bold)
        Spacer(modifier = Modifier.height(8.dp))
        OutlinedTextField(
            value = viewModel.audioQuality,
            onValueChange = { viewModel.audioQuality = it },
            label = { Text("Audio quality (low|medium|high)") },
            modifier = Modifier.fillMaxWidth()
        )
        Spacer(modifier = Modifier.height(8.dp))
        OutlinedTextField(
            value = viewModel.turnServers,
            onValueChange = { viewModel.turnServers = it },
            label = { Text("TURN servers (turn:host:port?username=...)") },
            modifier = Modifier.fillMaxWidth()
        )
        Spacer(modifier = Modifier.height(12.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(onClick = { viewModel.navigateTo(Screen.Home) }) { Text("Back") }
            OutlinedButton(onClick = { viewModel.navigateTo(Screen.Call) }) { Text("Call") }
        }
    }
}

@Composable
fun ChatList(messages: List<ChatItem>, listState: androidx.compose.foundation.lazy.LazyListState, modifier: Modifier = Modifier) {
    LazyColumn(
        state = listState,
        modifier = modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surfaceVariant)
    ) {
        items(messages) { item ->
            Text("[${item.time}] ${item.from}: ${item.text}", modifier = Modifier.padding(4.dp))
        }
    }
}

@Composable
fun ChatInput(value: String, onValueChange: (String) -> Unit, onSend: () -> Unit, enabled: Boolean) {
    val keyboard = LocalSoftwareKeyboardController.current
    Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedTextField(
            value = value,
            onValueChange = onValueChange,
            label = { Text("Message") },
            modifier = Modifier.weight(1f),
            enabled = enabled,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Text, imeAction = ImeAction.Send),
            keyboardActions = KeyboardActions(onSend = {
                onSend()
                keyboard?.hide()
            })
        )
        Button(onClick = onSend, enabled = enabled) {
            Text("Send")
        }
    }
}

@Composable
fun PttButton(pressed: Boolean, onDown: () -> Unit, onUp: () -> Unit, enabled: Boolean) {
    val color = if (pressed) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.secondary
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .height(80.dp)
            .background(color)
            .pointerInput(enabled) {
                if (enabled) {
                    detectTapGestures(
                        onPress = {
                            onDown()
                            tryAwaitRelease()
                            onUp()
                        }
                    )
                }
            },
        contentAlignment = Alignment.Center
    ) {
        Text(
            text = if (!enabled) "Connect to Talk" else if (pressed) "Talking..." else "Hold to Talk",
            color = MaterialTheme.colorScheme.onPrimary
        )
    }
}
