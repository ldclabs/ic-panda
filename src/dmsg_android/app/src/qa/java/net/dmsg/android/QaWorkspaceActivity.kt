package net.dmsg.android

import android.app.Application
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.ViewModelProvider
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import net.dmsg.android.core.ClientController
import net.dmsg.android.core.ReadState
import net.dmsg.android.platform.AppLanguage
import net.dmsg.android.ui.DMsgApp
import net.dmsg.core.MobileEngine
import net.dmsg.core.VaultEdit
import net.dmsg.core.FieldEdit
import java.io.File
import java.util.UUID

/** QA source set only. Every screen uses a separate synthetic, offline database. */
class QaWorkspaceActivity : ComponentActivity() {
    override fun attachBaseContext(newBase: android.content.Context) = super.attachBaseContext(AppLanguage.wrap(newBase))
    private lateinit var model: QaWorkspaceModel
    internal val qaClient: ClientController get() = model.client
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        AppLanguage.applyToWindow(this)
        enableEdgeToEdge()
        model = ViewModelProvider(this)[QaWorkspaceModel::class.java]
        setContent { DMsgApp(model.client, browser = { model.client.showError("UNSUPPORTED_PROTOCOL") }, share = { _, _ -> model.client.showError("UNSUPPORTED_PROTOCOL") }) }
        if (intent.getBooleanExtra("scale_probe", false)) model.seedScale()
    }
    override fun onStart() { super.onStart(); model.client.prompts.attach(this) }
    override fun onStop() { model.client.prompts.detach(this); super.onStop() }
}

class QaWorkspaceModel(application: Application) : AndroidViewModel(application) {
    private lateinit var directory: File
    private val cleanup = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var scaleStarted = false
    val client = ClientController(application) { paths, platform ->
        directory = File(paths.workspace, "ui-gallery/" + UUID.randomUUID())
        MobileEngine.openUiFixture(directory.absolutePath, paths.staging.absolutePath, platform).also { core ->
            val summary = core.vaultPage("", null, false, 0u, 100u).items.first { it.title == "Studio account" }
            val item = core.vaultItem(summary.id, null).item
            core.saveItem(item.id, item.revision, VaultEdit(item.title, item.kind, item.notes, item.favorite, item.tags,
                item.fields.map { field -> FieldEdit(field.id, field.kind, field.label, field.section, when (field.id) {
                    "username" -> "demo@example.test"
                    "password" -> "Sample-only password"
                    else -> field.value
                }) }, item.files))
        }
    }.apply { automaticSync(false); start() }

    /** Standalone real-clock profiling, never enabled in an ordinary APK. */
    fun seedScale() {
        if (scaleStarted) return
        scaleStarted = true
        cleanup.launch {
            client.state.first { it.unlocked }
            client.perform({ core ->
                repeat(1000) { index ->
                    core.saveItem(null, null, VaultEdit("Scale " + index.toString().padStart(4, '0'), "login", "Synthetic scale sample", index % 8 == 0,
                        listOf("scale-probe"), listOf(FieldEdit("password", "concealed", "", "", "Scale-only secret")), emptyList()))
                }
            }, key = "qa:scale-seed") {
                cleanup.launch {
                    client.vault.browse()
                    client.vault.listing.state.first { it is ReadState.Ready && it.value.items.size == 100 }
                    while (client.vault.listing.value()?.hasMore == true) {
                        val previous = client.vault.listing.value()!!.items.size
                        client.vault.browse(more = true)
                        client.vault.listing.state.first { it is ReadState.Ready && it.value.items.size > previous }
                    }
                    android.util.Log.i("dMsgScaleProbe", "ready; records=${client.vault.listing.value()?.items?.size}; synthetic offline QA")
                }
            }
        }
    }

    override fun onCleared() {
        cleanup.launch {
            client.shutdown()
            withContext(Dispatchers.IO) { if (::directory.isInitialized) directory.deleteRecursively() }
            cleanup.cancel()
        }
    }
}
