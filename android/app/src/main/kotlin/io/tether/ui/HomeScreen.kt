package io.tether.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.tether.pairing.PairingUiState
import io.tether.pairing.PairingViewModel

private val Bg = Color(0xFF0C0E14)
private val Surface = Color(0xFF161A25)
private val TextDim = Color(0xFF98A2B8)
private val Accent = Color(0xFF6C8CFF)

@Composable
fun HomeScreen(vm: PairingViewModel) {
    val state by vm.state.collectAsState()
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Bg),
        contentAlignment = Alignment.Center,
    ) {
        when (val s = state) {
            is PairingUiState.Idle -> IdleView()
            is PairingUiState.Status -> StatusView(s.statusKey, s.allowManual, onManual = vm::openManualEntry)
            is PairingUiState.Card -> PairingCard(
                peerName = s.peerName,
                emojis = s.emojis,
                onConfirm = vm::confirm,
                onMismatch = vm::mismatch,
            )
            is PairingUiState.ManualForm -> ManualForm(onSubmit = { addr, pin ->
                vm.submitManual(addr, pin)
            })
            is PairingUiState.Paired -> PairedView(s.peerName)
            is PairingUiState.Mismatch -> MismatchView(s.reason, onRetry = vm::restart)
            is PairingUiState.Exhausted -> ExhaustedView(onRetry = vm::restart, onManual = vm::openManualEntry)
        }
    }
}

@Composable
private fun IdleView() {
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(10.dp),
        modifier = Modifier.padding(24.dp),
    ) {
        Text(
            Strings.HomePhoneIdle,
            color = Color.White,
            style = TextStyle(fontSize = 24.sp, fontWeight = FontWeight.SemiBold),
            textAlign = TextAlign.Center,
        )
        Text(
            "Make sure Tether is open on your computer.",
            color = TextDim,
            style = TextStyle(fontSize = 14.sp),
            textAlign = TextAlign.Center,
        )
    }
}

@Composable
private fun StatusView(statusKey: String, allowManual: Boolean, onManual: () -> Unit) {
    val label = when (statusKey) {
        "cascade.mdns" -> Strings.CascadeMdns
        "cascade.fallback" -> Strings.CascadeFallback
        "cascade.usb.prompt" -> Strings.CascadeUsbPrompt
        "cascade.usb.detected" -> Strings.CascadeUsbDetected
        "cascade.usb.debug" -> Strings.CascadeUsbDebug
        "cascade.hotspot" -> Strings.CascadeHotspot
        else -> Strings.HomePhoneIdle
    }
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(14.dp),
        modifier = Modifier.padding(24.dp),
    ) {
        CircularProgressIndicator(
            modifier = Modifier.height(28.dp),
            strokeWidth = 2.dp,
            color = Accent,
        )
        Text(
            label,
            color = Color.White,
            style = TextStyle(fontSize = 22.sp, fontWeight = FontWeight.SemiBold),
            textAlign = TextAlign.Center,
        )
        if (allowManual) {
            Spacer(Modifier.height(12.dp))
            TextButton(onClick = onManual) {
                Text(Strings.PairManual, color = TextDim, fontSize = 13.sp)
            }
        }
    }
}

@Composable
private fun PairingCard(
    peerName: String,
    emojis: List<String>,
    onConfirm: () -> Unit,
    onMismatch: () -> Unit,
) {
    var confirmed by remember { mutableStateOf(false) }
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(14.dp),
        modifier = Modifier
            .padding(24.dp)
            .fillMaxWidth(),
    ) {
        Text(
            Strings.pairCardTitle(peerName),
            color = Color.White,
            style = TextStyle(fontSize = 24.sp, fontWeight = FontWeight.SemiBold),
            textAlign = TextAlign.Center,
        )
        Box(
            modifier = Modifier
                .background(Surface, RoundedCornerShape(20.dp))
                .padding(28.dp)
        ) {
            Text(
                emojis.joinToString("  "),
                style = TextStyle(fontSize = 64.sp),
            )
        }
        Text(
            Strings.PairCardSubhead,
            color = TextDim,
            style = TextStyle(fontSize = 14.sp),
            textAlign = TextAlign.Center,
        )
        Button(
            onClick = {
                if (!confirmed) {
                    confirmed = true
                    onConfirm()
                }
            },
            colors = ButtonDefaults.buttonColors(containerColor = Accent),
            enabled = !confirmed,
        ) {
            Text(
                if (confirmed) "Waiting for the other side…" else Strings.PairCardConfirm,
                color = Color.White,
                fontWeight = FontWeight.SemiBold,
            )
        }
        TextButton(onClick = onMismatch) {
            Text(Strings.PairMismatch, color = Color(0xFFF87171), fontSize = 13.sp, textAlign = TextAlign.Center)
        }
    }
}

@Composable
private fun ManualForm(onSubmit: (String, String) -> Unit) {
    var addr by remember { mutableStateOf("") }
    var pin by remember { mutableStateOf("") }
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(14.dp),
        modifier = Modifier.padding(24.dp).fillMaxWidth(),
    ) {
        Text(
            Strings.PairManual,
            color = Color.White,
            style = TextStyle(fontSize = 22.sp, fontWeight = FontWeight.SemiBold),
        )
        Text(
            "Enter your PC's address and the 6-digit code shown on screen.",
            color = TextDim,
            fontSize = 13.sp,
            textAlign = TextAlign.Center,
        )
        OutlinedTextField(
            value = addr,
            onValueChange = { addr = it },
            label = { Text("PC address", color = TextDim) },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        OutlinedTextField(
            value = pin,
            onValueChange = { pin = it.filter { c -> c.isDigit() }.take(6) },
            label = { Text("6-digit code", color = TextDim) },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        Button(
            onClick = { onSubmit(addr.trim(), pin) },
            colors = ButtonDefaults.buttonColors(containerColor = Accent),
            enabled = addr.isNotBlank() && pin.length == 6,
        ) {
            Text("Pair", color = Color.White, fontWeight = FontWeight.SemiBold)
        }
    }
}

@Composable
private fun PairedView(peerName: String) {
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(10.dp),
        modifier = Modifier.padding(24.dp),
    ) {
        Text("✓ $peerName", color = Color(0xFF4ADE80), style = TextStyle(fontSize = 28.sp, fontWeight = FontWeight.SemiBold))
        Text(Strings.PairSuccess, color = TextDim, fontSize = 14.sp, textAlign = TextAlign.Center)
    }
}

@Composable
private fun MismatchView(reason: String, onRetry: () -> Unit) {
    val msg = when (reason) {
        "timeout" -> "We waited but no one confirmed. Try again from both apps."
        "protocol" -> "Something didn't add up about the other device. Try again."
        else -> Strings.PairMismatch
    }
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(14.dp),
        modifier = Modifier.padding(24.dp),
    ) {
        Text(msg, color = Color(0xFFF87171), fontSize = 16.sp, textAlign = TextAlign.Center)
        Button(onClick = onRetry, colors = ButtonDefaults.buttonColors(containerColor = Accent)) {
            Text("Try again", color = Color.White)
        }
    }
}

@Composable
private fun ExhaustedView(onRetry: () -> Unit, onManual: () -> Unit) {
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(14.dp),
        modifier = Modifier.padding(24.dp),
    ) {
        Text(
            "We can't see your computer from this network. Plug in a USB cable to finish setup.",
            color = Color.White,
            fontSize = 18.sp,
            textAlign = TextAlign.Center,
            fontWeight = FontWeight.SemiBold,
        )
        Button(onClick = onRetry, colors = ButtonDefaults.buttonColors(containerColor = Accent)) {
            Text("Try again", color = Color.White)
        }
        TextButton(onClick = onManual) {
            Text(Strings.PairManual, color = TextDim, fontSize = 13.sp)
        }
    }
}
