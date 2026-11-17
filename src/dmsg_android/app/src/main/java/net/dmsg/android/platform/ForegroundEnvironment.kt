package net.dmsg.android.platform

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.os.Handler
import android.os.Looper
import android.os.PowerManager
import androidx.core.content.ContextCompat

/** Registration lifetime is the visible app, never a background keep-alive service. */
class ForegroundEnvironment(private val context: Context, private val networkChanged: () -> Unit, private val constrained: (Boolean) -> Unit) {
    private val connectivity = context.getSystemService(ConnectivityManager::class.java)
    private val power = context.getSystemService(PowerManager::class.java)
    private val main = Handler(Looper.getMainLooper())
    private var active = false
    private var validated: Network? = null
    private val network = object : ConnectivityManager.NetworkCallback() {
        override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) {
            val valid = capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
            main.post { if (active && valid && validated != network) { validated = network; networkChanged() } }
        }
        override fun onLost(network: Network) { main.post { if (validated == network) validated = null } }
    }
    private val receiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) { updatePower() }
    }
    private val thermal = PowerManager.OnThermalStatusChangedListener { updatePower() }
    private fun updatePower() {
        if (active) constrained(power.isPowerSaveMode || power.currentThermalStatus >= PowerManager.THERMAL_STATUS_SEVERE)
    }
    fun start() {
        if (active) return
        active = true
        connectivity.registerDefaultNetworkCallback(network)
        ContextCompat.registerReceiver(context, receiver, IntentFilter(PowerManager.ACTION_POWER_SAVE_MODE_CHANGED), ContextCompat.RECEIVER_NOT_EXPORTED)
        power.addThermalStatusListener(context.mainExecutor, thermal)
        updatePower()
    }
    fun stop() {
        if (!active) return
        active = false; validated = null
        connectivity.unregisterNetworkCallback(network)
        context.unregisterReceiver(receiver)
        power.removeThermalStatusListener(thermal)
    }
}
